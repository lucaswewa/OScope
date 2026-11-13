//! `ex-sidecar`: a small supervisor, as the desktop app will be (ADR-0015).
//! It starts the server managed, says what its ready line says, starts it
//! again whenever it exits with code 75, and stops it by closing its stdin.

use std::ffi::OsString;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use anyhow::{Context, ensure};
use ex_common::sidecar::{self, Sidecar};
use ex_common::{Example, get, post};
use serde_json::json;

fn main() -> ExitCode {
    if let Some(code) = sidecar::serve_if_asked() {
        return code;
    }
    let outcome = tokio::runtime::Runtime::new().map(|runtime| runtime.block_on(run()));
    match outcome.map_err(anyhow::Error::from).and_then(|ran| ran) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("ex-sidecar: {error:#}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> anyhow::Result<()> {
    // The server listens on any free port, as the contract has it.
    let example = Example::from_args("ex-sidecar", 0)?;
    let args = server_args(&example)?;
    match example.smoke {
        true => smoke(&args).await,
        false => supervise(&args).await,
    }
}

/// The server's arguments: a configuration of the fakes, with folders of
/// its own beside the example's.
fn server_args(example: &Example) -> anyhow::Result<Vec<OsString>> {
    let data = std::path::absolute(example.roots.data.join("server"))?;
    let scratch = std::path::absolute(example.roots.scratch.join("server"))?;
    std::fs::create_dir_all(&data)?;
    let config: PathBuf = data.join("oscope.toml");
    let text = format!(
        "[storage]\ndata_root = '{}'\nscratch_root = '{}'\n\n[devices]\ncamera = \"fake\"\n\
         stage = \"fake\"\nillumination = \"fake\"\nobjective = \"fake\"\n",
        data.display(),
        scratch.display()
    );
    std::fs::write(&config, text)?;
    Ok(vec!["--config".into(), config.into()])
}

/// Says what the server's ready line says.
fn say_ready(server: &Sidecar) {
    let address = server.address();
    println!(
        "ex-sidecar: process {} is ready on http://{address}/, its token in {}, its preview ring {}",
        server.pid(),
        server.ready["token"].as_str().unwrap_or("?"),
        server.ring().unwrap_or("none"),
    );
    println!("ex-sidecar: to restart it: curl -X POST http://{address}/api/v1/system/restart");
}

/// Starts the server, and again each time it exits with code 75, until it
/// exits otherwise or Ctrl-C stops it.
async fn supervise(args: &[OsString]) -> anyhow::Result<()> {
    loop {
        let mut server = Sidecar::start(args).await?;
        say_ready(&server);
        let exited = tokio::select! {
            code = server.exited() => Some(code?),
            _ = tokio::signal::ctrl_c() => None,
        };
        match exited {
            Some(Some(75)) => {
                println!("ex-sidecar: it exited with 75, to be restarted: starting it again")
            }
            Some(code) => {
                println!("ex-sidecar: it exited with {code:?}: not starting it again");
                return Ok(());
            }
            None => {
                let code = server.stop().await?;
                println!("ex-sidecar: Ctrl-C: its stdin closed, and it exited with {code:?}");
                return Ok(());
            }
        }
    }
}

/// `--smoke`: the server ready and answering; asked to restart, it exits
/// with 75; started again, it is a new process; its stdin closed, it exits
/// with 0.
async fn smoke(args: &[OsString]) -> anyhow::Result<()> {
    let mut server = Sidecar::start(args).await?;
    say_ready(&server);
    let first = server.pid();
    ensure!(server.ready["pid"] == first, "the ready line's PID");
    ensure!(
        get(server.address(), "/health").await?.status == 200,
        "/health"
    );
    let restart = post(server.address(), "/api/v1/system/restart", &json!({})).await?;
    ensure!(restart.status == 201, "restart: {}", restart.status);
    let code = tokio::time::timeout(Duration::from_secs(30), server.exited())
        .await
        .context("not stopped 30 s after the restart")??;
    ensure!(
        code == Some(75),
        "asked to restart, it exited with {code:?}"
    );
    println!("ex-sidecar: it exited with 75, to be restarted: starting it again");
    let mut again = Sidecar::start(args).await?;
    say_ready(&again);
    ensure!(again.pid() != first, "the same process");
    ensure!(
        get(again.address(), "/health").await?.status == 200,
        "/health again"
    );
    let code = again.stop().await?;
    ensure!(code == Some(0), "its stdin closed, it exited with {code:?}");
    println!(
        "supervised: ready, restarted with exit code 75 as process {}, then stopped by closing its stdin",
        again.pid()
    );
    println!("smoke: ok");
    Ok(())
}
