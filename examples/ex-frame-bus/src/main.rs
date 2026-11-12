//! A synthetic MIChrome 20 feeding the frame bus: 20 MP RGGB mosaics, 40 MB
//! each, at 14.5 fps, from a pool of 24 slabs. Three subscribers read them:
//!
//! - a lossless **recorder**, which checks it gets every frame in order;
//! - a slow latest-wins **preview**, which takes 200 ms per frame;
//! - a latest-wins **statistics** task, which reads a 512 × 512 ROI at up to
//!   10 Hz.
//!
//! Every second it prints the frame rate, throughput, each subscriber's counts
//! and the pool's use. `--seconds N` sets the run's length (10 by default);
//! `--smoke` runs for 3 s and checks the results.

use std::process::ExitCode;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use oscope_acquire::{FrameBus, WhenFull};
use oscope_core::{
    BayerPattern, Clock, Frame, FrameId, FrameMeta, FramePool, HeapAllocator, Ms, PixelFormat,
    PixelRect, SystemClock,
};

const WIDTH: u32 = 5472;
const HEIGHT: u32 = 3648;
const FPS: f64 = 14.5;
const FORMAT: PixelFormat = PixelFormat::Bayer16 {
    pattern: BayerPattern::Rggb,
    significant_bits: 16,
};

/// Makes `count` frames at 14.5 fps, writing every byte of each as a camera
/// would. A frame for which the pool has no slab is lost, and the pool counts
/// it.
fn produce(pool: FramePool, bus: FrameBus, clock: Arc<dyn Clock>, count: u64) {
    let (start, interval) = (Instant::now(), Duration::from_secs_f64(1.0 / FPS));
    for n in 0..count {
        thread::sleep((start + interval * n as u32).saturating_duration_since(Instant::now()));
        let Some(mut buffer) = pool.take() else {
            continue;
        };
        let exposure_start = clock.now();
        buffer.bytes_mut().fill(n as u8);
        let mut meta = FrameMeta::new(FrameId(n), FORMAT, WIDTH, HEIGHT, clock.now());
        meta.exposure_start = exposure_start;
        meta.exposure = Ms(5.0);
        meta.line_time = Some(Ms(0.0164));
        meta.mode = "5472x3648".into();
        bus.publish(Frame::new(buffer, meta).expect("a slab holds a frame"));
    }
}

#[tokio::main]
async fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let smoke = args.iter().any(|a| a == "--smoke");
    let seconds: u64 = match args.iter().position(|a| a == "--seconds") {
        Some(i) => args.get(i + 1).and_then(|s| s.parse().ok()).unwrap_or(10),
        None if smoke => 3,
        None => 10,
    };
    let pool = FramePool::new(FORMAT.frame_bytes(WIDTH, HEIGHT), 24, &HeapAllocator);
    let bus = FrameBus::new();
    let clock: Arc<dyn Clock> = Arc::new(SystemClock::new());

    let mut recorder = bus.subscribe_lossless("recorder", 8, WhenFull::Wait);
    let recording = tokio::spawn(async move {
        let (mut received, mut in_order, mut next) = (0u64, true, 0u64);
        while let Some(frame) = recorder.recv().await {
            in_order &= frame.meta().id.0 >= next;
            next = frame.meta().id.0 + 1;
            received += 1;
        }
        (received, in_order)
    });
    let mut preview = bus.subscribe_latest("preview");
    let previewing = tokio::spawn(async move {
        while preview.recv().await.is_some() {
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    });
    let mut statistics = bus.subscribe_latest("statistics");
    let measuring = tokio::spawn(async move {
        let mut mean = 0.0;
        while let Some(frame) = statistics.recv().await {
            let roi = frame
                .view()
                .roi(PixelRect::new(2480, 1568, 512, 512))
                .expect("inside the sensor");
            let sum: u64 = (0..512)
                .flat_map(|y| (0..512).map(move |x| (x, y)))
                .filter_map(|(x, y)| roi.sample(x, y, 0))
                .map(u64::from)
                .sum();
            mean = sum as f64 / (512.0 * 512.0);
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        mean
    });

    let count = (seconds as f64 * FPS).round() as u64;
    println!(
        "{count} frames of {WIDTH} x {HEIGHT} RGGB16 ({:.1} MB each) at {FPS} fps, from a pool of 24",
        FORMAT.frame_bytes(WIDTH, HEIGHT) as f64 / 1e6
    );
    let started = Instant::now();
    let producer = thread::spawn({
        let (pool, bus, clock) = (pool.clone(), bus.clone(), Arc::clone(&clock));
        move || produce(pool, bus, clock, count)
    });
    while !producer.is_finished() {
        tokio::time::sleep(Duration::from_secs(1)).await;
        let elapsed = started.elapsed().as_secs_f64();
        let fps = bus.published() as f64 / elapsed;
        let counts: Vec<String> = bus
            .subscribers()
            .iter()
            .map(|s| format!("{} {}/{} dropped", s.name, s.delivered, s.dropped))
            .collect();
        let pool_stats = pool.stats();
        println!(
            "{elapsed:5.1} s  {fps:5.2} fps  {:6.0} MB/s  {}  pool {}/{} in use, {} exhausted",
            fps * FORMAT.frame_bytes(WIDTH, HEIGHT) as f64 / 1e6,
            counts.join(", "),
            pool_stats.in_use,
            pool_stats.slabs,
            pool_stats.exhausted
        );
    }
    producer.join().expect("the producer finished");
    let elapsed = started.elapsed().as_secs_f64();
    let (published, exhausted) = (bus.published(), pool.stats().exhausted);
    let subscribers = bus.subscribers();
    drop(bus); // ends every subscription
    let (recorded, in_order) = recording.await.expect("the recorder finished");
    previewing.await.expect("the preview finished");
    let mean = measuring.await.expect("the statistics finished");
    let preview = subscribers
        .iter()
        .find(|s| s.name == "preview")
        .expect("subscribed");
    let rate = published as f64 / elapsed;
    println!(
        "published {published} in {elapsed:.1} s ({rate:.2} fps); recorder {recorded} in order: {in_order}; preview dropped {}; pool exhausted {exhausted}; last mean {mean:.0}",
        preview.dropped
    );

    if smoke {
        let checks = [
            (
                recorded == published && in_order,
                "the recorder got every frame, in order",
            ),
            (exhausted == 0, "the pool never ran out"),
            (preview.dropped > 0, "the slow preview dropped frames"),
            (rate > 0.9 * FPS, "the rate held at 14.5 fps"),
        ];
        let failed: Vec<&str> = checks
            .iter()
            .filter(|(ok, _)| !ok)
            .map(|(_, what)| *what)
            .collect();
        if !failed.is_empty() {
            eprintln!("smoke: failed: {}", failed.join("; "));
            return ExitCode::FAILURE;
        }
        println!("smoke: ok");
    }
    ExitCode::SUCCESS
}
