//! `ex-http-walkthrough`: the instrument on the control plane. It serves the
//! simulated microscope's Things (camera, stage, illumination, objective
//! and simulation) with a plain control page: the stage's position, and
//! buttons. The README walks through the same with `curl`.

use std::net::SocketAddr;
use std::path::Path;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, ensure};
use ex_common::{Example, get, post, put};
use oscope_api::Api;
use oscope_api::devices::{Devices, DevicesConfig};
use serde_json::{Value, json};

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("ex-http-walkthrough: {error:#}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> anyhow::Result<()> {
    let example = Example::from_args("ex-http-walkthrough", 8212)?;
    // The simulated microscope, on the scaled-down sensor, over the H&E
    // slide, in focus at z = 5 mm.
    let mut config = DevicesConfig::default();
    config.simulator.sensor = "quarter".to_owned();
    let devices = Devices::from_config(&config, Path::new("."))
        .await
        .map_err(anyhow::Error::msg)?;
    let page = Path::new(env!("CARGO_MANIFEST_DIR")).join("page");
    let api = Api::builder(&example.roots)
        .settings(example.roots.data.join("settings"))
        .devices(Arc::new(devices));
    example.serve(api, page, smoke).await
}

/// Invokes `action` with `input`, and waits for its invocation to end:
/// its final JSON.
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
        .context("the invocation's href")?
        .to_owned();
    let deadline = Instant::now() + Duration::from_secs(120);
    loop {
        let invocation = get(address, &href).await?.json()?;
        if !["pending", "running"].contains(&invocation["status"].as_str().unwrap_or("")) {
            return Ok(invocation);
        }
        ensure!(Instant::now() < deadline, "{action} didn't end");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// `--smoke`: the page; homing, a move and the position it reports; a
/// refused setting's status; the simulator's samples.
async fn smoke(address: SocketAddr) -> anyhow::Result<()> {
    let page = get(address, "/").await?;
    ensure!(
        page.status == 200 && page.content_type.starts_with("text/html"),
        "the page"
    );
    let home = invoke(address, "stage/home", json!({})).await?;
    ensure!(home["status"] == "completed", "home: {home}");
    let target = json!({"x": 30_000.0, "y": 10_000.0, "z": 5_000.0, "speed": 25_000.0});
    let moved = invoke(address, "stage/move_to", target).await?;
    ensure!(moved["status"] == "completed", "move_to: {moved}");
    let x = get(address, "/api/v1/stage/position").await?.json()?["position"]["x"]
        .as_f64()
        .unwrap_or(0.0);
    ensure!((x - 30_000.0).abs() < 2.5, "the stage is at x = {x}");
    let refused = put(address, "/api/v1/camera/gain", &json!(999)).await?;
    ensure!(
        refused.status == 400,
        "a gain of 999: {} {}",
        refused.status,
        refused.body
    );
    let samples = get(address, "/api/v1/simulation/samples").await?.json()?;
    ensure!(samples.as_array().map(Vec::len) == Some(7), "{samples}");
    println!(
        "served: the page; homed, moved to x = {x:.1} µm; a gain of 999 refused with 400; seven samples"
    );
    Ok(())
}
