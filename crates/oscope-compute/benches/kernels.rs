//! The kernels on a full 20 MP frame, in the live lane:
//! `cargo bench -p oscope-compute`. The crate's documentation gives the
//! conventions.

// criterion's macros make public functions without documentation.
#![allow(missing_docs)]

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use oscope_compute::{Compute, ComputeConfig, Lane};
use oscope_core::{BayerPattern, FrameView, PixelFormat};

/// The MIChrome 20's full frame.
const WIDTH: u32 = 5472;
const HEIGHT: u32 = 3648;

fn kernels(c: &mut Criterion) {
    let compute = Compute::new(&ComputeConfig::default()).expect("the pools start");
    let format = PixelFormat::Bayer16 {
        pattern: BayerPattern::Rggb,
        significant_bits: 16,
    };
    let bytes: Vec<u8> = (0..WIDTH * HEIGHT)
        .flat_map(|i| ((i * 7) as u16).to_le_bytes())
        .collect();
    let view = FrameView::new(&bytes, format, WIDTH, HEIGHT).expect("a whole frame");
    let image = compute.to_f32(Lane::Live, &view).expect("a 16-bit mosaic");

    let mut group = c.benchmark_group("20 MP");
    group.sample_size(20);
    group.throughput(Throughput::Bytes(bytes.len() as u64));
    group.bench_function("to_f32", |b| b.iter(|| compute.to_f32(Lane::Live, &view)));
    group.throughput(Throughput::Bytes((image.data().len() * 4) as u64));
    group.bench_function("bin2x2", |b| b.iter(|| compute.bin2x2(Lane::Live, &image)));
    group.bench_function("downsample_4", |b| {
        b.iter(|| compute.downsample(Lane::Live, &image, 4))
    });
    group.finish();
}

criterion_group!(benches, kernels);
criterion_main!(benches);
