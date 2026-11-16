//! `ex-volume`: P41's volumes. A z-scan through the simulated fluorescent
//! cells at 50×, seen through a confocal's pinhole, is stored as a volume
//! of its voxels and served as slices and bricks; the page draws a brick
//! with VTK.js, as a maximum intensity projection or composited. See the
//! README.

use std::net::SocketAddr;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, bail, ensure};
use ex_common::{Example, fetch, get, post, put};
use oscope_api::Api;
use oscope_api::devices::{Devices, DevicesConfig};
use oscope_jobs::{JobService, JobsConfig};
use oscope_sim::Slide;
use oscope_store::{Scratch, ScratchPolicy, Store};
use serde_json::{Value, json};

/// The stack, in µm: from 4 below the cells' focus to 6 above it, every
/// 0.5, from under the glass to over the cells.
const STACK: (f64, f64, f64) = (4.0, 6.0, 0.5);
/// A slice's or a brick's header, in bytes (ADR-0029).
const HEADER: usize = 64;
/// The voxels of the brick the page asks for, at most.
const BRICK: usize = 8_000_000;

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("ex-volume: {error:#}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> anyhow::Result<()> {
    let example = Example::from_args_with("ex-volume", 8246, &["--rate"])?;
    let roots = &example.roots;
    for folder in [&roots.data, &roots.scratch] {
        std::fs::create_dir_all(folder)?;
    }
    let mut config = DevicesConfig::default();
    let sim = &mut config.simulator;
    sim.sensor = "quarter".to_owned();
    sim.objective = "50x".to_owned();
    sim.sample = "fluorescent cells".to_owned();
    let rate = example
        .number("--rate")
        .unwrap_or(if example.smoke { 10 } else { 1 });
    sim.rate = rate as f64;
    let devices = Devices::from_config(&config, &roots.data).await;
    let devices = Arc::new(devices.map_err(anyhow::Error::msg)?);
    let jobs = JobService::open(&roots.data.join("jobs.sqlite"), JobsConfig::default())?;
    let store = Store::open(&roots.data, &roots.data.join("index.sqlite"))?;
    let scratch = Scratch::open(&roots.scratch, ScratchPolicy::default())?;
    let api = Api::builder(roots)
        .devices(devices)
        .projects(store)
        .scratch(scratch)
        .jobs(jobs);
    example.when_serving(|address| async move { walk(address, false).await });
    let page = concat!(env!("CARGO_MANIFEST_DIR"), "/page");
    example
        .serve(
            api,
            page,
            |address| async move { walk(address, true).await },
        )
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
    let deadline = Instant::now() + Duration::from_secs(600);
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

/// The ID of the first record a job made: the last part of its first
/// result that is a record's path.
fn made(job: &Value) -> anyhow::Result<String> {
    job["results"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .find(|r| r.starts_with("projects/"))
        .and_then(|r| r.rsplit('/').next())
        .map(str::to_owned)
        .context("a record the job made")
}

/// A header's `u32` at `at`.
fn word(body: &[u8], at: usize) -> u32 {
    let mut bytes = [0; 4];
    bytes.copy_from_slice(&body[at..at + 4]);
    u32::from_le_bytes(bytes)
}

/// The z-scan through the cells, and its volume. With `check`, held to
/// P41's spec.
async fn walk(address: SocketAddr, check: bool) -> anyhow::Result<()> {
    invoke(address, "stage/home", json!({})).await?;
    // The blue line alone, which lights the membranes' dye, seen through
    // the pinhole.
    for (channel, on) in [("transmitted", false), ("blue 488", true)] {
        let light = json!({"channel": channel, "on": on});
        invoke(address, "illumination/set_light", light).await?;
    }
    let confocal = put(address, "/api/v1/simulation/confocal", &json!(true)).await?;
    ensure!(confocal.status < 300, "the pinhole: {}", confocal.body);
    let cover = Slide::COVERSLIP;
    let (x, y) = (
        cover.origin.x + cover.size.width / 2.0,
        cover.origin.y + cover.size.height / 2.0,
    );
    let z = invoke(address, "simulation/in_focus", json!({"x": x, "y": y})).await?;
    let z = z.as_f64().context("the cells' focus")?;
    invoke(address, "stage/move_to", json!({"x": x, "y": y, "z": z})).await?;
    invoke(address, "camera/start_streaming", json!({})).await?;
    job(address, "exposure/auto", json!({})).await?;
    let made_project = invoke(address, "projects/create", json!({"name": "ex-volume"})).await?;
    let project = made_project["id"]
        .as_str()
        .context("the project")?
        .to_owned();
    let (below, above, step) = STACK;
    let stack = json!({"below_um": below, "above_um": above, "step_um": step});
    let request = json!({"pattern": "z_scan", "at": {"x": x, "y": y}, "stack": stack});
    let scan = json!({"request": request, "project": project, "name": "cells in z"});
    let started = Instant::now();
    let acquisition = made(&job(address, "scan/run", scan).await?)?;
    let took = started.elapsed().as_secs_f64();
    let base = format!("/api/v1/volume/{project}/acquisitions/{acquisition}");
    let meta = get(address, &format!("{base}/meta")).await?.json()?;
    let (size, voxel) = (&meta["size"], &meta["voxel_um"]);
    let um = |k: usize| voxel[k].as_f64().unwrap_or(0.0);
    let first = &meta["levels"][0];
    println!(
        "ex-volume: a z-scan of {} planes, {step} µm apart, through the fluorescent cells at 50× and a confocal, in {took:.1} s",
        size[2]
    );
    println!(
        "  a raw volume of {} × {} × {} voxels of {:.3} × {:.3} × {} µm, served from level {}: {}",
        size[0],
        size[1],
        size[2],
        um(0),
        um(1),
        um(2),
        first["level"],
        first["size"]
    );
    println!("  open http://{address}/ to see it drawn by VTK.js");
    if !check {
        return Ok(());
    }
    ensure!(
        meta["volume"] == true && meta["raw"] == true,
        "a z-scan's frames, a raw volume: {meta}"
    );
    let pixel = get(address, "/api/v1/objective/pixel_at_sample").await?;
    let pixel = pixel.json()?.as_f64().context("the pixel at the sample")?;
    ensure!(
        size[2] == json!(21) && um(2) == step && (um(0) / pixel - 1.0).abs() < 1e-6,
        "21 planes of 0.5 µm, pixels of {pixel} µm: {meta}"
    );
    // The page's brick: the finest level that holds it, a raw volume's
    // first.
    let brick = fetch(address, &format!("{base}/brick?max={BRICK}")).await?;
    ensure!(
        brick.status == 200 && brick.body.starts_with(b"OSV1"),
        "a brick: {}",
        brick.status
    );
    let body = &brick.body;
    let [w, h, d] = [12, 16, 20].map(|at| word(body, at) as usize);
    ensure!(
        word(body, 8) == 1 && json!([w, h, d]) == first["size"] && body.len() == HEADER + w * h * d,
        "a brick of level {} and {w} × {h} × {d} voxels, of {} bytes",
        word(body, 8),
        body.len()
    );
    // The pinhole's section: the planes through the cells are brighter than
    // those under the glass and over the cells.
    let means: Vec<f64> = body[HEADER..]
        .chunks(w * h)
        .map(|plane| plane.iter().map(|&v| f64::from(v)).sum::<f64>() / (w * h) as f64)
        .collect();
    let focus = (below / step) as usize;
    ensure!(
        means[focus] > means[0] && means[focus] > means[d - 1],
        "the planes' means, from the lowest: {means:.1?}"
    );
    // A slice across x, through the middle: the brick's own voxels.
    let column = w / 2;
    let slice = fetch(address, &format!("{base}/slice?axis=x&index={column}")).await?;
    let own: Vec<u8> = (0..d * h)
        .map(|zy| body[HEADER + zy * w + column])
        .collect();
    ensure!(
        slice.status == 200 && slice.body.get(HEADER..) == Some(&own[..]),
        "a slice across x: {}, {} bytes",
        slice.status,
        slice.body.len()
    );
    let page = get(address, "/").await?;
    ensure!(page.status == 200, "the page: {}", page.status);
    let vtk = get(address, "/vendor/vtk.js-38.0.4/vtk.js").await?;
    ensure!(vtk.status == 200, "VTK.js: {}", vtk.status);
    Ok(())
}
