//! `ex-checkpoint-b`: the demo of Checkpoint B (plan §7). One recipe of a
//! stock node, a Rust extension's and a Python extension's runs live on the
//! simulated microscope's preview, then as a batch job over the camera's
//! next frames. See the README.

use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, bail, ensure};
use ex_common::{Example, get, post, put};
use futures_util::{SinkExt, StreamExt};
use oscope_acquire::preview::wire::Header;
use oscope_api::Api;
use oscope_api::devices::{Devices, DevicesConfig};
use oscope_ext::{Extensions, PythonRuntime};
use oscope_jobs::{JobService, JobsConfig};
use serde_json::{Value, json};
use tokio_tungstenite::tungstenite::Message;

/// The example's port, one of the manual checks'.
const PORT: u16 = 8232;

/// The recipe: a stock blur, the Rust sample's emboss, the Python sample's
/// denoising, in that order.
fn recipe() -> Value {
    json!({
        "format": 1,
        "name": "Checkpoint B",
        "inputs": [{ "name": "image", "type": "image" }],
        "nodes": [
            { "id": "smooth", "node": "filter.gaussian", "params": { "sigma": 1.0 } },
            { "id": "emboss", "node": "example.emboss", "params": { "strength": 2.0 } },
            { "id": "denoise", "node": "example.denoise_tv", "params": { "weight": 0.05, "iterations": 10 } }
        ],
        "edges": [
            { "from": "input.image", "to": "smooth.image" },
            { "from": "smooth.image", "to": "emboss.image" },
            { "from": "emboss.image", "to": "denoise.image" }
        ],
        "outputs": [{ "name": "image", "from": "denoise.image" }]
    })
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let example = Example::from_args("ex-checkpoint-b", PORT)?;
    if let Err(error) = oscope_ext::find_uv(None) {
        if example.smoke {
            println!("smoke: skipped: {error}");
            return Ok(());
        }
        bail!(error);
    }
    for folder in [&example.roots.data, &example.roots.scratch] {
        std::fs::create_dir_all(folder)?;
    }
    // Both samples, packed and installed: uv makes the Python one's
    // environment.
    let work = &example.roots.scratch;
    let rust = work.join("org.oscope.examples.emboss-0.1.0.omext");
    ex_rust_ext::pack(&ex_rust_ext::library()?, work, &rust)?;
    let python = work.join("org.oscope.examples.tv-denoise-0.1.0.omext");
    oscope_ext::pack(&ex_rust_ext::sample_folder("ext-python-node"), &python)?;
    let runtime = Some(PythonRuntime::default());
    let root = example.roots.data.join("extensions");
    let extensions = Extensions::open_with(&root, runtime.clone())?;
    for package in [&rust, &python] {
        let (extension, unpacked) = extensions.install(package)?;
        let how = if unpacked {
            "installed"
        } else {
            "installed already"
        };
        println!(
            "ex-checkpoint-b: {how} {} {}",
            extension.manifest.id, extension.manifest.version
        );
    }

    // A start: the extensions, the simulated microscope, the job service.
    let extensions = Extensions::open_with(&root, runtime)?;
    let ids: Vec<_> = extensions
        .nodes()
        .iter()
        .map(|n| n.descriptor().id.clone())
        .collect();
    println!(
        "ex-checkpoint-b: the extensions loaded give the nodes {}",
        ids.join(" and ")
    );
    let devices = Devices::from_config(&DevicesConfig::default(), &example.roots.data).await;
    let devices = Arc::new(devices.map_err(anyhow::Error::msg)?);
    let jobs = JobService::open(
        &example.roots.data.join("index.sqlite"),
        JobsConfig::default(),
    )?;
    let api = Api::builder(&example.roots)
        .devices(devices)
        .jobs(jobs)
        .extensions(extensions);
    let page = Path::new(env!("CARGO_MANIFEST_DIR")).join("../ex-live-view/page");
    example.when_serving(checkpoint);
    example.serve(api, page, checkpoint).await
}

/// Invokes `action` with `input`, and waits for it to end: its output.
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
        .context("an invocation")?
        .to_owned();
    let path = href
        .split_once("/api/")
        .map(|(_, path)| format!("/api/{path}"))
        .context("its path")?;
    let deadline = Instant::now() + Duration::from_secs(120);
    loop {
        let invocation = get(address, &path).await?.json()?;
        match invocation["status"].as_str() {
            Some("completed") => return Ok(invocation["output"].clone()),
            Some("pending" | "running") => {}
            _ => bail!("{action}: {invocation}"),
        }
        ensure!(Instant::now() < deadline, "{action} didn't end");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// The checkpoint: the recipe live on the preview, then as a batch job.
async fn checkpoint(address: SocketAddr) -> anyhow::Result<()> {
    // The batch takes the camera's whole frames: the sensor's middle
    // 960 × 720, so that it takes seconds, not minutes, on two cores.
    let caps = get(address, "/api/v1/camera/capabilities").await?.json()?;
    let middle =
        |size: &str, part: u64| caps[size].as_u64().unwrap_or(part).saturating_sub(part) / 2;
    let (x, y) = (middle("sensor_width", 960), middle("sensor_height", 720));
    let roi = json!({ "x": x, "y": y, "width": 960, "height": 720 });
    let set = put(address, "/api/v1/camera/roi", &roi).await?;
    ensure!(set.status < 300, "the ROI: {} {}", set.status, set.body);
    invoke(address, "camera/start_streaming", json!({})).await?;
    // Python's denoising takes milliseconds a frame: a budget for it, at a
    // small viewport, as live recipes are timed at the defaults' size.
    put(address, "/api/v1/preview/recipe_budget_ms", &json!(2000.0)).await?;
    let defaults = json!({ "width": 320, "height": 240, "quality": 80, "max_fps": 30.0 });
    put(address, "/api/v1/preview/defaults", &defaults).await?;
    let checked = invoke(
        address,
        "preview/set_live_recipe",
        json!({ "recipe": recipe() }),
    )
    .await?;
    println!(
        "live: the recipe {} is on the preview: {:.1} ms on a {} × {} image",
        checked["steps"],
        checked["time_ms"].as_f64().unwrap_or(0.0),
        checked["width"],
        checked["height"]
    );

    // Previews keep coming through it: a failing recipe would stop them.
    let url = format!("ws://{address}/api/v1/preview/stream");
    let (mut socket, _) = tokio_tungstenite::connect_async(url).await?;
    let mut params = json!({ "width": 320, "height": 240, "quality": 70, "ack": 0 });
    socket.send(Message::text(params.to_string())).await?;
    let mut seen = 0;
    while seen < 5 {
        let message = tokio::time::timeout(Duration::from_secs(60), socket.next()).await;
        let message = message
            .context("no preview within 60 s")?
            .context("the stream closed")??;
        if let Message::Binary(bytes) = message {
            seen += 1;
            let header = Header::read(&bytes)?.header;
            params["ack"] = json!(header.sequence + 1);
            socket.send(Message::text(params.to_string())).await?;
        }
    }
    socket.close(None).await?;
    println!("live: {seen} previews through it");

    // The same recipe, as a batch job over the camera's next frames.
    let submit = json!({ "kind": "pipeline.batch", "params": { "recipe": recipe(), "frames": 3 } });
    let job = invoke(address, "jobs/submit", submit).await?;
    let href = job["href"].as_str().context("a job")?.to_owned();
    let deadline = Instant::now() + Duration::from_secs(300);
    let done = loop {
        let job = get(address, &href).await?.json()?;
        match job["state"].as_str() {
            Some("completed") => break job,
            Some("failed" | "cancelled" | "interrupted") => bail!("the batch: {job}"),
            _ => {}
        }
        ensure!(Instant::now() < deadline, "the batch didn't end");
        tokio::time::sleep(Duration::from_millis(100)).await;
    };
    let results = done["results"].as_array().map(Vec::len).unwrap_or(0);
    ensure!(results == 3, "the batch gave {results} results: {done}");
    println!("batch: three frames through it, saved in the data root's batch/:");
    for result in done["results"].as_array().into_iter().flatten() {
        println!("  {}", result.as_str().unwrap_or("?"));
    }
    println!("checkpoint B: one recipe of stock, Rust and Python nodes, live and as a batch job");
    Ok(())
}
