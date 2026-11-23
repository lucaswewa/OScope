//! `ex-live-view`: the live preview, and its benchmark. It serves the
//! simulated microscope on the full 5472 × 3648 sensor, homed and over the
//! H&E slide's tissue, lit and streaming at 14.5 fps, with a page that shows the
//! `preview` Thing's stream at 1000 × 667, with WebGL, and jog buttons.
//!
//! Timing is on: each step's times, from the camera's rendering to the
//! message sent, are written each second to `{data}/bench/timings.json`,
//! served as `/bench/timings.json`, and the page shows them beside its own.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, ensure};
use ex_common::{Example, get, post};
use futures_util::{SinkExt, StreamExt};
use oscope_acquire::preview::wire::Header;
use oscope_api::Api;
use oscope_api::devices::{Devices, DevicesConfig};
use oscope_core::{LightState, Um, Xyz, timing};
use oscope_sim::Rate;
use serde_json::{Value, json};
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
    timing::enable(true);
    // The simulated microscope on the full sensor, homed and placed over
    // the tissue at 50 times real time, then in real time. `--smoke` takes
    // the quarter sensor, the same field in a sixteenth of the pixels: on
    // two CPUs the full sensor's first tiles take over a minute.
    let mut config = DevicesConfig::default();
    let sensor = if example.smoke { "quarter" } else { "full" };
    config.simulator.sensor = sensor.to_owned();
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
    // The timings' file exists before the server is built, which serves
    // the files there are then.
    let bench = example.roots.data.join("bench");
    std::fs::create_dir_all(&bench).with_context(|| bench.display().to_string())?;
    let devices = Arc::new(devices);
    let file = bench.join("timings.json");
    std::fs::write(&file, timings(&devices).to_string())?;
    tokio::spawn(write_timings(Arc::clone(&devices), file));
    let api = Api::builder(&example.roots)
        .settings(example.roots.data.join("settings"))
        .devices(devices)
        .pages_under("bench", bench);
    // The camera starts streaming once the server answers, through the
    // API, after the Things have started and set their saved settings: in
    // mode `design`, full resolution in 16 bits at P29's 14.5 fps. Mode
    // `full`'s 7.24 fps is the MIChrome 20's measured rate in 16 bits.
    example.when_serving(|address| async move {
        let design = json!({
            "mode": "design",
            "format": {"kind": "bayer16", "pattern": "RGGB", "significant_bits": 12},
        });
        let set = post(address, "/api/v1/camera/set_mode", &design).await?;
        ensure!(set.status == 201, "set_mode: {}", set.status);
        let started = post(address, "/api/v1/camera/start_streaming", &json!({})).await?;
        ensure!(started.status == 201, "start_streaming: {}", started.status);
        Ok(())
    });
    example.serve(api, page, smoke).await
}

/// The steps' times over the last 30 s; the simulated camera's tiles; the
/// frames acquired; and what to add to a time on the camera's clock, in
/// ms, to put it on the UNIX epoch's, which the page's clock is on.
fn timings(devices: &Devices) -> Value {
    let since_epoch = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let now = devices.clock.now();
    let to_epoch_ms = since_epoch.as_secs_f64() * 1e3 - now.0 as f64 / 1e6;
    let camera = devices.simulator.as_ref().map(|simulator| {
        let cache = simulator.camera.cache_stats();
        // The GPU's tiles, when the camera renders there (ADR-0062).
        #[cfg(feature = "cuda")]
        let gpu = simulator.camera.gpu_stats().map(|s| {
            json!({
                "made": s.made, "layers": s.layers, "from_cpu": s.from_cpu,
                "coarse_frames": s.coarse_frames, "held": s.tiles, "layers_held": s.layer_tiles,
            })
        });
        #[cfg(not(feature = "cuda"))]
        let gpu: Option<Value> = None;
        json!({
            "gpu": simulator.camera.on_gpu(),
            "tiles": {"hits": cache.hits, "misses": cache.misses, "held": cache.tiles, "bytes": cache.bytes},
            "gpu_tiles": gpu,
        })
    });
    let (published, unplaced, refused) = devices.acquisition.counts();
    json!({
        "clock_to_epoch_ms": to_epoch_ms,
        "window_s": timing::WINDOW.as_secs(),
        "camera": camera,
        "frames": {"published": published, "unplaced": unplaced, "refused": refused},
        "steps": timing::snapshot(),
    })
}

/// Writes [`timings`] to `file` each second, whole: to another file first,
/// then moved over it, so that the page never reads half of one.
async fn write_timings(devices: Arc<Devices>, file: PathBuf) {
    let written = file.with_extension("json.new");
    let mut each = tokio::time::interval(Duration::from_secs(1));
    loop {
        each.tick().await;
        let json = timings(&devices).to_string();
        if tokio::fs::write(&written, json).await.is_ok() {
            let _ = tokio::fs::rename(&written, &file).await;
        }
    }
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
            let read = Header::read(&bytes)?;
            return Ok((read.header, read.jpeg.starts_with(&[0xff, 0xd8])));
        }
    }
}

/// `--smoke`: the page; streaming starts, and previews of 1000 × 667 of
/// the whole sensor, as the page asks for, arrive, acknowledged, during a
/// jog; the session is listed; the timings are served, with the preview's
/// steps.
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
    let mut params = json!({"width": 1000, "height": 667, "quality": 80, "ack": 0});
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
    ensure!(sizes.last() == Some(&(1000, 667)), "previews of {sizes:?}");
    let sessions = get(address, "/api/v1/preview/sessions").await?.json()?;
    ensure!(sessions.as_array().map(Vec::len) == Some(1), "{sessions}");
    socket.close(None).await?;
    // The timings' file is written each second.
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    let steps = loop {
        let timings = get(address, "/bench/timings.json").await?.json()?;
        let steps: Vec<String> = timings["steps"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|s| s["step"].as_str().map(str::to_owned))
            .collect();
        if [
            "camera.render",
            "preview.render",
            "preview.jpeg",
            "stream.latency",
        ]
        .iter()
        .all(|step| steps.iter().any(|s| s == step))
        {
            break steps;
        }
        ensure!(
            std::time::Instant::now() < deadline,
            "the timings: {timings}"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    };
    println!(
        "served: the page; four previews of 1000 × 667 during a jog, acknowledged; one session; \
         the timings of {} steps",
        steps.len()
    );
    Ok(())
}
