//! Acquisition: a frame's position from the stage's track at its exposure's
//! midpoint, waiting a bounded time for a report after it; and the
//! objective and lights it records.

use std::sync::Arc;
use std::time::{Duration, Instant};

use oscope_acquire::{Acquisition, FrameBus};
use oscope_core::{
    FrameId, FrameMeta, FramePool, HeapAllocator, LightState, Ms, PixelFormat, Timestamp, Um, Xyz,
};
use oscope_hal::{PositionSample, PositionTrack};

/// A 4 × 4 frame whose 1 ms exposure starts at `start` ms: its midpoint is
/// half a millisecond later.
fn exposed(pool: &FramePool, start_ms: u64) -> (oscope_core::FrameBuffer, FrameMeta) {
    let buffer = pool.take().unwrap();
    let mut meta = FrameMeta::new(FrameId(start_ms), PixelFormat::Mono8, 4, 4, Timestamp(0));
    meta.exposure_start = Timestamp(start_ms * 1_000_000);
    meta.exposure = Ms(1.0);
    (buffer, meta)
}

fn sample(ms: u64, x: f64) -> PositionSample {
    PositionSample {
        time: Timestamp(ms * 1_000_000),
        position: Xyz::new(Um(x), Um(0.0), Um(0.0)),
    }
}

#[test]
fn a_frame_gets_the_position_at_its_exposure_s_midpoint() {
    let bus = Arc::new(FrameBus::new());
    let mut frames = bus.subscribe_latest("test");
    let acquisition = Acquisition::new(Arc::clone(&bus), Duration::from_millis(200));
    let track = Arc::new(PositionTrack::new(16));
    acquisition.set_track(Some(Arc::clone(&track)));
    acquisition.set_objective(Some("20x".to_owned()));
    let light = LightState {
        channel: "transmitted".to_owned(),
        on: true,
        intensity: 0.5,
    };
    acquisition.set_lights(vec![light.clone()]);
    let pool = FramePool::new(16, 4, &HeapAllocator);
    let sink = acquisition.sink();

    // Reports at 10 and 20 ms, from x = 100 to 200 µm: a frame exposed from
    // 14 to 15 ms is placed at 14.5 ms, x = 145 µm.
    track.push(sample(10, 100.0));
    track.push(sample(20, 200.0));
    let (buffer, meta) = exposed(&pool, 14);
    sink(buffer, meta);
    let frame = frames.try_recv().unwrap();
    let meta = frame.meta();
    assert!((meta.position.unwrap().x.0 - 145.0).abs() < 1e-9);
    assert_eq!(
        (meta.objective.as_deref(), meta.lights.clone()),
        (Some("20x"), vec![light])
    );

    // A frame after the last report waits for the next: here it comes
    // 50 ms later, from another thread.
    let (buffer, meta) = exposed(&pool, 24);
    let later = Arc::clone(&track);
    let reporter = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(50));
        later.push(sample(30, 300.0));
    });
    sink(buffer, meta);
    reporter.join().unwrap();
    assert!((frames.try_recv().unwrap().meta().position.unwrap().x.0 - 245.0).abs() < 1e-9);
    assert_eq!(acquisition.counts(), (2, 0, 0));
}

#[test]
fn without_a_report_in_time_a_frame_has_no_position() {
    // No report after the midpoint: the sink waits its 100 ms, no more, and
    // publishes the frame without a position, counted.
    let bus = Arc::new(FrameBus::new());
    let mut frames = bus.subscribe_latest("test");
    let acquisition = Acquisition::new(Arc::clone(&bus), Duration::from_millis(100));
    let track = Arc::new(PositionTrack::new(16));
    track.push(sample(10, 100.0));
    acquisition.set_track(Some(track));
    let pool = FramePool::new(16, 4, &HeapAllocator);
    let (buffer, meta) = exposed(&pool, 14);
    let started = Instant::now();
    acquisition.sink()(buffer, meta);
    let waited = started.elapsed();
    assert!(
        waited >= Duration::from_millis(100) && waited < Duration::from_secs(5),
        "{waited:?}"
    );
    assert_eq!(frames.try_recv().unwrap().meta().position, None);
    // With no track at all, there's no wait.
    acquisition.set_track(None);
    let (buffer, meta) = exposed(&pool, 15);
    let started = Instant::now();
    acquisition.sink()(buffer, meta);
    assert!(started.elapsed() < Duration::from_millis(100));
    assert_eq!(acquisition.counts(), (2, 2, 0));
}
