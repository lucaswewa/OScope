//! The camera's exposure on the GPU against its CPU twin, with
//! `OSCOPE_CUDA=1` and the `cuda` feature: frames without noise within a
//! count, and noise of the same statistics.

#![cfg(feature = "cuda")]

mod common;

use std::sync::{Arc, PoisonError};
use std::time::Duration;

use common::{FOCUS, rig, set, start};
use oscope_compute::cuda::Cuda;
use oscope_core::{Ms, PixelRect, Point2, Timestamp, Um};
use oscope_hal::{Axis, Camera, Stage, UmPerS};
use oscope_sim::{HeTissue, Light, Mounting, Rate, SensorModel, SimCameraConfig, Slide, World};

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
    for (config, mode, roi, jog) in cases {
        let (cpu, _) = frames(config.clone(), None, mode, roi, jog).await;
        let (gpu, on_gpu) = frames(config, gpu(), mode, roi, jog).await;
        assert!(on_gpu, "{mode}: the camera fell back to the CPU");
        for (a, b) in cpu.iter().zip(&gpu) {
            assert_eq!(a.len(), b.len());
            let worst = a.iter().zip(b).map(|(x, y)| x.abs_diff(*y)).max().unwrap();
            let differ = a.iter().zip(b).filter(|(x, y)| x != y).count();
            assert!(
                worst <= 1 && differ * 1_000 < a.len(),
                "{mode}: {differ} differ, by up to {worst}"
            );
        }
    }
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
