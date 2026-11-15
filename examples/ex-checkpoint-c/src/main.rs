//! `ex-checkpoint-c`: Checkpoint C (P34), on the simulator, through the API:
//! calibrate the 10×'s geometry, autofocus, set the exposure and the white
//! balance, then capture a frame into a project, stored as OME-Zarr, and
//! say how to open it in napari. See the README.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
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

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("ex-checkpoint-c: {error:#}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> anyhow::Result<()> {
    let example = Example::from_args_with("ex-checkpoint-c", 8238, &["--rate"])?;
    let roots = &example.roots;
    for folder in [&roots.data, &roots.scratch] {
        std::fs::create_dir_all(folder)?;
    }
    let mut config = DevicesConfig::default();
    let sim = &mut config.simulator;
    if example.smoke {
        sim.sensor = "quarter".to_owned();
    }
    let rate = example
        .number("--rate")
        .unwrap_or(if example.smoke { 10 } else { 1 });
    sim.rate = rate as f64;
    let devices = Devices::from_config(&config, &roots.data).await;
    let devices = Arc::new(devices.map_err(anyhow::Error::msg)?);
    let store = Store::open(&roots.data, &roots.data.join("index.sqlite"))?;
    let scratch = Scratch::open(&roots.scratch, ScratchPolicy::default())?;
    let jobs = JobService::open(&roots.data.join("jobs.sqlite"), JobsConfig::default())?;
    let api = Api::builder(roots)
        .devices(devices)
        .projects(store)
        .scratch(scratch)
        .jobs(jobs);
    let (data, smoke) = (roots.data.clone(), example.smoke);
    let shown = data.clone();
    example.when_serving(move |address| async move { walk(address, &shown, smoke, false).await });
    let page = concat!(env!("CARGO_MANIFEST_DIR"), "/page");
    example
        .serve(api, page, move |address| async move {
            walk(address, &data, true, true).await
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

/// Runs the job `action` starts, and waits for it to complete: its
/// results.
async fn job(address: SocketAddr, action: &str, input: Value) -> anyhow::Result<Vec<String>> {
    let submitted = invoke(address, action, input).await?;
    let href = submitted["href"]
        .as_str()
        .context("a job's href")?
        .to_owned();
    let deadline = Instant::now() + Duration::from_secs(600);
    loop {
        let job = get(address, &href).await?.json()?;
        match job["state"].as_str() {
            Some("completed") => {
                let results = job["results"].as_array().into_iter().flatten();
                return Ok(results
                    .filter_map(|r| r.as_str().map(str::to_owned))
                    .collect());
            }
            Some("failed" | "cancelled") => bail!("{action}: {}", job["error"]),
            _ => {}
        }
        ensure!(Instant::now() < deadline, "{action} didn't end");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// The checkpoint: each step printed; with `check`, each held to what its
/// phase's tests hold it to.
async fn walk(address: SocketAddr, data: &Path, small: bool, check: bool) -> anyhow::Result<()> {
    // A region of the sensor's middle: the simulator renders each new depth
    // afresh, which takes it seconds for a 20 MP frame. 768 × 576 of the
    // quarter sensor for `--smoke`, 1024 × 768 of the 20 MP otherwise.
    let roi = match small {
        true => json!({"x": 300, "y": 168, "width": 768, "height": 576}),
        false => json!({"x": 2224, "y": 1440, "width": 1024, "height": 768}),
    };
    let set = put(address, "/api/v1/camera/roi", &roi).await?;
    ensure!(set.status < 300, "the region: {}", set.body);
    invoke(address, "stage/home", json!({})).await?;
    let island = HeTissue::new(1, Slide::COVERSLIP).islands()[0];
    let at = json!({"x": island.x, "y": island.y});
    let truth = invoke(address, "simulation/in_focus", at).await?;
    let truth = truth.as_f64().context("the truth")?;
    // Focused by hand, roughly: 4 µm off.
    let start = json!({"x": island.x, "y": island.y, "z": truth + 4.0});
    invoke(address, "stage/move_to", start).await?;
    invoke(address, "camera/start_streaming", json!({})).await?;

    println!("\n1. Calibrate the 10×'s geometry");
    job(address, "calibration/check_objective", json!({})).await?;
    let check_report = get(address, "/api/v1/calibration/last_check")
        .await?
        .json()?;
    job(address, "calibration/geometry", json!({})).await?;
    let active = get(address, "/api/v1/calibration/active").await?.json()?;
    let geometry = &active["geometry"]["values"];
    println!(
        "   the objective check: {}; the pixel size {:.5} µm, turned {:.2}°",
        if check_report["plausible"] == true {
            "plausible"
        } else {
            "flagged"
        },
        geometry["pixel_um"].as_f64().unwrap_or(f64::NAN),
        geometry["rotation_deg"].as_f64().unwrap_or(f64::NAN)
    );

    println!("2. Autofocus");
    job(address, "autofocus/run", json!({})).await?;
    let focus = get(address, "/api/v1/autofocus/last").await?.json()?;
    let z = focus["z_um"].as_f64().unwrap_or(f64::NAN);
    let quarter = focus["depth_of_field_um"].as_f64().unwrap_or(f64::NAN) / 4.0;
    println!(
        "   z = {z:.3} µm, {:+.3} µm from the truth (a quarter of the depth of field is {quarter:.2} µm), in {:.1} s",
        z - truth,
        focus["took_ms"].as_f64().unwrap_or(f64::NAN) / 1e3
    );

    println!("3. Set the exposure, and the white balance over clear glass");
    job(address, "exposure/auto", json!({})).await?;
    let exposure = get(address, "/api/v1/exposure/last").await?.json()?;
    // White balance over clear glass beside the coverslip, the light's own
    // white; then back to the tissue, from below, as autofocus left it.
    let glass = json!({"x": 10_000.0, "y": 12_500.0, "z": z});
    invoke(address, "stage/move_to", glass).await?;
    job(address, "exposure/white_balance", json!({})).await?;
    for dz in [10.0, 0.0] {
        let back = json!({"x": island.x, "y": island.y, "z": z - dz});
        invoke(address, "stage/move_to", back).await?;
    }
    let white = get(address, "/api/v1/exposure/last_white_balance")
        .await?
        .json()?;
    let mut look = get(address, "/api/v1/preview/look").await?.json()?;
    look["gains"] = white["gains"].clone();
    put(address, "/api/v1/preview/look", &look).await?;
    println!(
        "   the exposure {} ms; the white balance's gains {}, given to the preview",
        exposure["exposure_ms"], white["gains"]
    );

    println!("4. Capture into a project");
    let made = invoke(address, "projects/create", json!({"name": "checkpoint-c"})).await?;
    let project = made["id"].as_str().context("a project")?.to_owned();
    let params = json!({"project": project, "name": "focused", "rendition": "png"});
    let results = job(address, "capture/single", params).await?;
    let id = results
        .first()
        .and_then(|r| r.rsplit('/').next())
        .context("the acquisition")?;
    let zarr: PathBuf = [data, Path::new("projects"), Path::new(&project)]
        .iter()
        .collect::<PathBuf>()
        .join("acquisitions")
        .join(id)
        .join("raw.ome.zarr");
    println!("   stored at {}", zarr.display());
    println!("\nTo open it in napari, with uv:\n");
    println!(
        "uv run --no-project --python 3.13 --with \"napari[all]\" --with napari-ome-zarr \\\n    napari --plugin napari-ome-zarr \"{}\"",
        zarr.display()
    );
    if check {
        ensure!(
            check_report["plausible"] == true,
            "the objective check: {check_report}"
        );
        ensure!(geometry.is_object(), "no geometry is active: {active}");
        ensure!(
            focus["found"] == true && (z - truth).abs() < quarter,
            "autofocus: {focus}"
        );
        ensure!(exposure["converged"] == true, "auto exposure: {exposure}");
        ensure!(
            zarr.join("zarr.json").exists(),
            "{} isn't a store",
            zarr.display()
        );
    }
    Ok(())
}
