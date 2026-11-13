//! The server as a sidecar (ADR-0015), for the examples that supervise one.
//! An example runs itself as `oscope-server`, managed, through the server's
//! library, so nothing else needs building: [`serve_if_asked`] first in
//! `main`, then [`Sidecar::start`].

use std::ffi::OsString;
use std::io::{BufRead, BufReader};
use std::net::SocketAddr;
use std::process::{Child, Command, ExitCode, Stdio};
use std::time::Duration;

use anyhow::Context;
use serde_json::Value;

/// The argument that makes an example run as the server.
const SERVE: &str = "--serve-managed";

/// When this process was started as the server, runs it: its exit code.
/// Call it first in `main`, before any runtime starts.
pub fn serve_if_asked() -> Option<ExitCode> {
    let args: Vec<OsString> = std::env::args_os().collect();
    if args.get(1).is_none_or(|arg| arg != SERVE) {
        return None;
    }
    let args = std::iter::once(OsString::from("oscope-server")).chain(args[2..].iter().cloned());
    let cli = <oscope_server::Cli as clap::Parser>::parse_from(args);
    Some(match oscope_server::run(&cli) {
        Ok(stopped) => stopped.exit_code(),
        Err(error) => {
            eprintln!("oscope-server: {error}");
            error.exit_code()
        }
    })
}

/// The server, managed, and what its ready line said.
pub struct Sidecar {
    child: Child,
    /// The ready line.
    pub ready: Value,
}

impl Sidecar {
    /// Starts this program as the server, managed (`--managed` and `args`,
    /// `oscope-server`'s), and waits up to 60 s for its ready line.
    pub async fn start(args: &[OsString]) -> anyhow::Result<Sidecar> {
        let mut child = Command::new(std::env::current_exe()?)
            .arg(SERVE)
            .arg("--managed")
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        let stdout = child.stdout.take().context("the server's stdout")?;
        let line = tokio::task::spawn_blocking(move || {
            let mut line = String::new();
            BufReader::new(stdout).read_line(&mut line).map(|_| line)
        });
        let line = tokio::time::timeout(Duration::from_secs(60), line)
            .await
            .context("no ready line within 60 s")???;
        let ready: Value = serde_json::from_str(&line)
            .with_context(|| format!("the ready line isn't JSON: {line:?}"))?;
        Ok(Sidecar { child, ready })
    }

    /// Where it serves.
    pub fn address(&self) -> SocketAddr {
        let port = self.ready["port"].as_u64().unwrap_or(0) as u16;
        SocketAddr::from(([127, 0, 0, 1], port))
    }

    /// Its process ID.
    pub fn pid(&self) -> u32 {
        self.child.id()
    }

    /// Its preview ring's name, if it has one.
    pub fn ring(&self) -> Option<&str> {
        self.ready["shm"].as_str()
    }

    /// Waits for it to exit: its exit code.
    pub async fn exited(&mut self) -> anyhow::Result<Option<i32>> {
        loop {
            if let Some(status) = self.child.try_wait()? {
                return Ok(status.code());
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    /// Closes its stdin, which asks it to stop, and waits up to 30 s for it
    /// to exit: its exit code.
    pub async fn stop(&mut self) -> anyhow::Result<Option<i32>> {
        drop(self.child.stdin.take());
        tokio::time::timeout(Duration::from_secs(30), self.exited())
            .await
            .context("still running 30 s after its stdin closed")?
    }
}

impl Drop for Sidecar {
    fn drop(&mut self) {
        if matches!(self.child.try_wait(), Ok(None)) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}
