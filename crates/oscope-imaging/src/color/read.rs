//! Reading image files, for imported samples (P14): PNG, JPEG and TIFF, as
//! linear RGB.

use std::io::Cursor;
use std::path::Path;

use oscope_compute::{Image, Shape};
use tiff::ColorType;
use tiff::decoder::{Decoder, DecodingResult};
use zune_core::colorspace::ColorSpace;
use zune_core::options::DecoderOptions;
use zune_jpeg::JpegDecoder;

use super::ColorError;

/// Reads the image file at `path` as linear RGB from 0 to 1: every page of
/// a TIFF, or a PNG's or JPEG's one image.
///
/// - **PNG and JPEG** are taken as sRGB, the web's encoding, and decoded to
///   linear light (IEC 61966-2-1).
/// - **TIFF** is taken as linear data, as scientific images and stacks
///   are: integers scaled by their type's largest value, floats as they
///   are. Uncompressed, LZW, deflate and JPEG compression are read.
///
/// Grey becomes equal red, green and blue; alpha is dropped; palettes are
/// expanded.
pub fn read_image(path: &Path) -> Result<Vec<Image>, ColorError> {
    let failed = |what: String| ColorError::File(format!("{}: {what}", path.display()));
    let bytes = std::fs::read(path).map_err(|e| failed(e.to_string()))?;
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase);
    let pages = match extension.as_deref() {
        Some("png") => decode_png(&bytes).map(|image| vec![image]),
        Some("jpg" | "jpeg") => decode_jpeg(&bytes).map(|image| vec![image]),
        Some("tif" | "tiff") => decode_tiff(&bytes),
        _ => Err("not a PNG, JPEG or TIFF".to_owned()),
    };
    pages.map_err(failed)
}

/// sRGB's encoded value to linear light.
fn linear(v: f32) -> f32 {
    if v <= 0.040_45 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}

/// `values`, `channels` to a pixel, as an RGB image: grey to all three,
/// alpha dropped.
fn rgb(width: u32, height: u32, channels: usize, values: &[f32]) -> Result<Image, String> {
    let data = values
        .chunks(channels)
        .flat_map(|px| match channels {
            1 | 2 => [px[0]; 3],
            _ => [px[0], px[1], px[2]],
        })
        .collect();
    Image::from_vec(Shape::new(width, height, 3), data).map_err(|e| e.to_string())
}

fn decode_png(bytes: &[u8]) -> Result<Image, String> {
    let mut decoder = png::Decoder::new(Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::EXPAND);
    let mut reader = decoder.read_info().map_err(|e| e.to_string())?;
    let size = reader.output_buffer_size().ok_or("too large to decode")?;
    let mut buffer = vec![0; size];
    let info = reader.next_frame(&mut buffer).map_err(|e| e.to_string())?;
    let samples = &buffer[..info.buffer_size()];
    let values: Vec<f32> = if info.bit_depth == png::BitDepth::Sixteen {
        samples
            .chunks(2)
            .map(|b| linear(f32::from(u16::from_be_bytes([b[0], b[1]])) / 65_535.0))
            .collect()
    } else {
        samples
            .iter()
            .map(|b| linear(f32::from(*b) / 255.0))
            .collect()
    };
    rgb(info.width, info.height, info.color_type.samples(), &values)
}

fn decode_jpeg(bytes: &[u8]) -> Result<Image, String> {
    let options = DecoderOptions::default().jpeg_set_out_colorspace(ColorSpace::RGB);
    let mut decoder = JpegDecoder::new_with_options(Cursor::new(bytes), options);
    let pixels = decoder.decode().map_err(|e| e.to_string())?;
    let info = decoder.info().ok_or("no image")?;
    let values: Vec<f32> = pixels
        .iter()
        .map(|b| linear(f32::from(*b) / 255.0))
        .collect();
    rgb(u32::from(info.width), u32::from(info.height), 3, &values)
}

fn decode_tiff(bytes: &[u8]) -> Result<Vec<Image>, String> {
    let mut decoder = Decoder::new(Cursor::new(bytes)).map_err(|e| e.to_string())?;
    let mut pages = Vec::new();
    loop {
        let (width, height) = decoder.dimensions().map_err(|e| e.to_string())?;
        let channels = match decoder.colortype().map_err(|e| e.to_string())? {
            ColorType::Gray(_) => 1,
            ColorType::GrayA(_) => 2,
            ColorType::RGB(_) => 3,
            ColorType::RGBA(_) => 4,
            other => return Err(format!("pixels of {other:?}")),
        };
        let values: Vec<f32> = match decoder.read_image().map_err(|e| e.to_string())? {
            DecodingResult::U8(v) => v.iter().map(|x| f32::from(*x) / 255.0).collect(),
            DecodingResult::U16(v) => v.iter().map(|x| f32::from(*x) / 65_535.0).collect(),
            DecodingResult::U32(v) => v
                .iter()
                .map(|x| (f64::from(*x) / f64::from(u32::MAX)) as f32)
                .collect(),
            DecodingResult::F32(v) => v,
            DecodingResult::F64(v) => v.iter().map(|x| *x as f32).collect(),
            _ => return Err("samples of a type other than 8, 16 or 32 bits".to_owned()),
        };
        pages.push(rgb(width, height, channels, &values)?);
        if !decoder.more_images() {
            return Ok(pages);
        }
        decoder.next_image().map_err(|e| e.to_string())?;
    }
}
