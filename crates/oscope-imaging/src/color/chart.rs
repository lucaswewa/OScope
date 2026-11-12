//! Scenes with known colours, the mosaics a sensor would see of them, and
//! how close a result comes: to measure demosaicing.

use std::f32::consts::PI;

use oscope_compute::{Image, Shape};
use oscope_core::{BayerColour, BayerPattern};
use rayon::prelude::*;

use super::ColorError;

/// A test chart in RGB, from 0 to 1, of any size:
///
/// - **the top half:** 24 colour patches, 6 across and 4 down, of evenly
///   spaced hues, bright and dark;
/// - **the bottom left:** a smooth gradient of all three colours;
/// - **the bottom right:** a grey zone plate, rings that get finer outwards,
///   where demosaicing errs most.
pub fn chart(width: u32, height: u32) -> Image {
    let shape = Shape::new(width, height, 3);
    let mut image = Image::zeros(shape);
    if shape.is_empty() {
        return image;
    }
    let (w, h) = (width as f32, height as f32);
    image
        .data_mut()
        .par_chunks_mut(shape.row_len())
        .enumerate()
        .for_each(|(y, row)| {
            let v = y as f32 / h;
            for (x, rgb) in row.as_chunks_mut::<3>().0.iter_mut().enumerate() {
                let u = x as f32 / w;
                *rgb = if v < 0.5 {
                    let patch = (u * 6.0) as u32 + 6 * (v * 8.0) as u32;
                    let value = if (patch / 6).is_multiple_of(2) {
                        0.9
                    } else {
                        0.45
                    };
                    hsv(patch as f32 / 24.0, 0.75, value)
                } else if u < 0.5 {
                    [u * 2.0, (v - 0.5) * 2.0, 1.0 - u * 2.0]
                } else {
                    // Rings whose frequency grows with the radius r, in
                    // pixels: r / (4 × reach) cycles a pixel, a quarter at
                    // the quarter's edge, where red and blue alias.
                    let (dx, dy) = ((u - 0.75) * w, (v - 0.75) * h);
                    let reach = (0.25 * w).min(0.25 * h).max(1.0);
                    let phase = PI * (dx * dx + dy * dy) / (4.0 * reach);
                    [0.5 + 0.4 * phase.cos(); 3]
                };
            }
        });
    image
}

/// A hue, saturation and value as RGB, each from 0 to 1.
fn hsv(hue: f32, saturation: f32, value: f32) -> [f32; 3] {
    let sector = hue * 6.0;
    let channel = |n: f32| {
        let k = (n + sector) % 6.0;
        value - value * saturation * k.min(4.0 - k).clamp(0.0, 1.0)
    };
    [channel(5.0), channel(3.0), channel(1.0)]
}

/// What a sensor with `pattern` sees of an RGB image: at each pixel, the one
/// colour its filter passes.
pub fn mosaic(rgb: &Image, pattern: BayerPattern) -> Result<Image, ColorError> {
    if rgb.channels() != 3 {
        return Err(ColorError::Channels(rgb.channels()));
    }
    let shape = Shape::new(rgb.width(), rgb.height(), 1);
    let mut out = Image::zeros(shape);
    if shape.is_empty() {
        return Ok(out);
    }
    out.data_mut()
        .par_chunks_mut(shape.row_len())
        .enumerate()
        .for_each(|(y, row)| {
            let source = rgb.row(y as u32);
            for (x, sample) in row.iter_mut().enumerate() {
                let channel = match pattern.colour_at(x as u32, y as u32) {
                    BayerColour::Red => 0,
                    BayerColour::Green => 1,
                    BayerColour::Blue => 2,
                };
                *sample = source[3 * x + channel];
            }
        });
    Ok(out)
}

/// The peak signal-to-noise ratio of `image` against `reference`, in dB,
/// for samples from 0 to 1, leaving out `border` pixels at each edge.
/// Infinite when they agree.
pub fn psnr(reference: &Image, image: &Image, border: u32) -> Result<f64, ColorError> {
    if reference.shape() != image.shape() {
        return Err(ColorError::Shape {
            expected: reference.shape(),
            got: image.shape(),
        });
    }
    let (width, height) = (reference.width(), reference.height());
    let (x0, y0, x1, y1) = (
        border,
        border,
        width.saturating_sub(border),
        height.saturating_sub(border),
    );
    let channels = reference.channels();
    let (sum, count) = (y0..y1)
        .into_par_iter()
        .map(|y| {
            let span = x0 as usize * channels..x1.max(x0) as usize * channels;
            let (a, b) = (&reference.row(y)[span.clone()], &image.row(y)[span]);
            let sum: f64 = a.iter().zip(b).map(|(a, b)| f64::from(a - b).powi(2)).sum();
            (sum, a.len())
        })
        .reduce(|| (0.0, 0), |(s, n), (t, m)| (s + t, n + m));
    if count == 0 {
        return Err(ColorError::Empty);
    }
    let mse = sum / count as f64;
    Ok(if mse == 0.0 {
        f64::INFINITY
    } else {
        -10.0 * mse.log10()
    })
}
