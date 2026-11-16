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
