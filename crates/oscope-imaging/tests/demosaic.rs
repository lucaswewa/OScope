//! Demosaicing on the CPU: exact on flat colours at every size and pattern,
//! measured against a chart of known colours, at 20 MP too, and through
//! `Compute`'s lanes.

use oscope_compute::{Compute, ComputeConfig, Image, Lane, Shape, cpu};
use oscope_core::{BayerPattern, FrameView, PixelFormat};
use oscope_imaging::color::{ColorError, Demosaic, chart, demosaic, demosaic_frame, mosaic, psnr};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};

const PATTERNS: [BayerPattern; 4] = [
    BayerPattern::Rggb,
    BayerPattern::Grbg,
    BayerPattern::Gbrg,
    BayerPattern::Bggr,
];

/// An RGB image of one colour.
fn flat(width: u32, height: u32, rgb: [f32; 3]) -> Image {
    let data = rgb.repeat((width * height) as usize);
    Image::from_vec(Shape::new(width, height, 3), data).unwrap()
}

/// A rectangle of an image.
fn crop(image: &Image, x0: u32, y0: u32, x1: u32, y1: u32) -> Image {
    let c = image.channels();
    let data = (y0..y1)
        .flat_map(|y| image.row(y)[x0 as usize * c..x1 as usize * c].to_vec())
        .collect();
    Image::from_vec(Shape::new(x1 - x0, y1 - y0, c), data).unwrap()
}

/// The PSNR of each method on `truth` seen through `pattern`: superpixel
/// against `truth` averaged over 2 × 2 cells, the others against `truth`.
fn measure(truth: &Image, pattern: BayerPattern) -> [f64; 3] {
    let raw = mosaic(truth, pattern).unwrap();
    let half = cpu::downsample(truth, 2).unwrap();
    Demosaic::ALL.map(|method| {
        let rgb = demosaic(&raw, pattern, method).unwrap();
        match method {
            Demosaic::Superpixel => psnr(&half, &rgb, 2).unwrap(),
            _ => psnr(truth, &rgb, 4).unwrap(),
        }
    })
}

#[test]
fn a_flat_colour_comes_back_exactly_at_every_size_and_pattern() {
    let mut random = StdRng::seed_from_u64(8);
    for (width, height) in [(2, 2), (3, 3), (2, 7), (4, 5), (7, 6), (16, 9), (33, 18)] {
        for pattern in PATTERNS {
            let colour = [random.random(), random.random(), random.random()];
            let raw = mosaic(&flat(width, height, colour), pattern).unwrap();
            for method in Demosaic::ALL {
                let rgb = demosaic(&raw, pattern, method).unwrap();
                let expected = match method {
                    Demosaic::Superpixel => Shape::new(width / 2, height / 2, 3),
                    _ => Shape::new(width, height, 3),
                };
                assert_eq!(rgb.shape(), expected, "{method:?}");
                for (i, (&got, want)) in rgb.data().iter().zip(colour.iter().cycle()).enumerate() {
                    let near = (got - want).abs() < 1e-6;
                    assert!(
                        near,
                        "{width}×{height} {pattern:?} {method:?}: sample {i} is {got}, not {want}"
                    );
                }
            }
        }
    }
}

#[test]
fn each_method_meets_its_psnr_on_the_chart_for_every_pattern() {
    let truth = chart(512, 384);
    for pattern in PATTERNS {
        let [superpixel, bilinear, malvar] = measure(&truth, pattern);
        assert!(
            superpixel > 21.0,
            "{pattern:?} superpixel {superpixel:.2} dB"
        );
        assert!(bilinear > 21.0, "{pattern:?} bilinear {bilinear:.2} dB");
        assert!(malvar > 28.5, "{pattern:?} Malvar–He–Cutler {malvar:.2} dB");
        // The paper reports about 5.5 dB over bilinear on photographs.
        assert!(
            malvar - bilinear > 5.0,
            "{pattern:?}: {malvar:.2} against {bilinear:.2} dB"
        );
    }

    // By region: the gradient is linear, so both interpolations rebuild it;
    // the zone plate's fine rings are where Malvar–He–Cutler gains.
    let raw = mosaic(&truth, BayerPattern::Rggb).unwrap();
    let regions = [
        ("patches", 0, 0, 512, 192),
        ("gradient", 0, 192, 256, 384),
        ("rings", 256, 192, 512, 384),
    ];
    let [bilinear, malvar] = [Demosaic::Bilinear, Demosaic::MalvarHeCutler]
        .map(|method| demosaic(&raw, BayerPattern::Rggb, method).unwrap());
    // Malvar–He–Cutler overshoots at sharp edges, and is clamped to 0–1.
    assert!(malvar.data().iter().all(|v| (0.0..=1.0).contains(v)));
    let region = |image: &Image, (x0, y0, x1, y1)| crop(image, x0, y0, x1, y1);
    let psnrs = regions.map(|(name, x0, y0, x1, y1)| {
        let truth = region(&truth, (x0, y0, x1, y1));
        let b = psnr(&truth, &region(&bilinear, (x0, y0, x1, y1)), 4).unwrap();
        let m = psnr(&truth, &region(&malvar, (x0, y0, x1, y1)), 4).unwrap();
        (name, b, m)
    });
    let (_, b, m) = psnrs[0];
    assert!(
        b > 30.0 && m > 29.0,
        "patches: bilinear {b:.2}, Malvar–He–Cutler {m:.2} dB"
    );
    let (_, b, m) = psnrs[1];
    assert!(
        b > 60.0 && m > 60.0,
        "gradient: bilinear {b:.2}, Malvar–He–Cutler {m:.2} dB"
    );
    let (_, b, m) = psnrs[2];
    assert!(
        m > 25.0 && m - b > 8.0,
        "rings: bilinear {b:.2}, Malvar–He–Cutler {m:.2} dB"
    );
}

#[test]
fn a_full_20_mp_chart_meets_the_same_marks() {
    let truth = chart(5472, 3648);
    let [superpixel, bilinear, malvar] = measure(&truth, BayerPattern::Rggb);
    assert!(superpixel > 21.0, "superpixel {superpixel:.2} dB");
    assert!(bilinear > 20.5, "bilinear {bilinear:.2} dB");
    assert!(malvar > 29.0, "Malvar–He–Cutler {malvar:.2} dB");
}

#[test]
fn frames_are_demosaiced_in_the_lanes_from_8_and_16_bit_mosaics() {
    let compute = Compute::new(&ComputeConfig::default()).unwrap();
    let truth = chart(64, 48);
    for (bits, pattern) in [(16, BayerPattern::Grbg), (8, BayerPattern::Bggr)] {
        let raw = mosaic(&truth, pattern).unwrap();
        let (format, bytes): (PixelFormat, Vec<u8>) = match bits {
            16 => (
                PixelFormat::Bayer16 {
                    pattern,
                    significant_bits: 16,
                },
                raw.data()
                    .iter()
                    .flat_map(|v| ((v * 65_535.0).round() as u16).to_le_bytes())
                    .collect(),
            ),
            _ => (
                PixelFormat::Bayer8 { pattern },
                raw.data()
                    .iter()
                    .map(|v| (v * 255.0).round() as u8)
                    .collect(),
            ),
        };
        let view = FrameView::new(&bytes, format, 64, 48).unwrap();
        for lane in [Lane::Live, Lane::Batch] {
            let rgb = demosaic_frame(&compute, lane, &view, Demosaic::MalvarHeCutler).unwrap();
            let direct = demosaic(
                &compute.to_f32(lane, &view).unwrap(),
                pattern,
                Demosaic::MalvarHeCutler,
            );
            assert_eq!(rgb, direct.unwrap());
            let p = psnr(&truth, &rgb, 4).unwrap();
            // A small chart has fine rings almost everywhere.
            assert!(p > 20.0, "{bits} bits: {p:.2} dB");
        }
    }

    let grey = FrameView::new(&[0; 4], PixelFormat::Mono8, 2, 2).unwrap();
    let refused = demosaic_frame(&compute, Lane::Live, &grey, Demosaic::Bilinear);
    assert_eq!(refused, Err(ColorError::NotAMosaic(PixelFormat::Mono8)));
    assert_eq!(
        demosaic(&truth, BayerPattern::Rggb, Demosaic::Bilinear),
        Err(ColorError::Channels(3))
    );
}

#[test]
fn tiny_and_empty_mosaics_give_finite_output() {
    for (width, height) in [(0, 0), (1, 1), (1, 4), (5, 1), (2, 1)] {
        let raw = Image::from_vec(
            Shape::new(width, height, 1),
            vec![0.5; (width * height) as usize],
        )
        .unwrap();
        for method in Demosaic::ALL {
            let rgb = demosaic(&raw, BayerPattern::Rggb, method).unwrap();
            assert!(
                rgb.data().iter().all(|v| v.is_finite()),
                "{width}×{height} {method:?}"
            );
        }
    }
}

#[test]
fn mosaics_and_psnr_do_what_they_say() {
    let rgb = Image::from_vec(Shape::new(2, 1, 3), vec![0.1, 0.2, 0.3, 0.4, 0.5, 0.6]).unwrap();
    assert_eq!(mosaic(&rgb, BayerPattern::Rggb).unwrap().data(), [0.1, 0.5]);
    assert_eq!(mosaic(&rgb, BayerPattern::Bggr).unwrap().data(), [0.3, 0.5]);
    assert_eq!(mosaic(&rgb, BayerPattern::Grbg).unwrap().data(), [0.2, 0.4]);

    let a = flat(10, 10, [0.5; 3]);
    assert_eq!(psnr(&a, &a, 0).unwrap(), f64::INFINITY);
    // An error of 0.1 everywhere: 20 dB.
    let b = flat(10, 10, [0.6; 3]);
    assert!((psnr(&a, &b, 0).unwrap() - 20.0).abs() < 1e-4);
    // Only the border differs, and the border is left out.
    let mut c = a.clone();
    c.data_mut()[..30].fill(0.0);
    assert_eq!(psnr(&a, &c, 1).unwrap(), f64::INFINITY);
    assert_eq!(psnr(&a, &c, 5), Err(ColorError::Empty));
    assert!(matches!(
        psnr(&a, &flat(5, 5, [0.5; 3]), 0),
        Err(ColorError::Shape { .. })
    ));
}
