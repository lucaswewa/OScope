//! `ex-ext-camera`: the server with an extension's camera instead of the
//! simulator (P27, ADR-0036). The sample `ext-rust-camera` is packed,
//! installed and loaded at a start, and the configuration's `[devices]`
//! names its driver. Then the live view shows its frames, on
//! `ex-live-view`'s page. See the README.

use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, ensure};
use ex_common::{Example, get, post};
use futures_util::{SinkExt, StreamExt};
use oscope_acquire::preview::wire::Header;
use oscope_api::Api;
use oscope_api::devices::{Devices, DevicesConfig};
use oscope_ext::Extensions;
use serde_json::json;
use tokio_tungstenite::tungstenite::Message;

/// The example's port, one of the manual checks'.
const PORT: u16 = 8231;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let example = Example::from_args("ex-ext-camera", PORT)?;
    for folder in [&example.roots.data, &example.roots.scratch] {
        std::fs::create_dir_all(folder)?;
    }
    let library = ex_rust_ext::library_of("ext_rust_camera")?;
    let sample = ex_rust_ext::sample_folder("ext-rust-camera");
    let package = example
        .roots
        .scratch
        .join("org.oscope.examples.fakecam-0.1.0.omext");
    let manifest = ex_rust_ext::pack_sample(&sample, &library, &example.roots.scratch, &package)?;
    println!("ex-ext-camera: packed {} {}", manifest.id, manifest.version);
    let root = example.roots.data.join("extensions");
    let (_, unpacked) = Extensions::open(&root)?.install(&package)?;
    println!(
        "ex-ext-camera: {}",
        if unpacked {
            "installed"
        } else {
            "installed already"
        }
    );

    // A start: the extensions first, then the devices, which may name their
    // drivers.
    let extensions = Extensions::open(&root)?;
    for (name, kind) in extensions.drivers().names() {
        println!("ex-ext-camera: the extensions loaded drive a {kind}: {name}");
    }
    let fake = || "fake".to_owned();
    let config = DevicesConfig {
        camera: "example.fakecam".to_owned(),
        stage: fake(),
        illumination: fake(),
        objective: fake(),
        ..DevicesConfig::default()
    };
    let devices =
        Devices::from_config_with(&config, &example.roots.data, extensions.drivers()).await;
    let devices = Arc::new(devices.map_err(anyhow::Error::msg)?);
    println!(
        "ex-ext-camera: the camera is {}",
        devices.camera.capabilities().model
    );
    let api = Api::builder(&example.roots)
        .devices(devices)
        .extensions(extensions);
    let page = Path::new(env!("CARGO_MANIFEST_DIR")).join("../ex-live-view/page");
    example.when_serving(|address| async move {
        let started = post(address, "/api/v1/camera/start_streaming", &json!({})).await?;
        ensure!(started.status == 201, "start_streaming: {}", started.status);
        Ok(())
    });
    example.serve(api, page, smoke).await
}

/// `--smoke`: the camera Thing is the extension's; it streams, and
/// previews of its frames arrive, of the size asked for.
async fn smoke(address: SocketAddr) -> anyhow::Result<()> {
    let capabilities = get(address, "/api/v1/camera/capabilities").await?.json()?;
    let model = capabilities["model"].as_str().unwrap_or_default();
    ensure!(
        model == "OScope's example fake camera",
        "the camera is {capabilities}"
    );
    let started = post(address, "/api/v1/camera/start_streaming", &json!({})).await?;
    ensure!(started.status == 201, "start_streaming: {}", started.status);
    let url = format!("ws://{address}/api/v1/preview/stream");
    let (mut socket, _) = tokio_tungstenite::connect_async(url).await?;
    let mut params = json!({ "width": 320, "height": 320, "quality": 70, "ack": 0 });
    socket.send(Message::text(params.to_string())).await?;
    let mut sizes = Vec::new();
    while sizes.len() < 3 {
        let message = tokio::time::timeout(Duration::from_secs(60), socket.next()).await;
        let message = message
            .context("no preview within 60 s")?
            .context("the stream closed")??;
        if let Message::Binary(bytes) = message {
            let header = Header::read(&bytes)?.header;
            sizes.push((header.width, header.height));
            params["ack"] = json!(header.sequence + 1);
            socket.send(Message::text(params.to_string())).await?;
        }
    }
    ensure!(
        sizes.iter().all(|size| *size == (320, 240)),
        "previews of {sizes:?}"
    );
    socket.close(None).await?;
    println!("served: the extension's camera, streaming; three previews of 320 × 240");
    Ok(())
}
