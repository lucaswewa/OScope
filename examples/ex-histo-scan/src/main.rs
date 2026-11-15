//! `ex-histo-scan`: a histological scan of the simulated H&E slide through
//! the API, as a client would (P37): an overview, binned, the tissue found
//! in it, against the simulator's truth; then the tissue scanned at full
//! resolution, its focus map measured first. The page shows the overview,
//! the tissue and the tiles as they arrive. See the README.

use std::net::SocketAddr;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, bail, ensure};
use ex_common::{Example, get, post};
use oscope_api::Api;
use oscope_api::devices::{Devices, DevicesConfig};
use oscope_core::Point2;
use oscope_jobs::{JobService, JobsConfig};
use oscope_sim::{HeTissue, Slide};
use oscope_store::{Scratch, ScratchPolicy, Store};
use serde_json::{Value, json};

/// The overview's region, in µm: 8 × 6 mm across the first island's edge,
/// some 6 mm right of its middle, tissue and glass.
const REGION: (f64, f64) = (8_000.0, 6_000.0);

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("ex-histo-scan: {error:#}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> anyhow::Result<()> {
    let example = Example::from_args_with("ex-histo-scan", 8241, &["--rate"])?;
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

/// Runs the job `action` starts, and waits for it to end, which it must by
/// completing: the job.
async fn job(address: SocketAddr, action: &str, input: Value) -> anyhow::Result<Value> {
    let submitted = invoke(address, action, input).await?;
    let href = submitted["href"]
        .as_str()
        .context("a job's href")?
        .to_owned();
    let deadline = Instant::now() + Duration::from_secs(3_600);
    loop {
        let job = get(address, &href).await?.json()?;
        match job["state"].as_str() {
            Some("completed") => return Ok(job),
            Some("failed" | "cancelled") => bail!("{action}: {job}"),
            _ => {}
        }
        ensure!(Instant::now() < deadline, "{action} didn't end");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// Stops the camera, sets its mode, starts it again, and sets the exposure
/// for it: binned 3 × 3, a pixel gathers nine times the light.
async fn mode(address: SocketAddr, mode: &str) -> anyhow::Result<()> {
    invoke(address, "camera/stop_streaming", json!({})).await?;
    let format = get(address, "/api/v1/camera/format").await?.json()?;
    invoke(
        address,
        "camera/set_mode",
        json!({"mode": mode, "format": format}),
    )
    .await?;
    invoke(address, "camera/start_streaming", json!({})).await?;
    job(address, "exposure/auto", json!({})).await?;
    Ok(())
}

/// The overview, the tissue found against the truth, and the tissue's scan.
/// With `check`, held to P37's spec.
async fn walk(address: SocketAddr, smoke: bool, check: bool) -> anyhow::Result<()> {
    invoke(address, "stage/home", json!({})).await?;
    let tissue = HeTissue::new(1, Slide::COVERSLIP);
    let island = tissue.islands()[0];
    let truth = invoke(
        address,
        "simulation/in_focus",
        json!({"x": island.x, "y": island.y}),
    )
    .await?;
    let truth = truth.as_f64().context("the truth")?;
    let start = json!({"x": island.x, "y": island.y, "z": truth});
    invoke(address, "stage/move_to", start).await?;
    invoke(address, "camera/start_streaming", json!({})).await?;
    mode(address, "bin3").await?;

    // The overview, binned, a field apart.
    let (w, h) = REGION;
    let origin = Point2::new(island.x + 2_000.0, island.y - h / 2.0);
    let region = json!({"rect": {"origin": {"x": origin.x, "y": origin.y}, "size": {"width": w, "height": h}}});
    let started = Instant::now();
    job(address, "scan/overview", json!({"region": region})).await?;
    let view = get(address, "/api/v1/scan/last_overview").await?.json()?;
    println!(
        "ex-histo-scan: an overview of {:.0} × {:.0} mm across the first island's edge: {} frames binned \
         3 × 3, {} × {} pixels of 50 µm, in {:.1} s",
        w / 1e3,
        h / 1e3,
        view["frames"],
        view["width"],
        view["height"],
        started.elapsed().as_secs_f64()
    );
    let some = |n: &Value, what: &str| match n.as_u64().unwrap_or(0) {
        1 => format!("1 {what}"),
        n => format!("{n} {what}s"),
    };
    println!(
        "  tissue at saturation over {:.3}: {:.1} mm² in {}, {} dropped as dust",
        view["threshold"].as_f64().unwrap_or(0.0),
        view["area_mm2"].as_f64().unwrap_or(0.0),
        some(&view["kept"], "piece"),
        view["dropped"]
    );

    // Its tiles, against a raster's and the truth.
    let preview = invoke(
        address,
        "scan/plan",
        json!({"request": {"pattern": "tissue"}}),
    )
    .await?;
    let raster = json!({"request": {"pattern": "raster", "region": region}});
    let raster = invoke(address, "scan/plan", raster).await?;
    let tiles = preview["plan"]["tiles"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let all = raster["plan"]["tiles"].as_array().map_or(0, Vec::len);
    let corners = preview["plan"]["field"]["corners_um"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let (cx, cy) = (
        corners
            .iter()
            .filter_map(|c| c["x"].as_f64())
            .fold(0.0, f64::max),
        corners
            .iter()
            .filter_map(|c| c["y"].as_f64())
            .fold(0.0, f64::max),
    );
    let (mut points, mut covered) = (0_u32, 0_u32);
    for j in 0..(h / 50.0) as u32 {
        for i in 0..(w / 50.0) as u32 {
            let at = Point2::new(
                origin.x + 25.0 + 50.0 * f64::from(i),
                origin.y + 25.0 + 50.0 * f64::from(j),
            );
            if tissue.is_tissue(at) {
                points += 1;
                let inside = |t: &Value| {
                    let (x, y) = (
                        t["x_um"].as_f64().unwrap_or(f64::NAN),
                        t["y_um"].as_f64().unwrap_or(f64::NAN),
                    );
                    (at.x - x).abs() <= cx && (at.y - y).abs() <= cy
                };
                covered += u32::from(tiles.iter().any(inside));
            }
        }
    }
    let share = f64::from(covered) / f64::from(points.max(1));
    println!(
        "  {} tiles over the tissue, where a raster over the region takes {all}; the simulator's \
         truth: {covered} of {points} points of tissue, every 50 µm, in a tile ({:.2} %)",
        tiles.len(),
        100.0 * share
    );

    // The tissue's scan: full resolution (still binned 3 × 3 for `--smoke`,
    // quick to render), a focus map measured over it first.
    mode(address, if smoke { "bin3" } else { "full" }).await?;
    let made = invoke(address, "projects/create", json!({"name": "ex-histo-scan"})).await?;
    let project = made["id"].as_str().context("the project's ID")?.to_owned();
    let input = json!({"request": {"pattern": "tissue"}, "project": project, "name": "the tissue"});
    let started = Instant::now();
    let done = job(address, "scan/run", input).await?;
    let results: Vec<&str> = done["results"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect();
    let contents = get(address, &format!("/api/v1/projects/project/{project}"))
        .await?
        .json()?;
    let frames = contents["acquisitions"][0]["frames"]
        .as_array()
        .map_or(0, Vec::len);
    println!(
        "  scanned in {:.1} s: {}; {frames} frames into the project",
        started.elapsed().as_secs_f64(),
        results.first().copied().unwrap_or("")
    );
    println!("  open http://{address}/ to see the overview, the tissue and its tiles");
    if check {
        ensure!(
            share >= 0.99,
            "{covered} of {points} points of tissue covered"
        );
        ensure!(
            tiles.len() < all,
            "{} tiles of a raster's {all}",
            tiles.len()
        );
        ensure!(
            frames == tiles.len(),
            "{frames} frames of {} tiles",
            tiles.len()
        );
        let page = get(address, "/").await?;
        ensure!(page.status == 200, "the page: {}", page.status);
    }
    Ok(())
}
