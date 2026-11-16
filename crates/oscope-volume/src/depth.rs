//! Depth from focus (P42): each pixel's height from the plane where its
//! focus measure peaks through a stack, refined by a parabola, with a
//! confidence; and the all-in-focus image, each pixel from the planes about
//! its height. P42's notes give the behaviour, and its sources.
//!
//! Both are streamed, a plane at a time, so a stack's memory is a few
//! images of a plane's size: [`Peaks`] keeps each pixel's running peak, and
//! [`AllInFocus`] each pixel's blend.

use oscope_compute::{ComputeError, Image, Shape, filters};
use rayon::prelude::*;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{VolumeError, refused};

/// The focus measure's and the height map's settings.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, default)]
pub struct FocusOptions {
    /// The modified Laplacian's step: its neighbours this many pixels away.
    pub step: u32,
    /// The window's half-width: the measure is summed over 2r + 1 pixels
    /// square.
    pub radius: u32,
    /// The median's half-width over the height map, up to 7: 0 for none.
    pub median: u32,
}

/// A step of 2 and a window of 7: on the simulated camera's stacks, a step
/// of 1 measured its noise as much as the texture (P42's notes).
impl Default for FocusOptions {
    fn default() -> Self {
        Self {
            step: 2,
            radius: 3,
            median: 1,
        }
    }
}

/// Where a stack's planes are, in µm: the lowest's height, and the step up.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Planes {
    /// The lowest plane's height.
    pub z0_um: f64,
    /// The step from each plane to the next: above 0.
    pub step_um: f64,
}

impl Planes {
    fn check(&self) -> Result<(), VolumeError> {
        match self.step_um > 0.0 && self.step_um.is_finite() && self.z0_um.is_finite() {
            true => Ok(()),
            false => Err(refused(format!("planes {self:?}: a step above 0"))),
        }
    }
}

/// A stack's heights in µm, and their confidence, from 0 to 1.
#[derive(Clone, Debug)]
pub struct Depth {
    /// Each pixel's height.
    pub heights: Image,
    /// How sure each is.
    pub confidence: Image,
}

/// One pixel's running peak.
#[derive(Clone, Copy, Debug)]
struct Peak {
    /// The measure at the peak, at the plane before it, and after it.
    best: f32,
    before: f32,
    after: f32,
    /// The last plane's measure, and the measures' sum.
    last: f32,
    sum: f32,
    /// The peak's plane.
    at: u32,
}

/// Each pixel's running peak of its focus measure, plane by plane from the
/// lowest: all that depth from focus keeps of a stack.
#[derive(Clone, Debug)]
pub struct Peaks {
    shape: Shape,
    planes: u32,
    peaks: Vec<Peak>,
}

impl Peaks {
    /// For planes of `width` × `height` pixels.
    pub fn new(width: u32, height: u32) -> Self {
        let peak = Peak {
            best: f32::NEG_INFINITY,
            before: 0.0,
            after: 0.0,
            last: 0.0,
            sum: 0.0,
            at: 0,
        };
        Self {
            shape: Shape::new(width, height, 1),
            planes: 0,
            peaks: vec![peak; width as usize * height as usize],
        }
    }

    /// How many planes it has taken.
    pub fn planes(&self) -> u32 {
        self.planes
    }

    /// Takes the next plane's focus measures, grey and of the planes' size.
    /// A tie keeps the lower plane.
    pub fn push(&mut self, measures: &Image) -> Result<(), VolumeError> {
        if measures.shape() != self.shape {
            return Err(refused(format!(
                "focus measures of {:?} where the stack's planes are {:?}",
                measures.shape(),
                self.shape
            )));
        }
        let k = self.planes;
        self.peaks
            .par_iter_mut()
            .zip(measures.data().par_iter())
            .for_each(|(p, &f)| {
                if f > p.best {
                    (p.before, p.best, p.at) = (if k > 0 { p.last } else { 0.0 }, f, k);
                } else if k == p.at + 1 {
                    p.after = f;
                }
                p.last = f;
                p.sum += f;
            });
        self.planes += 1;
        Ok(())
    }

    /// The heights at `planes`, and their confidence, before any median:
    /// inside the stack, the vertex of the parabola through the peak and its
    /// neighbours' measures; at its first or last plane, that plane, with no
    /// confidence. At least 3 planes.
    pub fn finish(&self, planes: Planes) -> Result<Depth, VolumeError> {
        planes.check()?;
        let n = self.planes;
        if n < 3 {
            return Err(refused(format!("{n} planes: depth from focus needs 3")));
        }
        let (z0, step) = (planes.z0_um, planes.step_um);
        let (heights, confidence): (Vec<f32>, Vec<f32>) = self
            .peaks
            .par_iter()
            .map(|p| {
                let z = z0 + f64::from(p.at) * step;
                if p.at == 0 || p.at == n - 1 || p.best <= 0.0 {
                    return (z as f32, 0.0);
                }
                let (a, b, c) = (f64::from(p.before), f64::from(p.best), f64::from(p.after));
                let curve = a - 2.0 * b + c;
                let offset = match curve < 0.0 {
                    true => step * (a - c) / (2.0 * curve),
                    false => 0.0,
                };
                let mean = f64::from(p.sum) / f64::from(n);
                ((z + offset) as f32, ((b - mean) / b).clamp(0.0, 1.0) as f32)
            })
            .unzip();
        Ok(Depth {
            heights: Image::from_vec(self.shape, heights)?,
            confidence: Image::from_vec(self.shape, confidence)?,
        })
    }
}

/// The median of each (2`radius` + 1)² window of `heights`, or the heights
/// as they are for a radius of 0.
pub fn median(heights: &Image, radius: u32) -> Result<Image, VolumeError> {
    match radius {
        0 => Ok(heights.clone()),
        r => Ok(filters::median(heights, r)?),
    }
}

/// Depth from focus over `stack`, grey planes of one size from the lowest
/// at `planes`, each measured by `measure`: the sum-modified-Laplacian, on
/// the CPU or the GPU. The heights, their median taken, and their
/// confidence.
pub fn depth_from_focus(
    stack: &[Image],
    planes: Planes,
    options: &FocusOptions,
    measure: impl Fn(&Image) -> Result<Image, ComputeError>,
) -> Result<Depth, VolumeError> {
    let first = stack
        .first()
        .ok_or_else(|| refused("an empty stack has no depth"))?;
    let mut peaks = Peaks::new(first.width(), first.height());
    for plane in stack {
        peaks.push(&measure(plane)?)?;
    }
    let depth = peaks.finish(planes)?;
    Ok(Depth {
        heights: median(&depth.heights, options.median)?,
        ..depth
    })
}

/// The all-in-focus image, built plane by plane from the lowest: each pixel
/// the planes either side of its height, blended by where it lies between
/// them.
#[derive(Clone, Debug)]
pub struct AllInFocus {
    shape: Shape,
    count: u32,
    next: u32,
    /// Each pixel's place in the stack, in planes from the lowest.
    places: Vec<f32>,
    sum: Vec<f32>,
}

impl AllInFocus {
    /// For a stack of `count` planes at `planes`, of `channels`, and the
    /// heights found in it.
    pub fn new(
        heights: &Image,
        planes: Planes,
        count: u32,
        channels: usize,
    ) -> Result<Self, VolumeError> {
        planes.check()?;
        if count == 0 || heights.channels() != 1 {
            return Err(refused(format!(
                "{count} planes and heights of {} channels: one plane and grey heights at least",
                heights.channels()
            )));
        }
        let top = (count - 1) as f32;
        let places = heights
            .data()
            .iter()
            .map(|&z| {
                ((f64::from(z) - planes.z0_um) / planes.step_um).clamp(0.0, f64::from(top)) as f32
            })
            .collect();
        let shape = Shape::new(heights.width(), heights.height(), channels);
        Ok(Self {
            shape,
            count,
            next: 0,
            places,
            sum: vec![0.0; shape.len()],
        })
    }

    /// Takes the next plane, from the lowest.
    pub fn push(&mut self, plane: &Image) -> Result<(), VolumeError> {
        if plane.shape() != self.shape || self.next == self.count {
            return Err(refused(format!(
                "plane {} of {:?}, where the stack has {} of {:?}",
                self.next,
                plane.shape(),
                self.count,
                self.shape
            )));
        }
        let (k, c) = (self.next, self.shape.channels);
        self.sum
            .par_chunks_mut(c)
            .zip(plane.data().par_chunks(c))
            .zip(self.places.par_iter())
            .for_each(|((out, colour), &u)| {
                let below = u.floor() as u32;
                let t = u - below as f32;
                let weight = match k {
                    k if k == below => 1.0 - t,
                    k if k == below + 1 => t,
                    _ => return,
                };
                out.iter_mut()
                    .zip(colour)
                    .for_each(|(o, v)| *o += weight * v);
            });
        self.next += 1;
        Ok(())
    }

    /// The image, once every plane is in.
    pub fn finish(self) -> Result<Image, VolumeError> {
        if self.next != self.count {
            return Err(refused(format!(
                "{} planes of {} taken",
                self.next, self.count
            )));
        }
        Ok(Image::from_vec(self.shape, self.sum)?)
    }
}

/// The all-in-focus image of `stack`, planes of one size from the lowest at
/// `planes`, from the `heights` found in it.
pub fn all_in_focus(
    stack: &[Image],
    heights: &Image,
    planes: Planes,
) -> Result<Image, VolumeError> {
    let channels = stack.first().map_or(1, Image::channels);
    let mut image = AllInFocus::new(heights, planes, stack.len() as u32, channels)?;
    for plane in stack {
        image.push(plane)?;
    }
    image.finish()
}
