//! `ex-live-overlays`: the live view's overlays, drawn from live statistics
//! of the raw frames. It serves the simulated microscope, homed and over
//! the H&E slide's tissue, lit and streaming, with a page: crosshair, an
//! ROI to draw, a line profile, histograms, false colour and the
//! saturation mask.

use std::net::SocketAddr;
use std::path::Path;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, ensure};
use ex_common::{Example, get, post, put};
use futures_util::{SinkExt, StreamExt};
use oscope_acquire::preview::wire::{Header, MASK, STATISTICS};
use oscope_acquire::stats::{LiveStats, Mask};
use oscope_api::Api;
use oscope_api::devices::{Devices, DevicesConfig};
use oscope_core::{LightState, Um, Xyz};
use oscope_sim::Rate;
use serde_json::{Value, json};
use tokio_tungstenite::tungstenite::Message;

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("ex-live-overlays: {error:#}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> anyhow::Result<()> {
    let example = Example::from_args("ex-live-overlays", 8219)?;
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
    // The camera starts streaming once the server answers, through the
    // API, after the Things have started and set their saved settings.
    example.when_serving(|address| async move {
        let started = post(address, "/api/v1/camera/start_streaming", &json!({})).await?;
        ensure!(started.status == 201, "start_streaming: {}", started.status);
        Ok(())
    });
    example.serve(api, page, smoke).await
}

/// A preview's statistics and mask, if it carries them.
type Measured = (Header, Option<LiveStats>, Option<Mask>);

/// The next preview's header, statistics and mask.
async fn preview<S>(socket: &mut S) -> anyhow::Result<Measured>
where
    S: StreamExt<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin,
{
    loop {
        let message = tokio::time::timeout(Duration::from_secs(60), socket.next())
            .await
            .context("no preview within 60 s")?
            .context("the stream closed")??;
        if let Message::Binary(bytes) = message {
            let read = Header::read(&bytes)?;
            let block = |kind| {
                read.blocks
                    .iter()
                    .find(|(k, _)| *k == kind)
                    .map(|(_, b)| *b)
            };
            let stats = block(STATISTICS).map(serde_json::from_slice).transpose()?;
            let mask = block(MASK).and_then(Mask::from_bytes);
            return Ok((read.header, stats, mask));
        }
    }
}

/// `--smoke`: the page; statistics with an ROI and a profile; then, at
/// full light and a longer exposure, a saturation mask.
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
    let stats = json!({
        "roi": {"x": 600, "y": 400, "width": 120, "height": 80},
        "profile": {"from": [0.0, 456.0], "to": [1367.0, 456.0]},
        "bins": 64
    });
    let mut params = json!({"width": 342, "height": 228, "ack": 0, "stats": stats});
    socket.send(Message::text(params.to_string())).await?;
    let light = json!({"channel": "transmitted", "on": true, "intensity": 1.0});
    let (mut first, mut saturated) = (None::<Value>, None);
    for count in 0..60 {
        let (header, stats, mask) = preview(&mut socket).await?;
        params["ack"] = json!(header.sequence + 1);
        socket.send(Message::text(params.to_string())).await?;
        let Some(stats) = stats else { continue };
        if first.is_none() {
            let roi = stats.roi.as_ref().context("the ROI's statistics")?;
            let profile = stats.profile.as_ref().context("the profile")?;
            ensure!(stats.histogram.iter().all(|h| h.len() == 64), "64 bins");
            ensure!(
                profile.samples.len() == 685,
                "{} samples",
                profile.samples.len()
            );
            first = Some(json!({"mean": roi.mean, "focus": stats.focus}));
            post(address, "/api/v1/illumination/set_light", &light).await?;
            put(address, "/api/v1/camera/exposure", &json!(25.0)).await?;
        } else if let Some(mask) = mask {
            let set = mask.bits.iter().map(|b| b.count_ones()).sum::<u32>();
            saturated = Some((count, set, stats.saturated));
            break;
        }
    }
    let first = first.context("no statistics")?;
    let (count, set, share) = saturated.context("no saturation mask at full light and 25 ms")?;
    socket.close(None).await?;
    println!(
        "served: the page; statistics {first}; a mask of {set} pixels after {count} previews, saturated shares {share:.3?}"
    );
    Ok(())
}
