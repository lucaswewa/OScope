//! `ex-jobs`: the job service (P20, ADR-0016). Tours of the simulated stage
//! run as hardware jobs, one at a time, holding the hardware lock; number
//! crunching runs as compute jobs, two at a time, sharing a GPU budget. A
//! page shows the queue and its progress live.

use std::net::SocketAddr;
use std::path::Path;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, ensure};
use ex_common::{Example, get, post};
use oscope_api::Api;
use oscope_api::devices::{Devices, DevicesConfig};
use oscope_core::{Um, Xyz};
use oscope_hal::UmPerS;
use oscope_jobs::{Job, JobService, JobSpec, JobsConfig, Resource, Stop};
use oscope_sim::Rate;
use serde_json::{Value, json};

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("ex-jobs: {error:#}");
            ExitCode::FAILURE
        }
    }
}

/// Where tours start: over the H&E slide.
const START: (f64, f64) = (40_000.0, 12_000.0);

async fn run() -> anyhow::Result<()> {
    // Its port is ex-server-hello's: the manual-check range is used up.
    let example = Example::from_args("ex-jobs", 8211)?;
    // The simulator on the scaled-down sensor, homed and placed at 50 times
    // real time, then in real time.
    let mut config = DevicesConfig::default();
    config.simulator.sensor = "quarter".to_owned();
    config.simulator.rate = 50.0;
    let devices = Devices::from_config(&config, Path::new("."))
        .await
        .map_err(anyhow::Error::msg)?;
    let simulator = devices.simulator.clone().context("the simulator")?;
    devices.stage.home().await?;
    let start = Xyz::new(Um(START.0), Um(START.1), Um(5_000.0));
    devices.stage.move_to(start, None).await?;
    simulator.clock.set_rate(Rate::RealTime);
    let devices = Arc::new(devices);
    std::fs::create_dir_all(&example.roots.data)?;
    let budget = JobsConfig {
        workers: 2,
        gpu_mib: Some(2048),
        ..JobsConfig::default()
    };
    let jobs = JobService::open(&example.roots.data.join("index.sqlite"), budget)?;
    register(&jobs, &devices);
    let page = Path::new(env!("CARGO_MANIFEST_DIR")).join("page");
    let api = Api::builder(&example.roots)
        .settings(example.roots.data.join("settings"))
        .devices(devices)
        .jobs(jobs);
    example.serve(api, page, smoke).await
}

fn spec(title: String, resource: Resource, gpu_mib: u32) -> JobSpec {
    JobSpec {
        kind: String::new(),
        title,
        account: None,
        params: Value::Null,
        resource,
        pausable: true,
        gpu_mib,
    }
}

/// The two kinds of job: `demo.tour` and `demo.crunch`.
fn register(jobs: &JobService, devices: &Arc<Devices>) {
    let devices = Arc::clone(devices);
    // A tour of the stage: `points` stops along x, `step_um` apart, at
    // `speed` µm/s, and back to the start.
    jobs.register(
        "demo.tour",
        Arc::new(move |params| {
            let points = params["points"].as_u64().unwrap_or(5).clamp(1, 50);
            let step = params["step_um"].as_f64().unwrap_or(1_000.0);
            let speed = params["speed"].as_f64().unwrap_or(2_000.0);
            let devices = Arc::clone(&devices);
            let title = format!("A tour of {points} stops, {step} µm apart");
            Ok(Job {
                spec: spec(title, Resource::Hardware, 0),
                run: Box::new(move |ctx| {
                    Box::pin(async move {
                        let stage = &devices.stage;
                        for i in 0..=points {
                            ctx.checkpoint().await?;
                            let x = START.0 + step * (i % points) as f64;
                            let target = Xyz::new(Um(x), Um(START.1), Um(5_000.0));
                            stage
                                .move_to(target, Some(UmPerS(speed)))
                                .await
                                .map_err(Stop::failed)?;
                            ctx.count("stops", i);
                            let stage_text = format!("stop {i} of {points}, x = {x:.0} µm");
                            ctx.progress(Some(i as f64 / points as f64), stage_text);
                        }
                        Ok(())
                    })
                }),
            })
        }),
    );
    // Number crunching: `seconds` of work in 50 ms steps, holding `gpu_mib`
    // of the GPU budget.
    jobs.register(
        "demo.crunch",
        Arc::new(|params| {
            let seconds = params["seconds"].as_f64().unwrap_or(3.0).clamp(0.05, 600.0);
            let gpu_mib = params["gpu_mib"].as_u64().unwrap_or(0) as u32;
            let title = format!("{seconds} s of crunching, with {gpu_mib} MiB of GPU");
            Ok(Job {
                spec: spec(title, Resource::Compute, gpu_mib),
                run: Box::new(move |ctx| {
                    Box::pin(async move {
                        let steps = (seconds / 0.05).ceil() as u64;
                        for step in 1..=steps {
                            ctx.checkpoint().await?;
                            tokio::time::sleep(Duration::from_millis(50)).await;
                            let fraction = step as f64 / steps as f64;
                            ctx.progress(Some(fraction), format!("step {step} of {steps}"));
                        }
                        ctx.add_result(format!("{steps} steps crunched"));
                        Ok(())
                    })
                }),
            })
        }),
    );
}

/// Submits a job: its `JobRef`'s href.
async fn submit(address: SocketAddr, kind: &str, params: Value) -> anyhow::Result<String> {
    let started = post(
        address,
        "/api/v1/jobs/submit",
        &json!({"kind": kind, "params": params}),
    )
    .await?;
    ensure!(started.status == 201, "submit {kind}: {}", started.status);
    let invocation = started.json()?["href"]
        .as_str()
        .context("the invocation")?
        .to_owned();
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let ended = get(address, &invocation).await?.json()?;
        if let Some(href) = ended["output"]["href"].as_str() {
            return Ok(href.to_owned());
        }
        ensure!(ended["status"] != "error", "submit {kind}: {ended}");
        ensure!(Instant::now() < deadline, "submit {kind} didn't end");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// Waits up to 60 s for the job at `href` to end: its JSON.
async fn ended(address: SocketAddr, href: &str) -> anyhow::Result<Value> {
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        let job = get(address, href).await?.json()?;
        if ["completed", "failed", "cancelled", "interrupted"]
            .contains(&job["state"].as_str().unwrap_or(""))
        {
            return Ok(job);
        }
        ensure!(Instant::now() < deadline, "the job didn't end: {job}");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// `--smoke`: the page; a tour, during which a manual jog is refused; two
/// crunches that the GPU budget runs one after the other; all completed,
/// and listed.
async fn smoke(address: SocketAddr) -> anyhow::Result<()> {
    let page = get(address, "/").await?;
    ensure!(
        page.status == 200 && page.content_type.starts_with("text/html"),
        "the page"
    );
    let tour = submit(
        address,
        "demo.tour",
        json!({"points": 3, "step_um": 200.0, "speed": 2000.0}),
    )
    .await?;
    let crunches = [
        submit(
            address,
            "demo.crunch",
            json!({"seconds": 0.5, "gpu_mib": 1500}),
        )
        .await?,
        submit(
            address,
            "demo.crunch",
            json!({"seconds": 0.5, "gpu_mib": 1500}),
        )
        .await?,
    ];
    // While the tour holds the hardware, a manual jog is refused.
    let deadline = Instant::now() + Duration::from_secs(30);
    while get(address, "/api/v1/jobs/hardware")
        .await?
        .json()?
        .is_null()
    {
        ensure!(
            Instant::now() < deadline,
            "the tour never held the hardware"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let jog = post(
        address,
        "/api/v1/stage/jog",
        &json!({"axis": "x", "towards_max": true}),
    )
    .await?;
    let invocation = jog.json()?["href"]
        .as_str()
        .context("the jog's invocation")?
        .to_owned();
    let refused = loop {
        let now = get(address, &invocation).await?.json()?;
        if !["pending", "running"].contains(&now["status"].as_str().unwrap_or("")) {
            break now;
        }
        ensure!(Instant::now() < deadline, "the jog's invocation didn't end");
        tokio::time::sleep(Duration::from_millis(10)).await;
    };
    ensure!(
        refused["error"].to_string().contains("busy (409)"),
        "the jog: {refused}"
    );
    let mut spans = Vec::new();
    for href in [&tour, &crunches[0], &crunches[1]] {
        let job = ended(address, href).await?;
        ensure!(job["state"] == "completed", "{job}");
        spans.push((job["started"].clone(), job["finished"].clone()));
    }
    // 1500 MiB each, of 2048: the second started once the first finished.
    ensure!(
        spans[2].0.as_str() >= spans[1].1.as_str(),
        "the crunches overlapped: {spans:?}"
    );
    let listed = get(address, "/api/v1/jobs/list?state=completed")
        .await?
        .json()?;
    ensure!(listed.as_array().map(Vec::len) == Some(3), "{listed}");
    println!("ran: a tour, a jog refused during it, and two crunches one after the other");
    Ok(())
}
