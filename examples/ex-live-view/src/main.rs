//! `ex-live-view`: the live preview. It serves the simulated microscope,
//! homed and over the H&E slide's tissue, lit and streaming, with a page
//! that shows the `preview` Thing's stream: WebGL, zoom by ROI, fps and
//! latency, and jog buttons.

use std::net::SocketAddr;
use std::path::Path;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, ensure};
use ex_common::{Example, get, post};
use futures_util::{SinkExt, StreamExt};
use oscope_acquire::preview::wire::Header;
use oscope_api::Api;
use oscope_api::devices::{Devices, DevicesConfig};
use oscope_core::{LightState, Um, Xyz};
use oscope_sim::Rate;
use serde_json::json;
use tokio_tungstenite::tungstenite::Message;

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("ex-live-view: {error:#}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> anyhow::Result<()> {
    let example = Example::from_args("ex-live-view", 8218)?;
    // The simulated microscope on the scaled-down sensor, homed and placed
    // over the tissue at 50 times real time, then in real time.
    let mut config = DevicesConfig::default();
    config.simulator.sensor = "quarter".to_owned();
    config.simulator.rate = 50.0;
    let devices = Devices::from_config(&config, Path::new("."))
        .await
        .map_err(anyhow::Error::msg)?;
    let simulator = devices.simulator.clone().context("the simulator")?;
    devices.stage.home().await?;
    let over_tissue = Xyz::new(Um(40_000.0), Um(12_000.0), Um(5_000.0));
    devices.stage.move_to(over_tissue, None).await?;
    simulator.clock.set_rate(Rate::RealTime);
    let light = LightState {
        channel: "transmitted".to_owned(),
        on: true,
        intensity: 0.6,
    };
    devices.lights.set(light).await?;
    let page = Path::new(env!("CARGO_MANIFEST_DIR")).join("page");
    let api = Api::builder(&example.roots)
        .settings(example.roots.data.join("settings"))
        .devices(Arc::new(devices));
    example.serve(api, page, smoke).await
}

/// The next preview's header, and whether its payload is a JPEG.
async fn preview<S>(socket: &mut S) -> anyhow::Result<(Header, bool)>
where
    S: StreamExt<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin,
{
    loop {
        let message = tokio::time::timeout(Duration::from_secs(60), socket.next())
            .await
            .context("no preview within 60 s")?
            .context("the stream closed")??;
        if let Message::Binary(bytes) = message {
            let (header, jpeg) = Header::read(&bytes)?;
            return Ok((header, jpeg.starts_with(&[0xff, 0xd8])));
        }
    }
}

/// `--smoke`: the page; streaming starts, and previews of the size asked
/// for arrive, acknowledged, during a jog; the session is listed.
async fn smoke(address: SocketAddr) -> anyhow::Result<()> {
    let page = get(address, "/").await?;
    ensure!(
        page.status == 200 && page.content_type.starts_with("text/html"),
        "the page"
    );
    let started = post(address, "/api/v1/camera/start_streaming", &json!({})).await?;
    ensure!(started.status == 201, "start_streaming: {}", started.status);
    let url = format!("ws://{address}/api/v1/preview/stream");
    let (mut socket, _) = tokio_tungstenite::connect_async(url).await?;
    let mut params = json!({"width": 342, "height": 342, "quality": 70, "ack": 0});
    socket.send(Message::text(params.to_string())).await?;
    let jog = json!({"axis": "x", "towards_max": true, "speed": 500.0});
    ensure!(
        post(address, "/api/v1/stage/jog", &jog).await?.status == 201,
        "jog"
    );
    let mut sizes = Vec::new();
    for _ in 0..4 {
        let (header, jpeg) = preview(&mut socket).await?;
        ensure!(jpeg, "a preview that isn't a JPEG");
        sizes.push((header.width, header.height));
        params["ack"] = json!(header.sequence + 1);
        socket.send(Message::text(params.to_string())).await?;
    }
    ensure!(
        post(address, "/api/v1/stage/halt", &json!({}))
            .await?
            .status
            == 201,
        "halt"
    );
    ensure!(sizes.last() == Some(&(342, 228)), "previews of {sizes:?}");
    let sessions = get(address, "/api/v1/preview/sessions").await?.json()?;
    ensure!(sessions.as_array().map(Vec::len) == Some(1), "{sessions}");
    socket.close(None).await?;
    println!(
        "served: the page; four previews of 342 × 228 during a jog, acknowledged; one session"
    );
    Ok(())
}
