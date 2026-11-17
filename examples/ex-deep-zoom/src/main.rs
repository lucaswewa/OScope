//! `ex-deep-zoom`: P40's deep zoom on the stitched slide. A snake over the
//! simulated H&E slide is scanned and stitched as a job; deep zoom makes
//! the mosaic's colour overview and coarse levels as the stitch ends, and
//! serves its tiles, browsed in OpenSeadragon with levels and gamma.
//! `--bench N` times tiles and full-screen views of an N-megapixel mosaic
//! against §5.20's budgets instead. See the README.

use std::net::SocketAddr;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, bail, ensure};
use ex_common::{Example, fetch, get, post};
use oscope_api::devices::{Devices, DevicesConfig};
use oscope_api::{Api, Roots};
use oscope_core::{BayerColour, BayerPattern, PixelFormat};
use oscope_jobs::{Job, JobService, JobSpec, JobsConfig, Resource, Stop};
use oscope_sim::{HeTissue, Slide};
use oscope_store::{ImageInfo, NewProject, SampleType, Samples, Scratch, ScratchPolicy, Store};
use rayon::prelude::*;
use serde_json::{Value, json};

/// The scan's region about the first island, in µm: smaller for `--smoke`.
const REGION: (f64, f64) = (4_000.0, 3_000.0);
const SMOKE_REGION: (f64, f64) = (2_000.0, 1_500.0);

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("ex-deep-zoom: {error:#}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> anyhow::Result<()> {
    let example = Example::from_args_with("ex-deep-zoom", 8244, &["--rate", "--bench"])?;
    if let Some(megapixels) = example.number("--bench") {
        return bench(megapixels).await;
    }
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
    let region = if example.smoke { SMOKE_REGION } else { REGION };
    example.when_serving(move |address| async move { walk(address, region, false).await });
    let page = concat!(env!("CARGO_MANIFEST_DIR"), "/page");
    example
        .serve(api, page, move |address| async move {
            walk(address, region, true).await
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

/// Waits until `base`'s status says its colour overview is built, and its
/// coarse levels made: the status.
async fn warmed(address: SocketAddr, base: &str) -> anyhow::Result<Value> {
    let deadline = Instant::now() + Duration::from_secs(300);
    loop {
        let status = get(address, &format!("{base}/status")).await?.json()?;
        let ready = status["overview"] == "ready" || status["raw"] == false;
        if ready && status["coarse"] == true {
            return Ok(status);
        }
        ensure!(Instant::now() < deadline, "deep zoom didn't warm: {status}");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// Whether `answer`'s tile came from the cache, by its `Server-Timing`.
fn hit(answer: &ex_common::Fetched) -> anyhow::Result<bool> {
    let timing = answer.header("server-timing").context("a Server-Timing")?;
    Ok(timing.contains("desc=\"hit\""))
}

/// The scan over `region`, its stitch, and its deep zoom. With `check`, held
/// to P40's spec.
async fn walk(address: SocketAddr, region: (f64, f64), check: bool) -> anyhow::Result<()> {
    invoke(address, "stage/home", json!({})).await?;
    let island = HeTissue::new(1, Slide::COVERSLIP).islands()[0];
    let at = json!({"x": island.x, "y": island.y});
    let z = invoke(address, "simulation/in_focus", at).await?;
    let to = json!({"x": island.x, "y": island.y, "z": z});
    invoke(address, "stage/move_to", to).await?;
    invoke(address, "camera/start_streaming", json!({})).await?;
    job(address, "exposure/auto", json!({})).await?;
    let made_project = invoke(address, "projects/create", json!({"name": "ex-deep-zoom"})).await?;
    let project = made_project["id"]
        .as_str()
        .context("the project")?
        .to_owned();
    let (w, h) = region;
    let origin = json!({"x": island.x - w / 2.0, "y": island.y - h / 2.0});
    let rect = json!({"rect": {"origin": origin, "size": {"width": w, "height": h}}});
    let request = json!({"pattern": "snake", "region": rect});
    let scan = json!({"request": request, "project": project, "name": "a slide"});
    let scanned = job(address, "scan/run", scan).await?;
    let acquisition = made(&scanned)?;
    let params = json!({"project": project, "acquisition": acquisition});
    let started = Instant::now();
    let stitch = json!({"kind": "stitch.run", "params": params});
    let result = made(&job(address, "jobs/submit", stitch).await?)?;
    let stitched = started.elapsed().as_secs_f64();
    // As the stitch ended, deep zoom made its colour overview and coarse
    // levels.
    let base = format!("/api/v1/zoom/{project}/results/{result}");
    let status = warmed(address, &base).await?;
    let warm = started.elapsed().as_secs_f64();
    let size = &status["size"];
    println!(
        "ex-deep-zoom: a snake over {:.1} × {:.1} mm, stitched in {stitched:.1} s: a raw mosaic of {} × {} pixels, {} levels",
        w / 1e3,
        h / 1e3,
        size[0],
        size[1],
        status["levels"]
    );
    println!(
        "  its colour overview and coarse levels ({} tiles) ready {warm:.1} s after the stitch began",
        status["tiles"]["kept"]
    );
    println!("  open http://{address}/ to browse it");
    if !check {
        return Ok(());
    }
    ensure!(
        status["raw"] == true,
        "the mosaic of a colour camera's frames isn't raw: {status}"
    );
    let dzi = get(address, &format!("{base}/image.dzi")).await?;
    ensure!(
        dzi.status == 200 && dzi.body.contains(&format!("Width=\"{}\"", size[0])),
        "the descriptor: {} {}",
        dzi.status,
        dzi.body
    );
    // A coarse tile, made as the stitch ended; a full-size one, made when
    // asked for, then kept.
    let levels = status["levels"].as_u64().context("its levels")? as u32;
    let coarse = fetch(address, &format!("{base}/image_files/8/0_0.jpg")).await?;
    ensure!(
        coarse.status == 200 && hit(&coarse)?,
        "a coarse tile: {} {:?}",
        coarse.status,
        coarse.header("server-timing")
    );
    let full = format!("{base}/image_files/{}/1_1.jpg", levels - 1);
    let first = fetch(address, &full).await?;
    let again = fetch(address, &full).await?;
    ensure!(
        first.status == 200 && !hit(&first)? && hit(&again)? && first.body == again.body,
        "a full-size tile, made then kept: {:?}, {:?}",
        first.header("server-timing"),
        again.header("server-timing")
    );
    ensure!(
        first.header("content-type") == Some("image/jpeg"),
        "a tile's type"
    );
    let page = get(address, "/").await?;
    ensure!(page.status == 200, "the page: {}", page.status);
    let viewer = get(address, "/vendor/openseadragon-6.1.1/openseadragon.min.js").await?;
    ensure!(viewer.status == 200, "OpenSeadragon: {}", viewer.status);
    Ok(())
}

/// A smooth colour at `(x, y)`, from 0 to 1: value noise per channel.
fn colour(x: u64, y: u64, c: u64) -> f32 {
    let f = |s: f64| ((x as f64 / s).sin() * (y as f64 / (s * 1.3)).cos()) as f32;
    (0.5 + 0.2 * f(37.0 + 11.0 * c as f64) + 0.1 * f(7.0 + c as f64)).clamp(0.0, 1.0)
}

/// Server times from `Server-Timing`, in ms.
fn server_ms(answer: &ex_common::Fetched) -> anyhow::Result<f64> {
    let timing = answer.header("server-timing").context("a Server-Timing")?;
    let dur = timing.split("dur=").nth(1).context("a duration")?;
    Ok(dur.trim().parse()?)
}

/// The median and the 95th percentile of `ms`.
fn spread(mut ms: Vec<f64>) -> (f64, f64) {
    ms.sort_by(f64::total_cmp);
    let at = |p: f64| ms[((ms.len() - 1) as f64 * p).round() as usize];
    (at(0.5), at(0.95))
}

/// A full-screen view, 1920 × 1080, of the full-size level from `(x, y)`,
/// its tiles asked for six at a time, as a browser does: the wall time
/// and the slowest server time, in ms, and how many tiles.
async fn view(
    address: SocketAddr,
    base: &str,
    level: u32,
    (x, y): (u64, u64),
    look: &str,
) -> anyhow::Result<(f64, f64, usize)> {
    let (c0, r0, c1, r1) = (x / 256, y / 256, (x + 1919) / 256, (y + 1079) / 256);
    let tiles: Vec<(u64, u64)> = (r0..=r1)
        .flat_map(|r| (c0..=c1).map(move |c| (c, r)))
        .collect();
    let queue = Arc::new(std::sync::Mutex::new(tiles.clone()));
    let started = Instant::now();
    let mut workers = Vec::new();
    for _ in 0..6 {
        let (queue, base, look) = (Arc::clone(&queue), base.to_owned(), look.to_owned());
        workers.push(tokio::spawn(async move {
            let mut slowest = 0.0_f64;
            loop {
                let next = queue.lock().map_err(|_| anyhow::anyhow!("a queue"))?.pop();
                let Some((c, r)) = next else {
                    return Ok::<_, anyhow::Error>(slowest);
                };
                let path = format!("{base}/image_files/{level}/{c}_{r}.jpg{look}");
                let answer = fetch(address, &path).await?;
                ensure!(answer.status == 200, "{path}: {}", answer.status);
                slowest = slowest.max(server_ms(&answer)?);
            }
        }));
    }
    let mut slowest = 0.0_f64;
    for worker in workers {
        slowest = slowest.max(worker.await??);
    }
    Ok((started.elapsed().as_secs_f64() * 1e3, slowest, tiles.len()))
}

/// Times deep zoom against §5.20 on a raw mosaic of about `megapixels`:
/// cold and kept tiles, and full-screen views of regions never seen, with
/// and without a hardware job holding the instrument.
async fn bench(megapixels: usize) -> anyhow::Result<()> {
    ensure!(megapixels >= 4, "--bench needs 4 megapixels or more");
    let folder = tempfile::tempdir()?;
    let roots = Roots {
        data: folder.path().join("data"),
        scratch: folder.path().join("scratch"),
    };
    let store = Store::open(&roots.data, &roots.data.join("index.sqlite"))?;
    let new = NewProject {
        name: "bench".to_owned(),
        ..NewProject::default()
    };
    let project = store.create(new)?.id;
    // A raw mosaic, 4:3 and even, written a band of rows at a time.
    let side = ((megapixels as f64 * 1e6 / 12.0).sqrt() as u64) & !1;
    let (w, h) = (4 * side, 3 * side);
    let mut info = ImageInfo::new(w, h, 1, SampleType::U16);
    info.channel_names = vec!["raw".to_owned()];
    let started = Instant::now();
    let mut draft = store.acquisition(project, "a mosaic", &info)?;
    let pattern = BayerPattern::Rggb;
    {
        let writer = draft.image().context("a writer")?;
        for y0 in (0..h).step_by(512) {
            let rows = 512.min(h - y0);
            let values: Vec<f32> = (y0..y0 + rows)
                .into_par_iter()
                .flat_map_iter(|y| {
                    (0..w).map(move |x| {
                        let c = match pattern.colour_at(x as u32, y as u32) {
                            BayerColour::Red => 0,
                            BayerColour::Green => 1,
                            BayerColour::Blue => 2,
                        };
                        colour(x, y, c)
                    })
                })
                .collect();
            let samples = Samples::from_unit(SampleType::U16, &values);
            writer.write(0, (0, y0), (w, rows), &samples)?;
        }
    }
    let format = PixelFormat::Bayer16 {
        pattern,
        significant_bits: 12,
    };
    draft.record().camera = json!({ "format": format });
    let id = draft.commit()?.id;
    println!(
        "ex-deep-zoom: a raw mosaic of {w} × {h} pixels written in {:.1} s",
        started.elapsed().as_secs_f64()
    );
    // A hardware job that holds the instrument until cancelled, as a scan
    // would.
    let jobs = JobService::open(&roots.data.join("jobs.sqlite"), JobsConfig::default())?;
    jobs.register(
        "bench.hold",
        Arc::new(|value| {
            Ok(Job {
                spec: JobSpec {
                    kind: "bench.hold".to_owned(),
                    title: "hold the instrument".to_owned(),
                    account: None,
                    params: value.clone(),
                    resource: Resource::Hardware,
                    pausable: false,
                    gpu_mib: 0,
                },
                run: Box::new(|ctx| {
                    Box::pin(async move {
                        ctx.cancelled().await;
                        Err(Stop::Cancelled)
                    })
                }),
            })
        }),
    );
    let api = Api::builder(&roots)
        .projects(store)
        .jobs(jobs.clone())
        .build()?;
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await?;
    let address = listener.local_addr()?;
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    let serving = tokio::spawn(api.serve(listener, async {
        let _ = stopped.await;
    }));
    let base = format!("/api/v1/zoom/{project}/acquisitions/{id}");
    let last = {
        let status = get(address, &format!("{base}/status")).await?.json()?;
        status["levels"].as_u64().context("its levels")? as u32 - 1
    };
    // Its colour overview, built when first asked for.
    let started = Instant::now();
    fetch(address, &format!("{base}/image_files/{}/0_0.jpg", last - 1)).await?;
    let deadline = Instant::now() + Duration::from_secs(600);
    while get(address, &format!("{base}/status")).await?.json()?["overview"] != "ready" {
        ensure!(
            Instant::now() < deadline,
            "the colour overview wasn't built"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    println!(
        "  its colour overview built in {:.1} s",
        started.elapsed().as_secs_f64()
    );
    // Cold tiles, one at a time, spread over the full-size level, then the
    // same tiles kept.
    let (cols, rows) = (w.div_ceil(256), h.div_ceil(256));
    let spread_tiles: Vec<(u64, u64)> = (0..60)
        .map(|k: u64| ((k * 37 + 3) % cols, (k * 23 + 5) % rows))
        .collect();
    let mut cold = Vec::new();
    let mut kept = Vec::new();
    for pass in [&mut cold, &mut kept] {
        for &(c, r) in &spread_tiles {
            let path = format!("{base}/image_files/{last}/{c}_{r}.jpg?gamma=1.1");
            let answer = fetch(address, &path).await?;
            ensure!(answer.status == 200, "{path}: {}", answer.status);
            pass.push(server_ms(&answer)?);
        }
    }
    let ((cold50, cold95), (kept50, kept95)) = (spread(cold), spread(kept));
    println!(
        "  a cold tile: {cold50:.2} ms of server time, the median, {cold95:.2} the 95th percentile (budget 5)"
    );
    println!(
        "  a kept tile: {kept50:.2} ms, the median, {kept95:.2} the 95th percentile (budget 1)"
    );
    // What `cargo xtask perf` reads (P53).
    println!("perf: zoom.cold_ms {cold50:.3}");
    println!("perf: zoom.kept_ms {kept50:.3}");
    // Full-screen views of regions never seen, in looks never asked for.
    let (x, y) = (w / 3 + 128, h / 3 + 77);
    let (wall, slowest, n) = view(address, &base, last, (x, y), "?gamma=1.2").await?;
    println!(
        "  a full-screen view never seen: {n} tiles in {wall:.0} ms, the slowest {slowest:.2} ms of server time (budget 150)"
    );
    println!("perf: zoom.view_ms {wall:.1}");
    let held = post(
        address,
        "/api/v1/jobs/submit",
        &json!({"kind": "bench.hold", "params": {}}),
    )
    .await?;
    ensure!(held.status == 201, "the held job: {}", held.status);
    let deadline = Instant::now() + Duration::from_secs(30);
    while jobs.hardware().holder().is_none() {
        ensure!(Instant::now() < deadline, "the hardware job didn't start");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let (x, y) = (2 * w / 3 + 64, 2 * h / 3 + 31);
    let (wall, slowest, n) = view(address, &base, last, (x, y), "?gamma=1.3").await?;
    println!(
        "  the same while a hardware job holds the instrument: {n} tiles in {wall:.0} ms, the slowest {slowest:.2} ms (budget 150)"
    );
    println!("perf: zoom.view_held_ms {wall:.1}");
    if let Some(holder) = jobs.hardware().holder() {
        jobs.cancel(holder)?;
    }
    let _ = stop.send(());
    serving.await??;
    Ok(())
}
