//! `ex-sim-camera`: the simulated MIChrome 20, streaming in each mode over
//! the simulated H&E slide while the stage jogs. For each mode and format it
//! prints the frame rate, the frames dropped when rendering can't keep up,
//! the jitter of the frames' host times, and the position that acquisition
//! would record against the stage's true position. See the README.

use std::process::ExitCode;
use std::sync::mpsc::channel;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use anyhow::ensure;
use oscope_core::{Clock, FrameMeta, FramePool, HeapAllocator, Ms, Point2, Timestamp, Um, Xyz};
use oscope_hal::{Axis, Camera, FrameSink, ObjectiveCatalogue, Stage, UmPerS};
use oscope_sim::{
    HeTissue, Light, Optics, Rate, SimCamera, SimCameraConfig, SimClock, SimStage, SimStageConfig,
    Slide, World,
};

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("ex-sim-camera: {error:#}");
            ExitCode::FAILURE
        }
    }
}

/// What one mode's stream showed.
struct Run {
    mode: String,
    bits: u8,
    size: (u32, u32),
    rated: f64,
    frames: usize,
    dropped: u64,
    /// Frames delivered or dropped, a second of simulated time.
    rate: f64,
    jitter_ms: f64,
    error_um: f64,
}

async fn run() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let smoke = args.iter().any(|a| a == "--smoke");
    let full = args.iter().any(|a| a == "--full");
    let seconds = if smoke { 0.6 } else { 2.0 };
    let (width, height) = if full {
        (5472, 3648)
    } else if smoke {
        (384, 264)
    } else {
        (1368, 912)
    };

    // The slide, the stage over an island of tissue, and the camera through
    // the 10× objective.
    let clock = Arc::new(SimClock::new(Rate::RealTime));
    let tissue = HeTissue::new(1, Slide::COVERSLIP);
    let island = tissue.islands()[0];
    let focus = Um(5_000.0);
    let world = World::with_slide(Slide::new(Arc::new(tissue), Point2::new(0.0, 0.0)), focus);
    let stage = Arc::new(SimStage::new(
        SimStageConfig {
            require_homing: false,
            start: Xyz::new(Um(island.x), Um(island.y), Um(focus.0 + 2.0)),
            ..SimStageConfig::default()
        },
        Arc::clone(&clock),
    ));
    tokio::spawn({
        let stage = Arc::clone(&stage);
        async move { stage.run().await }
    });
    let catalogue = ObjectiveCatalogue::mitutoyo();
    let optics = Optics::new(&catalogue, catalogue.get("10x")?, Um(2.4));
    let camera = SimCamera::new(
        SimCameraConfig::small(width, height),
        Arc::clone(&clock),
        Arc::clone(&stage),
        Arc::new(RwLock::new(world)),
        optics,
    );
    camera.set_light(0.8, Light::Transmitted);
    // A slow jog in x, so that every frame sees the stage move.
    stage.jog(Axis::X, true, Some(UmPerS(200.0))).await?;

    println!(
        "the simulated MIChrome 20 on a {width} × {height} sensor, over H&E tissue, jogging at 0.2 mm/s; \
         {seconds} s in each mode:"
    );
    println!(
        "  {:<7} {:>4} {:>11} {:>8} {:>7} {:>7} {:>8} {:>9} {:>10}",
        "mode", "bits", "frame", "rated", "frames", "dropped", "rate", "jitter", "position"
    );
    let modes: Vec<_> = camera
        .capabilities()
        .modes
        .iter()
        .flat_map(|m| {
            m.rates
                .iter()
                .map(move |r| (m.name.clone(), r.format, r.fps))
        })
        .collect();
    let mut runs = Vec::new();
    for (mode, format, rated) in modes {
        let mut settings = camera.settings().await?;
        settings.mode = mode.clone();
        settings.format = format;
        settings.exposure = Ms(5.0);
        let settings = camera.configure(settings).await?;
        let (_, w, h) = camera.capabilities().frame_shape(&settings)?;
        let pool = FramePool::new(format.frame_bytes(w, h), 4, &HeapAllocator);
        let (sender, frames) = channel::<FrameMeta>();
        let sink: FrameSink = Arc::new(move |_, meta| {
            let _ = sender.send(meta);
        });
        let began = clock.now();
        camera.start(pool, sink).await?;
        let mut metas = Vec::new();
        while clock.now() < began + Duration::from_secs_f64(seconds) || metas.len() < 2 {
            if let Ok(meta) = frames.recv_timeout(Duration::from_secs(60)) {
                metas.push(meta);
            }
        }
        camera.stop().await?;
        runs.push(measure(
            &stage,
            mode,
            format == oscope_sim::camera::BAYER8,
            (w, h),
            rated,
            &metas,
        ));
    }

    for r in &runs {
        println!(
            "  {:<7} {:>4} {:>11} {:>4.2} fps {:>6} {:>7} {:>4.2} fps {:>6.1} ms {:>7.3} µm",
            r.mode,
            r.bits,
            format!("{} × {}", r.size.0, r.size.1),
            r.rated,
            r.frames,
            r.dropped,
            r.rate,
            r.jitter_ms,
            r.error_um
        );
    }
    println!(
        "(rate counts frames delivered and dropped; position is the track's at each exposure's midpoint against the stage's true one)"
    );

    if smoke {
        for r in &runs {
            ensure!(
                r.frames >= 2,
                "{} {}-bit: {} frames",
                r.mode,
                r.bits,
                r.frames
            );
            ensure!(
                (r.rate / r.rated - 1.0).abs() < 0.02,
                "{} {}-bit: {:.2} fps against {:.2}",
                r.mode,
                r.bits,
                r.rate,
                r.rated
            );
            ensure!(
                r.error_um < 0.5,
                "{} {}-bit: position off by {} µm",
                r.mode,
                r.bits,
                r.error_um
            );
        }
        println!("smoke: ok");
    }
    Ok(())
}

/// One stream's numbers, from its frames' metadata.
fn measure(
    stage: &SimStage,
    mode: String,
    eight: bool,
    size: (u32, u32),
    rated: f64,
    metas: &[FrameMeta],
) -> Run {
    let first = metas.first().map_or(Timestamp(0), |m| m.exposure_start);
    let last = metas.last().map_or(first, |m| m.exposure_start);
    let dropped: u64 = metas.iter().skip(1).map(|m| m.dropped_before).sum();
    // Frames delivered and dropped between the first and the last.
    let slots = (metas.len().saturating_sub(1)) as f64 + dropped as f64;
    let span = last.saturating_since(first).as_secs_f64();
    let rate = if span > 0.0 { slots / span } else { 0.0 };
    let gaps: Vec<f64> = metas
        .windows(2)
        .map(|w| {
            w[1].host_time
                .saturating_since(w[0].host_time)
                .as_secs_f64()
                * 1e3
        })
        .collect();
    let mean = gaps.iter().sum::<f64>() / gaps.len().max(1) as f64;
    let jitter_ms =
        (gaps.iter().map(|g| (g - mean).powi(2)).sum::<f64>() / gaps.len().max(1) as f64).sqrt();
    // What acquisition would record: the position track at the exposure's
    // midpoint, against where the stage truly was.
    let track = stage.track();
    let error_um = metas
        .iter()
        .filter_map(|m| {
            let middle = m.exposure_midpoint();
            let recorded = track.at(middle)?;
            let truth = stage.true_position(middle);
            Some(((recorded.x.0 - truth.x.0).powi(2) + (recorded.y.0 - truth.y.0).powi(2)).sqrt())
        })
        .fold(0.0, f64::max);
    Run {
        mode,
        bits: if eight { 8 } else { 16 },
        size,
        rated,
        frames: metas.len(),
        dropped,
        rate,
        jitter_ms,
        error_um,
    }
}
