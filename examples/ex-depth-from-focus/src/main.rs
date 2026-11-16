//! `ex-depth-from-focus`: P42's depth from focus. A z-scan of the simulated
//! reflective chip at 50×, over a die's dome, in reflected light, becomes
//! heights, their confidence and an all-in-focus image, through the job
//! `volume.depth_from_focus`. The heights' error against the simulator's
//! truth is printed, and the page draws them with VTK.js, as a surface
//! textured with the all-in-focus image. See the README.

use std::net::SocketAddr;
use std::process::ExitCode;
use std::sync::{Arc, PoisonError, RwLock};
use std::time::{Duration, Instant};

use anyhow::{Context, bail, ensure};
use ex_common::{Example, fetch, get, post, put};
use oscope_api::Api;
use oscope_api::devices::{Devices, DevicesConfig};
use oscope_core::Point2;
use oscope_jobs::{JobService, JobsConfig};
use oscope_sim::{Slide, World};
use oscope_store::{Scratch, ScratchPolicy, Store};
use serde_json::{Value, json};

/// The stack, in µm: from 5 below the dome's top to 5 above it, every 0.5.
const STACK: (f64, f64, f64) = (5.0, 5.0, 0.5);
/// A slice's or a brick's header, in bytes (ADR-0029).
const HEADER: usize = 64;

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("ex-depth-from-focus: {error:#}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> anyhow::Result<()> {
    let example = Example::from_args_with("ex-depth-from-focus", 8247, &["--rate"])?;
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
    // The truth, from the simulator's own world.
    let world = devices
        .simulator
        .as_ref()
        .context("the simulator")?
        .camera
        .world();
    let jobs = JobService::open(&roots.data.join("jobs.sqlite"), JobsConfig::default())?;
    let store = Store::open(&roots.data, &roots.data.join("index.sqlite"))?;
    let scratch = Scratch::open(&roots.scratch, ScratchPolicy::default())?;
    let api = Api::builder(roots)
        .devices(devices)
        .projects(store)
        .scratch(scratch)
        .jobs(jobs);
    let (data, seen) = (roots.data.clone(), Arc::clone(&world));
    example.when_serving(move |address| async move { walk(address, &seen, &data, false).await });
    let page = concat!(env!("CARGO_MANIFEST_DIR"), "/page");
    let data = roots.data.clone();
    example
        .serve(api, page, move |address| async move {
            walk(address, &world, &data, true).await
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

/// A brick's size, and its voxels as floats.
fn floats(body: &[u8]) -> anyhow::Result<([usize; 3], Vec<f32>)> {
    ensure!(
        body.starts_with(b"OSV1") && body.get(6) == Some(&4),
        "a float brick"
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

/// The heights' error against the truth over the cells sure and smooth:
/// where the confidence is 0.5 or more and the truth varies by no more than
/// a step across the window a height draws on, 9 cells square: the
/// measure's 7, and the median's cell either side. The RMS, the
/// median, and how many cells.
fn errors(heights: &[f32], sure: &[f32], truth: &[f64], w: usize, step: f64) -> (f64, f64, usize) {
    let h = heights.len() / w;
    let smooth = |x: usize, y: usize| {
        let (mut lo, mut hi) = (f64::MAX, f64::MIN);
        for yy in y.saturating_sub(4)..(y + 5).min(h) {
            for xx in x.saturating_sub(4)..(x + 5).min(w) {
                (lo, hi) = (lo.min(truth[yy * w + xx]), hi.max(truth[yy * w + xx]));
            }
        }
        hi - lo <= step
    };
    let mut off: Vec<f64> = (0..heights.len())
        .filter(|&i| sure[i] >= 0.5 && smooth(i % w, i / w))
        .map(|i| f64::from(heights[i]) - truth[i])
        .collect();
    let rms = (off.iter().map(|e| e * e).sum::<f64>() / off.len().max(1) as f64).sqrt();
    off.iter_mut().for_each(|e| *e = e.abs());
    off.sort_by(f64::total_cmp);
    (
        rms,
        off.get(off.len() / 2).copied().unwrap_or(f64::NAN),
        off.len(),
    )
}

/// The z-scan over the dome, its depth from focus, and its error against
/// the truth in `world`. For `--smoke`, over the frame's middle ninth, as the
/// simulator renders each plane afresh, and held to P42's spec; its data in
/// `data`.
async fn walk(
    address: SocketAddr,
    world: &RwLock<World>,
    data: &std::path::Path,
    smoke: bool,
) -> anyhow::Result<()> {
    if smoke {
        let roi = json!({"x": 456, "y": 300, "width": 456, "height": 312});
        let set = put(address, "/api/v1/camera/roi", &roi).await?;
        ensure!(set.status < 300, "the region: {}", set.body);
    }
    invoke(address, "stage/home", json!({})).await?;
    for (channel, on) in [("transmitted", false), ("reflected", true)] {
        let light = json!({"channel": channel, "on": on});
        invoke(address, "illumination/set_light", light).await?;
    }
    // The chip sits on the coverslip's middle, a die's centre; the die's
    // dome is 50 µm right of it and 50 µm up.
    let cover = Slide::COVERSLIP;
    let (x, y) = (
        cover.origin.x + cover.size.width / 2.0 + 50.0,
        cover.origin.y + cover.size.height / 2.0 - 50.0,
    );
    let z = invoke(address, "simulation/in_focus", json!({"x": x, "y": y})).await?;
    let z = z.as_f64().context("the dome's focus")?;
    invoke(address, "stage/move_to", json!({"x": x, "y": y, "z": z})).await?;
    invoke(address, "camera/start_streaming", json!({})).await?;
    job(address, "exposure/auto", json!({})).await?;
    let made_project = invoke(
        address,
        "projects/create",
        json!({"name": "ex-depth-from-focus"}),
    )
    .await?;
    let project = made_project["id"]
        .as_str()
        .context("the project")?
        .to_owned();
    let (below, above, step) = STACK;
    let stack = json!({"below_um": below, "above_um": above, "step_um": step});
    let request = json!({"pattern": "z_scan", "at": {"x": x, "y": y}, "stack": stack});
    let scan = json!({"request": request, "project": project, "name": "a dome"});
    let started = Instant::now();
    let scanned = job(address, "scan/run", scan).await?;
    let acquisition = made(&scanned)
        .into_iter()
        .next()
        .context("the scan's acquisition")?;
    let scan_s = started.elapsed().as_secs_f64();
    let started = Instant::now();
    let params =
        json!({"project": project, "acquisition": acquisition, "options": {"mesh": "ply"}});
    let depth = job(
        address,
        "jobs/submit",
        json!({"kind": "volume.depth_from_focus", "params": params}),
    )
    .await?;
    let depth_s = started.elapsed().as_secs_f64();
    let [heights, confidence, image] = <[String; 3]>::try_from(made(&depth))
        .map_err(|m| anyhow::anyhow!("three results, not {m:?}"))?;

    // The heights and their confidence, as float bricks of their cells.
    let brick =
        |id: &str| format!("/api/v1/volume/{project}/results/{id}/brick?format=f32&level=0");
    let (size, found) = floats(&fetch(address, &brick(&heights)).await?.body)?;
    let (_, sure) = floats(&fetch(address, &brick(&confidence)).await?.body)?;
    let [w, h, _] = size;
    // Each cell's place on the stage, through the scan's field: its frame's
    // corners about the stack's place, the cells two pixels across.
    let contents = get(address, &format!("/api/v1/projects/project/{project}"))
        .await?
        .json()?;
    let plan = &contents["acquisitions"][0]["plan"];
    let corner = |k: usize| -> anyhow::Result<[f64; 2]> {
        let c = &plan["field"]["corners_um"][k];
        Ok([
            c["x"].as_f64().context("a corner")?,
            c["y"].as_f64().context("a corner")?,
        ])
    };
    let (tl, tr, bl) = (corner(0)?, corner(1)?, corner(3)?);
    let truth: Vec<f64> = {
        let world = world.read().unwrap_or_else(PoisonError::into_inner);
        (0..w * h)
            .map(|i| {
                let (u, v) = (
                    (2 * (i % w) + 1) as f64 / (2 * w) as f64,
                    (2 * (i / w) + 1) as f64 / (2 * h) as f64,
                );
                let at = |k: usize| [x, y][k] + tl[k] + u * (tr[k] - tl[k]) + v * (bl[k] - tl[k]);
                world
                    .in_focus(Point2::new(at(0), at(1)))
                    .map_or(f64::NAN, |z| z.0)
            })
            .collect()
    };
    let (rms, median, cells) = errors(&found, &sure, &truth, w, step);
    let share = sure.iter().filter(|c| **c >= 0.5).count() as f64 / sure.len().max(1) as f64;
    println!(
        "ex-depth-from-focus: a z-scan of {} planes, {step} µm apart, over a die's dome at 50× in reflected light, in {scan_s:.1} s",
        ((below + above) / step) as usize + 1
    );
    println!(
        "  depth from focus in {depth_s:.1} s: heights of {w} × {h} cells, {:.0} % of them sure",
        100.0 * share
    );
    println!(
        "  against the simulator's truth, over {cells} cells sure and smooth: {rms:.3} µm rms, {median:.3} µm the median (half a step {:.2})",
        step / 2.0
    );
    println!("  open http://{address}/ to see the surface drawn by VTK.js");
    if !smoke {
        return Ok(());
    }
    ensure!(
        rms < step / 2.0 && cells * 2 >= w * h,
        "{rms:.3} µm rms over {cells} of {} cells",
        w * h
    );
    let fused = fetch(
        address,
        &format!("/api/v1/volume/{project}/results/{image}/brick?level=0"),
    )
    .await?;
    ensure!(
        fused.body.starts_with(b"OSV1")
            && fused.body[6] == 1
            && fused.body[7] == 3
            && fused.body.len() == HEADER + 3 * w * h,
        "the all-in-focus image's brick: {} bytes",
        fused.body.len()
    );
    let mesh = data
        .join("projects")
        .join(&project)
        .join("results")
        .join(&heights)
        .join("exports/surface.ply");
    ensure!(mesh.is_file(), "the mesh at {}", mesh.display());
    let page = get(address, "/").await?;
    ensure!(page.status == 200, "the page: {}", page.status);
    let vtk = get(address, "/vendor/vtk.js-38.0.4/vtk.js").await?;
    ensure!(vtk.status == 200, "VTK.js: {}", vtk.status);
    Ok(())
}
