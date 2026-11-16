//! `ex-deconvolve`: P43's deconvolution. A widefield z-scan of the
//! simulated fluorescent cells at 50× is deconvolved by the job
//! `volume.deconvolve`, Richardson–Lucy by Born and Wolf's PSF. The NRMSE of
//! the stack and of its deconvolution against the cells' true membranes is
//! printed, and the page draws either with VTK.js: a maximum intensity
//! projection, composited, or an isosurface. See the README.

use std::net::SocketAddr;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, bail, ensure};
use ex_common::{Example, fetch, get, post, put};
use oscope_api::Api;
use oscope_api::devices::{Devices, DevicesConfig};
use oscope_core::{Point2, Um};
use oscope_jobs::{JobService, JobsConfig};
use oscope_sim::{CellCulture, Slide};
use oscope_store::{Scratch, ScratchPolicy, Store};
use serde_json::{Value, json};

/// The stack, in µm: from 5 below the cells' focus to 5 above it, every
/// 0.5.
const STACK: (f64, f64, f64) = (5.0, 5.0, 0.5);
/// A slice's or a brick's header, in bytes (ADR-0029).
const HEADER: usize = 64;
/// The stage z at which the slide's surface is sharp: the simulator's.
const FOCUS: f64 = 5_000.0;

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("ex-deconvolve: {error:#}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> anyhow::Result<()> {
    let example = Example::from_args_with("ex-deconvolve", 8248, &["--rate"])?;
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

/// The ID of the first record a job made.
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

/// A float brick's size, and its voxels.
async fn floats(address: SocketAddr, path: &str) -> anyhow::Result<([usize; 3], Vec<f32>)> {
    let body = fetch(address, path).await?.body;
    ensure!(
        body.starts_with(b"OSV1") && body.get(6) == Some(&4),
        "a float brick at {path}"
    );
    let word = |at: usize| u32::from_le_bytes([body[at], body[at + 1], body[at + 2], body[at + 3]]);
    let size = [12, 16, 20].map(|at| word(at) as usize);
    let values = body[HEADER..]
        .as_chunks::<4>()
        .0
        .iter()
        .map(|&b| f32::from_le_bytes(b))
        .collect();
    Ok((size, values))
}

/// The NRMSE of `a` against `b`, each standardised: √(2(1 − ρ)), as
/// `oscope_volume::nrmse` gives it.
fn nrmse(a: &[f32], b: &[f64]) -> f64 {
    let n = a.len() as f64;
    let (ma, mb) = (
        a.iter().map(|x| f64::from(*x)).sum::<f64>() / n,
        b.iter().sum::<f64>() / n,
    );
    let (mut ab, mut aa, mut bb) = (0.0, 0.0, 0.0);
    for (x, y) in a.iter().zip(b) {
        let (x, y) = (f64::from(*x) - ma, y - mb);
        (ab, aa, bb) = (ab + x * y, aa + x * x, bb + y * y);
    }
    (2.0 * (1.0 - ab / (aa * bb).sqrt().max(f64::MIN_POSITIVE)))
        .max(0.0)
        .sqrt()
}

/// The widefield z-scan through the cells, its deconvolution, and their
/// errors against the truth. For `--smoke`, over the frame's middle ninth,
/// and held to P43's spec.
async fn walk(address: SocketAddr, smoke: bool) -> anyhow::Result<()> {
    if smoke {
        let roi = json!({"x": 456, "y": 300, "width": 456, "height": 312});
        let set = put(address, "/api/v1/camera/roi", &roi).await?;
        ensure!(set.status < 300, "the region: {}", set.body);
    }
    invoke(address, "stage/home", json!({})).await?;
    for (channel, on) in [("transmitted", false), ("blue 488", true)] {
        let light = json!({"channel": channel, "on": on});
        invoke(address, "illumination/set_light", light).await?;
    }
    // Widefield: no pinhole.
    let set = put(address, "/api/v1/simulation/confocal", &json!(false)).await?;
    ensure!(set.status < 300, "the pinhole: {}", set.body);
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
    let made_project = invoke(address, "projects/create", json!({"name": "ex-deconvolve"})).await?;
    let project = made_project["id"]
        .as_str()
        .context("the project")?
        .to_owned();
    let (below, above, step) = STACK;
    let stack = json!({"below_um": below, "above_um": above, "step_um": step});
    let request = json!({"pattern": "z_scan", "at": {"x": x, "y": y}, "stack": stack});
    let scan = json!({"request": request, "project": project, "name": "cells in widefield"});
    let started = Instant::now();
    let acquisition = made(&job(address, "scan/run", scan).await?)?;
    let scan_s = started.elapsed().as_secs_f64();
    let fitted = get(address, "/api/v1/objective/fitted").await?.json()?;
    let na = fitted["na"].as_f64().context("the objective's NA")?;
    let started = Instant::now();
    let params = json!({"project": project, "acquisition": acquisition, "options": {"na": na}});
    let submitted = json!({"kind": "volume.deconvolve", "params": params});
    let result = made(&job(address, "jobs/submit", submitted).await?)?;
    let deconvolve_s = started.elapsed().as_secs_f64();

    // The stack's cells and the deconvolved voxels, against the cells'
    // membranes through each voxel's depth.
    let base = format!("/api/v1/volume/{project}");
    let raw = format!("{base}/acquisitions/{acquisition}/brick?format=f32&level=1");
    let (size, stacked) = floats(address, &raw).await?;
    let (_, sharpened) = floats(
        address,
        &format!("{base}/results/{result}/brick?format=f32&level=0"),
    )
    .await?;
    let [w, h, d] = size;
    let contents = get(address, &format!("/api/v1/projects/project/{project}"))
        .await?
        .json()?;
    let record = &contents["acquisitions"][0];
    let corner = &record["plan"]["field"]["corners_um"][0];
    let cell = 2.0
        * record["image"]["pixel_size"]["x"]
            .as_f64()
            .context("a voxel")?;
    // Where the stage was, as its first frame records it: the stage's play
    // may leave it a few µm from where it was sent.
    let at = &record["frames"][0]["position"];
    let origin = Point2::new(
        at["x"].as_f64().context("a frame's x")? + corner["x"].as_f64().context("a corner")?,
        at["y"].as_f64().context("a frame's y")? + corner["y"].as_f64().context("a corner")?,
    );
    let cells = CellCulture::new(1, cover);
    let mut truth = Vec::with_capacity(w * h * d);
    for k in 0..d {
        let z = record["frames"][k]["position"]["z"]
            .as_f64()
            .context("a plane's z")?
            - FOCUS;
        let mut plane = vec![0.0_f64; w * h];
        for dz in [-0.2, -0.1, 0.0, 0.1, 0.2] {
            let slice = cells.truth(origin, Um(cell), w as u32, h as u32, Um(z + dz));
            for (v, p) in plane.iter_mut().zip(slice.data().chunks(3)) {
                *v += f64::from(p[1]) / 5.0;
            }
        }
        truth.extend(plane);
    }
    let (before, after) = (nrmse(&stacked, &truth), nrmse(&sharpened, &truth));
    println!(
        "ex-deconvolve: a widefield z-scan of {d} planes, {step} µm apart, through the fluorescent cells at 50×, in {scan_s:.1} s"
    );
    println!(
        "  deconvolved in {deconvolve_s:.1} s: {w} × {h} × {d} voxels, Richardson–Lucy by Born and Wolf's PSF at NA {na}"
    );
    println!(
        "  against the cells' true membranes: NRMSE {before:.3} the stack, {after:.3} deconvolved"
    );
    println!("  open http://{address}/ to see them drawn by VTK.js");
    if !smoke {
        return Ok(());
    }
    ensure!(
        after < before,
        "the deconvolution's NRMSE {after:.3}, the stack's {before:.3}"
    );
    ensure!(sharpened.len() == w * h * d, "the result's voxels");
    let page = get(address, "/").await?;
    ensure!(page.status == 200, "the page: {}", page.status);
    let vtk = get(address, "/vendor/vtk.js-38.0.4/vtk.js").await?;
    ensure!(vtk.status == 200, "VTK.js: {}", vtk.status);
    Ok(())
}
