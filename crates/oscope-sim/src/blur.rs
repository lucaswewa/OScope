//! The Gaussian blur that stands in for the point spread function: exact
//! for small blurs, and by cascaded box filters for large ones, whose cost
//! doesn't grow with the blur (Wells, 1986; Kovesi, 2010).

use oscope_compute::Image;
use rayon::prelude::*;

/// The standard deviation, in pixels, from which the blur uses box filters.
/// Below it, a kernel 6σ wide is cheap enough.
pub const BOXES_FROM: f64 = 8.0;

/// How many box filters stand in for a large Gaussian. Four come within
/// 2% of its standard deviation, and within 5% of its peak anywhere along
/// an axis.
const BOXES: usize = 4;

/// Blurs `image`, of any number of channels, with a Gaussian of standard
/// deviation `sigma` pixels, one axis after the other, repeating the edge
/// pixels. A blur under 0.2 pixels leaves it as it is.
///
/// From [`BOXES_FROM`] pixels it applies four box filters in turn along
/// each axis, of the widths Kovesi gives for the Gaussian's variance
/// (*Fast almost-Gaussian filtering*, 2010): by the central limit theorem
/// their result approaches a Gaussian (Wells, *Efficient synthesis of
/// Gaussian filters by cascaded uniform filters*, 1986).
pub fn gaussian(image: &mut Image, sigma: f64) {
    let (w, h, c) = (
        image.width() as usize,
        image.height() as usize,
        image.channels(),
    );
    if sigma < 0.2 || w == 0 || h == 0 || c == 0 {
        return;
    }
    let pass = |data: &mut [f32], width: usize| {
        if sigma < BOXES_FROM {
            let weights = kernel(sigma);
            along_rows(data, width * c, |line, out| {
                convolve(line, out, c, &weights)
            });
        } else {
            for radius in box_radii(sigma, BOXES) {
                along_rows(data, width * c, |line, out| boxes(line, out, c, radius));
            }
        }
    };
    pass(image.data_mut(), w);
    let mut turned = transpose(image.data(), w, h, c);
    pass(&mut turned, h);
    image
        .data_mut()
        .copy_from_slice(&transpose(&turned, h, w, c));
}

/// A Gaussian's weights from −3σ to 3σ, summing to 1.
fn kernel(sigma: f64) -> Vec<f32> {
    let radius = (3.0 * sigma).ceil() as i64;
    let raw: Vec<f64> = (-radius..=radius)
        .map(|k| (-(k * k) as f64 / (2.0 * sigma * sigma)).exp())
        .collect();
    let total: f64 = raw.iter().sum();
    raw.iter().map(|w| (w / total) as f32).collect()
}

/// The radii of `n` box filters whose cascade has the variance of a
/// Gaussian of `sigma`: `m` of the odd width just under the ideal, the rest
/// two wider (Kovesi, 2010).
pub fn box_radii(sigma: f64, n: usize) -> Vec<usize> {
    let variance = 12.0 * sigma * sigma;
    let nf = n as f64;
    let ideal = (variance / nf + 1.0).sqrt();
    let mut lower = ideal.floor() as i64;
    if lower % 2 == 0 {
        lower -= 1;
    }
    let wl = lower as f64;
    let m = ((variance - nf * wl * wl - 4.0 * nf * wl - 3.0 * nf) / (-4.0 * wl - 4.0))
        .round()
        .clamp(0.0, nf) as usize;
    (0..n)
        .map(|i| if i < m { lower } else { lower + 2 })
        .map(|width| (width.max(1) as usize - 1) / 2)
        .collect()
}

/// Runs `f` on each row of `data`, `row` samples long, in parallel, from a
/// copy of the row into the row itself.
fn along_rows(data: &mut [f32], row: usize, f: impl Fn(&[f32], &mut [f32]) + Sync) {
    data.par_chunks_mut(row).for_each(|out| {
        let line = out.to_vec();
        f(&line, out);
    });
}

/// One row through the Gaussian's weights.
fn convolve(line: &[f32], out: &mut [f32], c: usize, weights: &[f32]) {
    let n = (line.len() / c) as i64;
    let radius = (weights.len() / 2) as i64;
    for x in 0..n {
        for ch in 0..c {
            let mut acc = 0.0_f32;
            for (k, weight) in weights.iter().enumerate() {
                let at = (x + k as i64 - radius).clamp(0, n - 1) as usize;
                acc += weight * line[at * c + ch];
            }
            out[x as usize * c + ch] = acc;
        }
    }
}

/// One row through a box filter of `radius`, by a running sum.
fn boxes(line: &[f32], out: &mut [f32], c: usize, radius: usize) {
    let n = (line.len() / c) as i64;
    let r = radius as i64;
    let width = (2 * r + 1) as f64;
    for ch in 0..c {
        let at = |i: i64| f64::from(line[i.clamp(0, n - 1) as usize * c + ch]);
        let mut sum: f64 = (-r..=r).map(at).sum();
        for x in 0..n {
            out[x as usize * c + ch] = (sum / width) as f32;
            sum += at(x + r + 1) - at(x - r);
        }
    }
}

/// `data`, `w` × `h` pixels of `c` channels, with its rows and columns
/// swapped.
fn transpose(data: &[f32], w: usize, h: usize, c: usize) -> Vec<f32> {
    let mut out = vec![0.0_f32; data.len()];
    out.par_chunks_mut(h * c).enumerate().for_each(|(x, row)| {
        for y in 0..h {
            row[y * c..(y + 1) * c].copy_from_slice(&data[(y * w + x) * c..(y * w + x + 1) * c]);
        }
    });
    out
}
