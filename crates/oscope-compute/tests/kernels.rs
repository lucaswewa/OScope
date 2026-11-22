//! The CPU kernels: known values, a plain reference over seeded random
//! shapes, and a full 20 MP frame.

use oscope_compute::equivalence::{Tolerance, assert_equivalent, compare_images};
use oscope_compute::{Compute, ComputeConfig, ComputeError, Image, Lane, Shape, cpu};
use oscope_core::{BayerPattern, FrameView, PixelFormat, PixelRect};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};

fn bytes16(values: &[u16]) -> Vec<u8> {
    values.iter().flat_map(|v| v.to_le_bytes()).collect()
}

fn image(width: u32, height: u32, channels: usize, data: &[f32]) -> Image {
    Image::from_vec(Shape::new(width, height, channels), data.to_vec()).unwrap()
}

#[test]
fn to_f32_scales_8_and_16_bit_samples_from_0_to_1() {
    let bytes = bytes16(&[0, 65_535, 32_768, 1]);
    let view = FrameView::new(&bytes, PixelFormat::Mono16, 2, 2).unwrap();
    let mono = cpu::to_f32(&view).unwrap();
    assert_eq!(mono.shape(), Shape::new(2, 2, 1));
    assert_eq!(mono.data(), [0.0, 1.0, 32_768.0 / 65_535.0, 1.0 / 65_535.0]);

    let view = FrameView::new(&[0, 51, 255, 255, 0, 102], PixelFormat::Rgb8, 2, 1).unwrap();
    let rgb = cpu::to_f32(&view).unwrap();
    assert_eq!(rgb.shape(), Shape::new(2, 1, 3));
    assert_eq!(rgb.data(), [0.0, 0.2, 1.0, 1.0, 0.0, 0.4]);

    // A 12-bit mosaic, aligned high, stays a one-channel mosaic.
    let format = PixelFormat::Bayer16 {
        pattern: BayerPattern::Rggb,
        significant_bits: 12,
    };
    let bytes = bytes16(&[4095 << 4, 1 << 4]);
    let mosaic = cpu::to_f32(&FrameView::new(&bytes, format, 2, 1).unwrap()).unwrap();
    assert_eq!(mosaic.data(), [65_520.0 / 65_535.0, 16.0 / 65_535.0]);

    // A view of a rectangle reads its own rows, whatever the stride.
    let values: Vec<u16> = (0..16).collect();
    let bytes = bytes16(&values);
    let whole = FrameView::new(&bytes, PixelFormat::Mono16, 4, 4).unwrap();
    let part = whole.roi(PixelRect::new(1, 1, 2, 2)).unwrap();
    let expected: Vec<f32> = [5.0, 6.0, 9.0, 10.0].iter().map(|v| v / 65_535.0).collect();
    assert_eq!(cpu::to_f32(&part).unwrap().data(), expected);

    let planar = PixelFormat::PlanarF32 { planes: 1 };
    let floats = [0_u8; 16];
    let view = FrameView::new(&floats, planar, 2, 2).unwrap();
    assert_eq!(cpu::to_f32(&view), Err(ComputeError::Unsupported(planar)));
}

#[test]
fn binning_sums_blocks_and_downsampling_averages_them() {
    let small = image(3, 3, 1, &[1.0, 2.0, 9.0, 3.0, 4.0, 9.0, 9.0, 9.0, 9.0]);
    assert_eq!(cpu::bin2x2(&small), image(1, 1, 1, &[10.0]));
    assert_eq!(
        cpu::bin2x2(&image(1, 5, 1, &[1.0; 5])).shape(),
        Shape::new(0, 2, 1)
    );

    // The edge blocks are smaller, and averaged over what they have.
    let averaged = cpu::downsample(&small, 2).unwrap();
    assert_eq!(averaged, image(2, 2, 1, &[2.5, 9.0, 9.0, 9.0]));
    assert_eq!(cpu::downsample(&small, 1).unwrap(), small);
    assert_eq!(
        cpu::downsample(&small, 50).unwrap(),
        image(1, 1, 1, &[55.0 / 9.0])
    );
    assert_eq!(cpu::downsample(&small, 0), Err(ComputeError::Factor));

    let rgb = image(
        2,
        2,
        3,
        &[
            1.0, 10.0, 100.0, 2.0, 20.0, 200.0, 3.0, 30.0, 300.0, 4.0, 40.0, 400.0,
        ],
    );
    assert_eq!(cpu::bin2x2(&rgb), image(1, 1, 3, &[10.0, 100.0, 1000.0]));
    assert_eq!(
        cpu::downsample(&rgb, 2).unwrap(),
        image(1, 1, 3, &[2.5, 25.0, 250.0])
    );
}

/// 2 × 2 binning, written plainly, in f64.
fn plain_bin(input: &Image) -> Image {
    let shape = Shape::new(input.width() / 2, input.height() / 2, input.channels());
    let mut data = Vec::with_capacity(shape.len());
    for y in 0..shape.height {
        for x in 0..shape.width {
            for c in 0..shape.channels {
                let sample = |dx, dy| f64::from(input.get(2 * x + dx, 2 * y + dy, c).unwrap());
                data.push((sample(0, 0) + sample(1, 0) + sample(0, 1) + sample(1, 1)) as f32);
            }
        }
    }
    Image::from_vec(shape, data).unwrap()
}

/// Box downsampling, written plainly, in f64.
fn plain_downsample(input: &Image, factor: u32) -> Image {
    let (w, h) = (input.width(), input.height());
    let shape = Shape::new(w.div_ceil(factor), h.div_ceil(factor), input.channels());
    let mut data = Vec::with_capacity(shape.len());
    for oy in 0..shape.height {
        for ox in 0..shape.width {
            for c in 0..shape.channels {
                let (ys, xs) = (
                    oy * factor..(oy * factor + factor).min(h),
                    ox * factor..(ox * factor + factor).min(w),
                );
                let samples: Vec<f64> = ys
                    .flat_map(|y| xs.clone().map(move |x| (x, y)))
                    .map(|(x, y)| f64::from(input.get(x, y, c).unwrap()))
                    .collect();
                data.push((samples.iter().sum::<f64>() / samples.len() as f64) as f32);
            }
        }
    }
    Image::from_vec(shape, data).unwrap()
}

#[test]
fn the_kernels_match_a_plain_reference_on_300_seeded_cases() {
    let mut random = StdRng::seed_from_u64(6);
    let formats = [
        PixelFormat::Mono8,
        PixelFormat::Mono16,
        PixelFormat::Rgb8,
        PixelFormat::Rgb16,
        PixelFormat::Rgba8,
        PixelFormat::Bayer8 {
            pattern: BayerPattern::Gbrg,
        },
        PixelFormat::Bayer16 {
            pattern: BayerPattern::Rggb,
            significant_bits: 14,
        },
    ];
    let close = Tolerance {
        abs: 1e-6,
        rel: 1e-5,
    };
    for case in 0..300 {
        let format = formats[case % formats.len()];
        let (width, height) = (random.random_range(1..=40), random.random_range(1..=40));
        let bytes: Vec<u8> = (0..format.frame_bytes(width, height))
            .map(|_| random.random())
            .collect();
        let view = FrameView::new(&bytes, format, width, height).unwrap();
        let converted = cpu::to_f32(&view).unwrap();
        let max = if format.sample_bytes() == 2 {
            65_535.0
        } else {
            255.0
        };
        for (i, &value) in converted.data().iter().enumerate() {
            let pixel = i / format.channels();
            let (x, y) = (
                (pixel % width as usize) as u32,
                (pixel / width as usize) as u32,
            );
            let sample = view.sample(x, y, i % format.channels()).unwrap();
            assert_eq!(
                value,
                f32::from(sample) / max,
                "case {case}, {format:?}, ({x}, {y})"
            );
        }
        assert_equivalent(&plain_bin(&converted), &cpu::bin2x2(&converted), close);
        let factor = random.random_range(1..=7);
        let reference = plain_downsample(&converted, factor);
        assert_equivalent(
            &reference,
            &cpu::downsample(&converted, factor).unwrap(),
            close,
        );
    }
}

#[test]
fn a_full_20_mp_frame_converts_bins_and_downsamples_in_the_live_lane() {
    let (width, height) = (5472, 3648);
    let format = PixelFormat::Bayer16 {
        pattern: BayerPattern::Rggb,
        significant_bits: 16,
    };
    let values: Vec<u16> = (0..width * height).map(|i| (i % 65_536) as u16).collect();
    let bytes = bytes16(&values);
    let view = FrameView::new(&bytes, format, width, height).unwrap();
    let compute = Compute::new(&ComputeConfig::default()).unwrap();

    let converted = compute.to_f32(Lane::Live, &view).unwrap();
    assert_eq!(converted.shape(), Shape::new(5472, 3648, 1));
    let last = values[values.len() - 1];
    assert_eq!(
        converted.get(5471, 3647, 0),
        Some(f32::from(last) / 65_535.0)
    );

    // Binning sums the blocks that downsampling by 2 averages.
    let binned = compute.bin2x2(Lane::Live, &converted);
    let mut quartered = compute.downsample(Lane::Live, &converted, 2).unwrap();
    quartered.data_mut().iter_mut().for_each(|v| *v *= 4.0);
    let comparison = compare_images(&binned, &quartered, Tolerance::abs(1e-6)).unwrap();
    assert!(comparison.passed(), "{comparison}");
    assert_eq!(comparison.compared, 2736 * 1824);

    let preview = compute.downsample(Lane::Batch, &converted, 4).unwrap();
    assert_eq!(preview.shape(), Shape::new(1368, 912, 1));
}
