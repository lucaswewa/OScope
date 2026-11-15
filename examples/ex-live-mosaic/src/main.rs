//! `ex-live-mosaic`: a live mosaic of the simulated H&E slide through the
//! API, as a client would (P38): drawn as the stage roams from place to
//! place, then as a scan beside them takes its tiles. The page shows the
//! live view beside the mosaic, which grows as you move the stage with the
//! arrow keys, or scan. See the README.

use std::net::SocketAddr;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, bail, ensure};
use ex_common::{Example, get, post};
use oscope_api::Api;
use oscope_api::devices::{Devices, DevicesConfig};
use oscope_jobs::{JobService, JobsConfig};
use oscope_sim::{HeTissue, Slide};
use oscope_store::{Scratch, ScratchPolicy, Store};
use serde_json::{Value, json};

/// The mosaic's region about the first island, in µm.
const REGION: (f64, f64) = (6_000.0, 4_000.0);

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("ex-live-mosaic: {error:#}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> anyhow::Result<()> {
    let example = Example::from_args_with("ex-live-mosaic", 8242, &["--rate"])?;
    let roots = &example.roots;
    for folder in [&roots.data, &roots.scratch] {
        std::fs::create_dir_all(folder)?;
    }
    let mut config = DevicesConfig::default();
    let sim = &mut config.simulator;
    // The quarter sensor: the simulator renders each new place of a 20 MP
    // frame for seconds.
    sim.sensor = "quarter".to_owned();
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
    example.when_serving(move |address| async move { walk(address, false).await });
    let page = concat!(env!("CARGO_MANIFEST_DIR"), "/page");
    example
        .serve(api, page, move |address| async move {
            walk(address, true).await
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

/// Runs the job `action` starts, and waits for it to complete.
async fn job(address: SocketAddr, action: &str, input: Value) -> anyhow::Result<Value> {
    let submitted = invoke(address, action, input).await?;
    let href = submitted["href"].as_str().context("a job's href")?;
    let deadline = Instant::now() + Duration::from_secs(3_600);
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

/// The live mosaic's state, once it has drawn `frames` frames, or of a
/// scan's if `scans`.
async fn drawn(address: SocketAddr, frames: u64, scans: bool) -> anyhow::Result<Value> {
    let count = if scans { "scans" } else { "drawn" };
    let deadline = Instant::now() + Duration::from_secs(600);
    loop {
        let state = get(address, "/api/v1/live_mosaic/state").await?.json()?;
        if state["counts"][count].as_u64() >= Some(frames) {
            return Ok(state);
        }
        ensure!(
            Instant::now() < deadline,
            "{frames} frames weren't drawn: {state}"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// The mosaic begun about the first island, drawn as the stage stops at
/// four places, then by a scan beside them. With `check`, held to P38's
/// spec.
async fn walk(address: SocketAddr, check: bool) -> anyhow::Result<()> {
    invoke(address, "stage/home", json!({})).await?;
    let island = HeTissue::new(1, Slide::COVERSLIP).islands()[0];
    let at = json!({"x": island.x, "y": island.y});
    let z = invoke(address, "simulation/in_focus", at).await?;
    let over = json!({"x": island.x, "y": island.y, "z": z});
    invoke(address, "stage/move_to", over).await?;
    invoke(address, "camera/start_streaming", json!({})).await?;
    job(address, "exposure/auto", json!({})).await?;
    let (w, h) = REGION;
    let corner = (island.x - w / 2.0, island.y - h / 2.0);
    let region =
        json!({"origin": {"x": corner.0, "y": corner.1}, "size": {"width": w, "height": h}});
    let begun = invoke(
        address,
        "live_mosaic/begin",
        json!({"request": {"region": region}}),
    )
    .await?;
    let canvas = &begun["canvas"];
    let pixel = canvas["pixel_um"].as_f64().context("the canvas's pixel")?;
    println!(
        "ex-live-mosaic: a mosaic of {} × {} pixels of {pixel:.2} µm over 6 × 4 mm, in {} levels",
        canvas["width"], canvas["height"], begun["levels"]
    );
    // Roaming: a frame drawn once the stage stops at each place.
    let mut state = drawn(address, 1, false).await?;
    for (dx, dy) in [(1000.0, 0.0), (0.0, 800.0), (-1000.0, 0.0), (-1000.0, 0.0)] {
        let before = state["counts"]["drawn"].as_u64().unwrap_or(0);
        invoke(
            address,
            "stage/move_by",
            json!({"dx": dx, "dy": dy, "dz": 0.0}),
        )
        .await?;
        state = drawn(address, before + 1, false).await?;
    }
    let counts = &state["counts"];
    println!(
        "  roamed to 4 places: {} frames drawn, {} blurred on the way, {} showing a place drawn",
        counts["drawn"], counts["blurred"], counts["same"]
    );
    // A scan beside them: each tile drawn as it is taken.
    let made = invoke(
        address,
        "projects/create",
        json!({"name": "ex-live-mosaic"}),
    )
    .await?;
    let project = made["id"].as_str().context("the project's ID")?.to_owned();
    let beside = json!({"rect": {"origin": {"x": island.x + 1500.0, "y": island.y - 1000.0}, "size": {"width": 1400.0, "height": 2000.0}}});
    let scan = json!({"pattern": "snake", "region": beside});
    let planned = invoke(address, "scan/plan", json!({"request": scan})).await?;
    let tiles = planned["plan"]["tiles"].as_array().map_or(0, Vec::len) as u64;
    let input = json!({"request": scan, "project": project, "name": "beside"});
    job(address, "scan/run", input).await?;
    let state = drawn(address, tiles, true).await?;
    let counts = &state["counts"];
    println!(
        "  scanned {tiles} tiles beside them, each drawn as it was taken: {} frames drawn in all, in {} tiles at every level",
        counts["drawn"], state["tiles"]
    );
    println!(
        "  open http://{address}/ to see the live view beside the mosaic; the arrow keys move the stage"
    );
    if check {
        ensure!(
            counts["dropped"] == 0 && counts["failed"] == 0,
            "frames dropped or failed: {counts}"
        );
        // The tile under the island, a JPEG; the page and its viewer.
        let (col, row) = (
            (w / 2.0 / pixel) as u32 / 256,
            (h / 2.0 / pixel) as u32 / 256,
        );
        let tile = get(address, &format!("/api/v1/mosaic/tiles/0/{col}/{row}")).await?;
        ensure!(
            tile.status == 200 && tile.content_type == "image/jpeg",
            "the tile under the island: {} {}",
            tile.status,
            tile.content_type
        );
        for path in ["/", "/vendor/openseadragon-6.1.1/openseadragon.min.js"] {
            let page = get(address, path).await?;
            ensure!(page.status == 200, "{path}: {}", page.status);
        }
    }
    Ok(())
}
