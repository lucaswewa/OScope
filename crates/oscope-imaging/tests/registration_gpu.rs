//! Registration's CUDA twin (P31), with `OSCOPE_CUDA=1`: it agrees with
//! the CPU within 0.01 px and 0.01 of confidence, at odd sizes, in search
//! windows and in batches; and in a release build, a 2048² pair is timed
//! against §5.20's 15 ms.

#![cfg(feature = "cuda")]

mod field;

use std::time::{Duration, Instant};

use field::{Field, noisy};
use oscope_compute::{Compute, ComputeConfig, Image, Lane};
use oscope_imaging::registration::{self, RegisterOptions, Registration, gpu};
use rand::SeedableRng;
use rand::rngs::StdRng;

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

fn agree(cpu: &Registration, gpu: &Registration) {
    let close = |a: f64, b: f64| (a - b).abs() <= 0.01;
    assert!(
        close(cpu.dx, gpu.dx) && close(cpu.dy, gpu.dy) && close(cpu.confidence, gpu.confidence),
        "the CPU found {cpu:?}, the GPU {gpu:?}"
    );
}

#[test]
fn the_gpu_agrees_with_the_cpu() {
    let Some(compute) = gpu_compute() else { return };
    let cuda = compute.cuda().unwrap();
    let mut random = StdRng::seed_from_u64(41);
    // Even and odd sizes, square and not, sharp and blurred.
    let cases = [
        ((256, 256), 0.0),
        ((250, 190), 1.7),
        ((255, 129), 2.3),
        ((64, 9), 1.0),
    ];
    for (i, (size, blur)) in cases.into_iter().enumerate() {
        let field = Field::new(512, blur, i as u64);
        let shifts = [(0.0, 0.0), (3.3, -2.7), (-7.8, 1.15), (0.5, -0.5)];
        let pairs: Vec<(Image, Image)> = shifts
            .iter()
            .map(|shift| {
                let a = noisy(
                    field.window((0.0, 0.0), (100, 100), size),
                    size,
                    4.0,
                    &mut random,
                );
                let b = noisy(
                    field.window(*shift, (100, 100), size),
                    size,
                    4.0,
                    &mut random,
                );
                (a, b)
            })
            .collect();
        let refs: Vec<(&Image, &Image)> = pairs.iter().map(|(a, b)| (a, b)).collect();
        let near = RegisterOptions {
            expected: Some((3.0, -2.0)),
            radius: 10,
            ..RegisterOptions::default()
        };
        let unwindowed = RegisterOptions {
            window: false,
            sigma: 3.0,
            ..RegisterOptions::default()
        };
        for options in [RegisterOptions::default(), near, unwindowed] {
            let on_cpu = registration::register_batch(&refs, &options).unwrap();
            let on_gpu = gpu::register_batch(cuda, Lane::Batch, &refs, &options).unwrap();
            for (cpu, gpu) in on_cpu.iter().zip(&on_gpu) {
                agree(cpu, gpu);
            }
            // And one pair alone, on the live lane, as in the batch: cuFFT
            // plans a batch of another size its own way, so they agree to
            // the rounding, not to the bit.
            let alone = gpu::register(cuda, Lane::Live, refs[1].0, refs[1].1, &options).unwrap();
            let (x, y) = (
                (alone.dx - on_gpu[1].dx).abs(),
                (alone.dy - on_gpu[1].dy).abs(),
            );
            assert!(
                x.max(y) < 1e-4,
                "alone {alone:?}, in the batch {:?}",
                on_gpu[1]
            );
        }
    }
}

#[test]
fn a_2048_pair_registers_on_the_gpu_within_15_ms() {
    let Some(compute) = gpu_compute() else { return };
    if cfg!(debug_assertions) {
        eprintln!("skipped: §5.20's budget is timed in a release build");
        return;
    }
    let cuda = compute.cuda().unwrap();
    let mut random = StdRng::seed_from_u64(42);
    let size = (2048, 2048);
    let field = Field::new(2560, 1.7, 9);
    let shift = (37.25, -18.5);
    let a = noisy(
        field.window((0.0, 0.0), (256, 256), size),
        size,
        4.0,
        &mut random,
    );
    let b = noisy(
        field.window(shift, (256, 256), size),
        size,
        4.0,
        &mut random,
    );
    let options = RegisterOptions::default();
    let mut times = Vec::new();
    for _ in 0..11 {
        let start = Instant::now();
        let found = gpu::register(cuda, Lane::Live, &a, &b, &options).unwrap();
        times.push(start.elapsed());
        assert!((found.dx - shift.0).abs() <= 0.1 && (found.dy - shift.1).abs() <= 0.1);
    }
    // The first call loads cuFFT's kernels; the median of the rest counts.
    times.remove(0);
    times.sort();
    let median = times[times.len() / 2];
    eprintln!(
        "a 2048² pair on the GPU: median {median:?}, fastest {:?}",
        times[0]
    );
    assert!(median <= Duration::from_millis(15), "{median:?}");
}
