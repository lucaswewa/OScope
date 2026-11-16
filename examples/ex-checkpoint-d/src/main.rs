//! `ex-checkpoint-d`: Checkpoint D (plan stage 8), as a script over the API:
//! the simulated H&E slide histo-scanned, an overview, its tissue and the
//! tissue's tiles, while the live mosaic draws them (P37, P38); stitched as
//! a job (P39); then browsed in deep zoom (P40), on `ex-deep-zoom`'s page.
//! See the README.

use std::net::SocketAddr;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, bail, ensure};
use ex_common::{Example, fetch, get, post};
use oscope_api::Api;
use oscope_api::devices::{Devices, DevicesConfig};
use oscope_jobs::{JobService, JobsConfig};
use oscope_sim::{HeTissue, Slide};
use oscope_store::{Scratch, ScratchPolicy, Store};
use serde_json::{Value, json};

/// The overview's region, in µm, across the first island's edge, some
/// 2 mm right of its middle: smaller for `--smoke`.
const REGION: (f64, f64) = (6_000.0, 4_000.0);
const SMOKE_REGION: (f64, f64) = (3_000.0, 2_000.0);

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("ex-checkpoint-d: {error:#}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> anyhow::Result<()> {
    let example = Example::from_args_with("ex-checkpoint-d", 8245, &["--rate"])?;
    let roots = &example.roots;
    for folder in [&roots.data, &roots.scratch] {
        std::fs::create_dir_all(folder)?;
    }
    let mut config = DevicesConfig::default();
    config.simulator.sensor = "quarter".to_owned();
    let rate = example
        .number("--rate")
        .unwrap_or(if example.smoke { 10 } else { 1 });
    config.simulator.rate = rate as f64;
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
    let smoke = example.smoke;
    example.when_serving(move |address| async move { walk(address, false, false).await });
    let page = concat!(env!("CARGO_MANIFEST_DIR"), "/../ex-deep-zoom/page");
    example
        .serve(api, page, move |address| async move {
            walk(address, smoke, true).await
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

/// The camera in `mode`, its exposure set again for it.
async fn mode(address: SocketAddr, mode: &str) -> anyhow::Result<()> {
    invoke(address, "camera/stop_streaming", json!({})).await?;
    let format = get(address, "/api/v1/camera/format").await?.json()?;
    let set = json!({"mode": mode, "format": format});
    invoke(address, "camera/set_mode", set).await?;
    invoke(address, "camera/start_streaming", json!({})).await?;
    job(address, "exposure/auto", json!({})).await?;
    Ok(())
}

/// The ID of the first record a job made: the last part of its first
/// result that is a record's path, as others say what it did.
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

/// The histo-scan with the live mosaic, the stitch, and deep zoom. With
/// `check`, held to the checkpoint.
async fn walk(address: SocketAddr, smoke: bool, check: bool) -> anyhow::Result<()> {
    let began = Instant::now();
    invoke(address, "stage/home", json!({})).await?;
    let island = HeTissue::new(1, Slide::COVERSLIP).islands()[0];
    let at = json!({"x": island.x, "y": island.y});
    let z = invoke(address, "simulation/in_focus", at).await?;
    let to = json!({"x": island.x, "y": island.y, "z": z});
    invoke(address, "stage/move_to", to).await?;
    invoke(address, "camera/start_streaming", json!({})).await?;
    mode(address, "bin3").await?;
    // The overview, binned, and the tissue in it.
    let (w, h) = if smoke { SMOKE_REGION } else { REGION };
    let origin = json!({"x": island.x + 2_000.0 - w / 2.0, "y": island.y - h / 2.0});
    let rect = json!({"origin": origin, "size": {"width": w, "height": h}});
    job(address, "scan/overview", json!({"region": {"rect": rect}})).await?;
    let view = get(address, "/api/v1/scan/last_overview").await?.json()?;
    println!(
        "ex-checkpoint-d: an overview of {:.0} × {:.0} mm across the first island's edge: {:.1} mm² of tissue",
        w / 1e3,
        h / 1e3,
        view["area_mm2"].as_f64().unwrap_or(0.0)
    );
    // The live mosaic over the region, drawing the scan's tiles alone.
    mode(address, if smoke { "bin3" } else { "full" }).await?;
    let mosaic = json!({"request": {"region": rect, "roam": false}});
    invoke(address, "live_mosaic/begin", mosaic).await?;
    // The tissue's scan, its focus map measured first.
    let project = invoke(
        address,
        "projects/create",
        json!({"name": "ex-checkpoint-d"}),
    )
    .await?;
    let project = project["id"].as_str().context("the project")?.to_owned();
    let mut request = json!({"pattern": "tissue"});
    if smoke {
        request["z"] =
            json!({"kind": "measure", "columns": 2, "rows": 2, "surface": {"kind": "plane"}});
    }
    let scan = json!({"request": request, "project": project, "name": "the tissue"});
    let acquisition = made(&job(address, "scan/run", scan).await?)?;
    let contents = get(address, &format!("/api/v1/projects/project/{project}"))
        .await?
        .json()?;
    let tiles = contents["acquisitions"][0]["frames"]
        .as_array()
        .map_or(0, Vec::len) as u64;
    // The last tile may still be drawing as the scan ends: wait for it.
    let deadline = Instant::now() + Duration::from_secs(60);
    let live = loop {
        let live = get(address, "/api/v1/live_mosaic/state").await?.json()?;
        if live["counts"]["scans"] == json!(tiles) || Instant::now() > deadline {
            break live;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    };
    println!(
        "  the tissue scanned: {tiles} tiles, {} drawn on the live mosaic as they were taken",
        live["counts"]["scans"]
    );
    // Stitched, then deep zoom made its colour overview and coarse levels.
    let params = json!({"project": project, "acquisition": acquisition});
    let stitch = json!({"kind": "stitch.run", "params": params});
    let result = made(&job(address, "jobs/submit", stitch).await?)?;
    let report = get(
        address,
        &format!("/api/v1/data/{project}/results/{result}/stitch.json"),
    )
    .await?
    .json()?;
    let base = format!("/api/v1/zoom/{project}/results/{result}");
    let deadline = Instant::now() + Duration::from_secs(300);
    let status = loop {
        let status = get(address, &format!("{base}/status")).await?.json()?;
        if status["overview"] == "ready" && status["coarse"] == true {
            break status;
        }
        ensure!(Instant::now() < deadline, "deep zoom didn't warm: {status}");
        tokio::time::sleep(Duration::from_millis(100)).await;
    };
    println!(
        "  stitched: {} pairs kept of {}, missing by {:.2} px rms; a mosaic of {} × {} pixels in {} deep zoom levels",
        report["kept"],
        report["pairs"],
        report["rms_px"].as_f64().unwrap_or(f64::NAN),
        status["size"][0],
        status["size"][1],
        status["levels"]
    );
    println!(
        "  in {:.0} s; open http://{address}/ to browse it",
        began.elapsed().as_secs_f64()
    );
    if !check {
        return Ok(());
    }
    ensure!(tiles > 0, "the scan took no tiles");
    let counts = &live["counts"];
    ensure!(
        counts["scans"] == json!(tiles) && counts["dropped"] == json!(0),
        "the live mosaic drew {} of {tiles} tiles: {live}",
        counts["scans"]
    );
    ensure!(
        report["kept"].as_u64() > Some(0),
        "the stitch kept no pairs: {report}"
    );
    let dzi = get(address, &format!("{base}/image.dzi")).await?;
    ensure!(dzi.status == 200, "the descriptor: {}", dzi.status);
    let coarse = fetch(address, &format!("{base}/image_files/8/0_0.jpg")).await?;
    let timing = coarse.header("server-timing").unwrap_or("");
    ensure!(
        coarse.status == 200 && timing.contains("desc=\"hit\""),
        "a coarse tile, made as the stitch ended: {} {timing}",
        coarse.status
    );
    let levels = status["levels"].as_u64().context("its levels")?;
    let full = format!("{base}/image_files/{}/0_0.jpg", levels - 1);
    let tile = fetch(address, &full).await?;
    ensure!(
        tile.status == 200 && tile.header("content-type") == Some("image/jpeg"),
        "a full-size tile: {}",
        tile.status
    );
    let page = get(address, "/").await?;
    ensure!(page.status == 200, "the page: {}", page.status);
    Ok(())
}
