//! `ex-scan`: a snake scan of the simulated H&E slide through the API, as a
//! client would: planned and previewed, then run as a job, autofocusing
//! every tenth tile; its tiles printed as they arrive, and its acquisition
//! checked. The page draws the tiles as they arrive. See the README.

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
use oscope_store::{Scratch, ScratchPolicy, Store};
use serde_json::{Value, json};

/// The scan's side, in µm, over the first island's middle.
const SIDE: f64 = 1_000.0;

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("ex-scan: {error:#}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> anyhow::Result<()> {
    let example = Example::from_args_with("ex-scan", 8240, &["--rate"])?;
    let roots = &example.roots;
    for folder in [&roots.data, &roots.scratch] {
        std::fs::create_dir_all(folder)?;
    }
    let mut config = DevicesConfig::default();
    let sim = &mut config.simulator;
    // The slide tilted, as slides lie: 2 µm higher a mm along x, 1 lower
    // along y.
    sim.tilt = [2.0, -1.0];
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

/// Plans a snake over 1 × 1 mm of the first island, runs it, printing each
/// tile as it arrives, then the acquisition it made. With `check`, each
/// frame is held to its tile.
async fn walk(address: SocketAddr, smoke: bool, check: bool) -> anyhow::Result<()> {
    // A region of the sensor's middle: the simulator renders each new place
    // afresh. 256 × 192 of the quarter sensor for `--smoke`, 1024 × 768 of
    // the 20 MP otherwise; the camera keeps the width a multiple of 12.
    let roi = match smoke {
        true => json!({"x": 556, "y": 360, "width": 256, "height": 192}),
        false => json!({"x": 2224, "y": 1440, "width": 1024, "height": 768}),
    };
    let set = put(address, "/api/v1/camera/roi", &roi).await?;
    ensure!(set.status < 300, "the region: {}", set.body);
    invoke(address, "stage/home", json!({})).await?;
    let island = HeTissue::new(1, Slide::COVERSLIP).islands()[0];
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
    let made = invoke(address, "projects/create", json!({"name": "ex-scan"})).await?;
    let project = made["id"].as_str().context("the project's ID")?.to_owned();

    // The plan, and its preview.
    let (x, y) = (island.x - SIDE / 2.0, island.y - SIDE / 2.0);
    let region =
        json!({"rect": {"origin": {"x": x, "y": y}, "size": {"width": SIDE, "height": SIDE}}});
    let request = json!({
        "pattern": "snake",
        "region": region,
        "z": {"kind": "autofocus", "every": 10},
    });
    let preview = invoke(address, "scan/plan", json!({"request": request})).await?;
    let (plan, estimate) = (&preview["plan"], &preview["estimate"]);
    let tiles = plan["tiles"].as_array().context("the tiles")?.len();
    let cell = &plan["field"]["cell_um"];
    println!(
        "ex-scan: a snake over 1 × 1 mm of the first island: {tiles} tiles, {} × {}, of {:.1} × \
         {:.1} µm; about {:.0} s and {:.1} MB, autofocusing every tenth tile",
        plan["columns"],
        plan["rows"],
        cell["width"].as_f64().unwrap_or(0.0),
        cell["height"].as_f64().unwrap_or(0.0),
        estimate["seconds"].as_f64().unwrap_or(0.0),
        estimate["bytes"].as_f64().unwrap_or(0.0) / 1e6,
    );

    // The scan, its tiles as they arrive.
    let name = "snake over 1 × 1 mm";
    let input = json!({"request": request, "project": project, "name": name});
    let started = Instant::now();
    let job = invoke(address, "scan/run", input).await?;
    let href = job["href"].as_str().context("the job's href")?.to_owned();
    let mut shown = 0;
    let job = loop {
        let progress = get(address, "/api/v1/scan/progress").await?.json()?;
        let done = progress["done"].as_array().cloned().unwrap_or_default();
        for tile in &done[shown.min(done.len())..] {
            println!(
                "  tile {:>2} at ({:.3}, {:.3}) mm, focused at z {:.2} µm",
                tile["index"].as_u64().unwrap_or(0) + 1,
                tile["x_um"].as_f64().unwrap_or(0.0) / 1e3,
                tile["y_um"].as_f64().unwrap_or(0.0) / 1e3,
                tile["z_um"].as_f64().unwrap_or(0.0)
            );
        }
        shown = shown.max(done.len());
        let job = get(address, &href).await?.json()?;
        if matches!(
            job["state"].as_str(),
            Some("completed" | "failed" | "cancelled")
        ) {
            break job;
        }
        ensure!(
            started.elapsed() < Duration::from_secs(1_800),
            "the scan didn't end"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    };
    ensure!(job["state"] == "completed", "the scan: {job}");
    println!(
        "  scanned in {:.1} s, against {:.0} estimated",
        started.elapsed().as_secs_f64(),
        estimate["seconds"].as_f64().unwrap_or(0.0)
    );

    // Its acquisition: a frame for each tile, each where it was taken.
    let contents = get(address, &format!("/api/v1/projects/project/{project}"))
        .await?
        .json()?;
    let acquisition = &contents["acquisitions"][0];
    let frames = acquisition["frames"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let worst = frames
        .iter()
        .zip(plan["tiles"].as_array().into_iter().flatten())
        .map(|(frame, tile)| {
            let at = &frame["position"];
            let dx = at["x"].as_f64().unwrap_or(f64::NAN) - tile["x_um"].as_f64().unwrap_or(0.0);
            let dy = at["y"].as_f64().unwrap_or(f64::NAN) - tile["y_um"].as_f64().unwrap_or(0.0);
            dx.hypot(dy)
        })
        .fold(0.0, f64::max);
    println!(
        "  the acquisition {:?}: {} frames, each within {worst:.2} µm of its tile; the plan with \
         it. Open http://{address}/ to see the tiles.",
        acquisition["name"].as_str().unwrap_or(""),
        frames.len()
    );
    if check {
        ensure!(frames.len() == tiles, "{} frames of {tiles}", frames.len());
        ensure!(worst < 2.5, "a frame {worst} µm from its tile");
        ensure!(acquisition["plan"]["pattern"] == "snake", "{acquisition}");
        let page = get(address, "/").await?;
        ensure!(page.status == 200, "the page: {}", page.status);
    }
    Ok(())
}
