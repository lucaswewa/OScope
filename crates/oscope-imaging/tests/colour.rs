//! The colour steps, the display transform, regions with binning, and the
//! files, read back by independent decoders.

use std::io::Cursor;

use oscope_compute::equivalence::{Tolerance, assert_equivalent};
use oscope_compute::{Image, Shape, cpu};
use oscope_core::{BayerPattern, PixelRect};
use oscope_imaging::color::{
    ColorError, ColourCorrection, Curve, Demosaic, Display, chart, correct, crop, demosaic,
    demosaic_region, encode_png, encode_tiff16, mosaic, subtract_black, to_display,
};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};

fn image(width: u32, height: u32, channels: usize, data: &[f32]) -> Image {
    Image::from_vec(Shape::new(width, height, channels), data.to_vec()).unwrap()
}

#[test]
fn the_black_level_is_subtracted_and_the_rest_stretched() {
    let mut raw = image(4, 1, 1, &[0.0, 0.1, 0.55, 1.0]);
    subtract_black(&mut raw, 0.1).unwrap();
    assert_equivalent(
        &image(4, 1, 1, &[0.0, 0.0, 0.5, 1.0]),
        &raw,
        Tolerance::abs(1e-6),
    );
    for wrong in [-0.1, 1.0, f32::NAN] {
        assert!(
            matches!(subtract_black(&mut raw, wrong), Err(ColorError::Setting(_))),
            "{wrong}"
        );
    }
}

#[test]
fn white_balance_and_the_matrix_apply_in_that_order() {
    let mut rgb = image(1, 1, 3, &[0.2, 0.4, 0.1]);
    let correction = ColourCorrection {
        gains: [2.0, 1.0, 4.0],
        matrix: [[1.0, 0.0, 0.0], [0.5, 0.5, 0.0], [0.0, -1.0, 2.0]],
    };
    correct(&mut rgb, &correction).unwrap();
    // Gains first: (0.4, 0.4, 0.4); then the matrix's rows.
    assert_equivalent(
        &image(1, 1, 3, &[0.4, 0.4, 0.4]),
        &rgb,
        Tolerance::abs(1e-6),
    );
    let mut unchanged = image(1, 1, 3, &[0.2, 0.4, 0.1]);
    correct(&mut unchanged, &ColourCorrection::default()).unwrap();
    assert_eq!(unchanged.data(), [0.2, 0.4, 0.1]);
    assert_eq!(
        correct(&mut image(1, 1, 1, &[0.5]), &correction),
        Err(ColorError::Channels(1))
    );
}

#[test]
fn the_display_transform_follows_its_curves_and_points() {
    let show = |v: f32, display: Display| to_display(&image(1, 1, 1, &[v]), &display).unwrap()[0];
    let with = |curve| Display {
        curve,
        ..Display::default()
    };
    // sRGB: 0.5 is 0.7354 of white; its linear toe ends at 0.0031308.
    assert_eq!(
        [0.0, 0.5, 1.0].map(|v| show(v, Display::default())),
        [0, 188, 255]
    );
    assert_eq!(show(0.003, Display::default()), 10);
    assert_eq!(show(0.5, with(Curve::Linear)), 128);
    assert_eq!(show(0.5, with(Curve::Gamma { gamma: 2.2 })), 186);
    // Black and white points stretch 0.2–0.6 to the whole range.
    let points = Display {
        black: 0.2,
        white: 0.6,
        curve: Curve::Linear,
    };
    assert_eq!(
        [0.1, 0.2, 0.5, 0.6, 0.9].map(|v| show(v, points)),
        [0, 0, 191, 255, 255]
    );
    // Out of range and NaN-free: every 16-bit level, in order.
    let levels: Vec<f32> = (0..=u16::MAX).map(|c| f32::from(c) / 65_535.0).collect();
    let all = image(levels.len() as u32, 1, 1, &levels);
    for curve in [Curve::Linear, Curve::Gamma { gamma: 2.2 }, Curve::Srgb] {
        let shown = to_display(&all, &with(curve)).unwrap();
        assert!(shown.windows(2).all(|w| w[0] <= w[1]), "{curve:?}");
        assert_eq!((shown[0], shown[65_535]), (0, 255));
    }
    assert_eq!(show(-0.5, Display::default()), 0);
    assert_eq!(show(7.0, Display::default()), 255);
    let flat = Display {
        black: 0.5,
        white: 0.5,
        curve: Curve::Linear,
    };
    assert!(matches!(
        to_display(&all, &flat),
        Err(ColorError::Setting(_))
    ));
}

#[test]
fn a_region_with_binning_matches_demosaicing_all_then_cropping_and_binning() {
    let mut random = StdRng::seed_from_u64(9);
    let truth = chart(96, 64);
    for case in 0..120 {
        let pattern = [
            BayerPattern::Rggb,
            BayerPattern::Grbg,
            BayerPattern::Gbrg,
            BayerPattern::Bggr,
        ][case % 4];
        let method = Demosaic::ALL[case % 3];
        let raw = mosaic(&truth, pattern).unwrap();
        let whole = demosaic(&raw, pattern, method).unwrap();
        // Rectangles anywhere, edges and corners included.
        let (w, h) = (whole.width(), whole.height());
        let (x, y) = (random.random_range(0..w), random.random_range(0..h));
        let rect = PixelRect::new(
            x,
            y,
            random.random_range(1..=w - x),
            random.random_range(1..=h - y),
        );
        let bin = random.random_range(1..=4);
        let expected = cpu::downsample(&crop(&whole, rect).unwrap(), bin).unwrap();
        let region = demosaic_region(&raw, pattern, method, rect, bin).unwrap();
        assert_equivalent(&expected, &region, Tolerance::EXACT);
    }
    let raw = mosaic(&truth, BayerPattern::Rggb).unwrap();
    for (method, rect) in [
        (Demosaic::Bilinear, PixelRect::new(90, 0, 7, 1)),
        (Demosaic::Superpixel, PixelRect::new(0, 0, 49, 1)),
        (Demosaic::MalvarHeCutler, PixelRect::new(0, 0, 0, 4)),
    ] {
        let refused = demosaic_region(&raw, BayerPattern::Rggb, method, rect, 1);
        assert!(
            matches!(refused, Err(ColorError::Region(..))),
            "{method:?} {rect:?}"
        );
    }
}

#[test]
fn png_and_16_bit_tiff_files_read_back() {
    let rgb = chart(37, 21);
    // PNG, of the display's bytes.
    let shown = to_display(&rgb, &Display::default()).unwrap();
    let mut file = Vec::new();
    encode_png(&mut file, &shown, 37, 21).unwrap();
    let mut decoder = png::Decoder::new(Cursor::new(file)).read_info().unwrap();
    let mut read = vec![0; decoder.output_buffer_size().unwrap()];
    let info = decoder.next_frame(&mut read).unwrap();
    assert_eq!(
        (info.width, info.height, info.color_type, info.bit_depth),
        (37, 21, png::ColorType::Rgb, png::BitDepth::Eight)
    );
    assert_eq!(&read[..info.buffer_size()], &shown[..]);
    assert!(matches!(
        encode_png(Vec::new(), &shown, 36, 21),
        Err(ColorError::Setting(_))
    ));

    // TIFF, of the samples themselves, to 16 bits.
    let mut file = Vec::new();
    encode_tiff16(&mut file, &rgb).unwrap();
    let mut decoder = tiff::decoder::Decoder::new(Cursor::new(file)).unwrap();
    assert_eq!(decoder.dimensions().unwrap(), (37, 21));
    assert_eq!(decoder.colortype().unwrap(), tiff::ColorType::RGB(16));
    let tiff::decoder::DecodingResult::U16(samples) = decoder.read_image().unwrap() else {
        panic!("16-bit samples");
    };
    let expected: Vec<u16> = rgb
        .data()
        .iter()
        .map(|v| (v * 65_535.0).round() as u16)
        .collect();
    assert_eq!(samples, expected);
    assert_eq!(
        encode_tiff16(Vec::new(), &image(1, 1, 1, &[0.5])),
        Err(ColorError::Channels(1))
    );
}
