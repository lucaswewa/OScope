//! `ex-sim-bench`: how fast the simulated camera renders at full
//! resolution, against the budgets of §5.20: at least 14.5 frames a second
//! with CUDA, and 5 on the CPU. It times a cold frame, then warm frames with
//! the stage still and jogging, on the CPU and, built with `cuda`, on the
//! GPU. See the README.

use std::process::ExitCode;
use std::sync::mpsc::{Receiver, channel};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use anyhow::{Context, ensure};
use oscope_core::{
    FrameBuffer, FrameMeta, FramePool, HeapAllocator, Ms, Point2, Timestamp, Um, Xyz,
};
use oscope_hal::{Axis, Camera, FrameSink, ObjectiveCatalogue, Stage, UmPerS};
use oscope_sim::{
    HeTissue, Optics, Rate, Settling, SimCamera, SimCameraConfig, SimClock, SimStage,
    SimStageConfig, Slide, World,
};

const FOCUS: f64 = 1_000.0;

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("ex-sim-bench: {error:#}");
            ExitCode::FAILURE
        }
    }
}

/// What a backend did.
struct Timing {
    backend: &'static str,
    cold_ms: f64,
    /// The median of the warm frames, still and jogging at each speed.
    warm_ms: Vec<f64>,
    tiles: usize,
}

/// A camera of `config` over the tissue at 10×, on a stepped clock, on the
/// GPU if `gpu`; its stage, its clock, and its frames.
async fn rig(
    config: SimCameraConfig,
    gpu: bool,
) -> anyhow::Result<(
    SimCamera,
    Arc<SimStage>,
    Arc<SimClock>,
    Receiver<(FrameBuffer, FrameMeta)>,
)> {
    let tissue = HeTissue::new(1, Slide::COVERSLIP);
    let island = tissue.islands()[0];
    let clock = Arc::new(SimClock::new(Rate::Stepped));
    let stage = Arc::new(SimStage::new(
        SimStageConfig {
            acceleration: None,
            backlash: Um(0.0),
            settling: Settling {
                overshoot: Um(0.0),
                time: Ms(15.0),
            },
            require_homing: false,
            start: Xyz::new(Um(island.x), Um(island.y), Um(FOCUS + 2.0)),
            ..SimStageConfig::default()
        },
        Arc::clone(&clock),
    ));
    let world = World::with_slide(
        Slide::new(Arc::new(tissue), Point2::new(0.0, 0.0)),
        Um(FOCUS),
    );
    let catalogue = ObjectiveCatalogue::mitutoyo();
    let optics = Optics::new(&catalogue, catalogue.get("10x")?, config.pixel_size);
    let camera = SimCamera::new(
        config,
        Arc::clone(&clock),
        Arc::clone(&stage),
        Arc::new(RwLock::new(world)),
        optics,
    );
    if gpu {
        #[cfg(feature = "cuda")]
        camera
            .use_gpu(oscope_compute::cuda::Cuda::open(0)?, 512)
            .map_err(anyhow::Error::msg)?;
    }
    let mut settings = camera.settings().await?;
    settings.mode = "design".into();
    settings.exposure = Ms(1.0);
    let settings = camera.configure(settings).await?;
    let (_, w, h) = camera.capabilities().frame_shape(&settings)?;
    let pool = FramePool::new(settings.format.frame_bytes(w, h), 4, &HeapAllocator);
    let (sender, frames) = channel();
    let sink: FrameSink = Arc::new(move |buffer, meta| {
        let _ = sender.send((buffer, meta));
    });
    camera.start(pool, sink).await?;
    Ok((camera, stage, clock, frames))
}

/// Times `frames` frames of one backend: the first cold, then warm, still
/// and jogging at each of `speeds` µm/s.
async fn bench(
    config: &SimCameraConfig,
    gpu: bool,
    frames: u32,
    speeds: &[f64],
) -> anyhow::Result<Timing> {
    let (camera, stage, clock, stream) = rig(config.clone(), gpu).await?;
    let interval = Duration::from_secs_f64(1.0 / 14.5);
    let mut k = 0_u32;
    // Each frame: the clock past its end, and the host's time until it comes.
    let mut frame = || -> anyhow::Result<f64> {
        let started = Instant::now();
        clock.advance_to(Timestamp(0) + interval * (k + 1) - Duration::from_millis(1));
        k += 1;
        stream
            .recv_timeout(Duration::from_secs(600))
            .context("a frame")?;
        Ok(started.elapsed().as_secs_f64() * 1e3)
    };
    let cold_ms = frame()?;
    let median = |mut v: Vec<f64>| {
        v.sort_by(f64::total_cmp);
        v[v.len() / 2]
    };
    let mut warm_ms = vec![median(
        (0..frames)
            .map(|_| frame())
            .collect::<anyhow::Result<_>>()?,
    )];
    for speed in speeds {
        stage.jog(Axis::X, true, Some(UmPerS(*speed))).await?;
        warm_ms.push(median(
            (0..frames)
                .map(|_| frame())
                .collect::<anyhow::Result<_>>()?,
        ));
        stage.stop().await?;
    }
    ensure!(!gpu || camera.on_gpu(), "the camera fell back to the CPU");
    let tiles = camera.cache_stats().tiles;
    camera.stop().await?;
    Ok(Timing {
        backend: if gpu { "GPU" } else { "CPU" },
        cold_ms,
        warm_ms,
        tiles,
    })
}

async fn run() -> anyhow::Result<()> {
    let smoke = std::env::args().any(|a| a == "--smoke");
    let (config, frames) = if smoke {
        (SimCameraConfig::quarter(), 3)
    } else {
        (SimCameraConfig::default(), 8)
    };
    let speeds = [200.0, 1_000.0];
    let mut backends = vec![false];
    if cfg!(feature = "cuda") && !smoke {
        backends.push(true);
    }
    println!(
        "the simulated MIChrome 20, {} × {}, mode `design`, over H&E tissue at 10×; median of {frames} frames:",
        config.width, config.height
    );
    println!(
        "  {:<7} {:>9} {:>14} {:>14} {:>14} {:>7}",
        "", "cold", "still", "jog 0.2 mm/s", "jog 1 mm/s", "tiles"
    );
    let mut timings = Vec::new();
    for gpu in backends {
        let t = bench(&config, gpu, frames, &speeds).await?;
        let cell = |ms: f64| format!("{ms:>5.0} ms {:>4.1}/s", 1e3 / ms);
        println!(
            "  {:<7} {:>6.0} ms {:>14} {:>14} {:>14} {:>7}",
            t.backend,
            t.cold_ms,
            cell(t.warm_ms[0]),
            cell(t.warm_ms[1]),
            cell(t.warm_ms[2]),
            t.tiles
        );
        timings.push(t);
    }
    // §5.20's budgets are for full resolution.
    for t in timings.iter().filter(|_| !smoke) {
        let (budget, fps) = (
            if t.backend == "GPU" { 14.5 } else { 5.0 },
            1e3 / t.warm_ms[0],
        );
        let verdict = if fps >= budget { "meets" } else { "misses" };
        println!(
            "{}: {fps:.1} frames a second, still: {verdict} §5.20's {budget}",
            t.backend
        );
    }
    if smoke {
        let cpu = &timings[0];
        ensure!(
            cpu.warm_ms[0] < cpu.cold_ms,
            "a warm frame took {} ms, the cold one {} ms",
            cpu.warm_ms[0],
            cpu.cold_ms
        );
        ensure!(cpu.tiles > 0, "no tiles cached");
        println!("smoke: ok");
    }
    Ok(())
}
