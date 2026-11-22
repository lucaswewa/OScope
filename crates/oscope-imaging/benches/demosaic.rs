//! Each demosaicing method on a full 20 MP RGGB16 frame, from the raw frame
//! to RGB, in a live pool of 8 threads: §5.20's CPU budget is 80 ms.
//! `cargo bench -p oscope-imaging --bench demosaic`.

// criterion's macros make public functions without documentation.
#![allow(missing_docs)]

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use oscope_compute::{Compute, ComputeConfig, Lane};
use oscope_core::{BayerPattern, FrameView, PixelFormat};
use oscope_imaging::color::{Demosaic, chart, demosaic_frame, mosaic};

fn demosaicing(c: &mut Criterion) {
    let config = ComputeConfig {
        live_threads: Some(8),
        ..ComputeConfig::default()
    };
    let compute = Compute::new(&config).expect("the pools start");
    let (width, height) = (5472, 3648);
    let raw = mosaic(&chart(width, height), BayerPattern::Rggb).expect("an RGB chart");
    let bytes: Vec<u8> = raw
        .data()
        .iter()
        .flat_map(|v| ((v * 65_535.0).round() as u16).to_le_bytes())
        .collect();
    let format = PixelFormat::Bayer16 {
        pattern: BayerPattern::Rggb,
        significant_bits: 16,
    };
    let view = FrameView::new(&bytes, format, width, height).expect("a whole frame");

    let mut group = c.benchmark_group("20 MP, 8 threads");
    group.sample_size(20);
    group.throughput(Throughput::Bytes(bytes.len() as u64));
    for method in Demosaic::ALL {
        group.bench_function(format!("{method:?}"), |b| {
            b.iter(|| demosaic_frame(&compute, Lane::Live, &view, method))
        });
    }
    group.finish();
}

criterion_group!(benches, demosaicing);
criterion_main!(benches);
