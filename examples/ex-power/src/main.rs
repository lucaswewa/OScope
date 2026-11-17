//! `ex-power`: P51's power actions, in a dry run, which says what each
//! would run and does nothing. Refused unconfirmed; refused while a job
//! holds the instrument, unless forced; and what restarting, rebooting and
//! shutting down would run here. See the README.

use std::net::SocketAddr;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, ensure};
use ex_common::{Example, get, post};
use oscope_api::{Api, Restart};
use oscope_jobs::{Job, JobCtx, JobService, JobSpec, JobsConfig, Resource, Stop};
use serde_json::{Value, json};

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("ex-power: {error:#}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> anyhow::Result<()> {
    let example = Example::from_args("ex-power", 8214)?;
    let roots = &example.roots;
    std::fs::create_dir_all(&roots.data)?;
    let jobs = JobService::open(&roots.data.join("jobs.sqlite"), JobsConfig::default())?;
    // A job that holds the instrument until it is cancelled, as a scan
    // would.
    jobs.register(
        "example.hold",
        Arc::new(|params: &Value| {
            let spec = JobSpec {
                kind: "example.hold".to_owned(),
                title: "hold the instrument".to_owned(),
                account: None,
                params: params.clone(),
                resource: Resource::Hardware,
                pausable: false,
                gpu_mib: 0,
            };
            let run = Box::new(|ctx: JobCtx| {
                Box::pin(async move {
                    ctx.cancelled().await;
                    Err(Stop::Cancelled)
                }) as _
            });
            Ok(Job { spec, run })
        }),
    );
    // Always a dry run: an example never reboots the computer it runs on.
    let api = Api::builder(roots).jobs(jobs).power(true);
    let restart = api.restart();
    let early = Arc::clone(&restart);
    example.when_serving(move |address| async move { walk(address, &early, false).await });
    let page = concat!(env!("CARGO_MANIFEST_DIR"), "/page");
    example
        .serve(api, page, |address| async move {
            walk(address, &restart, true).await
        })
        .await
}

/// Invokes `action` with `input`: its final invocation.
async fn act(address: SocketAddr, action: &str, input: Value) -> anyhow::Result<Value> {
    let started = post(address, &format!("/api/v1/{action}"), &input).await?;
    ensure!(started.status == 201, "{action}: {}", started.status);
    let href = started.json()?["href"]
        .as_str()
        .context("an href")?
        .to_owned();
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let invocation = get(address, &href).await?.json()?;
        if !["pending", "running"].contains(&invocation["status"].as_str().unwrap_or("")) {
            return Ok(invocation);
        }
        ensure!(Instant::now() < deadline, "{action} didn't end");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// What a power action said it would run, or why it was refused.
fn said(invocation: &Value) -> String {
    match invocation["output"]["command"].as_array() {
        Some(words) => words
            .iter()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>()
            .join(" "),
        None if invocation["error"].is_null() => "(the server itself)".to_owned(),
        None => invocation["error"]["detail"]
            .as_str()
            .unwrap_or("refused")
            .to_owned(),
    }
}

/// The safeguards, then each action. With `check`, held to P51's spec.
/// It goes on only in a dry run: checked before any action, and after the
/// restart, done first, through `restart`, the server's restart request.
async fn walk(address: SocketAddr, restart: &Restart, check: bool) -> anyhow::Result<()> {
    let dry = get(address, "/api/v1/power/dry_run").await?.json()?;
    ensure!(dry == true, "not a dry run: {dry}");
    let unconfirmed = act(address, "power/reboot", json!({})).await?;
    println!(
        "ex-power: a dry run ({dry}); rebooting unconfirmed: {}",
        said(&unconfirmed)
    );
    // Had the dry run acted, the walk stops here, before a shutdown.
    let restarted = act(address, "power/restart", json!({"confirm": true})).await?;
    ensure!(!restart.is_requested(), "a dry run acted: {restarted}");
    let held = act(
        address,
        "jobs/submit",
        json!({"kind": "example.hold", "params": {}}),
    )
    .await?;
    let job = held["output"]["href"].as_str().context("a job")?.to_owned();
    let deadline = Instant::now() + Duration::from_secs(30);
    while get(address, &job).await?.json()?["state"] != "running" {
        ensure!(Instant::now() < deadline, "the job didn't start");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let busy = act(address, "power/shutdown", json!({"confirm": true})).await?;
    let forced = act(
        address,
        "power/shutdown",
        json!({"confirm": true, "force": true}),
    )
    .await?;
    println!(
        "  shutting down while a job holds the instrument: {}",
        said(&busy)
    );
    println!("  forced: {}", said(&forced));
    act(address, "jobs/cancel", json!({"id": held["output"]["id"]})).await?;
    // Cancelled at its next checkpoint: wait for it to end.
    while get(address, &job).await?.json()?["state"] == "running" {
        ensure!(Instant::now() < deadline, "the job didn't end");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let mut done = vec![("restart", restarted)];
    for action in ["reboot", "shutdown"] {
        let path = format!("power/{action}");
        let invocation = act(address, &path, json!({"confirm": true})).await?;
        done.push((action, invocation));
    }
    for (action, invocation) in &done {
        println!("  {action}: {}", said(invocation));
    }
    println!("  open http://{address}/ to try them");
    if !check {
        return Ok(());
    }
    ensure!(
        said(&unconfirmed).starts_with("refused (400)"),
        "{unconfirmed}"
    );
    ensure!(said(&busy).contains("example.hold"), "{busy}");
    ensure!(forced["output"]["dry_run"] == true, "{forced}");
    for (_, invocation) in &done {
        ensure!(invocation["output"]["dry_run"] == true, "{invocation}");
    }
    let index = get(address, "/").await?;
    ensure!(
        index.status == 200 && index.body.contains("app.js"),
        "the page: {}",
        index.status
    );
    Ok(())
}
