//! A textured field for registration's tests: periodic white noise, blurred
//! by a Gaussian as the optics blur, from which windows are cut at any
//! shift. A sub-pixel shift is made by the shift theorem, so that B(x, y) =
//! A(x − dx, y − dy) exactly, where the windows overlap.

use oscope_compute::{Image, Shape};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use rustfft::FftPlanner;
use rustfft::num_complex::Complex32;

/// A periodic random field: its spectrum, and its samples unshifted.
pub struct Field {
    size: usize,
    spectrum: Vec<Complex32>,
    samples: Vec<f32>,
}

/// A frequency index of `n`, in cycles per pixel.
fn frequency(k: usize, n: usize) -> f64 {
    let f = k as f64 / n as f64;
    if k < n / 2 { f } else { f - 1.0 }
}

/// The two-dimensional transform of a square `data`, in place.
fn fft2(data: &mut [Complex32], n: usize, inverse: bool) {
    let mut planner = FftPlanner::new();
    let fft = match inverse {
        false => planner.plan_fft_forward(n),
        true => planner.plan_fft_inverse(n),
    };
    for _ in 0..2 {
        data.chunks_mut(n).for_each(|row| fft.process(row));
        for y in 0..n {
            for x in y + 1..n {
                data.swap(y * n + x, x * n + y);
            }
        }
    }
}

/// A standard normal value, by Box and Muller's method.
pub fn normal(random: &mut StdRng) -> f64 {
    let (u, v) = (random.random::<f64>().max(1e-300), random.random::<f64>());
    (-2.0 * u.ln()).sqrt() * (std::f64::consts::TAU * v).cos()
}

impl Field {
    /// A `size` × `size` field from `seed`, blurred by a Gaussian of `blur`
    /// pixels (0 leaves it sharp), with a mean of 100 and a standard
    /// deviation of about 10 at any blur.
    pub fn new(size: usize, blur: f64, seed: u64) -> Self {
        let mut random = StdRng::seed_from_u64(seed);
        let mut data: Vec<Complex32> = (0..size * size)
            .map(|_| Complex32::new(normal(&mut random) as f32, 0.0))
            .collect();
        fft2(&mut data, size, false);
        let mut power = 0.0;
        for y in 0..size {
            for x in 0..size {
                let f2 = frequency(x, size).powi(2) + frequency(y, size).powi(2);
                let g = (-2.0 * std::f64::consts::PI.powi(2) * blur * blur * f2).exp();
                // No Nyquist row or column, so that every shift stays real.
                let g = if x == size / 2 || y == size / 2 {
                    0.0
                } else {
                    g
                };
                data[y * size + x] *= g as f32;
                power += g * g;
            }
        }
        // A window's samples then have a variance of the mean of g²: scale
        // it to a standard deviation of 10.
        let scale = 10.0 / (power / (size * size) as f64).sqrt();
        data.iter_mut().for_each(|c| *c *= scale as f32);
        data[0] = Complex32::new(100.0 * (size * size) as f32, 0.0);
        let mut samples = data.clone();
        fft2(&mut samples, size, true);
        let samples = samples
            .iter()
            .map(|c| c.re / (size * size) as f32)
            .collect();
        Self {
            size,
            spectrum: data,
            samples,
        }
    }

    /// The field shifted by `(dx, dy)`, its window of `(width, height)` with
    /// its top left at `(x, y)`.
    pub fn window(&self, shift: (f64, f64), at: (usize, usize), size: (usize, usize)) -> Vec<f32> {
        let n = self.size;
        if shift.0.fract() == 0.0 && shift.1.fract() == 0.0 {
            // A whole shift moves the samples as they are.
            let (dx, dy) = (shift.0 as i64, shift.1 as i64);
            let wrap = |i: usize, d: i64| (i as i64 - d).rem_euclid(n as i64) as usize;
            let rows = (at.1..at.1 + size.1).map(|y| wrap(y, dy));
            return rows
                .flat_map(|y| (at.0..at.0 + size.0).map(move |x| (y, wrap(x, dx))))
                .map(|(y, x)| self.samples[y * n + x])
                .collect();
        }
        let mut data = self.spectrum.clone();
        for y in 0..n {
            for x in 0..n {
                let phase = -std::f64::consts::TAU
                    * (frequency(x, n) * shift.0 + frequency(y, n) * shift.1);
                let ramp = Complex32::new(phase.cos() as f32, phase.sin() as f32);
                data[y * n + x] *= ramp;
            }
        }
        fft2(&mut data, n, true);
        let scale = 1.0 / (n * n) as f32;
        let mut out = Vec::with_capacity(size.0 * size.1);
        for y in at.1..at.1 + size.1 {
            for x in at.0..at.0 + size.0 {
                out.push(data[(y % n) * n + x % n].re * scale);
            }
        }
        out
    }
}

/// An image of `size` from `data`, with Gaussian noise added whose standard
/// deviation is 10 / `snr`: the field's, divided by the signal-to-noise
/// ratio. An infinite ratio adds none.
pub fn noisy(data: Vec<f32>, size: (usize, usize), snr: f64, random: &mut StdRng) -> Image {
    let sd = 10.0 / snr;
    let data = data
        .into_iter()
        .map(|v| v + (sd * normal(random)) as f32)
        .collect();
    Image::from_vec(Shape::new(size.0 as u32, size.1 as u32, 1), data).unwrap()
}
