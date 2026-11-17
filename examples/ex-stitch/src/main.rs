//! `ex-stitch`: a scan of the simulated H&E slide, its stage stepping blind,
//! without encoders and losing steps, stitched through the API as a client
//! would (P39): each tile placed where its image says, against the
//! simulator's truth. `--bench N` times the stitch of N synthetic tiles of
//! 20 MP instead (§5.20). See the README.

use std::net::SocketAddr;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, bail, ensure};
use ex_common::{Example, get, post};
use oscope_api::Api;
use oscope_api::devices::{Devices, DevicesConfig};
use oscope_core::{Rect, Timestamp, Um, Xyz};
use oscope_jobs::{JobService, JobsConfig};
use oscope_sim::{HeTissue, Simulator, Slide};
use oscope_store::{
    FrameRecord, ImageInfo, NewProject, SampleType, Samples, Scratch, ScratchPolicy, Store,
};
use oscope_workflows::scan::{Field, Region, ZPolicy, raster};
use oscope_workflows::stitch::{StitchOptions, stitch};
use rayon::prelude::*;
use serde_json::{Value, json};

/// The scan's region about the first island, in µm.
const REGION: (f64, f64) = (3_000.0, 2_000.0);

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("ex-stitch: {error:#}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> anyhow::Result<()> {
    let example = Example::from_args_with("ex-stitch", 8243, &["--rate", "--bench"])?;
    if let Some(tiles) = example.number("--bench") {
        return tokio::task::spawn_blocking(move || bench(tiles)).await?;
    }
    let roots = &example.roots;
    for folder in [&roots.data, &roots.scratch] {
        std::fs::create_dir_all(folder)?;
    }
    let mut config = DevicesConfig::default();
    let sim = &mut config.simulator;
    // The quarter sensor, and a stage stepping blind: it reports where it
    // was sent, and half its moves lose 6 µm, which add up.
    sim.sensor = "quarter".to_owned();
    (sim.encoders, sim.lost_steps) = (false, [0.5, 6.0]);
    let rate = example
        .number("--rate")
        .unwrap_or(if example.smoke { 10 } else { 1 });
    sim.rate = rate as f64;
    let devices = Devices::from_config(&config, &roots.data).await;
    let devices = Arc::new(devices.map_err(anyhow::Error::msg)?);
    let simulator = devices.simulator.clone().context("a simulator")?;
    let jobs = JobService::open(&roots.data.join("jobs.sqlite"), JobsConfig::default())?;
    let store = Store::open(&roots.data, &roots.data.join("index.sqlite"))?;
    let scratch = Scratch::open(&roots.scratch, ScratchPolicy::default())?;
    let api = Api::builder(roots)
        .devices(devices)
        .projects(store)
        .scratch(scratch)
        .jobs(jobs);
    let walked = Arc::clone(&simulator);
    example.when_serving(move |address| async move { walk(address, &walked, false).await });
    let page = concat!(env!("CARGO_MANIFEST_DIR"), "/page");
    example
        .serve(api, page, move |address| async move {
            walk(address, &simulator, true).await
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

/// Runs the job `action` starts, and waits for it to complete: its first
/// result.
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

/// The root mean square of `found` less `truth`, in µm, each less its mean.
fn rms(found: &[[f64; 2]], truth: &[[f64; 2]]) -> f64 {
    let n = truth.len() as f64;
    let mean = [0, 1].map(|a| {
        (0..truth.len())
            .map(|k| found[k][a] - truth[k][a])
            .sum::<f64>()
            / n
    });
    let sum: f64 = (0..truth.len())
        .map(|k| {
            (found[k][0] - truth[k][0] - mean[0]).powi(2)
                + (found[k][1] - truth[k][1] - mean[1]).powi(2)
        })
        .sum();
    (sum / n).sqrt()
}

/// The scan, its stitch, and both against the simulator's truth. With
/// `check`, held to P39's spec.
async fn walk(address: SocketAddr, sim: &Simulator, check: bool) -> anyhow::Result<()> {
    invoke(address, "stage/home", json!({})).await?;
    let island = HeTissue::new(1, Slide::COVERSLIP).islands()[0];
    let at = json!({"x": island.x, "y": island.y});
    let z = invoke(address, "simulation/in_focus", at).await?;
    invoke(
        address,
        "stage/move_to",
        json!({"x": island.x, "y": island.y, "z": z}),
    )
    .await?;
    invoke(address, "camera/start_streaming", json!({})).await?;
    job(address, "exposure/auto", json!({})).await?;
    let made = invoke(address, "projects/create", json!({"name": "ex-stitch"})).await?;
    let project = made["id"].as_str().context("the project's ID")?.to_owned();
    let (w, h) = REGION;
    let (x, y) = (island.x - w / 2.0, island.y - h / 2.0);
    let region = json!({"rect": {"origin": {"x": x, "y": y}, "size": {"width": w, "height": h}}});
    let scan = json!({"request": {"pattern": "snake", "region": region}, "project": project, "name": "a blind stage's scan"});
    let scanned = job(address, "scan/run", scan).await?;
    let acquisition = scanned["results"][0]
        .as_str()
        .and_then(|r| r.rsplit('/').next())
        .context("the scan's acquisition")?
        .to_owned();
    // The frames: where the stage said it was, and where it truly was.
    let contents = get(address, &format!("/api/v1/projects/project/{project}"))
        .await?
        .json()?;
    let record = &contents["acquisitions"][0];
    let frames: Vec<FrameRecord> = serde_json::from_value(record["frames"].clone())?;
    let said: Vec<[f64; 2]> = frames
        .iter()
        .map(|f| {
            f.position
                .map(|p| [p.x.0, p.y.0])
                .context("a frame's position")
        })
        .collect::<anyhow::Result<_>>()?;
    let truth: Vec<[f64; 2]> = frames
        .iter()
        .map(|f| {
            let at = f.timestamp.context("a frame's time")?;
            let p: Xyz = sim.stage.true_position(at);
            Ok([p.x.0, p.y.0])
        })
        .collect::<anyhow::Result<_>>()?;
    // The stitch, as a job.
    let params = json!({"project": project, "acquisition": acquisition});
    let started = Instant::now();
    let stitched = job(
        address,
        "jobs/submit",
        json!({"kind": "stitch.run", "params": params}),
    )
    .await?;
    let took = started.elapsed().as_secs_f64();
    let result = stitched["results"][0]
        .as_str()
        .and_then(|r| r.rsplit('/').next())
        .context("the stitch's result")?
        .to_owned();
    let report = get(
        address,
        &format!("/api/v1/data/{project}/results/{result}/stitch.json"),
    )
    .await?
    .json()?;
    let offsets: Vec<[f64; 2]> = serde_json::from_value(report["offsets"].clone())?;
    // The offsets in µm, through the plan's M: its field's corners.
    let corners = &record["plan"]["field"]["corners_um"];
    let corner = |k: usize, axis: &str| corners[k][axis].as_f64().unwrap_or(f64::NAN);
    let (fw, fh) = (
        record["image"]["width"].as_f64(),
        record["image"]["height"].as_f64(),
    );
    let (fw, fh) = (
        fw.context("the frames' width")?,
        fh.context("their height")?,
    );
    let m = [
        [
            (corner(1, "x") - corner(0, "x")) / fw,
            (corner(3, "x") - corner(0, "x")) / fh,
        ],
        [
            (corner(1, "y") - corner(0, "y")) / fw,
            (corner(3, "y") - corner(0, "y")) / fh,
        ],
    ];
    let placed: Vec<[f64; 2]> = offsets
        .iter()
        .map(|o| {
            [
                m[0][0] * o[0] + m[0][1] * o[1],
                m[1][0] * o[0] + m[1][1] * o[1],
            ]
        })
        .collect();
    let (before, after) = (rms(&said, &truth), rms(&placed, &truth));
    let size = &report["size"];
    println!(
        "ex-stitch: a snake of {} tiles over 3 × 2 mm, the stage stepping blind: where it said it was, {before:.2} µm rms off the truth",
        frames.len()
    );
    println!(
        "  stitched in {took:.1} s: {} pairs kept of {}, {} rejected, missing by {:.2} px rms; the tiles {after:.2} µm rms off the truth",
        report["kept"],
        report["pairs"],
        report["rejected"],
        report["rms_px"].as_f64().unwrap_or(f64::NAN)
    );
    println!(
        "  the mosaic: {} × {} pixels, result {result} of the project",
        size[0], size[1]
    );
    println!("  open http://{address}/ to see it");
    if check {
        ensure!(
            before > 2.0,
            "the stage was only {before:.2} µm off: no errors to correct"
        );
        ensure!(
            after < 1.0,
            "the stitch left the tiles {after:.2} µm off the truth"
        );
        let thumbnail = get(
            address,
            &format!("/api/v1/data/{project}/results/{result}/thumbnail.jpg"),
        )
        .await?;
        ensure!(
            thumbnail.status == 200,
            "the mosaic's thumbnail: {}",
            thumbnail.status
        );
        let page = get(address, "/").await?;
        ensure!(page.status == 200, "the page: {}", page.status);
    }
    Ok(())
}

/// A value from 0 to 1 at the lattice point `(i, j)` of `seed`.
fn lattice(i: i64, j: i64, seed: u64) -> f64 {
    let mut h = (i as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ (j as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F)
        ^ seed;
    h ^= h >> 31;
    h = h.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    h ^= h >> 29;
    (h >> 11) as f64 / (1u64 << 53) as f64
}

/// The bench's texture at `(x, y)`, from 0 to 1: value noise, smoothly
/// interpolated between lattice points 7 and 23 pixels apart.
fn texture(x: f64, y: f64) -> f64 {
    let octave = |spacing: f64, seed: u64| {
        let (u, v) = (x / spacing, y / spacing);
        let (i, j) = (u.floor() as i64, v.floor() as i64);
        let smooth = |f: f64| f * f * (3.0 - 2.0 * f);
        let (fx, fy) = (smooth(u - i as f64), smooth(v - j as f64));
        let top = lattice(i, j, seed) + fx * (lattice(i + 1, j, seed) - lattice(i, j, seed));
        let low =
            lattice(i, j + 1, seed) + fx * (lattice(i + 1, j + 1, seed) - lattice(i, j + 1, seed));
        top + fy * (low - top)
    };
    0.6 * octave(7.0, 1) + 0.4 * octave(23.0, 2)
}

/// Times the stitch of `count` tiles of 20 MP, 5472 × 3648 16-bit grey, a
/// tenth overlapping, in rows of five, recorded up to 10 µm off (§5.20: at
/// least 10 a second, in a release build).
fn bench(count: usize) -> anyhow::Result<()> {
    ensure!(count >= 2, "--bench needs 2 tiles or more");
    let folder = tempfile::tempdir()?;
    let data = folder.path().join("data");
    let store = Store::open(&data, &data.join("index.sqlite"))?;
    let new = NewProject {
        name: "bench".to_owned(),
        ..NewProject::default()
    };
    let project = store.create(new)?.id;
    let (tw, th) = (5472_u32, 3648_u32);
    let field = Field::nominal(1.0, tw, th, 0.1)?;
    let (columns, rows) = (5.min(count), count.div_ceil(5));
    let (step_x, step_y) = (field.step_um.width, field.step_um.height);
    let region = Region::Rect(Rect::new(
        0.0,
        0.0,
        f64::from(tw) + step_x * (columns - 1) as f64,
        f64::from(th) + step_y * (rows - 1) as f64,
    ));
    let plan = raster(&region, &field, ZPolicy::Fixed { z_um: 0.0 })?;
    let tiles = &plan.tiles[..count.min(plan.tiles.len())];
    let mut info = ImageInfo::new(u64::from(tw), u64::from(th), 1, SampleType::U16);
    info.planes = tiles.len() as u32;
    let mut draft = store.acquisition(project, "a bench", &info)?;
    let mut state = 3_u64;
    let mut jitter = || {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (state >> 11) as f64 / (1u64 << 53) as f64 * 2.0 - 1.0
    };

    let mut frames = Vec::new();
    let written = Instant::now();
    for (k, tile) in tiles.iter().enumerate() {
        let (x0, y0) = (
            tile.x_um - f64::from(tw) / 2.0,
            tile.y_um - f64::from(th) / 2.0,
        );
        let samples: Vec<u16> = (0..th as usize * tw as usize)
            .into_par_iter()
            .map(|i| {
                let (u, v) = ((i % tw as usize) as f64, (i / tw as usize) as f64);
                (texture(x0 + u, y0 + v) * 65_535.0) as u16
            })
            .collect();
        let writer = draft.image().context("an acquisition's image")?;
        writer.write(
            k as u32,
            (0, 0),
            (u64::from(tw), u64::from(th)),
            &Samples::U16(samples),
        )?;
        let recorded = Xyz::new(
            Um(tile.x_um + 10.0 * jitter()),
            Um(tile.y_um + 10.0 * jitter()),
            Um(0.0),
        );
        frames.push(FrameRecord {
            index: k as u64,
            position: Some(recorded),
            timestamp: Some(Timestamp(k as u64)),
            trigger: None,
        });
    }
    let record = draft.record();
    record.plan = serde_json::to_value(&plan)?;
    record.frames = frames;
    let acquisition = draft.commit()?.id;
    println!(
        "ex-stitch: {} tiles of 20 MP written in {:.1} s",
        tiles.len(),
        written.elapsed().as_secs_f64()
    );
    let started = Instant::now();
    // Each phase's end, from the job's progress.
    let phases = std::sync::Mutex::new(Vec::<(String, f64)>::new());
    let progress = |_share: f64, what: String| {
        let phase = what.split_whitespace().last().unwrap_or("").to_owned();
        let mut seen = phases.lock().expect("the phases");
        match seen.last_mut() {
            Some(last) if last.0 == phase => last.1 = started.elapsed().as_secs_f64(),
            _ => seen.push((phase, started.elapsed().as_secs_f64())),
        }
        true
    };
    let options = StitchOptions::default();
    let (_, report) = stitch(&store, project, acquisition, &options, &progress)?;
    let ends: Vec<String> = phases
        .into_inner()
        .expect("the phases")
        .iter()
        .map(|(phase, at)| format!("{phase} by {at:.1} s"))
        .collect();
    println!("  {}", ends.join(", "));
    let took = started.elapsed().as_secs_f64();
    println!(
        "  stitched in {took:.1} s: {:.1} tiles a second; {} pairs kept of {}, missing by {:.2} px rms; at most {} MB of parts at once",
        tiles.len() as f64 / took,
        report.kept,
        report.pairs,
        report.rms_px,
        report.peak_bytes >> 20
    );
    // What `cargo xtask perf` reads (P53).
    println!("perf: stitch.tiles_per_s {:.2}", tiles.len() as f64 / took);
    println!("perf: stitch.peak_mb {}", report.peak_bytes >> 20);
    // The bound over many writers, which the tests' small mosaics can't
    // show: level 1's quarters waiting in far bands would break it.
    let budget = u64::from(options.memory_mb) << 20;
    ensure!(
        report.peak_bytes <= budget,
        "{} MB held at once, over the budget of {} MB",
        report.peak_bytes >> 20,
        options.memory_mb
    );
    Ok(())
}
