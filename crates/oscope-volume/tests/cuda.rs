//! Depth from focus on the GPU, with `OSCOPE_CUDA=1`: the heights and the
//! confidence the CPU finds, to the bit.

#![cfg(feature = "cuda")]

use oscope_compute::{Compute, ComputeConfig, Image, Lane, Shape, focus};
use oscope_volume::{FocusOptions, Planes, depth_from_focus};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};

#[test]
fn the_gpu_finds_the_cpu_s_heights() {
    if std::env::var("OSCOPE_CUDA").as_deref() != Ok("1") {
        eprintln!("skipped: set OSCOPE_CUDA=1 to run the tests that need a GPU");
        return;
    }
    let compute = Compute::new(&ComputeConfig::default()).unwrap();
    assert!(compute.cuda().is_some(), "OSCOPE_CUDA=1, but no GPU");
    let mut random = StdRng::seed_from_u64(9);
    let (w, h) = (201, 133);
    let stack: Vec<Image> = (0..7)
        .map(|_| {
            let data = (0..w * h).map(|_| random.random_range(0.0..1.0)).collect();
            Image::from_vec(Shape::new(w, h, 1), data).unwrap()
        })
        .collect();
    let options = FocusOptions::default();
    let planes = Planes {
        z0_um: 5_000.0,
        step_um: 0.5,
    };
    let (s, r) = (options.step, options.radius);
    let cpu = depth_from_focus(&stack, planes, &options, |p| focus::sml(p, s, r)).unwrap();
    let gpu = depth_from_focus(&stack, planes, &options, |p| {
        compute.focus_sml(Lane::Batch, p, s, r)
    })
    .unwrap();
    assert_eq!(cpu.heights.data(), gpu.heights.data());
    assert_eq!(cpu.confidence.data(), gpu.confidence.data());
}

#[test]
fn deconvolution_on_the_gpu_is_the_cpu_s_within_a_thousandth() {
    use oscope_volume::deconvolve::{DeconvolveOptions, Padded, Volume, richardson_lucy};
    use oscope_volume::psf::{Optics, psf};
    if std::env::var("OSCOPE_CUDA").as_deref() != Ok("1") {
        eprintln!("skipped: set OSCOPE_CUDA=1 to run the tests that need a GPU");
        return;
    }
    let compute = Compute::new(&ComputeConfig::default()).unwrap();
    let cuda = compute.cuda().expect("OSCOPE_CUDA=1, but no GPU");
    let mut random = StdRng::seed_from_u64(43);
    let size = [45, 38, 12];
    let data: Vec<f32> = (0..size.iter().product::<usize>())
        .map(|_| random.random_range(0.0..1.0))
        .collect();
    let volume = Volume { size, data };
    let optics = Optics {
        na: 0.55,
        wavelength_um: 0.519,
        medium: 1.0,
    };
    let kernel = psf(&optics, [0.2, 0.2, 0.4], size).unwrap();
    let padded = Padded::new(&volume, &kernel).unwrap();
    for tv in [0.0, 0.002] {
        let options = DeconvolveOptions { iterations: 10, tv };
        let cpu = richardson_lucy(&padded, &options, &mut |_| true).unwrap();
        let gpu = cuda
            .richardson_lucy(
                Lane::Batch,
                &padded.data,
                &padded.half_otf(),
                padded.size,
                10,
                tv,
                &mut |_| true,
            )
            .unwrap()
            .unwrap();
        let top = cpu.iter().copied().fold(0.0_f32, f32::max);
        let worst = cpu
            .iter()
            .zip(&gpu)
            .map(|(a, b)| (a - b).abs())
            .fold(0.0_f32, f32::max);
        eprintln!(
            "total variation {tv}: the GPU within {:.2e} of the largest voxel",
            worst / top
        );
        assert!(worst <= 1e-3 * top, "{worst} of {top}");
    }
    // A cancel at the second iteration.
    let stopped = cuda
        .richardson_lucy(
            Lane::Batch,
            &padded.data,
            &padded.half_otf(),
            padded.size,
            10,
            0.0,
            &mut |k| k < 2,
        )
        .unwrap();
    assert!(stopped.is_none());
}
