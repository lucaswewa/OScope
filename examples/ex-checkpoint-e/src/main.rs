//! `ex-checkpoint-e`: Checkpoint E, from one server. The simulated
//! reflective chip's dome is z-scanned at 50× and its heights found by depth
//! from focus (P42); then the fluorescent cells are loaded, z-scanned in
//! widefield and deconvolved (P43). The page shows both in VTK.js, side by
//! side: the heights as a surface, the cells as a volume. See the README.

use std::net::SocketAddr;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, bail, ensure};
use ex_common::{Example, get, post, put};
use oscope_api::Api;
use oscope_api::devices::{Devices, DevicesConfig};
use oscope_jobs::{JobService, JobsConfig};
use oscope_sim::Slide;
use oscope_store::{Scratch, ScratchPolicy, Store};
use serde_json::{Value, json};

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("ex-checkpoint-e: {error:#}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> anyhow::Result<()> {
    let example = Example::from_args_with("ex-checkpoint-e", 8249, &["--rate"])?;
    let roots = &example.roots;
    for folder in [&roots.data, &roots.scratch] {
        std::fs::create_dir_all(folder)?;
    }
    let mut config = DevicesConfig::default();
    let sim = &mut config.simulator;
    sim.sensor = "quarter".to_owned();
    sim.objective = "50x".to_owned();
    sim.sample = "reflective surface".to_owned();
    let rate = example
        .number("--rate")
        .unwrap_or(if example.smoke { 10 } else { 1 });
    sim.rate = rate as f64;
    let devices = Devices::from_config(&config, &roots.data).await;
    let devices = Arc::new(devices.map_err(anyhow::Error::msg)?);
    let jobs = JobService::open(&roots.data.join("jobs.sqlite"), JobsConfig::default())?;
    let store = Store::open(&roots.data, &roots.data.join("index.sqlite"))?;
    let scratch = Scratch::open(&roots.scratch, ScratchPolicy::default())?;
    // The two examples' pages, beside this one's.
    let examples = concat!(env!("CARGO_MANIFEST_DIR"), "/..");
    let api = Api::builder(roots)
        .devices(devices)
        .projects(store)
        .scratch(scratch)
        .jobs(jobs)
        .pages_under("depth", format!("{examples}/ex-depth-from-focus/page"))
        .pages_under("deconvolve", format!("{examples}/ex-deconvolve/page"));
    let smoke = example.smoke;
    example.when_serving(move |address| async move { walk(address, smoke, false).await });
    let page = concat!(env!("CARGO_MANIFEST_DIR"), "/page");
    example
        .serve(api, page, |address| async move {
            walk(address, true, true).await
        })
        .await
}

/// Invokes `action` with `input`, and waits for its output.
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

/// Runs the job `action` starts, and waits for it to complete.
async fn job(address: SocketAddr, action: &str, input: Value) -> anyhow::Result<Value> {
    let submitted = invoke(address, action, input).await?;
    let href = submitted["href"].as_str().context("a job's href")?;
    let deadline = Instant::now() + Duration::from_secs(1_800);
    loop {
        let job = get(address, href).await?.json()?;
        match job["state"].as_str() {
            Some("completed") => return Ok(job),
            Some("failed" | "cancelled") => bail!("{action}: {job}"),
            _ => {}
        }
        ensure!(Instant::now() < deadline, "{action} didn't end");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// The IDs of the records a job made, in order.
fn made(job: &Value) -> Vec<String> {
    job["results"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .filter(|r| r.starts_with("projects/"))
        .filter_map(|r| r.rsplit('/').next())
        .map(str::to_owned)
        .collect()
}

/// Only `light` on, of the two the walk uses.
async fn light(address: SocketAddr, light: &str) -> anyhow::Result<()> {
    for channel in ["transmitted", "reflected", "blue 488"] {
        let on = json!({"channel": channel, "on": channel == light});
        invoke(address, "illumination/set_light", on).await?;
    }
    Ok(())
}

/// A z-scan of 21 planes 0.5 µm apart about the focus at (`x`, `y`), into
/// `project` as `name`: the acquisition.
async fn stack(
    address: SocketAddr,
    project: &str,
    (x, y): (f64, f64),
    name: &str,
) -> anyhow::Result<String> {
    let z = invoke(address, "simulation/in_focus", json!({"x": x, "y": y})).await?;
    let z = z.as_f64().context("the focus")?;
    invoke(address, "stage/move_to", json!({"x": x, "y": y, "z": z})).await?;
    job(address, "exposure/auto", json!({})).await?;
    let stack = json!({"below_um": 5.0, "above_um": 5.0, "step_um": 0.5});
    let request = json!({"pattern": "z_scan", "at": {"x": x, "y": y}, "stack": stack});
    let scan = json!({"request": request, "project": project, "name": name});
    let scanned = job(address, "scan/run", scan).await?;
    made(&scanned)
        .into_iter()
        .next()
        .context("the scan's acquisition")
}

/// Checkpoint E: the chip's heights, then the cells deconvolved. For
/// `--smoke`, over the frame's middle ninth; with `check`, held to it.
async fn walk(address: SocketAddr, smoke: bool, check: bool) -> anyhow::Result<()> {
    if smoke {
        let roi = json!({"x": 456, "y": 300, "width": 456, "height": 312});
        let set = put(address, "/api/v1/camera/roi", &roi).await?;
        ensure!(set.status < 300, "the region: {}", set.body);
    }
    let started = Instant::now();
    invoke(address, "stage/home", json!({})).await?;
    invoke(address, "camera/start_streaming", json!({})).await?;
    let made_project = invoke(address, "projects/create", json!({"name": "Checkpoint E"})).await?;
    let project = made_project["id"]
        .as_str()
        .context("the project")?
        .to_owned();
    let cover = Slide::COVERSLIP;
    let middle = (
        cover.origin.x + cover.size.width / 2.0,
        cover.origin.y + cover.size.height / 2.0,
    );

    // The chip: its dome's heights and all-in-focus image, and its mesh.
    light(address, "reflected").await?;
    let dome = (middle.0 + 50.0, middle.1 - 50.0);
    let chip = stack(address, &project, dome, "a dome").await?;
    let params = json!({"project": project, "acquisition": chip, "options": {"mesh": "ply"}});
    let depth = job(
        address,
        "jobs/submit",
        json!({"kind": "volume.depth_from_focus", "params": params}),
    )
    .await?;
    let heights = made(&depth);

    // The cells, in widefield: deconvolved.
    invoke(
        address,
        "simulation/load",
        json!({"name": "fluorescent cells"}),
    )
    .await?;
    light(address, "blue 488").await?;
    let set = put(address, "/api/v1/simulation/confocal", &json!(false)).await?;
    ensure!(set.status < 300, "the pinhole: {}", set.body);
    let cells = stack(address, &project, middle, "cells in widefield").await?;
    let na = get(address, "/api/v1/objective/fitted").await?.json()?["na"]
        .as_f64()
        .context("the objective's NA")?;
    let params = json!({"project": project, "acquisition": cells, "options": {"na": na}});
    let deconvolved = job(
        address,
        "jobs/submit",
        json!({"kind": "volume.deconvolve", "params": params}),
    )
    .await?;
    let volume = made(&deconvolved);
    println!(
        "ex-checkpoint-e: the chip's dome in heights and the cells deconvolved, in {:.1} s",
        started.elapsed().as_secs_f64()
    );
    println!("  open http://{address}/ to see both drawn by VTK.js");
    if !check {
        return Ok(());
    }
    ensure!(
        heights.len() == 3 && volume.len() == 1,
        "the results: {heights:?}, {volume:?}"
    );
    for (what, id) in [("heights", &heights[0]), ("deconvolved", &volume[0])] {
        let meta = get(
            address,
            &format!("/api/v1/volume/{project}/results/{id}/meta"),
        )
        .await?;
        ensure!(meta.status == 200, "the {what}' metadata: {}", meta.status);
    }
    for path in [
        "/",
        "/depth/index.html",
        "/depth/app.js",
        "/deconvolve/index.html",
        "/deconvolve/app.js",
        "/vendor/vtk.js-38.0.4/vtk.js",
    ] {
        let page = get(address, path).await?;
        ensure!(page.status == 200, "{path}: {}", page.status);
    }
    Ok(())
}
