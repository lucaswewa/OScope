//! The camera's exposure on the GPU against its CPU twin, with
//! `OSCOPE_CUDA=1` and the `cuda` feature: frames without noise within the
//! bounds half-precision tiles allow (ADR-0062), and noise of the same
//! statistics.

#![cfg(feature = "cuda")]

mod common;

use std::sync::mpsc::Receiver;
use std::sync::{Arc, PoisonError};
use std::time::Duration;

use common::{FOCUS, rig, set, start};
use oscope_compute::cuda::Cuda;
use oscope_core::{FrameBuffer, FrameMeta, Ms, PixelRect, Point2, Timestamp, Um};
use oscope_hal::{Axis, Camera, Stage, UmPerS};
use oscope_sim::{
    HeTissue, Light, Mounting, Rate, SensorModel, SimCameraConfig, Slide, TestTarget, World,
};

/// The GPU, when `OSCOPE_CUDA=1` asks for the tests that need one.
fn gpu() -> Option<Cuda> {
    if std::env::var("OSCOPE_CUDA").as_deref() != Ok("1") {
        eprintln!("skipped: set OSCOPE_CUDA=1 to run the tests that need a GPU");
        return None;
    }
    Some(Cuda::open(0).expect("OSCOPE_CUDA=1, but no GPU"))
}

/// What one camera, on the GPU or not, shows of the tissue in `mode`, with
/// `roi`, through `mounting`: two frames, the stage jogging during the
/// second if `jog`; and whether it was still on the GPU at the end.
async fn frames(
    config: SimCameraConfig,
    cuda: Option<Cuda>,
    mode: &str,
    roi: Option<PixelRect>,
    jog: bool,
) -> (Vec<Vec<u16>>, bool) {
    let tissue = HeTissue::new(1, Slide::COVERSLIP);
    let island = tissue.islands()[0];
    let rig = rig(config, false, island, Rate::Stepped);
    *rig.camera
        .world()
        .write()
        .unwrap_or_else(PoisonError::into_inner) = World::with_slide(
        Slide::new(Arc::new(tissue), Point2::new(0.0, 0.0)),
        Um(FOCUS),
    );
    if let Some(cuda) = cuda {
        rig.camera.use_gpu(cuda, 256).unwrap();
    }
    let settings = set(&rig.camera, |s| {
        s.mode = mode.into();
        s.roi = roi;
        s.exposure = Ms(5.0);
    })
    .await;
    let rate = rig
        .camera
        .capabilities()
        .mode(&settings.mode)
        .unwrap()
        .fps(settings.format)
        .unwrap();
    let stream = start(&rig.camera).await;
    let mut out = Vec::new();
    for k in 0..2_u32 {
        if k == 1 && jog {
            rig.stage
                .jog(Axis::X, true, Some(UmPerS(300.0)))
                .await
                .unwrap();
        }
        let at = Timestamp(0) + Duration::from_secs_f64(f64::from(k + 1) / rate)
            - Duration::from_millis(1);
        rig.clock.advance_to(at);
        let (buffer, meta) = stream.recv_timeout(Duration::from_secs(120)).unwrap();
        out.push(
            buffer.bytes()[..(meta.width * meta.height * 2) as usize]
                .chunks(2)
                .map(|b| u16::from_le_bytes([b[0], b[1]]) >> 4)
                .collect(),
        );
    }
    let on_gpu = rig.camera.on_gpu();
    rig.camera.stop().await.unwrap();
    (out, on_gpu)
}

#[tokio::test(flavor = "multi_thread")]
async fn the_gpu_exposes_frames_as_the_cpu_does() {
    let Some(_) = gpu() else { return };
    // Without noise: at full resolution, still; turned, binned 2 × 2 with
    // an ROI, the stage jogging; and turned the other way and mirrored,
    // binned 3 × 3. A mirror makes the mounting's cross terms equal, so
    // both are needed to tell them apart.
    let mut quiet = SimCameraConfig::small(768, 576);
    quiet.sensor.noise = false;
    let mounted = |rotation: f64, mirrored: bool| {
        let mut config = quiet.clone();
        config.mounting = Mounting {
            rotation,
            mirrored,
            offset: Point2::new(3.0, -2.0),
        };
        config
    };
    let roi = Some(PixelRect::new(96, 48, 480, 384));
    // And vignetted, through a tinted colour response (P33), binned in a
    // region away from the sensor's middle.
    let mut vignetted = mounted(0.4, false);
    vignetted.vignetting = 0.3;
    vignetted.response = [[0.9, 0.1, 0.0], [0.05, 0.75, 0.05], [0.0, 0.1, 0.5]];
    let cases = [
        (quiet.clone(), "full", None, false),
        (mounted(0.4, false), "bin2", roi, true),
        (mounted(-0.7, true), "bin3", None, true),
        (vignetted, "bin2", roi, false),
    ];
    let mut failed = Vec::new();
    for (config, mode, roi, jog) in cases {
        let (cpu, _) = frames(config.clone(), None, mode, roi, jog).await;
        let (gpu, on_gpu) = frames(config, gpu(), mode, roi, jog).await;
        assert!(on_gpu, "{mode}: the camera fell back to the CPU");
        for (k, (a, b)) in cpu.iter().zip(&gpu).enumerate() {
            match within_half_precision(a, b) {
                Ok(report) => println!("{mode}, frame {k}: {report}"),
                Err(report) => failed.push(format!("{mode}, frame {k}: {report}")),
            }
        }
    }
    assert!(failed.is_empty(), "{failed:#?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn the_gpu_s_noise_has_the_cpu_s_statistics() {
    let Some(_) = gpu() else { return };
    // The empty stage for 5 ms, on both: at 0.3 of full light, 1,200
    // electrons a pixel; and dark. Only the sensor's noise.
    let stats = |v: &[u16]| {
        let m = v.iter().map(|x| f64::from(*x)).sum::<f64>() / v.len() as f64;
        let var = v.iter().map(|x| (f64::from(*x) - m).powi(2)).sum::<f64>() / (v.len() - 1) as f64;
        (m, var)
    };
    // In light, shot noise dominates; dark, at a gain of 240, read noise.
    for (light, gain) in [(0.3, 0.0), (0.0, 240.0)] {
        let mut results = Vec::new();
        for cuda in [None, gpu()] {
            let rig = rig(
                SimCameraConfig::small(768, 576),
                false,
                Point2::new(0.0, 0.0),
                Rate::Stepped,
            );
            let wanted = cuda.is_some();
            if let Some(cuda) = cuda {
                rig.camera.use_gpu(cuda, 64).unwrap();
            }
            rig.camera.set_light(light, Light::Transmitted);
            set(&rig.camera, |s| {
                s.exposure = Ms(5.0);
                s.gain = gain;
            })
            .await;
            let stream = start(&rig.camera).await;
            rig.clock
                .advance_to(Timestamp(0) + Duration::from_millis(66));
            let (buffer, meta) = stream.recv_timeout(Duration::from_secs(60)).unwrap();
            let counts: Vec<u16> = buffer.bytes()[..(meta.width * meta.height * 2) as usize]
                .chunks(2)
                .map(|b| u16::from_le_bytes([b[0], b[1]]) >> 4)
                .collect();
            assert_eq!(rig.camera.on_gpu(), wanted);
            rig.camera.stop().await.unwrap();
            results.push(stats(&counts));
        }
        let s = SensorModel::default();
        let cpe = s.counts_per_electron(gain);
        let electrons = light * s.electrons_per_ms * 5.0 + s.dark_current * 0.005;
        let mean = f64::from(s.black) + electrons * cpe;
        let variance = (electrons + s.read_noise * s.read_noise) * cpe * cpe + 1.0 / 12.0;
        for (m, v) in results {
            assert!((m - mean).abs() < 0.3, "light {light}: {m} against {mean}");
            assert!(
                (v / variance - 1.0).abs() < 0.03,
                "light {light}: {v} against {variance}"
            );
        }
    }
}

/// Whether frame `b`, from GPU tiles in half precision, is frame `a`, from
/// the CPU's, within the bounds that allows. A tile's sample in half
/// precision is within 2^-12 of itself, so a pixel's count moves by at most
/// 2^-12 of its counts above black, and the ADC's rounding tips it a count
/// with at most that chance. So no pixel differs by more than 2 counts
/// (P15's one, and up to 2^-12 of 4095 more); the share of pixels that
/// differ and the mean difference are each at most 2^-12 of the mean
/// counts above black, with P15's 0.1%; and the differences have no bias.
/// The report says how near it came, or why it failed.
fn within_half_precision(a: &[u16], b: &[u16]) -> Result<String, String> {
    assert_eq!(a.len(), b.len());
    let n = a.len() as f64;
    let worst = a
        .iter()
        .zip(b)
        .map(|(x, y)| x.abs_diff(*y))
        .max()
        .unwrap_or(0);
    let differ = a.iter().zip(b).filter(|(x, y)| x != y).count();
    let delta = |(x, y): (&u16, &u16)| f64::from(*y) - f64::from(*x);
    let mean = a.iter().zip(b).map(delta).sum::<f64>() / n;
    let mean_abs = a.iter().zip(b).map(|p| delta(p).abs()).sum::<f64>() / n;
    let black = f64::from(SensorModel::default().black);
    let signal = a
        .iter()
        .map(|x| (f64::from(*x) - black).max(0.0))
        .sum::<f64>()
        / n;
    let bound = signal / 4096.0 + 0.001;
    let report = format!(
        "{differ} of {} differ, by up to {worst}; mean {mean:+.3}, mean |Δ| {mean_abs:.3}, \
         against {bound:.3} for {signal:.0} counts",
        a.len()
    );
    let share = differ as f64 / n;
    match worst > 2 || share > bound || mean_abs > bound || mean.abs() > 0.05 {
        true => Err(report),
        false => Ok(report),
    }
}

/// A camera of the scaled-down sensor over `world` at `at`, without noise
/// unless `noise`, in `light`, on the GPU if `cuda` with a budget of
/// `budget` tiles; its rig, its stream, and the frame rate of mode `full`.
async fn camera(
    world: World,
    at: Point2,
    light: Light,
    cuda: Option<Cuda>,
    budget: usize,
    noise: bool,
) -> (common::Rig, Receiver<(FrameBuffer, FrameMeta)>, f64) {
    let mut config = SimCameraConfig::small(768, 576);
    config.sensor.noise = noise;
    let rig = common::rig_over(config, world, at, Rate::Stepped);
    if let Some(cuda) = cuda {
        rig.camera.use_gpu(cuda, 256).unwrap();
        rig.camera.set_tile_budget(budget);
    }
    rig.camera.set_light(1.0, light);
    let settings = set(&rig.camera, |s| s.exposure = Ms(5.0)).await;
    let rate = rig
        .camera
        .capabilities()
        .mode(&settings.mode)
        .unwrap()
        .fps(settings.format)
        .unwrap();
    let stream = start(&rig.camera).await;
    (rig, stream, rate)
}

/// Frame `k` of a stream at `rate`, from 1, as 12-bit counts.
fn frame(
    rig: &common::Rig,
    stream: &Receiver<(FrameBuffer, FrameMeta)>,
    rate: f64,
    k: u32,
) -> Vec<u16> {
    let at = Timestamp(0) + Duration::from_secs_f64(f64::from(k) / rate) - Duration::from_millis(1);
    rig.clock.advance_to(at);
    let (buffer, meta) = stream.recv_timeout(Duration::from_secs(120)).unwrap();
    buffer.bytes()[..(meta.width * meta.height * 2) as usize]
        .chunks(2)
        .map(|b| u16::from_le_bytes([b[0], b[1]]) >> 4)
        .collect()
}

/// The section, and where its first island is.
fn section() -> (World, Point2) {
    let tissue = HeTissue::new(1, Slide::COVERSLIP);
    let island = tissue.islands()[0];
    let world = World::with_slide(
        Slide::new(Arc::new(tissue), Point2::new(0.0, 0.0)),
        Um(FOCUS),
    );
    (world, island)
}

#[tokio::test(flavor = "multi_thread")]
async fn a_frame_past_its_budget_is_coarse_and_the_next_are_fine() {
    let Some(_) = gpu() else { return };
    // The scaled-down sensor needs 20 tiles: a budget of 4 makes the first
    // frame coarse. With noise, as frames are: it is the same on the GPU
    // for the same frame and tiles.
    let (world, at) = section();
    let (rig, stream, rate) = camera(world.clone(), at, Light::Transmitted, gpu(), 4, true).await;
    let (reference, fine, _) = camera(world, at, Light::Transmitted, gpu(), usize::MAX, true).await;
    let first = frame(&rig, &stream, rate, 1);
    let stats = rig.camera.gpu_stats().unwrap();
    assert_eq!(stats.coarse_frames, 1, "{stats:?}");
    assert!(stats.most_in_a_frame <= 4, "{stats:?}");
    assert!(
        first != frame(&reference, &fine, rate, 1),
        "the coarse frame was the fine one"
    );
    // Between frames the fine tiles were made: the second is fine, and is
    // the fine camera's to the bit.
    let second = frame(&rig, &stream, rate, 2);
    let stats = rig.camera.gpu_stats().unwrap();
    assert_eq!((stats.coarse_frames, stats.last_frame), (1, 0), "{stats:?}");
    assert!(
        second == frame(&reference, &fine, rate, 2),
        "the second frame isn't the fine one"
    );
    rig.camera.stop().await.unwrap();
    reference.camera.stop().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn the_tiles_ahead_of_a_jog_are_made_between_frames() {
    let Some(_) = gpu() else { return };
    let (world, at) = section();
    let (rig, stream, rate) = camera(world, at, Light::Transmitted, gpu(), 128, false).await;
    frame(&rig, &stream, rate, 1);
    // 500 µm/s: 69 µm a frame, more than a tile of 61 µm at 10×. The first
    // frame of the jog makes its own new tiles, and says where the next
    // will look; from then on, its tiles are ready before it.
    rig.stage
        .jog(Axis::X, true, Some(UmPerS(500.0)))
        .await
        .unwrap();
    frame(&rig, &stream, rate, 2);
    assert!(rig.camera.gpu_stats().unwrap().last_frame > 0);
    for k in 3..=6 {
        let before = rig.camera.gpu_stats().unwrap().made;
        frame(&rig, &stream, rate, k);
        let stats = rig.camera.gpu_stats().unwrap();
        assert_eq!(
            stats.last_frame, 0,
            "frame {k} made its own tiles: {stats:?}"
        );
        assert!(
            stats.made > before,
            "frame {k}: nothing was made ahead: {stats:?}"
        );
    }
    rig.camera.stop().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn other_samples_and_light_come_from_the_cpu_s_tiles() {
    let Some(_) = gpu() else { return };
    let target_at = Point2::new(30_000.0, 10_000.0);
    let target = || {
        World::with_slide(
            Slide::new(
                Arc::new(TestTarget::new(target_at, 0.5, 0.0)),
                Point2::new(0.0, 0.0),
            ),
            Um(FOCUS),
        )
    };
    let (tissue, island) = section();
    let cases = [
        ("the test target", target(), target_at, Light::Transmitted),
        (
            "the section in reflected light",
            tissue,
            island,
            Light::Reflected,
        ),
        ("no slide", World::empty(), island, Light::Transmitted),
    ];
    for (name, world, at, light) in cases {
        let (cpu, cpu_stream, rate) = camera(world.clone(), at, light, None, 128, false).await;
        let (gpu_rig, gpu_stream, _) = camera(world, at, light, gpu(), 128, false).await;
        let a = frame(&cpu, &cpu_stream, rate, 1);
        let b = frame(&gpu_rig, &gpu_stream, rate, 1);
        let stats = gpu_rig.camera.gpu_stats().unwrap();
        assert!(stats.from_cpu > 0 && stats.made == 0, "{name}: {stats:?}");
        match within_half_precision(&a, &b) {
            Ok(report) => println!("{name}: {report}"),
            Err(report) => panic!("{name}: {report}"),
        }
        cpu.camera.stop().await.unwrap();
        gpu_rig.camera.stop().await.unwrap();
    }
}
