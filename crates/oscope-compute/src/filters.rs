//! Filters on the CPU (P23): the Gaussian, median and bilateral filters,
//! and CLAHE. They are the references their CUDA twins are compared with
//! (ADR-0010), so they add and multiply in the order the kernels do
//! (`kernels/filters.cu`). Pixels beyond the edges repeat the edge pixels.

use rayon::prelude::*;

use crate::{ComputeError, Image, Shape};

/// The largest median window's radius: a window of 15 × 15 samples.
pub const MEDIAN_RADIUS: u32 = 7;

/// The largest spatial sigma of the bilateral filter, in pixels: a window
/// of 33 × 33.
pub const BILATERAL_SIGMA: f32 = 8.0;

/// The largest Gaussian sigma, in pixels.
pub const GAUSSIAN_SIGMA: f32 = 32.0;

fn setting(message: String) -> ComputeError {
    ComputeError::Setting(message)
}

/// `i` within `0..n`: the edge's index beyond the edges.
fn clamp(i: i64, n: u32) -> usize {
    i.clamp(0, i64::from(n) - 1) as usize
}

/// A Gaussian's weights, from `−r` to `r` with `r = ⌈3σ⌉`, summing to 1.
pub fn gaussian_weights(sigma: f32) -> Result<Vec<f32>, ComputeError> {
    if sigma.is_nan() || sigma <= 0.0 || sigma > GAUSSIAN_SIGMA {
        return Err(setting(format!(
            "a Gaussian's sigma, {sigma}, must be above 0 and at most {GAUSSIAN_SIGMA}"
        )));
    }
    let radius = (3.0 * f64::from(sigma)).ceil() as i64;
    let two_s2 = 2.0 * f64::from(sigma) * f64::from(sigma);
    let weights: Vec<f64> = (-radius..=radius)
        .map(|k| (-((k * k) as f64) / two_s2).exp())
        .collect();
    let total: f64 = weights.iter().sum();
    Ok(weights.iter().map(|w| (w / total) as f32).collect())
}

/// One pass of a convolution with `weights`, along rows or down columns.
fn convolve(image: &Image, weights: &[f32], along_rows: bool) -> Image {
    let shape = image.shape();
    let (width, height, channels) = (shape.width, shape.height, shape.channels);
    let radius = (weights.len() / 2) as i64;
    let mut out = Image::zeros(shape);
    out.data_mut()
        .par_chunks_mut(shape.row_len())
        .enumerate()
        .for_each(|(y, row)| {
            for (i, sample) in row.iter_mut().enumerate() {
                let (x, c) = ((i / channels) as i64, i % channels);
                let mut sum = 0.0_f32;
                for k in -radius..=radius {
                    let (xx, yy) = match along_rows {
                        true => (clamp(x + k, width), y),
                        false => (x as usize, clamp(y as i64 + k, height)),
                    };
                    sum += weights[(k + radius) as usize]
                        * image.data()[(yy * width as usize + xx) * channels + c];
                }
                *sample = sum;
            }
        });
    out
}

/// A Gaussian blur of `sigma` pixels: separable, along rows, then down
/// columns.
pub fn gaussian(image: &Image, sigma: f32) -> Result<Image, ComputeError> {
    let weights = gaussian_weights(sigma)?;
    if image.shape().is_empty() {
        return Ok(image.clone());
    }
    Ok(convolve(&convolve(image, &weights, true), &weights, false))
}

/// The median of each `(2r + 1)²` window, channel by channel, for a
/// `radius` up to [`MEDIAN_RADIUS`].
pub fn median(image: &Image, radius: u32) -> Result<Image, ComputeError> {
    if radius > MEDIAN_RADIUS {
        return Err(setting(format!(
            "a median's radius, {radius}, is at most {MEDIAN_RADIUS}"
        )));
    }
    let shape = image.shape();
    if shape.is_empty() {
        return Ok(image.clone());
    }
    let (width, height, channels) = (shape.width, shape.height, shape.channels);
    let r = i64::from(radius);
    let mut out = Image::zeros(shape);
    out.data_mut()
        .par_chunks_mut(shape.row_len())
        .enumerate()
        .for_each(|(y, row)| {
            let mut window = Vec::with_capacity(((2 * r + 1) * (2 * r + 1)) as usize);
            for (i, sample) in row.iter_mut().enumerate() {
                let (x, c) = ((i / channels) as i64, i % channels);
                window.clear();
                for dy in -r..=r {
                    let yy = clamp(y as i64 + dy, height);
                    for dx in -r..=r {
                        window.push(
                            image.data()
                                [(yy * width as usize + clamp(x + dx, width)) * channels + c],
                        );
                    }
                }
                let middle = window.len() / 2;
                let order =
                    |a: &f32, b: &f32| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal);
                *sample = *window.select_nth_unstable_by(middle, order).1;
            }
        });
    Ok(out)
}

/// The bilateral filter's spatial weights, `exp(−(dx² + dy²) / 2σ²)` over
/// the window `r = ⌈2σ⌉` across, row by row, and `r`.
pub fn bilateral_weights(sigma_space: f32) -> Result<(Vec<f32>, u32), ComputeError> {
    if sigma_space.is_nan() || sigma_space <= 0.0 || sigma_space > BILATERAL_SIGMA {
        let most = BILATERAL_SIGMA;
        return Err(setting(format!(
            "a bilateral filter's spatial sigma, {sigma_space}, must be above 0 and at most {most}"
        )));
    }
    let radius = (2.0 * f64::from(sigma_space)).ceil() as i64;
    let two_s2 = 2.0 * f64::from(sigma_space) * f64::from(sigma_space);
    let weights = (-radius..=radius)
        .flat_map(|dy| {
            (-radius..=radius).map(move |dx| (-((dx * dx + dy * dy) as f64) / two_s2).exp() as f32)
        })
        .collect();
    Ok((weights, radius as u32))
}

/// The bilateral filter (Tomasi and Manduchi, 1998): each pixel the mean of
/// its window, weighted by distance, `σ_space` pixels, and by difference,
/// the colours' Euclidean distance against `σ_range`. Up to four channels.
pub fn bilateral(image: &Image, sigma_space: f32, sigma_range: f32) -> Result<Image, ComputeError> {
    let (spatial, radius) = bilateral_weights(sigma_space)?;
    if sigma_range.is_nan() || sigma_range <= 0.0 {
        return Err(setting(format!(
            "a bilateral filter's range sigma, {sigma_range}, must be above 0"
        )));
    }
    let shape = image.shape();
    if shape.channels > 4 {
        return Err(ComputeError::Channels {
            expected: 4,
            got: shape.channels,
        });
    }
    if shape.is_empty() {
        return Ok(image.clone());
    }
    let (width, height, channels) = (shape.width, shape.height, shape.channels);
    let inverse = 1.0 / (2.0 * sigma_range * sigma_range);
    let r = i64::from(radius);
    let data = image.data();
    let mut out = Image::zeros(shape);
    out.data_mut()
        .par_chunks_mut(shape.row_len())
        .enumerate()
        .for_each(|(y, row)| {
            for (x, pixel) in row.chunks_mut(channels).enumerate() {
                let centre = &data[(y * width as usize + x) * channels..][..channels];
                let (mut sums, mut total, mut t) = ([0.0_f32; 4], 0.0_f32, 0);
                for dy in -r..=r {
                    let yy = clamp(y as i64 + dy, height);
                    for dx in -r..=r {
                        let q = &data
                            [(yy * width as usize + clamp(x as i64 + dx, width)) * channels..]
                            [..channels];
                        let mut d2 = 0.0_f32;
                        for c in 0..channels {
                            let d = q[c] - centre[c];
                            d2 += d * d;
                        }
                        let w = spatial[t] * (-d2 * inverse).exp();
                        t += 1;
                        total += w;
                        for c in 0..channels {
                            sums[c] += w * q[c];
                        }
                    }
                }
                for (c, value) in pixel.iter_mut().enumerate() {
                    *value = sums[c] / total;
                }
            }
        });
    Ok(out)
}

/// CLAHE's settings.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Clahe {
    /// The tiles across and down: each tile's histogram maps its pixels.
    pub tiles: (u32, u32),
    /// How far above the mean a bin may count before it is clipped, the
    /// excess shared among all bins: 1 equalises nothing, more equalises
    /// more.
    pub clip: f32,
    /// The histograms' bins, over 0–1.
    pub bins: u32,
}

impl Clahe {
    /// Checks the settings for an image of `shape`: the tiles it takes, at
    /// most one a pixel.
    pub fn tiles_for(&self, shape: Shape) -> Result<(u32, u32), ComputeError> {
        let (tx, ty) = self.tiles;
        if tx == 0
            || ty == 0
            || self.clip.is_nan()
            || self.clip < 1.0
            || !(2..=65_536).contains(&self.bins)
        {
            return Err(setting(format!(
                "CLAHE takes tiles of at least 1, a clip of at least 1 and 2–65,536 bins: {self:?}"
            )));
        }
        Ok((tx.min(shape.width.max(1)), ty.min(shape.height.max(1))))
    }

    /// The clip, in counts, for a tile of `count` pixels.
    pub fn clip_count(&self, count: u32) -> u32 {
        ((self.clip * count as f32 / self.bins as f32) as u32).max(1)
    }
}

/// The bin of `v` among `bins` over 0–1.
fn bin(v: f32, bins: u32) -> usize {
    ((v.clamp(0.0, 1.0) * bins as f32) as u32).min(bins - 1) as usize
}

/// Tile `t` of `tiles` along `n` pixels: its first pixel and the one past
/// its last.
fn span(t: u32, tiles: u32, n: u32) -> (u32, u32) {
    let at = |t: u32| (u64::from(t) * u64::from(n) / u64::from(tiles)) as u32;
    (at(t), at(t + 1))
}

/// The tiles whose centres are either side of pixel `p`, along `n` pixels
/// in `tiles` tiles, and how far `p` is from the first towards the second.
pub(crate) fn around(p: u32, n: u32, tiles: u32) -> (u32, u32, f32) {
    let centre = |t: u32| {
        let (start, end) = span(t, tiles, n);
        0.5_f32 * (start + end) as f32
    };
    let at = p as f32 + 0.5_f32;
    if at <= centre(0) {
        return (0, 0, 0.0);
    }
    for t in 0..tiles - 1 {
        let (low, high) = (centre(t), centre(t + 1));
        if at < high {
            return (t, t + 1, (at - low) / (high - low));
        }
    }
    (tiles - 1, tiles - 1, 0.0)
}

/// CLAHE's lookup tables: for each channel, then each tile row by row, its
/// `bins` values. Each tile's histogram is clipped, the excess shared among
/// the bins, the first bins taking one more each for what doesn't divide;
/// its table is the clipped histogram's cumulative share.
pub fn clahe_maps(image: &Image, clahe: &Clahe) -> Result<Vec<f32>, ComputeError> {
    let shape = image.shape();
    let (tx, ty) = clahe.tiles_for(shape)?;
    let (channels, bins) = (shape.channels, clahe.bins);
    let tiles: Vec<(usize, u32, u32)> = (0..channels)
        .flat_map(|c| (0..ty).flat_map(move |j| (0..tx).map(move |i| (c, i, j))))
        .collect();
    let maps = tiles.par_iter().flat_map_iter(|&(c, i, j)| {
        let ((x0, x1), (y0, y1)) = (span(i, tx, shape.width), span(j, ty, shape.height));
        let mut histogram = vec![0_u32; bins as usize];
        for y in y0..y1 {
            for x in x0..x1 {
                histogram[bin(
                    image.data()[(y as usize * shape.width as usize + x as usize) * channels + c],
                    bins,
                )] += 1;
            }
        }
        let count = (x1 - x0) * (y1 - y0);
        let clip = clahe.clip_count(count);
        let mut excess = 0;
        for h in &mut histogram {
            if *h > clip {
                excess += *h - clip;
                *h = clip;
            }
        }
        let (each, rest) = (excess / bins, excess % bins);
        let mut sum = 0_u32;
        let map: Vec<f32> = histogram
            .iter()
            .enumerate()
            .map(|(b, h)| {
                sum += h + each + u32::from((b as u32) < rest);
                if count > 0 {
                    sum as f32 / count as f32
                } else {
                    0.0
                }
            })
            .collect();
        map
    });
    Ok(maps.collect())
}

/// Contrast-limited adaptive histogram equalisation (Zuiderveld, 1994),
/// channel by channel over 0–1: each pixel through its tile's table,
/// interpolated bilinearly between the tables of the four tiles around it.
pub fn clahe(image: &Image, clahe: &Clahe) -> Result<Image, ComputeError> {
    let maps = clahe_maps(image, clahe)?;
    let shape = image.shape();
    let (tx, ty) = clahe.tiles_for(shape)?;
    let (width, channels, bins) = (shape.width, shape.channels, clahe.bins as usize);
    let mut out = Image::zeros(shape);
    if shape.is_empty() {
        return Ok(out);
    }
    out.data_mut()
        .par_chunks_mut(shape.row_len())
        .enumerate()
        .for_each(|(y, row)| {
            let (j0, j1, fy) = around(y as u32, shape.height, ty);
            for (i, sample) in row.iter_mut().enumerate() {
                let (x, c) = ((i / channels) as u32, i % channels);
                let (i0, i1, fx) = around(x, width, tx);
                let b = bin(image.data()[y * shape.row_len() + i], clahe.bins);
                let map = |tx_: u32, ty_: u32| {
                    maps[((c * ty as usize + ty_ as usize) * tx as usize + tx_ as usize) * bins + b]
                };
                let top = (1.0_f32 - fx) * map(i0, j0) + fx * map(i1, j0);
                let bottom = (1.0_f32 - fx) * map(i0, j1) + fx * map(i1, j1);
                *sample = (1.0_f32 - fy) * top + fy * bottom;
            }
        });
    Ok(out)
}
