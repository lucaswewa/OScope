//! `ex-shm-reader`: a second process reading the preview ring (ADR-0015),
//! reporting the rate its previews come at, and any torn reads. It starts a
//! server, managed, streaming from the fake camera; or, with `OSCOPE_RING`
//! naming a running server's ring, reads that one.

use std::ffi::OsString;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use anyhow::{Context, ensure};
use ex_common::sidecar::{self, Sidecar};
use ex_common::{Example, post};
use oscope_acquire::preview::wire::Header;
use oscope_shm::{Read, RingReader, ShmError};
use serde_json::json;

fn main() -> ExitCode {
    if let Some(code) = sidecar::serve_if_asked() {
        return code;
    }
    let outcome = tokio::runtime::Runtime::new().map(|runtime| runtime.block_on(run()));
    match outcome.map_err(anyhow::Error::from).and_then(|ran| ran) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("ex-shm-reader: {error:#}");
            ExitCode::FAILURE
        }
    }
}

/// What reading the ring found.
#[derive(Default)]
struct Report {
    previews: u64,
    bytes: u64,
    torn: u64,
    overtaken: u64,
}

async fn run() -> anyhow::Result<()> {
    let example = Example::from_args_with("ex-shm-reader", 0, &["--seconds"])?;
    let default = if example.smoke { 2 } else { 10 };
    let seconds = example.number("--seconds").unwrap_or(default) as u64;
    let (mut server, name) = match std::env::var("OSCOPE_RING") {
        Ok(name) => (None, name),
        Err(_) => {
            let server = Sidecar::start(&server_args(&example)?).await?;
            let path = "/api/v1/camera/start_streaming";
            let started = post(server.address(), path, &json!({})).await?;
            ensure!(started.status == 201, "start_streaming: {}", started.status);
            let name = server.ring().context("the server has no ring")?.to_owned();
            (Some(server), name)
        }
    };
    let reader = RingReader::open(&name)?;
    println!(
        "ex-shm-reader: reading the ring {name}, which process {} writes, for {seconds} s",
        reader.writer()
    );
    let reading = tokio::task::spawn_blocking(move || read_for(reader, seconds));
    let report = reading.await??;
    let rate = report.previews as f64 / seconds as f64;
    println!(
        "ex-shm-reader: {} previews, {rate:.1} a second, {:.1} MB/s; {} torn reads read again, {} overtaken",
        report.previews,
        report.bytes as f64 / seconds as f64 / 1e6,
        report.torn,
        report.overtaken
    );
    if let Some(server) = &mut server {
        let code = server.stop().await?;
        ensure!(code == Some(0), "the server exited with {code:?}");
    }
    if example.smoke {
        ensure!(
            report.previews >= 3,
            "{} previews in {seconds} s",
            report.previews
        );
        println!("smoke: ok");
    }
    Ok(())
}

/// The server's arguments: a configuration of the fakes, with folders of
/// its own beside the example's.
fn server_args(example: &Example) -> anyhow::Result<Vec<OsString>> {
    let data = std::path::absolute(example.roots.data.join("server"))?;
    let scratch = std::path::absolute(example.roots.scratch.join("server"))?;
    std::fs::create_dir_all(&data)?;
    let config = data.join("oscope.toml");
    let text = format!(
        "[storage]\ndata_root = '{}'\nscratch_root = '{}'\n\n[devices]\ncamera = \"fake\"\n\
         stage = \"fake\"\nillumination = \"fake\"\nobjective = \"fake\"\n",
        data.display(),
        scratch.display()
    );
    std::fs::write(&config, text)?;
    Ok(vec!["--config".into(), config.into()])
}

/// Reads the newest preview as often as it can for `seconds`, checking
/// each, and says each second how it is going.
fn read_for(mut reader: RingReader, seconds: u64) -> anyhow::Result<Report> {
    let (start, mut report, mut message) = (Instant::now(), Report::default(), Vec::new());
    let mut next_report = Duration::from_secs(1);
    while start.elapsed() < Duration::from_secs(seconds) {
        match reader.read(&mut message) {
            Ok(Read::New(_)) => {
                let read = Header::read(&message)?;
                ensure!(
                    read.jpeg.starts_with(&[0xff, 0xd8]),
                    "a preview that isn't a JPEG"
                );
                report.previews += 1;
                report.bytes += message.len() as u64;
                if start.elapsed() >= next_report {
                    let header = read.header;
                    let made = header.ready.saturating_since(header.exposure_end);
                    println!(
                        "{:>3} s: {} previews; frame {} at {} × {}, ready {:.0} ms after its exposure",
                        next_report.as_secs(),
                        report.previews,
                        header.frame.0,
                        header.width,
                        header.height,
                        made.as_secs_f64() * 1e3
                    );
                    next_report += Duration::from_secs(1);
                }
            }
            Ok(Read::Same) => std::thread::sleep(Duration::from_millis(1)),
            Err(ShmError::Overtaken(_)) => report.overtaken += 1,
            Err(error) => return Err(error.into()),
        }
    }
    report.torn = reader.torn();
    Ok(report)
}
