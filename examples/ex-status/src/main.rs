//! `ex-status`: P50's instrument status. The simulated microscope's
//! subsystems report their health, and the `status` Thing rolls it up. The
//! script injects each fault the simulator offers, in turn, and takes it
//! away again: the camera unplugged, the stage in alarm, the scratch disk
//! full. The page shows the indicator, and its buttons inject them. See the
//! README.

use std::net::SocketAddr;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::ensure;
use ex_common::{Example, get, invoke};
use oscope_api::Api;
use oscope_api::devices::{Devices, DevicesConfig};
use oscope_jobs::{JobService, JobsConfig};
use oscope_store::{Scratch, ScratchPolicy};
use serde_json::{Value, json};

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("ex-status: {error:#}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> anyhow::Result<()> {
    let example = Example::from_args("ex-status", 8215)?;
    let roots = &example.roots;
    for folder in [&roots.data, &roots.scratch] {
        std::fs::create_dir_all(folder)?;
    }
    let mut config = DevicesConfig::default();
    config.simulator.rate = if example.smoke { 10.0 } else { 1.0 };
    let devices = Devices::from_config(&config, &roots.data).await;
    let devices = Arc::new(devices.map_err(anyhow::Error::msg)?);
    let jobs = JobService::open(&roots.data.join("jobs.sqlite"), JobsConfig::default())?;
    let policy = ScratchPolicy {
        reserve_bytes: 0,
        low_bytes: 0,
    };
    let scratch = Scratch::open(&roots.scratch, policy)?;
    let api = Api::builder(roots)
        .devices(devices)
        .jobs(jobs)
        .scratch(scratch);
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

/// The instrument's status, once `wanted` holds of it.
async fn until(address: SocketAddr, wanted: impl Fn(&Value) -> bool) -> anyhow::Result<Value> {
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        let status = get(address, "/api/v1/status/instrument").await?.json()?;
        if wanted(&status) {
            return Ok(status);
        }
        ensure!(Instant::now() < deadline, "the status stayed {status}");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// Injects `fault`, or with `active` false takes it away.
async fn inject(address: SocketAddr, fault: &str, active: bool) -> anyhow::Result<()> {
    let input = json!({"fault": fault, "active": active});
    invoke(address, "simulation/inject_fault", input).await?;
    Ok(())
}

/// The stage homed, then each fault in turn. With `check`, held to P50's
/// spec.
async fn walk(address: SocketAddr, check: bool) -> anyhow::Result<()> {
    let start = until(address, |s| s["state"] != Value::Null).await?;
    invoke(address, "stage/home", json!({})).await?;
    let well = until(address, |s| s["state"] == "ok").await?;
    println!(
        "ex-status: at the start, {}: {}; homed, {}",
        start["state"], start["message"], well["message"]
    );
    let mut seen = Vec::new();
    for (fault, want) in [
        ("camera_unplugged", "camera: disconnected"),
        (
            "stage_alarm",
            "stage: in alarm: a limit switch (a fault injected)",
        ),
        (
            "scratch_full",
            "disks: the scratch disk is full (a fault injected)",
        ),
    ] {
        inject(address, fault, true).await?;
        let faulty = until(address, |s| s["state"] == "fault").await?;
        inject(address, fault, false).await?;
        let back = until(address, |s| s["state"] == "ok").await?;
        println!(
            "  {fault}: {}, {}; taken away, {}",
            faulty["state"], faulty["message"], back["message"]
        );
        seen.push((faulty["message"].clone(), json!(want)));
    }
    let gpu = well["subsystems"]
        .as_array()
        .and_then(|s| s.iter().find(|h| h["subsystem"] == "gpu"))
        .cloned()
        .unwrap_or_default();
    println!("  the GPU: {}, {}", gpu["state"], gpu["message"]);
    println!("  open http://{address}/ to see the indicator, and inject faults");
    if !check {
        return Ok(());
    }
    ensure!(
        start["message"] == "stage: in alarm: homing required",
        "at the start: {start}"
    );
    for (said, want) in seen {
        ensure!(said == want, "{said}, not {want}");
    }
    let index = get(address, "/").await?;
    ensure!(
        index.status == 200 && index.body.contains("app.js"),
        "the page: {}",
        index.status
    );
    Ok(())
}
