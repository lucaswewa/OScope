//! Registration (P31): the shift between two images, by phase correlation,
//! on the CPU and on CUDA, as P31's notes specify it (Kuglin and Hines,
//! 1975; Harris, 1978; Knapp and Carter, 1976; Willert and Gharib, 1991).
//!
//! 1. Each image's mean is taken away, and the Hann window applied.
//! 2. The normalised cross-power spectrum of their transforms is weighted
//!    by a Gaussian of the frequency, and transformed back, scaled so that a
//!    perfect match peaks at 1: the correlation surface, whose peak is a
//!    sampled Gaussian at the shift.
//! 3. The shifts considered (the search window around an expected shift, or
//!    every shift) are gathered with a pixel around them, by either backend.
//! 4. From that region, the same code finds the integer peak, refines it by
//!    the three-point Gaussian estimate, and scores it by the peak ratio.

use std::f64::consts::PI;
use std::sync::Arc;

use oscope_compute::Image;
use rayon::prelude::*;
use rustfft::num_complex::Complex32;
use rustfft::{Fft, FftPlanner};

#[cfg(feature = "cuda")]
pub mod gpu;

/// The working memory that the CPU's pairs may take at once, about.
const CPU_MEMORY: usize = 256 << 20;

/// Why a registration can't be made.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum RegistrationError {
    /// The images or the options don't suit it.
    #[error("{0}")]
    Refused(String),
    /// The compute backend failed.
    #[error(transparent)]
    Compute(#[from] oscope_compute::ComputeError),
}

/// How to register.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RegisterOptions {
    /// The shift expected, in pixels: only shifts within `radius` of it,
    /// along each axis, are considered. `None` considers every shift.
    pub expected: Option<(f64, f64)>,
    /// How far from the expected shift to look, in pixels. Within the
    /// peak's neighbourhood (`3 sigma`), no shift is left to compare the
    /// peak with, and the confidence is 1 whenever the peak is positive.
    pub radius: u32,
    /// Whether to apply the Hann window.
    pub window: bool,
    /// The spectral weight's width σ_w, in pixels, at least 1: the
    /// standard deviation of the Gaussian that the correlation peak
    /// becomes. 2 suits images blurred as the 5× to the 20× blur them; a
    /// blurrier image wants about its blur's.
    pub sigma: f64,
}

impl Default for RegisterOptions {
    /// Every shift, with the window, and σ_w of 2 pixels.
    fn default() -> Self {
        Self {
            expected: None,
            radius: 16,
            window: true,
            sigma: 2.0,
        }
    }
}

/// The shift that carries A's content onto B's, `B(x, y) ≈ A(x − dx, y −
/// dy)`, and how sure it is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Registration {
    /// Across, in pixels: positive when B's content lies to the right of
    /// A's.
    pub dx: f64,
    /// Down, in pixels.
    pub dy: f64,
    /// From 0 to 1: one minus the ratio of the highest value outside the
    /// peak's neighbourhood to the peak's.
    pub confidence: f64,
    /// The peak's value: about the fraction of the images that overlap.
    pub peak: f64,
}

/// The region of the correlation surface gathered: the shifts considered
/// and a pixel around them, from shift `(x0, y0)`, `w` × `h` of them,
/// wrapping round the surface.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Region {
    pub(crate) x0: i64,
    pub(crate) y0: i64,
    pub(crate) w: usize,
    pub(crate) h: usize,
}

/// What registering images of one size needs besides them, the same for
/// both backends.
#[derive(Debug)]
pub(crate) struct Setup {
    pub(crate) width: usize,
    pub(crate) height: usize,
    /// The window across a row and down a column: ones without it.
    pub(crate) window: (Vec<f32>, Vec<f32>),
    /// The weight by frequency index, across and down: G is their product.
    pub(crate) weight: (Vec<f32>, Vec<f32>),
    /// The surface's scale, 1 / ΣG.
    pub(crate) scale: f32,
    pub(crate) region: Region,
    /// The peak's neighbourhood, in pixels each way: ⌈3σ_w⌉.
    pub(crate) reach: usize,
}

impl Setup {
    /// Checks `pairs` and `options`, and sets up their registration.
    pub(crate) fn new(
        pairs: &[(&Image, &Image)],
        options: &RegisterOptions,
    ) -> Result<Self, RegistrationError> {
        let (width, height) = checked(pairs)?;
        let sigma = options.sigma;
        if !(sigma.is_finite() && sigma >= 1.0) {
            return refused(format!("a weight of σ_w = {sigma} px: at least 1"));
        }
        if let Some((ex, ey)) = options.expected
            && !(ex.is_finite() && ey.is_finite())
        {
            return refused(format!("an expected shift of ({ex}, {ey})"));
        }
        let window = |n: usize| -> Vec<f32> {
            (0..n)
                .map(|i| if options.window { hann(i, n) } else { 1.0 })
                .collect()
        };
        let weight = |n: usize| -> Vec<f32> {
            (0..n)
                .map(|k| (-2.0 * PI * PI * sigma * sigma * frequency(k, n).powi(2)).exp() as f32)
                .collect()
        };
        let weight = (weight(width), weight(height));
        let sum = |g: &[f32]| g.iter().map(|g| f64::from(*g)).sum::<f64>();
        Ok(Self {
            width,
            height,
            window: (window(width), window(height)),
            scale: (1.0 / (sum(&weight.0) * sum(&weight.1))) as f32,
            weight,
            region: region(width, height, options),
            reach: (3.0 * sigma).ceil() as usize,
        })
    }
}

fn refused<T>(why: String) -> Result<T, RegistrationError> {
    Err(RegistrationError::Refused(why))
}

/// Checks that `pairs` are of one channel, all of one size, at least
/// 8 × 8: their width and height.
fn checked(pairs: &[(&Image, &Image)]) -> Result<(usize, usize), RegistrationError> {
    let Some((first, _)) = pairs.first() else {
        return refused("no pairs to register".to_owned());
    };
    let shape = first.shape();
    for (a, b) in pairs {
        for image in [a, b] {
            if image.shape() != shape {
                let (w, h) = (image.width(), image.height());
                let c = image.channels();
                return refused(format!(
                    "an image of {w} × {h} × {c} among images of {} × {} × {}",
                    shape.width, shape.height, shape.channels
                ));
            }
        }
    }
    if shape.channels != 1 {
        let channels = shape.channels;
        return refused(format!(
            "images of {channels} channels, where one is needed"
        ));
    }
    if shape.width < 8 || shape.height < 8 {
        let (w, h) = (shape.width, shape.height);
        return refused(format!("images of {w} × {h}: at least 8 × 8"));
    }
    Ok((shape.width as usize, shape.height as usize))
}

/// The region of a `width` × `height` surface that `options` consider.
fn region(width: usize, height: usize, options: &RegisterOptions) -> Region {
    let r = options.radius as usize;
    let axis = |size: usize, expected: Option<f64>| -> (i64, usize) {
        match expected {
            Some(centre) if 2 * r < size => (centre.round() as i64 - r as i64 - 1, 2 * r + 3),
            _ => (-(size as i64 / 2) - 1, size + 2),
        }
    };
    let (x0, w) = axis(width, options.expected.map(|e| e.0));
    let (y0, h) = axis(height, options.expected.map(|e| e.1));
    Region { x0, y0, w, h }
}

/// Frequency index `k` of `n`, in cycles per pixel: k / n below n / 2, and
/// k / n − 1 from it.
fn frequency(k: usize, n: usize) -> f64 {
    let f = k as f64 / n as f64;
    if k < n / 2 { f } else { f - 1.0 }
}

/// The Hann window's value at `n` of `size`: ½ (1 − cos(2πn / (N − 1))).
fn hann(n: usize, size: usize) -> f32 {
    (0.5 * (1.0 - (2.0 * PI * n as f64 / (size - 1) as f64).cos())) as f32
}

/// Each image's mean, A's then B's for each pair, summed in a fixed order;
/// or a refusal if an image holds a value that isn't finite.
pub(crate) fn means(pairs: &[(&Image, &Image)]) -> Result<Vec<f32>, RegistrationError> {
    let mean = |image: &Image| -> f64 {
        let data = image.data();
        let sums: Vec<f64> = data
            .par_chunks(1 << 16)
            .map(|chunk| chunk.iter().map(|v| f64::from(*v)).sum())
            .collect();
        sums.iter().sum::<f64>() / data.len() as f64
    };
    let mut means = Vec::with_capacity(2 * pairs.len());
    for image in pairs.iter().flat_map(|(a, b)| [a, b]) {
        let mean = mean(image);
        if !mean.is_finite() {
            return refused("an image holds a value that isn't finite".to_owned());
        }
        means.push(mean as f32);
    }
    Ok(means)
}

/// The registration that a gathered `region`'s `values` give: the integer
/// peak among the shifts considered, refined along each axis by the
/// three-point Gaussian estimate, and scored by the highest value outside
/// its neighbourhood of `reach` pixels each way.
pub(crate) fn locate(values: &[f32], region: Region, reach: usize) -> Registration {
    let (w, h) = (region.w, region.h);
    let at = |x: usize, y: usize| values[y * w + x];
    // The shifts considered are the rows and columns inside the margin, row
    // by row in parallel. The peak is the first of equals in reading order.
    let rows = || values.par_chunks(w).enumerate().skip(1).take(h - 2);
    let first = |a: (usize, usize, f32), b: (usize, usize, f32)| {
        let later = (b.1, b.0) > (a.1, a.0);
        if b.2 > a.2 || (b.2 == a.2 && !later) {
            b
        } else {
            a
        }
    };
    let (px, py, c0) = rows()
        .map(|(y, row)| {
            let inner = row[1..w - 1].iter().enumerate();
            let (x, v) = inner.fold((0, f32::NEG_INFINITY), |m, (x, v)| match *v > m.1 {
                true => (x, *v),
                false => m,
            });
            (x + 1, y, v)
        })
        .reduce(|| (1, 1, f32::NEG_INFINITY), first);
    let dx = (region.x0 + px as i64) as f64 + refine(at(px - 1, py), c0, at(px + 1, py));
    let dy = (region.y0 + py as i64) as f64 + refine(at(px, py - 1), c0, at(px, py + 1));
    // The highest value outside the peak's neighbourhood: in a row beside
    // it, only left and right of the neighbourhood.
    let c2 = rows()
        .map(|(y, row)| {
            let (left, right) = match y.abs_diff(py) > reach {
                true => (w - 1, w - 1),
                false => (px.saturating_sub(reach), (px + reach + 1).min(w - 1)),
            };
            let outside = row[1..left.max(1)].iter().chain(&row[right..w - 1]);
            outside.fold(f32::NEG_INFINITY, |m, v| m.max(*v))
        })
        .reduce(|| f32::NEG_INFINITY, f32::max);
    let confidence = match c0 > 0.0 {
        true => (1.0 - f64::from(c2.max(0.0)) / f64::from(c0)).clamp(0.0, 1.0),
        false => 0.0,
    };
    Registration {
        dx,
        dy,
        confidence,
        peak: f64::from(c0),
    }
}

/// The three-point Gaussian estimate of the peak's offset from its middle
/// sample (Willert and Gharib, 1991): 0 unless the three are positive and
/// curve down, and never beyond a neighbour.
fn refine(before: f32, peak: f32, after: f32) -> f64 {
    if !(before > 0.0 && peak > 0.0 && after > 0.0) {
        return 0.0;
    }
    let (b, p, a) = (
        f64::from(before).ln(),
        f64::from(peak).ln(),
        f64::from(after).ln(),
    );
    let curvature = b - 2.0 * p + a;
    match curvature < 0.0 {
        true => ((b - a) / (2.0 * curvature)).clamp(-1.0, 1.0),
        false => 0.0,
    }
}

/// The transforms one size needs, forward and back, along rows and columns.
struct Transforms {
    rows: (Arc<dyn Fft<f32>>, Arc<dyn Fft<f32>>),
    columns: (Arc<dyn Fft<f32>>, Arc<dyn Fft<f32>>),
}

impl Transforms {
    fn new(width: usize, height: usize) -> Self {
        let mut planner = FftPlanner::new();
        Self {
            rows: (
                planner.plan_fft_forward(width),
                planner.plan_fft_inverse(width),
            ),
            columns: (
                planner.plan_fft_forward(height),
                planner.plan_fft_inverse(height),
            ),
        }
    }

    /// The two-dimensional transform of `data`, `width` wide, in place.
    fn apply(&self, data: &mut [Complex32], width: usize, inverse: bool) {
        let height = data.len() / width;
        let (rows, columns) = match inverse {
            false => (&self.rows.0, &self.columns.0),
            true => (&self.rows.1, &self.columns.1),
        };
        data.par_chunks_mut(width).for_each(|row| rows.process(row));
        let mut transposed = vec![Complex32::default(); data.len()];
        for (y, row) in data.chunks(width).enumerate() {
            for (x, value) in row.iter().enumerate() {
                transposed[x * height + y] = *value;
            }
        }
        transposed
            .par_chunks_mut(height)
            .for_each(|column| columns.process(column));
        for (x, column) in transposed.chunks(height).enumerate() {
            for (y, value) in column.iter().enumerate() {
                data[y * width + x] = *value;
            }
        }
    }
}

/// An image, its mean taken away and the window applied, as complex
/// numbers: in the order its CUDA twin computes it.
fn prepared(image: &Image, mean: f32, setup: &Setup) -> Vec<Complex32> {
    let (across, down) = (&setup.window.0, &setup.window.1);
    let rows = image.data().chunks(setup.width).zip(down);
    rows.flat_map(|(row, d)| {
        let row = row.iter().zip(across);
        row.map(move |(v, a)| Complex32::new((v - mean) * a * d, 0.0))
    })
    .collect()
}

/// One frequency's normalised cross power, F_B · conj(F_A) / |F_A| |F_B|,
/// weighted by `g`, or 0 where the magnitude is below 10⁻³⁰: in the order
/// its CUDA twin computes it. The magnitudes are taken apart, so the
/// product of two large ones can't overflow.
fn cross_power(a: Complex32, b: Complex32, g: f32) -> Complex32 {
    let re = b.re * a.re + b.im * a.im;
    let im = b.im * a.re - b.re * a.im;
    let magnitude = (a.re * a.re + a.im * a.im).sqrt() * (b.re * b.re + b.im * b.im).sqrt();
    if magnitude < 1e-30 {
        return Complex32::default();
    }
    let scale = g / magnitude;
    Complex32::new(re * scale, im * scale)
}

/// The region of one pair's correlation surface, on the CPU.
fn correlated(
    transforms: &Transforms,
    setup: &Setup,
    (a, b): (&Image, &Image),
    means: &[f32],
) -> Vec<f32> {
    let width = setup.width;
    let (mut fa, mut fb) = (prepared(a, means[0], setup), prepared(b, means[1], setup));
    transforms.apply(&mut fa, width, false);
    transforms.apply(&mut fb, width, false);
    let (across, down) = (&setup.weight.0, &setup.weight.1);
    for ((ra, rb), d) in fa.chunks_mut(width).zip(fb.chunks(width)).zip(down) {
        for ((a, b), g) in ra.iter_mut().zip(rb).zip(across) {
            *a = cross_power(*a, *b, g * d);
        }
    }
    transforms.apply(&mut fa, width, true);
    let Region { x0, y0, w, h } = setup.region;
    let (width, height) = (width as i64, setup.height as i64);
    let mut out = Vec::with_capacity(w * h);
    for ry in 0..h as i64 {
        let y = (y0 + ry).rem_euclid(height);
        for rx in 0..w as i64 {
            let x = (x0 + rx).rem_euclid(width);
            out.push(fa[(y * width + x) as usize].re * setup.scale);
        }
    }
    out
}

/// Registers `b` against `a`, on the CPU.
pub fn register(
    a: &Image,
    b: &Image,
    options: &RegisterOptions,
) -> Result<Registration, RegistrationError> {
    Ok(register_batch(&[(a, b)], options)?.remove(0))
}

/// Registers each pair's B against its A, on the CPU: as if each were
/// registered alone. Pairs run in parallel, as many at once as fit in
/// 256 MiB of working memory.
pub fn register_batch(
    pairs: &[(&Image, &Image)],
    options: &RegisterOptions,
) -> Result<Vec<Registration>, RegistrationError> {
    let setup = Setup::new(pairs, options)?;
    let means = means(pairs)?;
    let transforms = Transforms::new(setup.width, setup.height);
    let working = 3 * setup.width * setup.height * size_of::<Complex32>();
    let at_once = (CPU_MEMORY / working).max(1);
    let mut out = Vec::with_capacity(pairs.len());
    for (chunk, means) in pairs.chunks(at_once).zip(means.chunks(2 * at_once)) {
        let found: Vec<Registration> = chunk
            .par_iter()
            .zip(means.par_chunks(2))
            .map(|(pair, means)| {
                let values = correlated(&transforms, &setup, *pair, means);
                locate(&values, setup.region, setup.reach)
            })
            .collect();
        out.extend(found);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A sampled Gaussian of `sigma` centred at `centre`, over `region`.
    fn gaussian_region(region: Region, centre: (f64, f64), sigma: f64, height: f32) -> Vec<f32> {
        let mut values = Vec::new();
        for ry in 0..region.h {
            for rx in 0..region.w {
                let (x, y) = (
                    (region.x0 + rx as i64) as f64,
                    (region.y0 + ry as i64) as f64,
                );
                let r2 = (x - centre.0).powi(2) + (y - centre.1).powi(2);
                values.push(height * (-r2 / (2.0 * sigma * sigma)).exp() as f32);
            }
        }
        values
    }

    #[test]
    fn a_sampled_gaussian_peak_is_found_exactly() {
        let region = Region {
            x0: -9,
            y0: -9,
            w: 19,
            h: 19,
        };
        for centre in [(0.0, 0.0), (0.3, -0.45), (-2.5, 4.49), (6.2, -7.8)] {
            let found = locate(&gaussian_region(region, centre, 2.0, 0.8), region, 6);
            assert!(
                (found.dx - centre.0).abs() < 1e-4,
                "{found:?} for {centre:?}"
            );
            assert!(
                (found.dy - centre.1).abs() < 1e-4,
                "{found:?} for {centre:?}"
            );
            assert!((found.peak - 0.8).abs() < 0.06, "{found:?}");
        }
    }

    #[test]
    fn the_confidence_compares_the_peak_with_the_highest_value_outside_its_neighbourhood() {
        let region = Region {
            x0: -12,
            y0: -12,
            w: 25,
            h: 25,
        };
        let mut values = gaussian_region(region, (0.0, 0.0), 1.0, 0.8);
        let index = |x: i64, y: i64| ((y - region.y0) * region.w as i64 + x - region.x0) as usize;
        // Inside the neighbourhood, a second bump doesn't count; outside, it
        // does, but not in the margin, which holds no shift considered.
        values[index(3, 3)] = 0.6;
        assert!(locate(&values, region, 3).confidence > 0.999);
        values[index(-12, 0)] = 0.7;
        assert!(locate(&values, region, 3).confidence > 0.999);
        values[index(-11, 0)] = 0.2;
        assert!((locate(&values, region, 3).confidence - 0.75).abs() < 1e-6);
        // Nothing positive: no confidence, and no refinement.
        let flat = vec![-0.1; values.len()];
        let found = locate(&flat, region, 3);
        assert_eq!((found.confidence, found.dx, found.dy), (0.0, -11.0, -11.0));
    }

    #[test]
    fn regions_cover_every_shift_or_the_search_window_with_a_margin() {
        let every = RegisterOptions::default();
        assert_eq!(
            region(256, 9, &every),
            Region {
                x0: -129,
                y0: -5,
                w: 258,
                h: 11
            }
        );
        let near = RegisterOptions {
            expected: Some((10.4, -3.6)),
            radius: 5,
            ..every
        };
        assert_eq!(
            region(256, 64, &near),
            Region {
                x0: 4,
                y0: -10,
                w: 13,
                h: 13
            }
        );
        // A window as wide as the surface is the whole surface.
        assert_eq!(region(256, 10, &near).h, 12);
    }
}
