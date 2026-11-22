//! §5.20's budgets for the simulated camera at full resolution, still,
//! once its tiles are cached: at least 5 frames a second on the CPU, and
//! 14.5 on the GPU. Benchmarks, so ignored unless asked for:
//!
//! ```sh
//! cargo test --release -p oscope-sim --test bench -- --ignored
//! OSCOPE_CUDA=1 cargo test --release -p oscope-sim --features cuda --test bench -- --ignored
//! ```

mod common;

use std::sync::{Arc, PoisonError};
use std::time::{Duration, Instant};

use common::{FOCUS, rig, set, start};
use oscope_core::{Ms, Point2, Timestamp, Um};
use oscope_hal::Camera;
use oscope_sim::{HeTissue, Rate, SimCamera, SimCameraConfig, Slide, World};

/// The median time, in seconds, of eight warm full-resolution frames of the
/// tissue, in mode `design`, after `prepare` has set the camera up.
async fn warm_frame_seconds(prepare: impl FnOnce(&SimCamera)) -> f64 {
    let tissue = HeTissue::new(1, Slide::COVERSLIP);
    let island = tissue.islands()[0];
    let rig = rig(SimCameraConfig::default(), false, island, Rate::Stepped);
    *rig.camera
        .world()
        .write()
        .unwrap_or_else(PoisonError::into_inner) = World::with_slide(
        Slide::new(Arc::new(tissue), Point2::new(0.0, 0.0)),
        Um(FOCUS),
    );
    prepare(&rig.camera);
    set(&rig.camera, |s| {
        s.mode = "design".into();
        s.exposure = Ms(1.0);
    })
    .await;
    let frames = start(&rig.camera).await;
    let interval = Duration::from_secs_f64(1.0 / 14.5);
    let mut times: Vec<f64> = (1..=9_u32)
        .map(|k| {
            let started = Instant::now();
            rig.clock
                .advance_to(Timestamp(0) + interval * k - Duration::from_millis(1));
            frames.recv_timeout(Duration::from_secs(600)).unwrap();
            started.elapsed().as_secs_f64()
        })
        .skip(1)
        .collect();
    rig.camera.stop().await.unwrap();
    times.sort_by(f64::total_cmp);
    times[times.len() / 2]
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "a benchmark: run it in release with --ignored"]
async fn full_resolution_on_the_cpu_keeps_up() {
    let fps = 1.0 / warm_frame_seconds(|_| {}).await;
    println!("CPU: {fps:.1} frames a second at full resolution");
    assert!(fps >= 5.0, "{fps:.1} frames a second, under §5.20's 5");
}

#[cfg(feature = "cuda")]
#[tokio::test(flavor = "multi_thread")]
#[ignore = "a benchmark: run it in release with --ignored, and OSCOPE_CUDA=1"]
async fn full_resolution_on_the_gpu_keeps_up() {
    if std::env::var("OSCOPE_CUDA").as_deref() != Ok("1") {
        eprintln!("skipped: set OSCOPE_CUDA=1 to run the tests that need a GPU");
        return;
    }
    let cuda = oscope_compute::cuda::Cuda::open(0).unwrap();
    let fps = 1.0 / warm_frame_seconds(|camera| camera.use_gpu(cuda, 512).unwrap()).await;
    println!("GPU: {fps:.1} frames a second at full resolution");
    assert!(fps >= 14.5, "{fps:.1} frames a second, under §5.20's 14.5");
}
