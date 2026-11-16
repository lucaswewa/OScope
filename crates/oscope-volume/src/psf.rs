//! The widefield point spread function (P43): Born and Wolf's (1999, §8.8)
//! for an aberration-free lens, scalar and paraxial. At lateral distance r
//! and axial distance z from focus, in the optical coordinates v = (2π/λ)
//! NA r and u = (2π/λ)(NA²/n) z, its intensity is
//!
//! I(u, v) = |2 ∫₀¹ J₀(vρ) e^(−iuρ²/2) ρ dρ|²,
//!
//! 1 at focus, Airy's (2J₁(v)/v)² in the focal plane and (sin(u/4)/(u/4))²
//! on the axis. P43's notes give the behaviour, and its sources.

use std::f64::consts::PI;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::deconvolve::Volume;
use crate::{VolumeError, refused};

/// The Bessel function of the first kind of order 0, from the polynomial
/// approximations of Abramowitz and Stegun (1964), 9.4.1 and 9.4.3: within
/// 5 × 10⁻⁸.
pub fn bessel_j0(x: f64) -> f64 {
    let x = x.abs();
    if x <= 3.0 {
        let t = (x / 3.0).powi(2);
        let c = [
            1.0,
            -2.249_999_7,
            1.265_620_8,
            -0.316_386_6,
            0.044_447_9,
            -0.003_944_4,
            0.000_21,
        ];
        return c.iter().rev().fold(0.0, |sum, c| sum * t + c);
    }
    let t = 3.0 / x;
    let f = [
        0.797_884_56,
        -0.000_000_77,
        -0.005_527_40,
        -0.000_095_12,
        0.001_372_37,
        -0.000_728_05,
        0.000_144_76,
    ];
    // The table's first, 0.78539816, is π/4.
    let theta = [
        -std::f64::consts::FRAC_PI_4,
        -0.041_663_97,
        -0.000_039_54,
        0.002_625_73,
        -0.000_541_25,
        -0.000_293_33,
        0.000_135_58,
    ];
    let poly = |c: &[f64]| c.iter().rev().fold(0.0, |sum, c| sum * t + c);
    poly(&f) * (x + poly(&theta)).cos() / x.sqrt()
}

/// Born and Wolf's intensity at optical coordinates `u` and `v`, by
/// Simpson's rule over enough intervals that the phase and the Bessel
/// function turn a sixteenth of a cycle at most in each.
pub fn born_wolf(u: f64, v: f64) -> f64 {
    let turns = (u.abs() / 2.0 + v) / (2.0 * PI);
    let n = ((16.0 * turns).ceil() as usize).max(32).next_multiple_of(2);
    let h = 1.0 / n as f64;
    let (mut re, mut im) = (0.0, 0.0);
    for k in 0..=n {
        let rho = k as f64 * h;
        let weight = match k {
            0 => 1.0,
            k if k == n => 1.0,
            k if k % 2 == 1 => 4.0,
            _ => 2.0,
        };
        let (sin, cos) = (u * rho * rho / 2.0).sin_cos();
        let common = weight * bessel_j0(v * rho) * rho;
        re += common * cos;
        im -= common * sin;
    }
    let scale = 2.0 * h / 3.0;
    (re * scale).powi(2) + (im * scale).powi(2)
}

/// A widefield objective, and the light it images.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Optics {
    /// The numerical aperture.
    pub na: f64,
    /// The emission's wavelength, in µm.
    pub wavelength_um: f64,
    /// The medium's refractive index: 1 for a dry objective.
    pub medium: f64,
}

impl Optics {
    fn check(&self) -> Result<(), VolumeError> {
        let fine = self.na > 0.0 && self.na < self.medium && self.wavelength_um > 0.0;
        match fine && self.medium.is_finite() {
            true => Ok(()),
            false => Err(refused(format!(
                "{self:?}: an NA above 0 and under the medium's index, and a wavelength above 0"
            ))),
        }
    }

    /// Airy's radius, the in-focus pattern's first zero: 0.61 λ / NA.
    pub fn airy_radius(&self) -> f64 {
        0.61 * self.wavelength_um / self.na
    }
}

/// The PSF of `optics` for a volume of `size` voxels of `voxel` µm, x, y
/// and z: sampled at the voxels' centres about its own, of odd size. It
/// spans the volume's depth either way, and laterally the geometric cone's
/// radius at that depth plus three Airy radii, at most the volume's width
/// either way. It sums to 1.
pub fn psf(optics: &Optics, voxel: [f64; 3], size: [usize; 3]) -> Result<Volume, VolumeError> {
    optics.check()?;
    if voxel.iter().any(|v| *v <= 0.0 || !v.is_finite()) || size.contains(&0) {
        return Err(refused(format!(
            "voxels of {voxel:?} µm, a volume of {size:?}"
        )));
    }
    let (na, n, k) = (optics.na, optics.medium, 2.0 * PI / optics.wavelength_um);
    let depth = (size[2] - 1) as f64 * voxel[2];
    let reach = depth * na / (n * n - na * na).sqrt() + 3.0 * optics.airy_radius();
    let half = [
        ((reach / voxel[0]).ceil() as usize).min(size[0] - 1),
        ((reach / voxel[1]).ceil() as usize).min(size[1] - 1),
        size[2] - 1,
    ];
    let dims = half.map(|h| 2 * h + 1);
    // Each plane's intensity against r, every quarter voxel, then each
    // voxel's from the nearest two.
    let step = voxel[0].min(voxel[1]) / 4.0;
    let r_max = (half[0] as f64 * voxel[0]).hypot(half[1] as f64 * voxel[1]);
    let samples = (r_max / step).ceil() as usize + 2;
    let mut data = vec![0.0_f32; dims.iter().product()];
    for kz in 0..=half[2] {
        let u = k * na * na / n * kz as f64 * voxel[2];
        let radial: Vec<f64> = (0..samples)
            .map(|s| born_wolf(u, k * na * s as f64 * step))
            .collect();
        // The plane kz above focus, and its mirror below: I(−u, v) = I(u, v).
        for iz in [half[2] + kz, half[2] - kz] {
            for y in 0..dims[1] {
                let dy = (y as f64 - half[1] as f64) * voxel[1];
                for x in 0..dims[0] {
                    let dx = (x as f64 - half[0] as f64) * voxel[0];
                    let at = dx.hypot(dy) / step;
                    let (s, t) = (at.floor() as usize, at.fract());
                    let value = radial[s] * (1.0 - t) + radial[(s + 1).min(samples - 1)] * t;
                    data[(iz * dims[1] + y) * dims[0] + x] = value as f32;
                }
            }
        }
    }
    let sum: f64 = data.iter().map(|v| f64::from(*v)).sum();
    data.iter_mut()
        .for_each(|v| *v = (f64::from(*v) / sum) as f32);
    Ok(Volume { size: dims, data })
}
