//! Feathered blending and the flat field (P39's spec, 7 and 8): a chunk of
//! the mosaic from the parts of the tiles over it, each pixel the mean of
//! theirs weighted by how far each is from its own edge (Szeliski, 2006);
//! and the shading the tiles share, estimated from them.

use crate::tiles::PixelBox;

/// A tile's part over a chunk: where in the tile, its samples by rows,
/// interleaved, and where in the chunk it goes.
#[derive(Clone, Debug)]
pub struct Part<'a> {
    /// The part, in the tile's pixels.
    pub within: PixelBox,
    /// Its samples, interleaved, by rows.
    pub samples: &'a [f32],
    /// Its top left in the chunk.
    pub to: (u32, u32),
}

/// The shading the tiles share, at a coarse level: each pixel's gain, about
/// 1, interleaved.
#[derive(Clone, Debug, PartialEq)]
pub struct Flat {
    /// Its width, a coarse tile's.
    pub width: u32,
    /// Its height.
    pub height: u32,
    /// Its channels.
    pub channels: u32,
    /// A tile's pixels to one of its: 2 to the level's power.
    pub scale: u32,
    /// The gains.
    pub gains: Vec<f32>,
}

impl Flat {
    /// The gain at a tile's pixel `(u, v)`, channel `k`.
    fn gain(&self, u: i64, v: i64, k: usize) -> f32 {
        let s = i64::from(self.scale);
        let x = (u / s).clamp(0, i64::from(self.width) - 1) as usize;
        let y = (v / s).clamp(0, i64::from(self.height) - 1) as usize;
        self.gains[(y * self.width as usize + x) * self.channels as usize + k]
    }
}

/// A chunk `width` × `height` of `channels`, from `parts` of tiles
/// `tile` pixels in size, each divided by `flat` if given: the mean of the
/// parts at each pixel, each weighted by its distance from its tile's
/// nearest edge plus a half. Pixels no part covers are 0.
pub fn blend(
    (width, height): (u32, u32),
    channels: u32,
    tile: (u32, u32),
    parts: &[Part<'_>],
    flat: Option<&Flat>,
) -> Vec<f32> {
    let c = channels as usize;
    let mut sums = vec![0.0_f32; width as usize * height as usize * c];
    let mut weights = vec![0.0_f32; width as usize * height as usize];
    let (tw, th) = (i64::from(tile.0), i64::from(tile.1));
    for part in parts {
        let pw = part.within.width() as usize;
        for (row, line) in part.samples.chunks_exact(pw * c).enumerate() {
            let v = part.within.y0 + row as i64;
            let y = part.to.1 as usize + row;
            let dy = v.min(th - 1 - v);
            for (col, pixel) in line.chunks_exact(c).enumerate() {
                let u = part.within.x0 + col as i64;
                let x = part.to.0 as usize + col;
                let w = u.min(tw - 1 - u).min(dy) as f32 + 0.5;
                let at = y * width as usize + x;
                weights[at] += w;
                for (k, &s) in pixel.iter().enumerate() {
                    let gain = flat.map_or(1.0, |f| f.gain(u, v, k));
                    sums[at * c + k] += w * s / gain;
                }
            }
        }
    }
    for (pixel, w) in sums.chunks_exact_mut(c).zip(&weights) {
        if *w > 0.0 {
            pixel.iter_mut().for_each(|s| *s /= *w);
        }
    }
    sums
}

/// The flat field of `tiles`, each `width` × `height` of `channels`,
/// interleaved, at a level `scale` times coarser than the tiles: each
/// pixel's median over them, smoothed by a Gaussian a 32nd of the shorter
/// side wide, and divided by its channel's mean. A raw tile's coarse level
/// mixes its cell's colours, so its flat corrects shading, not colour.
pub fn flat_field(
    tiles: &[&[f32]],
    (width, height): (u32, u32),
    channels: u32,
    scale: u32,
) -> Flat {
    let (w, h, c) = (width as usize, height as usize, channels as usize);
    let mut median = vec![1.0_f32; w * h * c];
    let mut values = Vec::with_capacity(tiles.len());
    for (i, m) in median.iter_mut().enumerate() {
        values.clear();
        values.extend(tiles.iter().map(|t| t[i]));
        values.sort_by(f32::total_cmp);
        let n = values.len();
        if n > 0 {
            *m = 0.5 * (values[(n - 1) / 2] + values[n / 2]);
        }
    }
    let sigma = (w.min(h) as f64 / 32.0).max(1.0);
    let mut gains = vec![1.0_f32; w * h * c];
    for k in 0..c {
        let mut plane: Vec<f32> = median.iter().skip(k).step_by(c).copied().collect();
        smooth(&mut plane, w, h, sigma);
        let mean = plane.iter().sum::<f32>() / plane.len().max(1) as f32;
        for (i, v) in plane.iter().enumerate() {
            gains[i * c + k] = if mean > 0.0 && *v > 0.0 {
                v / mean
            } else {
                1.0
            };
        }
    }
    Flat {
        width,
        height,
        channels,
        scale,
        gains,
    }
}

/// Blurs `plane`, `w` × `h`, by a Gaussian of `sigma` pixels, along rows
/// then columns, the edges repeated.
fn smooth(plane: &mut [f32], w: usize, h: usize, sigma: f64) {
    let radius = (3.0 * sigma).ceil() as i64;
    let kernel: Vec<f32> = (-radius..=radius)
        .map(|i| (-(i as f64).powi(2) / (2.0 * sigma * sigma)).exp() as f32)
        .collect();
    let total: f32 = kernel.iter().sum();
    let pass = |plane: &mut [f32], len: usize, count: usize, at: &dyn Fn(usize, usize) -> usize| {
        let mut line = vec![0.0_f32; len];
        for n in 0..count {
            for (i, out) in line.iter_mut().enumerate() {
                *out = kernel
                    .iter()
                    .enumerate()
                    .map(|(j, k)| {
                        let src = (i as i64 + j as i64 - radius).clamp(0, len as i64 - 1) as usize;
                        k * plane[at(n, src)]
                    })
                    .sum::<f32>()
                    / total;
            }
            for (i, v) in line.iter().enumerate() {
                plane[at(n, i)] = *v;
            }
        }
    };
    pass(plane, w, h, &|row, i| row * w + i);
    pass(plane, h, w, &|col, i| i * w + col);
}
