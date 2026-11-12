//! Image files: 8-bit PNG for viewing, and 16-bit TIFF that keeps the
//! detail.

use std::io::Write;

use oscope_compute::Image;

use super::ColorError;

/// Writes 8-bit RGB samples, such as [`to_display`](super::to_display)'s, as
/// a PNG.
pub fn encode_png(out: impl Write, rgb8: &[u8], width: u32, height: u32) -> Result<(), ColorError> {
    let expected = width as usize * height as usize * 3;
    if rgb8.len() != expected {
        return Err(ColorError::Setting(format!(
            "{} bytes for {width} × {height} RGB",
            rgb8.len()
        )));
    }
    let mut encoder = png::Encoder::new(out, width, height);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    let file = |error: png::EncodingError| ColorError::File(error.to_string());
    let mut writer = encoder.write_header().map_err(file)?;
    writer.write_image_data(rgb8).map_err(file)?;
    writer.finish().map_err(file)
}

/// Writes an RGB image as a 16-bit TIFF: each sample from 0–1 to 0–65,535,
/// rounded and clamped. The file is baseline TIFF, uncompressed and
/// little-endian, in one strip.
pub fn encode_tiff16(mut out: impl Write, rgb: &Image) -> Result<(), ColorError> {
    if rgb.channels() != 3 {
        return Err(ColorError::Channels(rgb.channels()));
    }
    let (width, height) = (rgb.width(), rgb.height());
    let data_len = u32::try_from(rgb.data().len() * 2)
        .ok()
        .filter(|len| len.checked_add(1024).is_some())
        .ok_or_else(|| ColorError::Setting("a TIFF of over 4 GB".to_owned()))?;
    // The header, the samples, the bits per sample, the two resolutions,
    // then the directory.
    let bits_at = 8 + data_len;
    let resolution_at = bits_at + 6;
    let directory_at = resolution_at + 16;
    const SHORT: u16 = 3;
    const LONG: u16 = 4;
    const RATIONAL: u16 = 5;
    let entries: [(u16, u16, u32, u32); 13] = [
        (256, LONG, 1, width),
        (257, LONG, 1, height),
        (258, SHORT, 3, bits_at),
        (259, SHORT, 1, 1),       // no compression
        (262, SHORT, 1, 2),       // RGB
        (273, LONG, 1, 8),        // the strip's offset
        (277, SHORT, 1, 3),       // samples per pixel
        (278, LONG, 1, height),   // rows per strip
        (279, LONG, 1, data_len), // the strip's length
        (282, RATIONAL, 1, resolution_at),
        (283, RATIONAL, 1, resolution_at + 8),
        (284, SHORT, 1, 1), // samples interleaved
        (296, SHORT, 1, 2), // resolution in inches
    ];
    let mut bytes = Vec::with_capacity(directory_at as usize + 2 + entries.len() * 12 + 4);
    bytes.extend_from_slice(b"II");
    bytes.extend_from_slice(&42_u16.to_le_bytes());
    bytes.extend_from_slice(&directory_at.to_le_bytes());
    for v in rgb.data() {
        let sample = (v.clamp(0.0, 1.0) * 65_535.0).round() as u16;
        bytes.extend_from_slice(&sample.to_le_bytes());
    }
    for word in [16_u16; 3] {
        bytes.extend_from_slice(&word.to_le_bytes());
    }
    for word in [72_u32, 1, 72, 1] {
        bytes.extend_from_slice(&word.to_le_bytes());
    }
    bytes.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    for (tag, kind, count, value) in entries {
        bytes.extend_from_slice(&tag.to_le_bytes());
        bytes.extend_from_slice(&kind.to_le_bytes());
        bytes.extend_from_slice(&count.to_le_bytes());
        // A single short sits in the first two bytes of the four.
        let value = if kind == SHORT && count == 1 {
            value & 0xffff
        } else {
            value
        };
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    bytes.extend_from_slice(&0_u32.to_le_bytes());
    out.write_all(&bytes)
        .map_err(|error| ColorError::File(error.to_string()))
}
