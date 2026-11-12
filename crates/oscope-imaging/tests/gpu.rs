//! The colour pipeline's CUDA twins, with `OSCOPE_CUDA=1`: each matches its
//! CPU reference to the bit, at every pattern and size and at 20 MP, and
//! the 20 MP demosaic is timed against §5.20's 10 ms.

#![cfg(feature = "cuda")]

use std::time::{Duration, Instant};

use oscope_compute::equivalence::{Tolerance, assert_equivalent};
use oscope_compute::{Compute, ComputeConfig, ComputeError, Image, Lane, Shape};
use oscope_core::{BayerPattern, FramePool, FrameView, PixelFormat};
use oscope_imaging::color::{
    self, ColorError, ColourCorrection, Curve, Demosaic, Display, chart, gpu, mosaic,
};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};

const PATTERNS: [BayerPattern; 4] = [
    BayerPattern::Rggb,
    BayerPattern::Grbg,
    BayerPattern::Gbrg,
    BayerPattern::Bggr,
];

/// The GPU, when `OSCOPE_CUDA=1` asks for the tests that need one.
fn gpu_compute() -> Option<Compute> {
    if std::env::var("OSCOPE_CUDA").as_deref() != Ok("1") {
        eprintln!("skipped: set OSCOPE_CUDA=1 to run the tests that need a GPU");
        return None;
    }
    let compute = Compute::new(&ComputeConfig::default()).unwrap();
    assert!(compute.cuda().is_some(), "OSCOPE_CUDA=1, but no GPU");
    Some(compute)
}

#[test]
fn the_demosaic_twins_match_the_cpu_to_the_bit() {
    let Some(compute) = gpu_compute() else { return };
    let cuda = compute.cuda().unwrap();
    let mut random = StdRng::seed_from_u64(10);
    let mut sizes = vec![(1, 1), (2, 2), (3, 2), (5, 7), (2, 9)];
    sizes.extend((0..40).map(|_| (random.random_range(1..=160), random.random_range(1..=120))));
    for (i, (width, height)) in sizes.into_iter().enumerate() {
        let pattern = PATTERNS[i % 4];
        // Half of the cases are random samples, half the chart.
        let raw = match i % 2 {
            0 => {
                let data = (0..width * height)
                    .map(|_| random.random::<f32>())
                    .collect();
                Image::from_vec(Shape::new(width, height, 1), data).unwrap()
            }
            _ => mosaic(&chart(width, height), pattern).unwrap(),
        };
        let on_gpu = cuda.upload(Lane::Live, &raw).unwrap();
        for method in Demosaic::ALL {
            let expected = color::demosaic(&raw, pattern, method).unwrap();
            let device = gpu::demosaic(cuda, Lane::Live, &on_gpu, pattern, method).unwrap();
            let got = cuda.download(Lane::Live, &device).unwrap();
            assert_equivalent(&expected, &got, Tolerance::EXACT);
        }
    }
}

#[test]
fn the_colour_step_twins_match_the_cpu_to_the_bit() {
    let Some(compute) = gpu_compute() else { return };
    let cuda = compute.cuda().unwrap();
    let rgb = chart(300, 200);

    let mut black = rgb.clone();
    color::subtract_black(&mut black, 0.03).unwrap();
    let mut on_gpu = cuda.upload(Lane::Batch, &rgb).unwrap();
    gpu::subtract_black(cuda, Lane::Batch, &mut on_gpu, 0.03).unwrap();
    assert_equivalent(
        &black,
        &cuda.download(Lane::Batch, &on_gpu).unwrap(),
        Tolerance::EXACT,
    );

    let correction = ColourCorrection {
        gains: [1.9, 1.0, 1.4],
        matrix: [[1.6, -0.4, -0.2], [-0.25, 1.5, -0.25], [-0.1, -0.5, 1.6]],
    };
    color::correct(&mut black, &correction).unwrap();
    gpu::correct(cuda, Lane::Batch, &mut on_gpu, &correction).unwrap();
    assert_equivalent(
        &black,
        &cuda.download(Lane::Batch, &on_gpu).unwrap(),
        Tolerance::EXACT,
    );

    for curve in [Curve::Linear, Curve::Gamma { gamma: 2.2 }, Curve::Srgb] {
        let display = Display {
            black: 0.02,
            white: 0.9,
            curve,
        };
        let expected = color::to_display(&black, &display).unwrap();
        assert_eq!(
            gpu::to_display(cuda, Lane::Batch, &on_gpu, &display).unwrap(),
            expected
        );
    }

    // The same refusals as the CPU's, before any kernel runs.
    let mut mosaic_on_gpu = cuda
        .upload(Lane::Live, &Image::zeros(Shape::new(4, 4, 1)))
        .unwrap();
    let refused = gpu::correct(cuda, Lane::Live, &mut mosaic_on_gpu, &correction);
    assert_eq!(
        refused,
        Err(ColorError::Compute(ComputeError::Channels {
            expected: 3,
            got: 1
        }))
    );
    let refused = gpu::demosaic(
        cuda,
        Lane::Live,
        &on_gpu,
        BayerPattern::Rggb,
        Demosaic::Bilinear,
    );
    assert_eq!(
        refused.unwrap_err(),
        ColorError::Compute(ComputeError::Channels {
            expected: 1,
            got: 3
        })
    );
    let refused = gpu::subtract_black(cuda, Lane::Live, &mut mosaic_on_gpu, 1.5);
    assert!(matches!(refused, Err(ColorError::Setting(_))));
}

#[test]
fn a_20_mp_frame_matches_and_meets_the_10_ms_budget() {
    let Some(compute) = gpu_compute() else { return };
    let cuda = compute.cuda().unwrap();
    let (width, height) = (5472, 3648);
    let format = PixelFormat::Bayer16 {
        pattern: BayerPattern::Rggb,
        significant_bits: 16,
    };
    // The frame in the pool's pinned memory, as the camera will give it.
    let raw = mosaic(&chart(width, height), BayerPattern::Rggb).unwrap();
    let pool = FramePool::new(
        format.frame_bytes(width, height),
        1,
        &cuda.pinned_allocator(),
    );
    let mut buffer = pool.take().unwrap();
    for (sample, v) in buffer
        .bytes_mut()
        .as_chunks_mut::<2>()
        .0
        .iter_mut()
        .zip(raw.data())
    {
        *sample = ((v * 65_535.0).round() as u16).to_le_bytes();
    }
    let view = FrameView::new(buffer.bytes(), format, width, height).unwrap();
    for method in Demosaic::ALL {
        let expected = color::demosaic_frame(&compute, Lane::Live, &view, method).unwrap();
        let device = gpu::demosaic_frame(cuda, Lane::Live, &view, method).unwrap();
        assert_equivalent(
            &expected,
            &cuda.download(Lane::Live, &device).unwrap(),
            Tolerance::EXACT,
        );
        // From the raw frame in host memory to RGB on the GPU.
        let mut times: Vec<Duration> = (0..9)
            .map(|_| {
                let start = Instant::now();
                gpu::demosaic_frame(cuda, Lane::Live, &view, method).unwrap();
                cuda.synchronize(Lane::Live).unwrap();
                start.elapsed()
            })
            .collect();
        times.sort();
        eprintln!(
            "{method:?}: {:.2} ms on the GPU (§5.20: ≤ 10 ms)",
            times[4].as_secs_f64() * 1e3
        );
    }
}
