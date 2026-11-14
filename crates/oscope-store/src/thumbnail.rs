//! Thumbnails (plan §5.12): a JPEG of at most 256 × 256 pixels, made from
//! an image's pyramid when its record is made, for the Gallery's cards.

use std::path::Path;

use oscope_compute::{Image, Shape};
use oscope_imaging::color::{Curve, Display, encode_jpeg, to_display};

use crate::{ImageReader, Result, SampleType, StoreError};

/// A record's thumbnail, in its folder.
pub(crate) const FILE: &str = "thumbnail.jpg";

/// A thumbnail's longest side, at most.
const SIDE: usize = 256;

fn failed(error: impl std::fmt::Display) -> StoreError {
    StoreError::Zarr(format!("a thumbnail: {error}"))
}

/// The 0.5th and 99.5th percentiles of `samples`.
fn percentiles(samples: &[f32]) -> (f32, f32) {
    let mut sorted: Vec<f32> = samples.iter().copied().filter(|s| s.is_finite()).collect();
    if sorted.is_empty() {
        return (0.0, 1.0);
    }
    sorted.sort_by(f32::total_cmp);
    let at = |p: f64| sorted[((sorted.len() - 1) as f64 * p).round() as usize];
    (at(0.005), at(0.995))
}

/// Writes `reader`'s thumbnail to `out`: its middle plane, made smaller by
/// whole steps from the level of at most 1,024 pixels across that gives the
/// largest. 8-bit images are shown as they are; others stretched from their
/// 0.5th to their 99.5th percentile, through sRGB's curve, as linear data
/// from a camera wants.
pub(crate) fn make(reader: &ImageReader, out: &Path) -> Result<()> {
    let info = reader.info();
    let longest = |level: u32| {
        let (w, h) = info.level_size(level);
        w.max(h) as usize
    };
    let side = |level: u32| longest(level).div_ceil(longest(level).div_ceil(SIDE));
    let level = (0..info.levels)
        .filter(|&level| longest(level) <= 4 * SIDE)
        .max_by_key(|&level| (side(level), level))
        .unwrap_or(info.levels - 1);
    let (width, height) = info.level_size(level);
    let samples = reader.read(level, info.planes / 2, (0, 0), (width, height))?;
    let unit = samples.to_unit();
    let display = match info.sample {
        SampleType::U8 => Display {
            black: 0.0,
            white: 1.0,
            curve: Curve::Linear,
        },
        _ => {
            let (black, white) = percentiles(&unit);
            Display {
                black,
                white: white.max(black + 1e-6),
                curve: Curve::Srgb,
            }
        }
    };
    let (w, h, channels) = (width as usize, height as usize, info.channels as usize);
    let step = w.max(h).div_ceil(SIDE).max(1);
    let (tw, th) = (w.div_ceil(step), h.div_ceil(step));
    let mut small = Vec::with_capacity(tw * th * channels);
    for y in 0..th {
        for x in 0..tw {
            let (rows, cols) = (
                y * step..(y * step + step).min(h),
                x * step..(x * step + step).min(w),
            );
            let count = (rows.len() * cols.len()) as f32;
            for c in 0..channels {
                let sum: f32 = rows
                    .clone()
                    .flat_map(|yy| cols.clone().map(move |xx| (yy * w + xx) * channels + c))
                    .map(|i| unit[i])
                    .sum();
                small.push(sum / count);
            }
        }
    }
    let image =
        Image::from_vec(Shape::new(tw as u32, th as u32, channels), small).map_err(failed)?;
    let shown = to_display(&image, &display).map_err(failed)?;
    let rgb: Vec<u8> = shown
        .chunks_exact(channels)
        .flat_map(|p| match channels {
            1 => [p[0]; 3],
            2 => [p[0], p[1], 0],
            _ => [p[0], p[1], p[2]],
        })
        .collect();
    let jpeg = encode_jpeg(&rgb, tw as u32, th as u32, 85).map_err(failed)?;
    std::fs::write(out, jpeg).map_err(crate::io(out))
}
