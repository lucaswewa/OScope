//! The JPEG encoder, against an independent decoder: what comes back, at
//! each quality, at awkward sizes, and what is refused.

use std::io::Cursor;

use oscope_imaging::color::encode_jpeg;
use zune_core::colorspace::ColorSpace;
use zune_core::options::DecoderOptions;
use zune_jpeg::JpegDecoder;

/// A test picture: smooth gradients, a disc with a sharp edge, and fine
/// stripes, `w` × `h`.
fn picture(w: u32, h: u32) -> Vec<u8> {
    (0..h)
        .flat_map(|y| {
            (0..w).flat_map(move |x| {
                let (fx, fy) = (f64::from(x) / f64::from(w), f64::from(y) / f64::from(h));
                let disc = (fx - 0.5).hypot(fy - 0.5) < 0.25;
                let stripe = if (x / 4) % 2 == 0 { 40.0 } else { 0.0 };
                let r = 255.0 * fx;
                let g = if disc { 220.0 } else { 60.0 + stripe };
                let b = 255.0 * fy;
                [r as u8, g as u8, b as u8]
            })
        })
        .collect()
}

/// The JPEG decoded by zune-jpeg, as RGB, with its size.
fn decode(jpeg: &[u8]) -> (Vec<u8>, u32, u32) {
    let options = DecoderOptions::default().jpeg_set_out_colorspace(ColorSpace::RGB);
    let mut decoder = JpegDecoder::new_with_options(Cursor::new(jpeg), options);
    let pixels = decoder.decode().expect("a JPEG zune-jpeg decodes");
    let info = decoder.info().unwrap();
    (pixels, u32::from(info.width), u32::from(info.height))
}

/// The peak signal-to-noise ratio of `b` against `a`, in dB.
fn psnr(a: &[u8], b: &[u8]) -> f64 {
    let mse = a
        .iter()
        .zip(b)
        .map(|(x, y)| (f64::from(*x) - f64::from(*y)).powi(2))
        .sum::<f64>()
        / a.len() as f64;
    10.0 * (255.0 * 255.0 / mse).log10()
}

#[test]
fn it_decodes_close_to_the_picture_at_each_quality() {
    let (w, h) = (640, 480);
    let rgb = picture(w, h);
    let mut last_size = usize::MAX;
    // 4:2:0 halves the chroma, so the picture's sharp coloured edges cost
    // a few dB at every quality: measured 36.0, 35.3, 31.2 and 27.7.
    for (quality, at_least) in [(95, 35.0), (80, 34.0), (50, 30.0), (20, 26.0)] {
        let jpeg = encode_jpeg(&rgb, w, h, quality).unwrap();
        let (back, bw, bh) = decode(&jpeg);
        assert_eq!((bw, bh), (w, h));
        let db = psnr(&rgb, &back);
        assert!(db >= at_least, "quality {quality}: {db:.1} dB");
        assert!(
            jpeg.len() < last_size,
            "quality {quality}: {} bytes",
            jpeg.len()
        );
        last_size = jpeg.len();
    }
}

#[test]
fn awkward_sizes_and_a_flat_colour_decode() {
    // Sizes that aren't whole units of 16, down to one pixel; and a flat
    // colour, which comes back within a step of rounding.
    for (w, h) in [(37, 23), (1, 1), (16, 17), (300, 9)] {
        let rgb = picture(w, h);
        let (back, bw, bh) = decode(&encode_jpeg(&rgb, w, h, 90).unwrap());
        assert_eq!((bw, bh), (w, h));
        assert!(psnr(&rgb, &back) > 24.0, "{w} × {h}");
    }
    let flat: Vec<u8> = [90_u8, 160, 30].repeat(48 * 32);
    let (back, _, _) = decode(&encode_jpeg(&flat, 48, 32, 90).unwrap());
    let worst = flat
        .iter()
        .zip(&back)
        .map(|(a, b)| a.abs_diff(*b))
        .max()
        .unwrap();
    assert!(worst <= 2, "{worst}");
}

#[test]
fn a_grey_gradient_keeps_its_detail_at_high_quality() {
    // No chroma to lose: at quality 95, above 40 dB.
    let (w, h) = (256_u32, 128_u32);
    let grey: Vec<u8> = (0..w * h)
        .flat_map(|i| [((i % w) as u8).wrapping_add((i / w) as u8 / 4); 3])
        .collect();
    let (back, _, _) = decode(&encode_jpeg(&grey, w, h, 95).unwrap());
    let db = psnr(&grey, &back);
    assert!(db > 40.0, "{db:.1} dB");
}

#[test]
fn restart_markers_follow_each_row_in_order() {
    // 300 rows: 19 rows of 16 × 16 units, so 18 markers, RST0 to RST7 in
    // turn. Decoders built on libjpeg check the numbers; zune-jpeg doesn't.
    let (w, h) = (64, 300);
    let jpeg = encode_jpeg(&picture(w, h), w, h, 80).unwrap();
    let scan = jpeg
        .windows(2)
        .position(|pair| pair == [0xff, 0xda])
        .unwrap();
    let markers: Vec<u8> = jpeg[scan..]
        .windows(2)
        .filter(|pair| pair[0] == 0xff && (0xd0..=0xd7).contains(&pair[1]))
        .map(|pair| pair[1] - 0xd0)
        .collect();
    let expected: Vec<u8> = (0..18).map(|i| i % 8).collect();
    assert_eq!(markers, expected);
}

#[test]
fn a_wrong_buffer_is_refused() {
    assert!(encode_jpeg(&[0; 10], 2, 2, 90).is_err());
    assert!(encode_jpeg(&[], 0, 0, 90).is_err());
}
