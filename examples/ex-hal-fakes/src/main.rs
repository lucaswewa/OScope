//! `ex-hal-fakes`: the HAL's scripted fakes. It prints their capability
//! reports, then takes frames from the fake camera, moves the fake stage and
//! reads its track, sets the lights and fits an objective. Then it runs the
//! conformance suite on each fake, and on a camera scripted to fail. With
//! `--smoke`, it also checks what it prints. See the README.

use std::process::ExitCode;
use std::sync::Arc;

use anyhow::Context;
use oscope_core::{
    BayerPattern, Clock, FramePool, HeapAllocator, LightState, PixelFormat, SystemClock, Timestamp,
    Um, Xyz,
};
use oscope_hal::conformance;
use oscope_hal::fake::{FakeCamera, FakeIllumination, FakeObjectiveMount, FakeStage};
use oscope_hal::{
    Camera, Error, FrameSink, Illumination, ObjectiveCatalogue, ObjectiveMount, Stage,
};
use tokio::sync::mpsc::unbounded_channel;

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    match run().await {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(error) => {
            eprintln!("ex-hal-fakes: {error:#}");
            ExitCode::FAILURE
        }
    }
}

/// Runs the example; `false` if a `--smoke` check failed.
async fn run() -> anyhow::Result<bool> {
    let smoke = std::env::args().any(|a| a == "--smoke");
    let mut failures = Vec::new();
    let mut check = |ok: bool, what: &str| {
        if !ok {
            failures.push(what.to_owned());
        }
    };
    let clock: Arc<dyn Clock> = Arc::new(SystemClock::new());
    let camera = FakeCamera::new(Arc::clone(&clock));
    let stage = FakeStage::new(Arc::clone(&clock));
    let lights = FakeIllumination::two_lights();
    let mount = FakeObjectiveMount::new(ObjectiveCatalogue::mitutoyo());

    println!("Capability reports");
    // Each report in its fields' own order.
    let reports = [
        (
            "camera",
            serde_json::to_string_pretty(camera.capabilities())?,
        ),
        ("stage", serde_json::to_string_pretty(stage.capabilities())?),
        (
            "lights",
            serde_json::to_string_pretty(lights.capabilities())?,
        ),
        (
            "objective mount",
            serde_json::to_string_pretty(mount.capabilities())?,
        ),
    ];
    for (device, report) in &reports {
        println!("\n{device}: {report}");
    }

    println!("\nThe camera: five frames of moving stripes, in 16 bits");
    let mut settings = camera.settings().await?;
    settings.format = PixelFormat::Bayer16 {
        pattern: BayerPattern::Rggb,
        significant_bits: 16,
    };
    let settings = camera.configure(settings).await?;
    let (_, width, height) = camera.capabilities().frame_shape(&settings)?;
    let pool = FramePool::new(
        settings.format.frame_bytes(width, height),
        4,
        &HeapAllocator,
    );
    let (sender, mut frames) = unbounded_channel();
    let sink: FrameSink = Arc::new(move |buffer, meta| {
        let _ = sender.send((buffer, meta));
    });
    camera.start(pool, sink).await?;
    let mut ids = Vec::new();
    for _ in 0..5 {
        let (buffer, meta) = frames.recv().await.context("a frame")?;
        let first = u16::from_le_bytes([buffer.bytes()[0], buffer.bytes()[1]]);
        println!(
            "  {}: {} × {}, mode {}, exposed {} from {}, its first row at {first}",
            meta.id, meta.width, meta.height, meta.mode, meta.exposure, meta.exposure_start
        );
        ids.push(meta.id.0);
    }
    camera.stop().await?;
    check(ids == [0, 1, 2, 3, 4], "the camera's frames");

    println!("\nThe stage: homed, then two moves, and its track between them");
    let homed = stage.home().await?;
    let first = stage
        .move_to(
            Xyz::new(Um::from_mm(10.0), Um::from_mm(20.0), Um::from_mm(5.0)),
            None,
        )
        .await?;
    let second = stage
        .move_by(Xyz::new(Um::from_mm(1.0), Um(0.0), Um(-500.0)), None)
        .await?;
    for (what, sample) in [("homed", homed), ("moved", first), ("moved by", second)] {
        let p = sample.position;
        println!(
            "  {what:<8} at {}: x {}, y {}, z {}",
            sample.time, p.x, p.y, p.z
        );
    }
    let midway = Timestamp((first.time.0 + second.time.0) / 2);
    let between = stage
        .track()
        .at(midway)
        .context("the track at the midpoint")?;
    println!(
        "  at {midway}, halfway between the moves' reports, the track says x {}, z {}",
        between.x, between.z
    );
    let halfway = |a: Um, b: Um| Um((a.0 + b.0) / 2.0);
    let expected_x = halfway(first.position.x, second.position.x);
    check(
        (between.x.0 - expected_x.0).abs() < 1.0,
        "the track at the midpoint",
    );

    println!("\nThe lights and the objective");
    let set = |channel: &str, intensity: f64| LightState {
        channel: channel.to_owned(),
        on: true,
        intensity,
    };
    let transmitted = lights.set(set("transmitted", 0.6)).await?;
    let ring = lights.set(set("ring", 0.3)).await?;
    println!(
        "  transmitted asked at 0.6, set at {}",
        transmitted.intensity
    );
    println!(
        "  ring asked at 0.3, set at {}: it can't dim",
        ring.intensity
    );
    let fitted = mount.fit("20x".into()).await?;
    println!("  objective turned to {}: {}", fitted.name, fitted.model);
    check(ring.intensity == 1.0, "the ring light at full intensity");

    println!("\nThe conformance suite");
    let results = [
        ("camera", conformance::camera(&camera).await),
        ("stage", conformance::stage(&stage, Um(0.0)).await),
        ("lights", conformance::illumination(&lights).await),
        (
            "objective mount",
            conformance::objective_mount(&mount).await,
        ),
    ];
    for (device, result) in &results {
        match result {
            Ok(()) => println!("  {device}: passed"),
            Err(failure) => println!("  {device}: FAILED at {failure}"),
        }
    }
    check(results.iter().all(|(_, r)| r.is_ok()), "the fakes conform");

    let broken = FakeCamera::new(Arc::clone(&clock));
    broken.script().fail_next("start", Error::Disconnected);
    let caught = conformance::camera(&broken).await;
    match &caught {
        Ok(()) => println!("  a camera whose start fails: passed, wrongly"),
        Err(failure) => println!("  a camera whose start fails: caught at {failure}"),
    }
    check(
        caught.is_err_and(|f| f.check == "start streaming"),
        "a scripted failure caught",
    );
    println!(
        "  that camera was asked: {}",
        broken.script().calls().join(", ")
    );

    if !smoke {
        return Ok(true);
    }
    if !failures.is_empty() {
        eprintln!("\nsmoke: failed: {}", failures.join(", "));
        return Ok(false);
    }
    println!("\nsmoke: ok");
    Ok(true)
}
