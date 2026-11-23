//! `ex-calibrate`: calibrates the simulated instrument, whose camera is
//! turned by 30° and mirrored, whose objectives magnify 3 % more than the
//! catalogue says, and whose stage has 5 µm of backlash; and prints what
//! calibration recovered against that truth. All through the API, as a
//! client would. See the README.

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
use serde_json::{Value, json};

/// The truth injected into the simulator.
const ROTATION: f64 = 30.0;
const MIRRORED: bool = true;
const SCALE: f64 = 1.03;
const BACKLASH: f64 = 5.0;

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("ex-calibrate: {error:#}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> anyhow::Result<()> {
    let example = Example::from_args_with("ex-calibrate", 8235, &["--rate"])?;
    let roots = &example.roots;
    std::fs::create_dir_all(&roots.data)?;
    // The MIChrome 20's sensor in real time; the quarter sensor at ten times
    // real time for `--smoke`.
    let mut config = DevicesConfig::default();
    let sim = &mut config.simulator;
    (sim.rotation, sim.mirrored, sim.scale, sim.backlash) = (ROTATION, MIRRORED, SCALE, BACKLASH);
    if example.smoke {
        sim.sensor = "quarter".to_owned();
    }
    let rate = example
        .number("--rate")
        .unwrap_or(if example.smoke { 10 } else { 1 });
    sim.rate = rate as f64;
    let sensor_pixel = if example.smoke { 9.6 } else { 2.4 };
    let focus = sim.focus;
    let devices = Devices::from_config(&config, &roots.data).await;
    let devices = Arc::new(devices.map_err(anyhow::Error::msg)?);
    let jobs = JobService::open(&roots.data.join("jobs.sqlite"), JobsConfig::default())?;
    let api = Api::builder(roots).devices(devices).jobs(jobs);
    let truth = Truth {
        pixel: sensor_pixel / (10.0 * SCALE),
        focus,
    };
    example.when_serving(move |address| async move { tour(address, &truth, false).await });
    let page = concat!(env!("CARGO_MANIFEST_DIR"), "/page");
    example
        .serve(api, page, move |address| async move {
            tour(address, &truth, true).await
        })
        .await
}

/// What calibration should find through the 10×.
#[derive(Clone, Copy)]
struct Truth {
    pixel: f64,
    focus: f64,
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
    let deadline = Instant::now() + Duration::from_secs(300);
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

/// Starts the job that `action` asks for, and waits for it to end.
async fn job(address: SocketAddr, action: &str, input: Value) -> anyhow::Result<()> {
    let started = invoke(address, action, input).await?;
    let href = started["href"].as_str().context("a job's href")?.to_owned();
    let deadline = Instant::now() + Duration::from_secs(600);
    loop {
        let job = get(address, &href).await?.json()?;
        match job["state"].as_str() {
            Some("completed") => return Ok(()),
            Some("queued" | "running") => {}
            _ => bail!("{action}: {job}"),
        }
        ensure!(Instant::now() < deadline, "{action}'s job didn't end");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

fn number(value: &Value) -> anyhow::Result<f64> {
    value
        .as_f64()
        .with_context(|| format!("a number, not {value}"))
}

/// Homes the stage, takes it over the tissue, streams, and calibrates: the
/// objective check, the geometry, and the stage. Prints the truth against
/// what was recovered; with `check`, holds each to P32's tolerances.
async fn tour(address: SocketAddr, truth: &Truth, check: bool) -> anyhow::Result<()> {
    println!("homing, and moving over the tissue");
    invoke(address, "stage/home", json!({})).await?;
    let island = HeTissue::new(1, Slide::COVERSLIP).islands()[0];
    let over = json!({"x": island.x, "y": island.y, "z": truth.focus + 2.0});
    invoke(address, "stage/move_to", over).await?;
    invoke(address, "camera/start_streaming", json!({})).await?;
    println!("checking the objective, and calibrating the geometry and the stage");
    job(address, "calibration/check_objective", json!({})).await?;
    let checked = get(address, "/api/v1/calibration/last_check")
        .await?
        .json()?;
    job(address, "calibration/geometry", json!({})).await?;
    job(address, "calibration/stage", json!({})).await?;
    let active = get(address, "/api/v1/calibration/active").await?.json()?;
    let geometry = &active["geometry"]["values"];
    let stage = &active["stage"]["values"];
    let found = [
        number(&geometry["pixel_um"])?,
        number(&geometry["rotation_deg"])?,
        number(&geometry["aspect"])?,
        number(&geometry["skew"])?,
        number(&stage["backlash_um"]["x"])?,
        number(&stage["backlash_um"]["y"])?,
    ];
    let mirrored = geometry["mirrored"].as_bool().context("mirrored")?;
    let yes = |b: bool| if b { "yes" } else { "no" };
    println!("\nthe truth injected into the simulator, and what calibration recovered:");
    println!("{:<14} {:>12} {:>12}", "", "truth", "recovered");
    let rows = [
        (
            "pixel size",
            format!("{:.5} µm", truth.pixel),
            format!("{:.5} µm", found[0]),
        ),
        (
            "rotation",
            format!("{ROTATION:.3}°"),
            format!("{:.3}°", found[1]),
        ),
        (
            "mirrored",
            yes(MIRRORED).to_owned(),
            yes(mirrored).to_owned(),
        ),
        ("aspect", "1".to_owned(), format!("{:.5}", found[2])),
        (
            "skew",
            "0".to_owned(),
            format!("{:.5}", (found[3] * 1e5).round() / 1e5 + 0.0),
        ),
        (
            "backlash x",
            format!("{BACKLASH:.2} µm"),
            format!("{:.3} µm", found[4]),
        ),
        (
            "backlash y",
            format!("{BACKLASH:.2} µm"),
            format!("{:.3} µm", found[5]),
        ),
    ];
    for (what, truth, recovered) in rows {
        println!("{what:<14} {truth:>12} {recovered:>12}");
    }
    let ratio = number(&checked["ratio"])?;
    let plausible = checked["plausible"].as_bool().context("plausible")?;
    println!(
        "\nthe objective check, before calibrating: the 10× {} (measured over nominal {ratio:.3})",
        if plausible { "plausible" } else { "flagged" }
    );
    if check {
        let turn = (found[1] - ROTATION + 540.0).rem_euclid(360.0) - 180.0;
        ensure!(
            (found[0] / truth.pixel - 1.0).abs() < 0.002,
            "the pixel size: {found:?}"
        );
        ensure!(
            turn.abs() < 0.1 && mirrored == MIRRORED,
            "the turn: {found:?}"
        );
        ensure!(
            (found[2] - 1.0).abs() < 0.002 && found[3].abs() < 0.002,
            "{found:?}"
        );
        let backlash_off = found[4..].iter().map(|b| (b - BACKLASH).abs());
        ensure!(
            backlash_off.fold(0.0, f64::max) < 0.2,
            "the backlash: {found:?}"
        );
        ensure!(plausible, "the objective check: {checked}");
    }
    Ok(())
}
