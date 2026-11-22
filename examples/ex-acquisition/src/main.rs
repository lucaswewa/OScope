//! A bounded, hardware-free checkpoint for live acquisition.

use std::process::ExitCode;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use anyhow::{Context, ensure};
use oscope_acquire::{Acquisition, FrameBus, WhenFull};
use oscope_core::{FramePool, HeapAllocator, LightState, Point2, Um, Xyz};
use oscope_hal::{Camera, ObjectiveCatalogue, Stage};
use oscope_sim::{
    HeTissue, Light, Optics, Rate, SimCamera, SimCameraConfig, SimClock, SimStage, SimStageConfig,
    Slide, World,
};

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("ex-acquisition: {error:#}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> anyhow::Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    ensure!(
        args.iter()
            .all(|arg| matches!(arg.as_str(), "--smoke" | "--require-cuda")),
        "usage: ex-acquisition [--smoke] [--require-cuda]"
    );
    let smoke = args.iter().any(|arg| arg == "--smoke");
    let required = args.iter().any(|arg| arg == "--require-cuda");
    let frames = if smoke { 6 } else { 30 };
    let clock = Arc::new(SimClock::new(Rate::RealTime));
    let tissue = HeTissue::new(1, Slide::COVERSLIP);
    let island = tissue.islands()[0];
    let position = Xyz::new(Um(island.x), Um(island.y), Um(5_000.0));
    let stage = Arc::new(SimStage::new(
        SimStageConfig {
            require_homing: false,
            start: position,
            ..SimStageConfig::default()
        },
        Arc::clone(&clock),
    ));
    let config = SimCameraConfig::quarter();
    let catalogue = ObjectiveCatalogue::mitutoyo();
    let optics = Optics::new(&catalogue, catalogue.get("10x")?, config.pixel_size);
    let world = World::with_slide(
        Slide::new(Arc::new(tissue), Point2::new(0.0, 0.0)),
        position.z,
    );
    let camera = SimCamera::new(
        config,
        clock,
        Arc::clone(&stage),
        Arc::new(RwLock::new(world)),
        optics,
    );
    let gpu = camera.prefer_gpu();
    println!("renderer: {}", if gpu { "CUDA" } else { "CPU" });
    ensure!(!required || gpu, "CUDA required but unavailable");
    camera.set_light(0.8, Light::Transmitted);
    let lights = vec![LightState {
        channel: "transmitted".into(),
        on: true,
        intensity: 0.8,
    }];
    let settings = camera.settings().await?;
    let (_, width, height) = camera.capabilities().frame_shape(&settings)?;
    let bytes = settings.format.frame_bytes(width, height);
    let pool = FramePool::new(bytes, 8, &HeapAllocator);
    let bus = Arc::new(FrameBus::new());
    let mut recorder = bus.subscribe_lossless("checkpoint", 16, WhenFull::Wait);
    let mut latest = bus.subscribe_latest("preview");
    let acquisition = Acquisition::new(Arc::clone(&bus), Duration::from_millis(250));
    acquisition.set_track(Some(stage.track()));
    acquisition.set_objective(Some("10x".into()));
    acquisition.set_lights(lights.clone());
    let stage_task = tokio::spawn({
        let stage = Arc::clone(&stage);
        async move { stage.run().await }
    });
    // Keep cleanup outside the fallible capture block so a timeout or failed
    // assertion also stops the camera and the stage actor.
    let capture: anyhow::Result<()> = async {
        camera.start(pool.clone(), acquisition.sink()).await?;
        let mut previous = None;
        for _ in 0..frames {
            let frame = tokio::time::timeout(Duration::from_secs(30), recorder.recv())
                .await
                .context("timed out waiting for a simulated frame")?
                .context("frame bus closed early")?;
            let meta = frame.meta();
            ensure!(frame.bytes().len() == bytes, "unexpected frame byte count");
            ensure!(
                (meta.width, meta.height) == (width, height),
                "unexpected shape"
            );
            ensure!(meta.format == settings.format, "unexpected pixel format");
            ensure!(
                meta.objective.as_deref() == Some("10x"),
                "missing objective"
            );
            ensure!(meta.lights == lights, "incorrect light metadata");
            let placed = meta
                .position
                .context("missing exposure-midpoint position")?;
            ensure!(
                (placed.x.0 - position.x.0).abs() < 0.01
                    && (placed.y.0 - position.y.0).abs() < 0.01
                    && (placed.z.0 - position.z.0).abs() < 0.01,
                "incorrect exposure-midpoint position"
            );
            ensure!(
                meta.host_time >= meta.exposure_midpoint(),
                "invalid frame times"
            );
            if let Some((id, time)) = previous {
                ensure!(
                    meta.id > id && meta.exposure_start > time,
                    "frames out of order"
                );
            }
            previous = Some((meta.id, meta.exposure_start));
        }
        let preview = latest
            .try_recv()
            .context("latest-frame subscriber is empty")?;
        ensure!(
            preview.meta().id >= previous.unwrap().0,
            "stale latest-frame subscriber"
        );
        Ok(())
    }
    .await;
    let stopped = camera.stop().await;
    stage_task.abort();
    let joined = stage_task.await;
    ensure!(
        joined.is_err_and(|error| error.is_cancelled()),
        "stage actor did not cancel"
    );
    stopped?;
    capture?;
    ensure!(
        !required || camera.on_gpu(),
        "camera fell back to CPU during capture"
    );
    let (published, unplaced, refused) = acquisition.counts();
    ensure!(
        published >= frames && unplaced == 0 && refused == 0,
        "incomplete acquisition metadata"
    );
    println!(
        "received {frames} ordered {width}x{height} frames; published {published}, unplaced {unplaced}, refused {refused}"
    );
    drop((recorder, latest, acquisition, bus));
    let stats = pool.stats();
    ensure!(
        stats.exhausted == 0 && stats.in_use == 0,
        "frame slabs not returned: {stats:?}"
    );
    println!("camera stopped; stage cancelled; pool {stats:?}");
    if smoke {
        println!("smoke: ok");
    }
    Ok(())
}
