//! `ex-wizard`: walks P33's calibration wizard from the command line, on a
//! simulated instrument with its camera turned and mirrored, vignetting
//! optics, a sensor whose colours cross, and stage backlash. For each step
//! it prints the prompt, does what it asks on the simulator, confirms, and
//! prints what the step made. All through the API, as a client would. See
//! the README.

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

/// A sensor whose colours cross: red reads some green, and so on, in a warm
/// light.
const CROSSED: [[f64; 3]; 3] = [[0.85, 0.2, 0.05], [0.1, 0.8, 0.1], [0.02, 0.18, 0.6]];

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("ex-wizard: {error:#}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> anyhow::Result<()> {
    let example = Example::from_args_with("ex-wizard", 8236, &["--rate"])?;
    let roots = &example.roots;
    std::fs::create_dir_all(&roots.data)?;
    let mut config = DevicesConfig::default();
    let sim = &mut config.simulator;
    (sim.rotation, sim.mirrored, sim.scale, sim.backlash) = (30.0, true, 1.03, 5.0);
    (sim.vignetting, sim.response) = (0.25, CROSSED);
    if example.smoke {
        sim.sensor = "quarter".to_owned();
    }
    let rate = example
        .number("--rate")
        .unwrap_or(if example.smoke { 10 } else { 1 });
    sim.rate = rate as f64;
    let focus = sim.focus;
    let devices = Devices::from_config(&config, &roots.data).await;
    let devices = Arc::new(devices.map_err(anyhow::Error::msg)?);
    let jobs = JobService::open(&roots.data.join("jobs.sqlite"), JobsConfig::default())?;
    let api = Api::builder(roots).devices(devices).jobs(jobs);
    example.when_serving(move |address| async move { walk(address, focus, false).await });
    let page = concat!(env!("CARGO_MANIFEST_DIR"), "/page");
    example
        .serve(api, page, move |address| async move {
            walk(address, focus, true).await
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

/// Moves the stage to `(x, y)` µm, `z` µm.
async fn go(address: SocketAddr, x: f64, y: f64, z: f64) -> anyhow::Result<()> {
    invoke(address, "stage/move_to", json!({"x": x, "y": y, "z": z}))
        .await
        .map(drop)
}

/// Confirms the wizard's step `index`, and waits for it to settle: its
/// state.
async fn confirm(address: SocketAddr, index: usize) -> anyhow::Result<Value> {
    invoke(address, "calibration/wizard_confirm", json!({})).await?;
    let deadline = Instant::now() + Duration::from_secs(600);
    loop {
        let wizard = get(address, "/api/v1/calibration/wizard").await?.json()?;
        let state = &wizard["steps"][index]["state"];
        if state["state"] != "running" {
            return Ok(state.clone());
        }
        ensure!(Instant::now() < deadline, "step {index} didn't end");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

fn number(value: &Value) -> f64 {
    value.as_f64().unwrap_or(f64::NAN)
}

/// Walks the wizard: each step's prompt, the simulator made to do what it
/// asks, the step confirmed and settled. Then what the sets say; with
/// `check`, held to what P33's tests hold them to.
async fn walk(address: SocketAddr, focus: f64, check: bool) -> anyhow::Result<()> {
    invoke(address, "stage/home", json!({})).await?;
    let island = HeTissue::new(1, Slide::COVERSLIP).islands()[0];
    go(address, island.x, island.y, focus + 2.0).await?;
    invoke(address, "camera/start_streaming", json!({})).await?;
    let wizard = invoke(address, "calibration/wizard_start", json!({})).await?;
    let steps = wizard["steps"].as_array().context("the steps")?.clone();
    for (index, step) in steps.iter().enumerate() {
        let kind = step["kind"].as_str().unwrap_or("");
        println!("\n{}. {}", index + 1, step["title"].as_str().unwrap_or(""));
        println!("   {}", step["prompt"].as_str().unwrap_or(""));
        // What a person would do, done on the simulator.
        match kind {
            "flat" => {
                println!("   → the stage moves to clear glass, beside the coverslip");
                go(address, 10_000.0, 12_500.0, focus + 2.0).await?;
            }
            "colour" => {
                println!("   → the colour target is loaded, and the stage centres it");
                invoke(address, "simulation/load", json!({"name": "colour target"})).await?;
                go(address, 47_000.0, 12_500.0, focus + 0.05).await?;
            }
            _ => {}
        }
        let state = confirm(address, index).await?;
        match state["state"].as_str() {
            Some("done") => println!("   ✓ {}", state["result"].as_str().unwrap_or("")),
            _ => bail!("{kind}: {state}"),
        }
    }
    let active = get(address, "/api/v1/calibration/active").await?.json()?;
    let (g, s, d, f, c) = (
        &active["geometry"]["values"],
        &active["stage"]["values"],
        &active["dark"]["values"],
        &active["flat"]["values"],
        &active["colour"]["values"],
    );
    println!("\nwhat the wizard made:");
    println!(
        "  geometry   {:.5} µm a pixel, turned {:.2}°{}",
        number(&g["pixel_um"]),
        number(&g["rotation_deg"]),
        if g["mirrored"] == true {
            ", mirrored"
        } else {
            ""
        }
    );
    println!(
        "  stage      backlash {:.2} µm in x, {:.2} µm in y",
        number(&s["backlash_um"]["x"]),
        number(&s["backlash_um"]["y"])
    );
    println!(
        "  dark       mean {:.5}, spread {:.5}",
        number(&d["mean"]),
        number(&d["spread"])
    );
    let gains = &f["gains"];
    println!(
        "  flat       {:.1} % uneven before; white balance {:.3}, {:.3}, {:.3}",
        100.0 * number(&f["uneven"]),
        number(&gains[0]),
        number(&gains[1]),
        number(&gains[2])
    );
    println!(
        "  colour     ΔE*ab {:.2} on average after the matrix (largest {:.2}), from {:.2}",
        number(&c["delta_e_mean"]),
        number(&c["delta_e_max"]),
        number(&c["delta_e_before"])
    );
    if check {
        ensure!(number(&c["delta_e_mean"]) < 0.5, "colour: {c}");
        ensure!(number(&f["uneven"]) > 0.05, "the flat field: {f}");
        let turn = (number(&g["rotation_deg"]) - 30.0 + 540.0).rem_euclid(360.0) - 180.0;
        ensure!(
            turn.abs() < 0.1 && g["mirrored"] == true,
            "the geometry: {g}"
        );
        ensure!(
            (number(&s["backlash_um"]["x"]) - 5.0).abs() < 0.2,
            "the stage: {s}"
        );
        let wizard = get(address, "/api/v1/calibration/wizard").await?.json()?;
        ensure!(
            wizard["finished"] == true && wizard["cancelled"] == false,
            "{wizard}"
        );
    }
    Ok(())
}
