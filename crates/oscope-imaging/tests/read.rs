//! Reading image files: PNG, JPEG and TIFF, each written here and read
//! back as linear RGB.

use std::path::Path;

use oscope_imaging::color::{encode_png, read_image};
use tiff::encoder::compression::DeflateLevel;
use tiff::encoder::{Compression, TiffEncoder, colortype};

/// sRGB's encoded value to linear light (IEC 61966-2-1).
fn linear(v: f32) -> f32 {
    if v <= 0.040_45 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}

/// A 6 × 4 RGB pattern, every sample different.
fn pattern() -> Vec<u8> {
    (0..6 * 4 * 3).map(|i| (i * 10 + 5) as u8).collect()
}

#[test]
fn an_8_bit_png_is_read_as_linear_light() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("pattern.png");
    let mut file = Vec::new();
    encode_png(&mut file, &pattern(), 6, 4).unwrap();
    std::fs::write(&path, file).unwrap();
    let pages = read_image(&path).unwrap();
    assert_eq!(pages.len(), 1);
    assert_eq!(
        (pages[0].width(), pages[0].height(), pages[0].channels()),
        (6, 4, 3)
    );
    for (read, written) in pages[0].data().iter().zip(pattern()) {
        assert!((read - linear(f32::from(written) / 255.0)).abs() < 1e-6);
    }
}

#[test]
fn a_16_bit_grey_png_fills_all_three_channels() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("grey.png");
    let samples: Vec<u16> = (0..8).map(|i| i * 8_000).collect();
    let mut file = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut file, 4, 2);
        encoder.set_color(png::ColorType::Grayscale);
        encoder.set_depth(png::BitDepth::Sixteen);
        let mut writer = encoder.write_header().unwrap();
        let bytes: Vec<u8> = samples.iter().flat_map(|s| s.to_be_bytes()).collect();
        writer.write_image_data(&bytes).unwrap();
    }
    std::fs::write(&path, file).unwrap();
    let image = &read_image(&path).unwrap()[0];
    for (rgb, s) in image.data().chunks(3).zip(&samples) {
        let expected = linear(f32::from(*s) / 65_535.0);
        assert!(rgb.iter().all(|v| (v - expected).abs() < 1e-6), "{rgb:?}");
    }
}

/// Writes `pages` of 16-bit grey, `width` × `height`, with `compression`.
fn write_stack(path: &Path, width: u32, height: u32, pages: &[Vec<u16>], compression: Compression) {
    let mut file = std::fs::File::create(path).unwrap();
    let mut encoder = TiffEncoder::new(&mut file)
        .unwrap()
        .with_compression(compression);
    for page in pages {
        encoder
            .write_image::<colortype::Gray16>(width, height, page)
            .unwrap();
    }
}

#[test]
fn every_page_of_a_tiff_stack_is_read_as_linear_data() {
    // Three pages, uncompressed, LZW and deflate: TIFF's samples are taken
    // as linear, scaled by 65,535.
    let folder = tempfile::tempdir().unwrap();
    let pages: Vec<Vec<u16>> = (0..3_u16)
        .map(|k| (0..20_u16).map(|i| k * 20_000 + i * 100).collect())
        .collect();
    for compression in [
        Compression::Uncompressed,
        Compression::Lzw,
        Compression::Deflate(DeflateLevel::default()),
    ] {
        let path = folder.path().join("stack.tif");
        write_stack(&path, 5, 4, &pages, compression);
        let read = read_image(&path).unwrap();
        assert_eq!(read.len(), 3);
        for (image, page) in read.iter().zip(&pages) {
            assert_eq!((image.width(), image.height()), (5, 4));
            for (rgb, s) in image.data().chunks(3).zip(page) {
                assert!(
                    rgb.iter()
                        .all(|v| (v - f32::from(*s) / 65_535.0).abs() < 1e-6)
                );
            }
        }
    }
}

#[test]
fn a_jpeg_is_read_within_its_compression_s_error() {
    // Flat 8 × 8 blocks of colour, which JPEG keeps closely at quality 100.
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("blocks.jpg");
    let (w, h) = (32_u16, 16_u16);
    let colour = |x: u16, y: u16| [(x / 8) as u8 * 60, (y / 8) as u8 * 120 + 40, 200];
    let rgb: Vec<u8> = (0..h)
        .flat_map(|y| (0..w).flat_map(move |x| colour(x, y)))
        .collect();
    let mut file = Vec::new();
    jpeg_encoder::Encoder::new(&mut file, 100)
        .encode(&rgb, w, h, jpeg_encoder::ColorType::Rgb)
        .unwrap();
    std::fs::write(&path, file).unwrap();
    let image = &read_image(&path).unwrap()[0];
    assert_eq!((image.width(), image.height()), (32, 16));
    let worst = image
        .data()
        .iter()
        .zip(&rgb)
        .map(|(read, written)| (read - linear(f32::from(*written) / 255.0)).abs())
        .fold(0.0_f32, f32::max);
    assert!(worst < 0.02, "{worst}");
}

#[test]
fn other_files_are_refused_with_their_names() {
    let folder = tempfile::tempdir().unwrap();
    let text = folder.path().join("notes.txt");
    std::fs::write(&text, "not an image").unwrap();
    let missing = folder.path().join("missing.png");
    for path in [text, missing] {
        let error = read_image(&path).unwrap_err().to_string();
        assert!(error.contains(&path.display().to_string()), "{error}");
    }
}
