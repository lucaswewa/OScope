//! `ex-autofocus`: autofocus on the simulator's H&E tissue, reflective chip
//! and fluorescent cells, all through the API. Each starts defocused, is
//! focused, and its error against the simulator's truth is printed with the
//! time taken; then an empty field is refused, and the exposure and white
//! balance set. The page plots the latest run's focus curve. See the README.

use std::net::SocketAddr;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, bail, ensure};
use ex_common::{Example, get, post, put};
use oscope_api::Api;
use oscope_api::devices::{Devices, DevicesConfig};
use oscope_jobs::{JobService, JobsConfig};
use oscope_sim::{HeTissue, Slide};
use serde_json::{Value, json};

/// What each run focuses: its name, the sample, the light, how far from
/// focus it starts, in µm, and the sweep.
const RUNS: [(&str, &str, &str, f64, &str); 4] = [
    ("H&E tissue", "H&E tissue", "transmitted", 15.0, "stepped"),
    (
        "H&E tissue",
        "H&E tissue",
        "transmitted",
        15.0,
        "continuous",
    ),
    (
        "reflective chip",
        "reflective surface",
        "reflected",
        -15.0,
        "stepped",
    ),
    (
        "fluorescent cells",
        "fluorescent cells",
        "blue 488",
        15.0,
        "stepped",
    ),
];

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("ex-autofocus: {error:#}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> anyhow::Result<()> {
    let example = Example::from_args_with("ex-autofocus", 8237, &["--rate"])?;
    let roots = &example.roots;
    std::fs::create_dir_all(&roots.data)?;
    let mut config = DevicesConfig::default();
    let sim = &mut config.simulator;
    if example.smoke {
        sim.sensor = "quarter".to_owned();
    }
    let rate = example
        .number("--rate")
        .unwrap_or(if example.smoke { 10 } else { 1 });
    sim.rate = rate as f64;
    let devices = Devices::from_config(&config, &roots.data).await;
    let devices = Arc::new(devices.map_err(anyhow::Error::msg)?);
    let jobs = JobService::open(&roots.data.join("jobs.sqlite"), JobsConfig::default())?;
    let api = Api::builder(roots).devices(devices).jobs(jobs);
    let smoke = example.smoke;
    example.when_serving(move |address| async move { walk(address, smoke, false).await });
    let page = concat!(env!("CARGO_MANIFEST_DIR"), "/page");
    example
        .serve(api, page, move |address| async move {
            walk(address, true, true).await
        })
        .await
}

/// Invokes `action` with `input`, and waits for its invocation to end: its
/// output.
async fn invoke(address: SocketAddr, action: &str, input: Value) -> anyhow::Result<Value> {
    let started = post(address, &format!("/api/v1/{action}"), &input).await?;
    ensure!(
        started.status == 201,
        "{action}: {} {}",
        started.status,
        started.body
    );
    let href = started.json()?["href"]
        .as_str()
        .context("an href")?
        .to_owned();
    let deadline = Instant::now() + Duration::from_secs(600);
    loop {
        let invocation = get(address, &href).await?.json()?;
        match invocation["status"].as_str() {
            Some("completed") => return Ok(invocation["output"].clone()),
            Some("pending" | "running") => {}
            _ => bail!("{action}: {invocation}"),
        }
        ensure!(Instant::now() < deadline, "{action} didn't end");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// Runs the job `action` starts, and waits for it to end: the job.
async fn job(address: SocketAddr, action: &str, input: Value) -> anyhow::Result<Value> {
    let submitted = invoke(address, action, input).await?;
    let href = submitted["href"]
        .as_str()
        .context("a job's href")?
        .to_owned();
    let deadline = Instant::now() + Duration::from_secs(600);
    loop {
        let job = get(address, &href).await?.json()?;
        if matches!(
            job["state"].as_str(),
            Some("completed" | "failed" | "cancelled")
        ) {
            return Ok(job);
        }
        ensure!(Instant::now() < deadline, "{action} didn't end");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

async fn stage_z(address: SocketAddr) -> anyhow::Result<f64> {
    let position = get(address, "/api/v1/stage/position").await?.json()?;
    position["position"]["z"].as_f64().context("the stage's z")
}

/// Only `light` on, of the three the runs use.
async fn light(address: SocketAddr, light: &str) -> anyhow::Result<()> {
    for channel in ["transmitted", "reflected", "blue 488"] {
        let on = channel == light;
        invoke(
            address,
            "illumination/set_light",
            json!({"channel": channel, "on": on}),
        )
        .await?;
    }
    Ok(())
}

/// Focuses each run's sample from defocused, printing its error against the
/// truth; then an empty field, and the exposure and white balance. With
/// `check`, each is held to P34's tolerances.
async fn walk(address: SocketAddr, smoke: bool, check: bool) -> anyhow::Result<()> {
    // A region of the sensor's middle: the simulator renders each new depth
    // afresh, which takes it seconds for a 20 MP frame. 256 × 192 of the
    // quarter sensor for `--smoke`, 1024 × 768 of the 20 MP otherwise.
    let roi = match smoke {
        true => json!({"x": 556, "y": 360, "width": 256, "height": 192}),
        false => json!({"x": 2224, "y": 1440, "width": 1024, "height": 768}),
    };
    let set = put(address, "/api/v1/camera/roi", &roi).await?;
    ensure!(set.status < 300, "the region: {}", set.body);
    invoke(address, "stage/home", json!({})).await?;
    invoke(address, "camera/start_streaming", json!({})).await?;
    let island = HeTissue::new(1, Slide::COVERSLIP).islands()[0];
    let cover = Slide::COVERSLIP;
    let middle = (
        cover.origin.x + cover.size.width / 2.0,
        cover.origin.y + cover.size.height / 2.0,
    );
    println!(
        "\n{:<18} {:<11} {:>10} {:>10} {:>8} {:>7} {:>7} {:>8}",
        "sample", "sweep", "truth µm", "found µm", "error", "÷ d/4", "frames", "time"
    );
    for (name, sample, on, offset, sweep) in RUNS {
        invoke(address, "simulation/load", json!({"name": sample})).await?;
        light(address, on).await?;
        let (x, y) = if sample == "H&E tissue" {
            (island.x, island.y)
        } else {
            middle
        };
        let truth = invoke(address, "simulation/in_focus", json!({"x": x, "y": y})).await?;
        let truth = truth.as_f64().context("the truth")?;
        let start = json!({"x": x, "y": y, "z": truth + offset});
        invoke(address, "stage/move_to", start).await?;
        let done = job(address, "autofocus/run", json!({"sweep": sweep})).await?;
        if done["state"] != "completed" {
            let last = get(address, "/api/v1/autofocus/last").await?.json()?;
            let curve: Vec<String> = last["coarse"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|p| {
                    format!(
                        "{:.1}:{:.4}",
                        p["z_um"].as_f64().unwrap_or(0.0) - truth,
                        p["value"].as_f64().unwrap_or(0.0)
                    )
                })
                .collect();
            bail!(
                "{name}: {} (curve, z from the truth: {}; speed {})",
                done["error"],
                curve.join(" "),
                last["speed_um_s"]
            );
        }
        let last = get(address, "/api/v1/autofocus/last").await?.json()?;
        let z = stage_z(address).await?;
        let quarter = last["depth_of_field_um"].as_f64().unwrap_or(f64::NAN) / 4.0;
        println!(
            "{name:<18} {sweep:<11} {truth:>10.3} {z:>10.3} {:>+8.3} {:>7.2} {:>7} {:>6.1} s",
            z - truth,
            (z - truth) / quarter,
            last["frames"],
            last["took_ms"].as_f64().unwrap_or(f64::NAN) / 1e3
        );
        if check {
            ensure!((z - truth).abs() < quarter, "{name}, {sweep}: {last}");
        }
    }
    // An empty field: clear glass beside the coverslip.
    invoke(address, "simulation/load", json!({"name": "H&E tissue"})).await?;
    light(address, "transmitted").await?;
    let glass = json!({"x": 10_000.0, "y": 12_500.0, "z": 1_002.0});
    invoke(address, "stage/move_to", glass).await?;
    let refused = job(address, "autofocus/run", json!({})).await?;
    println!("\nclear glass: {}", refused["error"].as_str().unwrap_or(""));
    if check {
        ensure!(refused["state"] == "failed", "the empty field: {refused}");
    }
    // The exposure and white balance there, the gains given to the preview.
    let exposed = job(address, "exposure/auto", json!({})).await?;
    let exposure = get(address, "/api/v1/exposure/last").await?.json()?;
    println!(
        "auto exposure: {} ms, the 99.5th percentile at {:.3} of full scale, in {} steps",
        exposure["exposure_ms"],
        exposure["level"].as_f64().unwrap_or(f64::NAN),
        exposure["steps"].as_array().map_or(0, Vec::len)
    );
    let balanced = job(address, "exposure/white_balance", json!({})).await?;
    let white = get(address, "/api/v1/exposure/last_white_balance")
        .await?
        .json()?;
    let mut look = get(address, "/api/v1/preview/look").await?.json()?;
    look["gains"] = white["gains"].clone();
    let given = put(address, "/api/v1/preview/look", &look).await?;
    println!(
        "white balance: gains {}, given to the preview",
        white["gains"]
    );
    if check {
        ensure!(exposed["state"] == "completed", "auto exposure: {exposed}");
        ensure!(
            balanced["state"] == "completed",
            "white balance: {balanced}"
        );
        ensure!(given.status < 300, "the preview's look: {}", given.body);
    }
    Ok(())
}
