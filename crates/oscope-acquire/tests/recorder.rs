//! The recorder: the test sensor's frames at the design rate, every one
//! written and none dropped; every frame published before a stop written;
//! and a recording that can't write, ended without holding the camera up.

use std::future::Future;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use oscope_acquire::{Acquisition, FrameBus, QUEUE, Recorder};
use oscope_core::{
    BayerPattern, Frame, FrameId, FrameMeta, FramePool, HeapAllocator, Ms, PixelFormat,
    SystemClock, Timestamp,
};
use oscope_hal::fake::FakeCamera;
use oscope_hal::{Camera, CameraMode, ModeRate};
use oscope_store::{RecordingOptions, RecordingReader, Scratch, ScratchPolicy};

/// The MIChrome 20's frames at the design rate: 16 bits at 14.5 fps,
/// 580 MB/s.
const DESIGN: PixelFormat = PixelFormat::Bayer16 {
    pattern: BayerPattern::Rggb,
    significant_bits: 12,
};
const WIDTH: u32 = 5472;
const HEIGHT: u32 = 3648;

fn block_on<T>(work: impl Future<Output = T>) -> T {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(work)
}

fn scratch(root: &Path, reserve_bytes: u64) -> Scratch {
    let policy = ScratchPolicy {
        reserve_bytes,
        low_bytes: 0,
    };
    Scratch::open(root, policy).unwrap()
}

fn options(name: &str) -> RecordingOptions {
    RecordingOptions {
        name: name.to_owned(),
        ..RecordingOptions::default()
    }
}

/// Waits until `ready`, for a minute at most.
fn wait_for(mut ready: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + Duration::from_secs(60);
    while !ready() {
        if Instant::now() > deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    true
}

/// The fake camera as the test sensor: the MIChrome 20's frames, at the
/// design rate.
fn test_sensor() -> FakeCamera {
    let mut capabilities = FakeCamera::default_capabilities();
    capabilities.model = "test sensor".to_owned();
    (capabilities.sensor_width, capabilities.sensor_height) = (WIDTH, HEIGHT);
    capabilities.modes = vec![CameraMode {
        name: "design".to_owned(),
        width: WIDTH,
        height: HEIGHT,
        binning: 1,
        rates: vec![ModeRate {
            format: DESIGN,
            fps: 14.5,
        }],
        line_time: Some(Ms(0.019)),
    }];
    FakeCamera::with_capabilities(capabilities, Arc::new(SystemClock::new()))
}

#[test]
fn the_recorder_keeps_up_with_the_test_sensor_at_the_design_rate() {
    let root = tempfile::tempdir().unwrap();
    let scratch = scratch(root.path(), 0);
    let bus = Arc::new(FrameBus::new());
    let acquisition = Acquisition::new(Arc::clone(&bus), Duration::ZERO);
    let recorder = Recorder::start(&bus, &scratch, options("design rate"), QUEUE).unwrap();
    let camera = test_sensor();
    let pool = FramePool::new(DESIGN.frame_bytes(WIDTH, HEIGHT), 24, &HeapAllocator);
    block_on(camera.start(pool, acquisition.sink())).unwrap();
    // Two seconds' frames: 1.2 GB.
    let enough = wait_for(|| recorder.stats().frames >= 29);
    block_on(camera.stop()).unwrap();
    assert!(enough, "{:?}", recorder.stats());
    let stats = recorder.stats();
    let recording = recorder.stop().unwrap();

    // Every frame published was written, none dropped, and the queue never
    // filled, so the camera never waited.
    assert_eq!(recording.frames, bus.published());
    assert_eq!(recording.dropped, 0, "{stats:?}");
    assert_eq!(recording.error, None);
    assert!(stats.queued_max < QUEUE, "{stats:?}");
    let frame_bytes = DESIGN.frame_bytes(WIDTH, HEIGHT) as u64;
    assert_eq!(recording.bytes, recording.frames * frame_bytes);

    // The test pattern, frame by frame: row y of frame n is (y + n) mod 256
    // in every byte.
    let folder = scratch.folder(recording.id).unwrap();
    let reader = RecordingReader::open(&folder).unwrap();
    let frames = reader.frames();
    assert_eq!(frames.len() as u64, recording.frames);
    let row = 2 * WIDTH as usize;
    for (k, entry) in frames.iter().enumerate() {
        assert_eq!(entry.index, k as u64);
        assert_eq!((entry.meta.width, entry.meta.height), (WIDTH, HEIGHT));
        if k % 10 == 0 {
            let pixels = reader.pixels(entry).unwrap();
            for (y, line) in pixels.chunks_exact(row).enumerate() {
                let level = ((y as u64 + entry.meta.id.0) % 256) as u8;
                assert!(line.iter().all(|&b| b == level), "frame {k}, row {y}");
            }
        }
    }
}

/// `count` frames of 2 MB, published one after another from this thread.
fn publish(bus: &FrameBus, count: u64) {
    let pool = FramePool::new(1024 * 1024 * 2, count as usize, &HeapAllocator);
    for n in 0..count {
        let mut buffer = pool.take().unwrap();
        buffer.bytes_mut().fill(n as u8);
        let meta = FrameMeta::new(FrameId(n), PixelFormat::Mono16, 1024, 1024, Timestamp(n));
        bus.publish(Frame::new(buffer, meta).unwrap());
    }
}

#[test]
fn stopping_writes_every_frame_published_before() {
    let root = tempfile::tempdir().unwrap();
    let scratch = scratch(root.path(), 0);
    let bus = FrameBus::new();
    // A queue of four: publishing waits for the recorder, and frames are
    // still queued when it is told to stop.
    let recorder = Recorder::start(&bus, &scratch, options("twenty"), 4).unwrap();
    publish(&bus, 20);
    let recording = recorder.stop().unwrap();
    assert_eq!(recording.frames, 20);

    let reader = RecordingReader::open(&scratch.folder(recording.id).unwrap()).unwrap();
    for (n, entry) in reader.frames().iter().enumerate() {
        assert_eq!((entry.index, entry.meta.id), (n as u64, FrameId(n as u64)));
        assert!(reader.pixels(entry).unwrap().iter().all(|&b| b == n as u8));
    }
}

#[test]
fn a_recording_that_cannot_write_ends_without_holding_the_camera_up() {
    let root = tempfile::tempdir().unwrap();
    // A reserve no disk has: the first segment is refused.
    let scratch = scratch(root.path(), u64::MAX / 4);
    let bus = FrameBus::new();
    let recorder = Recorder::start(&bus, &scratch, options("no room"), 1).unwrap();
    // With a queue of one, these would wait for ever if the recorder held on.
    publish(&bus, 10);
    assert!(wait_for(|| recorder.stats().error.is_some()));
    let error = recorder.stats().error.unwrap();
    assert!(error.contains("too little for a segment"), "{error}");
    let recording = recorder.stop().unwrap();
    assert_eq!((recording.frames, recording.error), (0, Some(error)));
    assert!(recording.ended.is_some());
}
