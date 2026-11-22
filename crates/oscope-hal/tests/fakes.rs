//! The fakes' own behaviour: the camera's frames and their pattern, its
//! refusals while streaming, its software trigger, its schedule and lost
//! frames; the stage's alarm and homing; and the scripts.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use oscope_core::{
    BayerPattern, Clock, FakeClock, FrameBuffer, FrameMeta, FramePool, HeapAllocator, PixelFormat,
    SystemClock, Um, Xyz,
};
use oscope_hal::fake::{FakeCamera, FakeStage};
use oscope_hal::{Camera, CameraSettings, Error, FrameSink, Stage, StageStatus, Trigger};
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};

const BAYER16: PixelFormat = PixelFormat::Bayer16 {
    pattern: BayerPattern::Rggb,
    significant_bits: 16,
};

/// A sink into a channel.
fn tap() -> (FrameSink, UnboundedReceiver<(FrameBuffer, FrameMeta)>) {
    let (sender, frames) = unbounded_channel();
    let sink: FrameSink = Arc::new(move |buffer, meta| {
        let _ = sender.send((buffer, meta));
    });
    (sink, frames)
}

fn camera() -> FakeCamera {
    FakeCamera::new(Arc::new(SystemClock::new()))
}

async fn set(camera: &FakeCamera, change: impl FnOnce(&mut CameraSettings)) {
    let mut settings = camera.settings().await.unwrap();
    change(&mut settings);
    camera.configure(settings).await.unwrap();
}

#[tokio::test]
async fn the_camera_streams_moving_stripes() {
    let camera = camera();
    set(&camera, |s| s.format = BAYER16).await;
    let (sink, mut frames) = tap();
    let pool = FramePool::new(640 * 480 * 2, 4, &HeapAllocator);
    camera.start(pool, sink).await.unwrap();
    for n in 0..3_u16 {
        let (buffer, meta) = frames.recv().await.unwrap();
        assert_eq!(
            (meta.id.0, meta.width, meta.height),
            (u64::from(n), 640, 480)
        );
        assert_eq!(meta.mode, "full");
        // Row y of frame n holds (y + n) mod 256, times 257.
        let row = 640 * 2;
        for y in [0_u16, 1, 300] {
            let at = usize::from(y) * row;
            let level = (y + n) % 256 * 257;
            assert_eq!(buffer.bytes()[at..at + 2], level.to_le_bytes());
        }
    }
    camera.stop().await.unwrap();
    // The thread has ended and dropped the sink, so the channel closes.
    while frames.recv().await.is_some() {}
    assert_eq!(
        camera.script().calls(),
        ["settings", "configure", "start", "stop"]
    );
}

#[tokio::test]
async fn while_streaming_the_frame_can_t_change_but_the_exposure_can() {
    let camera = camera();
    let (sink, _frames) = tap();
    let pool = FramePool::new(640 * 480, 2, &HeapAllocator);
    camera.start(pool.clone(), Arc::clone(&sink)).await.unwrap();
    let error = camera.start(pool, sink).await.unwrap_err();
    assert!(matches!(error, Error::Busy { .. }));
    let mut settings = camera.settings().await.unwrap();
    settings.mode = "bin2".into();
    assert!(matches!(
        camera.configure(settings.clone()).await,
        Err(Error::Busy { .. })
    ));
    settings.mode = "full".into();
    settings.exposure.0 = 3.0;
    assert_eq!(camera.configure(settings).await.unwrap().exposure.0, 3.0);
    camera.stop().await.unwrap();
    assert!(!camera.streaming());
}

#[tokio::test]
async fn a_software_trigger_gives_one_frame_each() {
    let camera = camera();
    assert!(matches!(camera.trigger().await, Err(Error::Busy { .. })));
    set(&camera, |s| s.trigger = Trigger::Software).await;
    let (sink, mut frames) = tap();
    let pool = FramePool::new(640 * 480, 4, &HeapAllocator);
    camera.start(pool, sink).await.unwrap();
    for n in 0..3 {
        camera.trigger().await.unwrap();
        assert_eq!(frames.recv().await.unwrap().1.id.0, n);
    }
    camera.stop().await.unwrap();
    assert!(frames.recv().await.is_none(), "a frame without a trigger");
}

#[tokio::test]
async fn a_free_running_camera_keeps_its_schedule_losing_frames_while_held() {
    // 100 fps in 8 bits, a frame each 10 ms. The sink holds the camera's
    // thread for 55 ms at the third frame, as a lossless subscriber can.
    let camera = camera();
    let (sender, mut frames) = unbounded_channel();
    let count = AtomicU64::new(0);
    let sink: FrameSink = Arc::new(move |buffer, meta| {
        if count.fetch_add(1, Ordering::Relaxed) == 2 {
            std::thread::sleep(Duration::from_millis(55));
        }
        let _ = sender.send((buffer, meta));
    });
    camera
        .start(FramePool::new(640 * 480, 8, &HeapAllocator), sink)
        .await
        .unwrap();
    let mut metas = Vec::new();
    while metas.len() < 4 {
        let (_, meta) = frames.recv().await.unwrap();
        metas.push(meta);
    }
    camera.stop().await.unwrap();
    // The frames whose time passed meanwhile are lost, and counted.
    assert!(metas[3].dropped_before >= 3, "{:?}", metas[3]);
}

#[tokio::test]
async fn frames_lost_to_a_full_pool_are_counted() {
    let camera = camera();
    let (sink, mut frames) = tap();
    let pool = FramePool::new(640 * 480, 1, &HeapAllocator);
    camera.start(pool.clone(), sink).await.unwrap();
    let held = frames.recv().await.unwrap();
    while pool.stats().exhausted < 3 {
        tokio::task::yield_now().await;
    }
    drop(held);
    let (_, meta) = frames.recv().await.unwrap();
    assert!(meta.dropped_before >= 3, "{}", meta.dropped_before);
    camera.stop().await.unwrap();

    // A slab too small for the frame is refused.
    let (sink, _frames) = tap();
    let small = FramePool::new(1_000, 1, &HeapAllocator);
    assert!(matches!(
        camera.start(small, sink).await,
        Err(Error::OutOfRange { .. })
    ));
}

#[tokio::test]
async fn the_stage_stops_for_an_alarm_until_cleared_or_homed() {
    let clock = Arc::new(FakeClock::new(SystemClock::new().start_utc()));
    let stage = FakeStage::new(clock.clone());
    let target = Xyz::new(Um(5_000.0), Um(6_000.0), Um(7_000.0));
    clock.advance(std::time::Duration::from_millis(10));
    let reached = stage.move_to(target, None).await.unwrap();
    assert_eq!((reached.position, reached.time), (target, clock.now()));
    assert_eq!(stage.track().len(), 2);

    stage.raise_alarm("hard limit");
    let error = stage.move_to(target, None).await.unwrap_err();
    assert_eq!(error.to_string(), "the stage is in alarm: hard limit");
    stage.clear_alarm().await.unwrap();
    stage.move_to(target, None).await.unwrap();

    stage.jog(oscope_hal::Axis::Y, false, None).await.unwrap();
    assert_eq!(stage.status().await.unwrap(), StageStatus::Moving);
    stage.raise_alarm("hard limit");
    let homed = stage.home().await.unwrap();
    assert_eq!(homed.position, Xyz::new(Um(0.0), Um(0.0), Um(0.0)));
    assert_eq!(stage.status().await.unwrap(), StageStatus::Idle);
}
