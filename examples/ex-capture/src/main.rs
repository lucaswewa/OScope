//! `ex-capture`: captures a frame of the simulated camera into a project,
//! with its display rendition; exports the raw frame as OME-TIFF and
//! downloads it, whole and by a range; and imports images: the rendition,
//! and the download. All through the API, as a client would. See the
//! README.

use std::net::SocketAddr;
use std::path::Path;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, bail, ensure};
use ex_common::{Example, get, post, put};
use oscope_api::Api;
use oscope_api::devices::{Devices, DevicesConfig};
use oscope_jobs::{JobService, JobsConfig};
use oscope_store::{Scratch, ScratchPolicy, Store};
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("ex-capture: {error:#}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> anyhow::Result<()> {
    let example = Example::from_args("ex-capture", 8234)?;
    let roots = &example.roots;
    for folder in [&roots.data, &roots.scratch] {
        std::fs::create_dir_all(folder)?;
    }
    let devices = Devices::from_config(&DevicesConfig::default(), &roots.data).await;
    let devices = Arc::new(devices.map_err(anyhow::Error::msg)?);
    let store = Store::open(&roots.data, &roots.data.join("index.sqlite"))?;
    let scratch = Scratch::open(&roots.scratch, ScratchPolicy::default())?;
    let jobs = JobService::open(&roots.data.join("index.sqlite"), JobsConfig::default())?;
    let api = Api::builder(roots)
        .devices(devices)
        .projects(store)
        .scratch(scratch)
        .jobs(jobs);
    let (data, smoke) = (roots.data.clone(), example.smoke);
    let downloads = data.join("downloads");
    let (shown, checked) = (downloads.clone(), downloads.clone());
    example
        .when_serving(move |address| async move { tour(address, &shown, false).await.map(drop) });
    let page = concat!(env!("CARGO_MANIFEST_DIR"), "/page");
    example
        .serve(api, page, move |address| async move {
            let made = tour(address, &checked, smoke).await?;
            check(address, &made).await
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
    let deadline = Instant::now() + Duration::from_secs(60);
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

/// Starts the job that `action` asks for, and waits for it: its results.
async fn job(address: SocketAddr, action: &str, input: Value) -> anyhow::Result<Vec<String>> {
    let started = invoke(address, action, input).await?;
    let href = started["href"].as_str().context("a job's href")?.to_owned();
    let deadline = Instant::now() + Duration::from_secs(300);
    loop {
        let job = get(address, &href).await?.json()?;
        match job["state"].as_str() {
            Some("completed") => {
                let results = job["results"].as_array().context("results")?;
                return Ok(results
                    .iter()
                    .filter_map(|r| r.as_str().map(str::to_owned))
                    .collect());
            }
            Some("queued" | "running") => {}
            _ => bail!("{action}: {job}"),
        }
        ensure!(Instant::now() < deadline, "{action}'s job didn't end");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// `GET path`, its range if given: the status and the body's bytes.
async fn download(
    address: SocketAddr,
    path: &str,
    range: Option<&str>,
) -> anyhow::Result<(u16, Vec<u8>)> {
    let mut stream = TcpStream::connect(address).await?;
    let range = range.map_or_else(String::new, |r| format!("range: {r}\r\n"));
    let head =
        format!("GET {path} HTTP/1.1\r\nhost: {address}\r\nconnection: close\r\n{range}\r\n");
    stream.write_all(head.as_bytes()).await?;
    let mut bytes = Vec::new();
    stream.read_to_end(&mut bytes).await?;
    let end = bytes
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .context("an HTTP answer")?;
    let head = String::from_utf8_lossy(&bytes[..end]);
    let status = head
        .split(' ')
        .nth(1)
        .and_then(|s| s.parse().ok())
        .context("a status")?;
    Ok((status, bytes.split_off(end + 4)))
}

/// What the tour made.
struct Made {
    project: String,
    captured: Value,
    export: Vec<u8>,
    imported: Vec<Value>,
}

/// The acquisition `path` names, in its project's contents.
async fn acquisition(address: SocketAddr, project: &str, path: &str) -> anyhow::Result<Value> {
    let id = path.rsplit('/').next().context("an acquisition's ID")?;
    let contents = get(address, &format!("/api/v1/projects/project/{project}"))
        .await?
        .json()?;
    let all = contents["acquisitions"]
        .as_array()
        .context("acquisitions")?;
    all.iter()
        .find(|a| a["id"] == id)
        .cloned()
        .context("the acquisition")
}

/// The tour: a capture, its rendition, an export downloaded, and imports.
/// For `--smoke`, of the sensor's middle 960 × 720, so that it takes
/// seconds on two cores.
async fn tour(address: SocketAddr, downloads: &Path, small: bool) -> anyhow::Result<Made> {
    let url = |path: &str| format!("http://{address}/api/v1/{path}");
    if small {
        let caps = get(address, "/api/v1/camera/capabilities").await?.json()?;
        let middle =
            |size: &str, part: u64| caps[size].as_u64().unwrap_or(part).saturating_sub(part) / 2;
        let roi = json!({ "x": middle("sensor_width", 960), "y": middle("sensor_height", 720), "width": 960, "height": 720 });
        let set = put(address, "/api/v1/camera/roi", &roi).await?;
        ensure!(set.status < 300, "the ROI: {} {}", set.status, set.body);
    }
    invoke(address, "camera/start_streaming", json!({})).await?;
    let made = invoke(address, "projects/create", json!({ "name": "ex-capture" })).await?;
    let project = made["id"].as_str().context("a project")?.to_owned();

    // A frame, as it came, with its rendition.
    let params = json!({ "project": project, "name": "field", "rendition": "png" });
    let results = job(address, "capture/single", params).await?;
    let captured = acquisition(address, &project, &results[0]).await?;
    let image = &captured["image"];
    println!(
        "ex-capture: captured \"field\", {} × {} pixels of {}, in project ex-capture",
        image["width"], image["height"], captured["camera"]["format"]["kind"]
    );
    println!("  its rendition: {}", url(&results[1]));

    // The raw frame as OME-TIFF, downloaded whole and by a range.
    let params = json!({ "project": project, "record": captured["id"], "format": "ome-tiff" });
    let input = json!({ "kind": "projects.export", "params": params });
    let exported = job(address, "jobs/submit", input).await?;
    let path = format!("/api/v1/{}", exported[0]);
    let (status, export) = download(address, &path, None).await?;
    ensure!(status == 200, "{path}: {status}");
    std::fs::create_dir_all(downloads)?;
    let saved = downloads.join("field.ome.tif");
    std::fs::write(&saved, &export)?;
    println!(
        "  exported as OME-TIFF, {:.1} MB, downloaded to {}",
        export.len() as f64 / 1e6,
        saved.display()
    );
    let (status, head) = download(address, &path, Some("bytes=0-3")).await?;
    ensure!(
        status == 206 && head == export[..4],
        "a range: {status}, {head:?}"
    );
    println!(
        "  its first 4 bytes, by a range: {:?}",
        String::from_utf8_lossy(&head)
    );

    // Imported: the rendition, a PNG, and the download.
    let rendition = results[1]
        .split_once(&format!("{project}/"))
        .context("a data path")?
        .1;
    let files = [
        (
            Path::new(&data_folder(downloads))
                .join("projects")
                .join(&project)
                .join(rendition),
            "the rendition",
        ),
        (saved, "the download"),
    ];
    let mut imported = Vec::new();
    for (file, name) in files {
        let params = json!({ "project": project, "path": file, "name": name });
        let results = job(
            address,
            "jobs/submit",
            json!({ "kind": "projects.import", "params": params }),
        )
        .await?;
        let made = acquisition(address, &project, &results[0]).await?;
        let image = &made["image"];
        let channels = match image["channels"].as_u64() {
            Some(1) => "grey",
            Some(3) => "RGB",
            Some(4) => "RGBA",
            _ => "two channels",
        };
        println!(
            "  imported {name}: {} × {}, {channels} of {}",
            image["width"], image["height"], image["sample"]
        );
        imported.push(made);
    }
    println!(
        "  the project's files: {}",
        url(&format!("projects/project/{project}"))
    );
    Ok(Made {
        project,
        captured,
        export,
        imported,
    })
}

/// The data root: the downloads' folder's parent.
fn data_folder(downloads: &Path) -> String {
    downloads
        .parent()
        .unwrap_or(downloads)
        .to_string_lossy()
        .into_owned()
}

/// `--smoke`: the capture whole, its export a TIFF, and each import as it
/// should be: the rendition in RGB, the download as captured.
async fn check(address: SocketAddr, made: &Made) -> anyhow::Result<()> {
    let image = &made.captured["image"];
    ensure!(
        image["width"] == 960 && image["height"] == 720,
        "the capture: {image}"
    );
    ensure!(image["planes"] == 1, "{image}");
    ensure!(
        made.export.starts_with(b"II*\0"),
        "not a little-endian TIFF"
    );
    let (rendition, back) = (&made.imported[0]["image"], &made.imported[1]["image"]);
    ensure!(
        rendition["channels"] == 3 && rendition["sample"] == "u8",
        "{rendition}"
    );
    for key in ["width", "height", "channels", "sample"] {
        ensure!(back[key] == image[key], "the download's {key}: {back}");
    }
    let contents = get(
        address,
        &format!("/api/v1/projects/project/{}", made.project),
    )
    .await?
    .json()?;
    let count = contents["acquisitions"].as_array().map_or(0, Vec::len);
    ensure!(count == 3, "{count} acquisitions");
    Ok(())
}
