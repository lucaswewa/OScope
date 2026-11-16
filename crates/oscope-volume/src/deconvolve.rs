//! Richardson–Lucy deconvolution (P43; Richardson, 1972; Lucy, 1974), with
//! total variation if asked (Dey et al., 2006): by FFT over the volume
//! padded by mirroring, on the CPU with rustfft or on the GPU with cuFFT;
//! and the NRMSE against a truth. P43's notes give the behaviour.

use std::sync::Arc;

use oscope_compute::{Compute, Image, Shape};
use rayon::prelude::*;
use rustfft::num_complex::Complex32;
use rustfft::{Fft, FftPlanner};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{VolumeError, refused};

/// A volume of floats: x fastest, then y, then z.
#[derive(Clone, Debug, PartialEq)]
pub struct Volume {
    /// Its voxels along x, y and z.
    pub size: [usize; 3],
    /// Its values.
    pub data: Vec<f32>,
}

impl Volume {
    /// A stack's grey planes as a volume, the first the lowest.
    pub fn from_planes(planes: &[Image]) -> Result<Self, VolumeError> {
        let first = planes.first().ok_or_else(|| refused("an empty stack"))?;
        let shape = Shape::new(first.width(), first.height(), 1);
        if let Some(other) = planes.iter().find(|p| p.shape() != shape) {
            return Err(refused(format!(
                "a plane of {:?} in a stack of grey {shape:?}",
                other.shape()
            )));
        }
        let data = planes
            .iter()
            .flat_map(|p| p.data().iter().copied())
            .collect();
        Ok(Self {
            size: [
                first.width() as usize,
                first.height() as usize,
                planes.len(),
            ],
            data,
        })
    }

    /// Its planes, as grey images, the first the lowest.
    pub fn planes(&self) -> Vec<Image> {
        let [w, h, _] = self.size;
        let shape = Shape::new(w as u32, h as u32, 1);
        self.data
            .chunks(w * h)
            .map(|plane| Image::from_vec(shape, plane.to_vec()).expect("a plane's samples"))
            .collect()
    }
}

/// Deconvolution's settings.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, default)]
pub struct DeconvolveOptions {
    /// Richardson–Lucy's iterations.
    pub iterations: u32,
    /// Total variation's weight, λ_TV: 0 for none, 0.002 typical.
    pub tv: f32,
}

impl Default for DeconvolveOptions {
    fn default() -> Self {
        Self {
            iterations: 20,
            tv: 0.0,
        }
    }
}

/// The next size from `n` whose prime factors are 2, 3, 5 and 7.
fn smooth(n: usize) -> usize {
    let smooth = |mut m: usize| {
        for p in [2, 3, 5, 7] {
            while m.is_multiple_of(p) {
                m /= p;
            }
        }
        m == 1
    };
    (n.max(1)..).find(|&m| smooth(m)).expect("a smooth size")
}

/// Index `i` of an axis of `len`, mirrored about its ends.
fn reflect(i: isize, len: usize) -> usize {
    let period = 2 * len as isize;
    let m = i.rem_euclid(period);
    match m < len as isize {
        true => m as usize,
        false => (period - 1 - m) as usize,
    }
}

/// A 3D FFT over a size, unnormalised: rustfft's 1D transforms along x,
/// then y, then z, each axis made the fastest in turn.
struct Fft3 {
    size: [usize; 3],
    forward: [Arc<dyn Fft<f32>>; 3],
    inverse: [Arc<dyn Fft<f32>>; 3],
}

impl Fft3 {
    fn new(size: [usize; 3]) -> Self {
        let mut planner = FftPlanner::new();
        Self {
            size,
            forward: size.map(|n| planner.plan_fft_forward(n)),
            inverse: size.map(|n| planner.plan_fft_inverse(n)),
        }
    }

    /// Transforms `data` in place.
    fn apply(&self, data: &mut [Complex32], inverse: bool) {
        let plans = if inverse {
            &self.inverse
        } else {
            &self.forward
        };
        let [nx, ny, nz] = self.size;
        let rows = |data: &mut [Complex32], n: usize, plan: &Arc<dyn Fft<f32>>| {
            data.par_chunks_mut(n * 64)
                .for_each(|chunk| plan.process(chunk));
        };
        rows(data, nx, &plans[0]);
        // Along y: each z slab transposed, transformed by rows, and back.
        data.par_chunks_mut(nx * ny).for_each(|slab| {
            let mut t: Vec<Complex32> =
                (0..nx * ny).map(|i| slab[(i % ny) * nx + i / ny]).collect();
            plans[1].process(&mut t);
            for (i, v) in t.into_iter().enumerate() {
                slab[(i % ny) * nx + i / ny] = v;
            }
        });
        // Along z: the volume made z-fastest, transformed, and back.
        let mut t = vec![Complex32::default(); data.len()];
        t.par_chunks_mut(nz).enumerate().for_each(|(xy, line)| {
            line.iter_mut()
                .enumerate()
                .for_each(|(z, v)| *v = data[z * nx * ny + xy]);
        });
        rows(&mut t, nz, &plans[2]);
        data.par_chunks_mut(nx * ny)
            .enumerate()
            .for_each(|(z, slab)| {
                slab.iter_mut()
                    .enumerate()
                    .for_each(|(xy, v)| *v = t[xy * nz + z]);
            });
    }
}

/// A volume made ready to deconvolve: mirrored about its edges by the PSF's
/// half-size, at most its own length, to sizes whose prime factors are 2, 3,
/// 5 and 7; and the PSF's transfer function over that size.
pub struct Padded {
    /// The padded size.
    pub size: [usize; 3],
    /// Where the volume starts in it.
    pub offset: [usize; 3],
    /// The volume's own size.
    pub inner: [usize; 3],
    /// The padded data.
    pub data: Vec<f32>,
    /// The PSF's transfer function: its 3D FFT, centred on the origin.
    pub otf: Vec<Complex32>,
}

impl Padded {
    /// `volume`, padded for `psf`, of odd size and summing to 1.
    pub fn new(volume: &Volume, psf: &Volume) -> Result<Self, VolumeError> {
        let (inner, s) = (volume.size, psf.size);
        if inner.contains(&0)
            || s.iter().any(|n| n % 2 == 0)
            || volume.data.len() != inner.iter().product::<usize>()
        {
            return Err(refused(format!(
                "a volume of {inner:?} and a PSF of {s:?}: none empty, the PSF odd"
            )));
        }
        let offset = [0, 1, 2].map(|a| (s[a] / 2).min(inner[a]));
        let size = [0, 1, 2].map(|a| smooth((inner[a] + 2 * offset[a]).max(s[a])));
        let [nx, ny, nz] = size;
        let mut data = vec![0.0_f32; nx * ny * nz];
        data.par_chunks_mut(nx).enumerate().for_each(|(row, line)| {
            let (y, z) = (row % ny, row / ny);
            let sy = reflect(y as isize - offset[1] as isize, inner[1]);
            let sz = reflect(z as isize - offset[2] as isize, inner[2]);
            for (x, v) in line.iter_mut().enumerate() {
                let sx = reflect(x as isize - offset[0] as isize, inner[0]);
                *v = volume.data[(sz * inner[1] + sy) * inner[0] + sx].max(0.0);
            }
        });
        let mut otf = vec![Complex32::default(); nx * ny * nz];
        for z in 0..s[2] {
            for y in 0..s[1] {
                for x in 0..s[0] {
                    let at = |i: usize, a: usize| {
                        (i as isize - (s[a] / 2) as isize).rem_euclid(size[a] as isize) as usize
                    };
                    let i = (at(z, 2) * ny + at(y, 1)) * nx + at(x, 0);
                    otf[i].re += psf.data[(z * s[1] + y) * s[0] + x];
                }
            }
        }
        Fft3::new(size).apply(&mut otf, false);
        Ok(Self {
            size,
            offset,
            inner,
            data,
            otf,
        })
    }

    /// The volume's part of a padded estimate.
    pub fn crop(&self, padded: &[f32]) -> Volume {
        let ([nx, ny, _], [w, h, d], [ox, oy, oz]) = (self.size, self.inner, self.offset);
        let data = (0..d)
            .flat_map(|z| {
                (0..h)
                    .flat_map(move |y| (0..w).map(move |x| ((z + oz) * ny + y + oy) * nx + x + ox))
            })
            .map(|i| padded[i])
            .collect();
        Volume {
            size: self.inner,
            data,
        }
    }

    /// The transfer function's half spectrum along x, as cuFFT's real
    /// transforms take it: each row's first `nx / 2 + 1` values.
    pub fn half_otf(&self) -> Vec<[f32; 2]> {
        let nx = self.size[0];
        self.otf
            .chunks(nx)
            .flat_map(|row| row[..nx / 2 + 1].iter().map(|c| [c.re, c.im]))
            .collect()
    }
}

/// Total variation's factor for each voxel of `u` over `size`, periodic:
/// 1 / max(0.5, 1 − λ div(∇u / |∇u|)), forward differences for the
/// gradient and backward ones for the divergence.
fn tv_factor(u: &[f32], size: [usize; 3], lambda: f32) -> Vec<f32> {
    let [nx, ny, nz] = size;
    let index = |x: usize, y: usize, z: usize| (z * ny + y) * nx + x;
    let normals: Vec<[f32; 3]> = (0..u.len())
        .into_par_iter()
        .map(|i| {
            let (x, y, z) = (i % nx, (i / nx) % ny, i / (nx * ny));
            let c = u[i];
            let g = [
                u[index((x + 1) % nx, y, z)] - c,
                u[index(x, (y + 1) % ny, z)] - c,
                u[index(x, y, (z + 1) % nz)] - c,
            ];
            let length = (g[0] * g[0] + g[1] * g[1] + g[2] * g[2] + 1e-12).sqrt();
            g.map(|v| v / length)
        })
        .collect();
    (0..u.len())
        .into_par_iter()
        .map(|i| {
            let (x, y, z) = (i % nx, (i / nx) % ny, i / (nx * ny));
            let div = normals[i][0] - normals[index((x + nx - 1) % nx, y, z)][0] + normals[i][1]
                - normals[index(x, (y + ny - 1) % ny, z)][1]
                + normals[i][2]
                - normals[index(x, y, (z + nz - 1) % nz)][2];
            1.0 / (1.0 - lambda * div).max(0.5)
        })
        .collect()
}

/// Richardson–Lucy over `padded`, periodic, on the CPU: the padded
/// estimate after the options' iterations, from the data. `progress` is
/// told each iteration done, and stops it by answering false: then `None`.
/// Without total variation, the estimate's sum stays the data's.
pub fn richardson_lucy(
    padded: &Padded,
    options: &DeconvolveOptions,
    progress: &mut dyn FnMut(u32) -> bool,
) -> Option<Vec<f32>> {
    let n = padded.data.len();
    let fft = Fft3::new(padded.size);
    let scale = 1.0 / n as f32;
    let largest = padded.data.iter().copied().fold(0.0_f32, f32::max);
    let floor = 1e-12 * largest;
    let mut u = padded.data.clone();
    let mut spectrum = vec![Complex32::default(); n];
    let convolve = |values: &[f32], spectrum: &mut Vec<Complex32>, mirrored: bool| -> Vec<f32> {
        spectrum
            .par_iter_mut()
            .zip(values.par_iter())
            .for_each(|(s, v)| *s = Complex32::new(*v, 0.0));
        fft.apply(spectrum, false);
        spectrum
            .par_iter_mut()
            .zip(padded.otf.par_iter())
            .for_each(|(s, h)| {
                *s = *s * if mirrored { h.conj() } else { *h } * scale;
            });
        fft.apply(spectrum, true);
        spectrum.par_iter().map(|s| s.re).collect()
    };
    for k in 0..options.iterations {
        let blurred = convolve(&u, &mut spectrum, false);
        let ratio: Vec<f32> = padded
            .data
            .par_iter()
            .zip(blurred.par_iter())
            .map(|(d, b)| if *b > floor { d / b } else { 0.0 })
            .collect();
        let correction = convolve(&ratio, &mut spectrum, true);
        let factor = (options.tv > 0.0).then(|| tv_factor(&u, padded.size, options.tv));
        u.par_iter_mut().enumerate().for_each(|(i, v)| {
            *v *= correction[i] * factor.as_ref().map_or(1.0, |f| f[i]);
        });
        if !progress(k + 1) {
            return None;
        }
    }
    Some(u)
}

/// `volume` blurred by `psf`: their convolution over the volume padded by
/// mirroring, as deconvolution models the microscope, cropped back.
pub fn blur(volume: &Volume, psf: &Volume) -> Result<Volume, VolumeError> {
    let padded = Padded::new(volume, psf)?;
    let fft = Fft3::new(padded.size);
    let mut spectrum: Vec<Complex32> = padded
        .data
        .iter()
        .map(|v| Complex32::new(*v, 0.0))
        .collect();
    fft.apply(&mut spectrum, false);
    let scale = 1.0 / spectrum.len() as f32;
    spectrum
        .par_iter_mut()
        .zip(padded.otf.par_iter())
        .for_each(|(s, h)| *s = *s * *h * scale);
    fft.apply(&mut spectrum, true);
    let blurred: Vec<f32> = spectrum.iter().map(|s| s.re).collect();
    Ok(padded.crop(&blurred))
}

/// Deconvolves `volume` by `psf`, on the GPU when `compute` has one:
/// Richardson–Lucy over the volume padded by mirroring, cropped back.
/// `None` if `progress` stopped it.
pub fn deconvolve(
    volume: &Volume,
    psf: &Volume,
    options: &DeconvolveOptions,
    compute: &Compute,
    progress: &mut dyn FnMut(u32) -> bool,
) -> Result<Option<Volume>, VolumeError> {
    let padded = Padded::new(volume, psf)?;
    #[cfg(feature = "cuda")]
    if let Some(cuda) = compute.cuda() {
        let iterate = cuda.richardson_lucy(
            oscope_compute::Lane::Batch,
            &padded.data,
            &padded.half_otf(),
            padded.size,
            options.iterations,
            options.tv,
            progress,
        )?;
        return Ok(iterate.map(|u| padded.crop(&u)));
    }
    let _ = compute;
    Ok(richardson_lucy(&padded, options, progress).map(|u| padded.crop(&u)))
}

/// The NRMSE of `a` against `b`, each standardised to zero mean and unit
/// variance: √(2(1 − ρ)), ρ being their correlation.
pub fn nrmse(a: &[f32], b: &[f32]) -> f64 {
    let n = a.len().min(b.len()) as f64;
    let mean = |v: &[f32]| v.iter().map(|x| f64::from(*x)).sum::<f64>() / n;
    let (ma, mb) = (mean(a), mean(b));
    let (mut ab, mut aa, mut bb) = (0.0, 0.0, 0.0);
    for (x, y) in a.iter().zip(b) {
        let (x, y) = (f64::from(*x) - ma, f64::from(*y) - mb);
        (ab, aa, bb) = (ab + x * y, aa + x * x, bb + y * y);
    }
    let rho = match aa > 0.0 && bb > 0.0 {
        true => ab / (aa * bb).sqrt(),
        false => 0.0,
    };
    (2.0 * (1.0 - rho)).max(0.0).sqrt()
}
