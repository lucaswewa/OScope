//! The CPU kernels: the reference that every CUDA twin is tested against.
//! They run with rayon, in whichever pool calls them.

use oscope_core::frame::{FrameView, PixelFormat};
use rayon::prelude::*;

use crate::{ComputeError, Image, Shape};

/// Each sample of a frame as a float from 0 to 1: a 16-bit sample divided
/// by 65,535, an 8-bit one by 255. A mosaic stays a mosaic, with one
/// channel, and RGB keeps its three. 16-bit data is aligned high, so this
/// holds whatever the sensor's bit depth.
pub fn to_f32(view: &FrameView<'_>) -> Result<Image, ComputeError> {
    let format = view.format();
    if !converts(format) {
        return Err(ComputeError::Unsupported(format));
    }
    let shape = Shape::new(view.width(), view.height(), format.channels());
    let mut image = Image::zeros(shape);
    if shape.is_empty() {
        return Ok(image);
    }
    let wide = format.sample_bytes() == 2;
    image
        .data_mut()
        .par_chunks_mut(shape.row_len())
        .enumerate()
        .for_each(|(y, out)| {
            let row = view.row(y as u32).expect("y is inside the view");
            if wide {
                for (sample, bytes) in out.iter_mut().zip(row.as_chunks::<2>().0) {
                    *sample = f32::from(u16::from_le_bytes(*bytes)) / 65_535.0;
                }
            } else {
                for (sample, &byte) in out.iter_mut().zip(row) {
                    *sample = f32::from(byte) / 255.0;
                }
            }
        });
    Ok(image)
}

/// 2 × 2 binning: each output pixel is the sum of a 2 × 2 block, channel by
/// channel. An odd last row or column is dropped, as a sensor drops it. On
/// a mosaic, each block holds one of each colour.
pub fn bin2x2(image: &Image) -> Image {
    let channels = image.channels();
    let shape = Shape::new(image.width() / 2, image.height() / 2, channels);
    let mut out = Image::zeros(shape);
    if shape.is_empty() {
        return out;
    }
    out.data_mut()
        .par_chunks_mut(shape.row_len())
        .enumerate()
        .for_each(|(y, row)| {
            let top = image.row(2 * y as u32);
            let bottom = image.row(2 * y as u32 + 1);
            for (x, pixel) in row.chunks_exact_mut(channels).enumerate() {
                for (c, sample) in pixel.iter_mut().enumerate() {
                    let i = 2 * x * channels + c;
                    *sample = top[i] + top[i + channels] + bottom[i] + bottom[i + channels];
                }
            }
        });
    out
}

/// Box downsampling by `factor`: each output pixel is the mean of a
/// `factor` × `factor` block, channel by channel. The blocks at the right
/// and bottom edges may be smaller, and are averaged over the pixels they
/// have, so the output covers the image: ⌈width / factor⌉ × ⌈height /
/// factor⌉.
pub fn downsample(image: &Image, factor: u32) -> Result<Image, ComputeError> {
    if factor == 0 {
        return Err(ComputeError::Factor);
    }
    let (width, height, channels) = (image.width(), image.height(), image.channels());
    let shape = Shape::new(width.div_ceil(factor), height.div_ceil(factor), channels);
    let mut out = Image::zeros(shape);
    if shape.is_empty() {
        return Ok(out);
    }
    out.data_mut()
        .par_chunks_mut(shape.row_len())
        .enumerate()
        .for_each(|(oy, row)| {
            let y0 = oy as u32 * factor;
            let y1 = (y0 + factor).min(height);
            for y in y0..y1 {
                let source = image.row(y);
                for (ox, pixel) in row.chunks_exact_mut(channels).enumerate() {
                    let x0 = ox * factor as usize;
                    let x1 = (x0 + factor as usize).min(width as usize);
                    for (c, sum) in pixel.iter_mut().enumerate() {
                        *sum += (x0..x1).map(|x| source[x * channels + c]).sum::<f32>();
                    }
                }
            }
            let rows = (y1 - y0) as usize;
            for (ox, pixel) in row.chunks_exact_mut(channels).enumerate() {
                let x0 = ox * factor as usize;
                let columns = (x0 + factor as usize).min(width as usize) - x0;
                let count = (rows * columns) as f32;
                pixel.iter_mut().for_each(|sum| *sum /= count);
            }
        });
    Ok(out)
}

/// The formats [`to_f32`] takes: one plane of 8- or 16-bit samples.
pub fn converts(format: PixelFormat) -> bool {
    format.planes() == 1 && format.sample_bytes() <= 2
}
