//! Samples: 3D, as layers at heights above the slide, each with its own
//! optical behaviour, rendered at any pixel size.

use oscope_compute::{Image, Shape};
use oscope_core::{Point2, Rect, Um};
use rayon::prelude::*;

use crate::fluorescence::Dye;

/// What a layer does to light.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayerKind {
    /// It absorbs, as a stain does. It renders as optical densities, and
    /// passes 10^−OD of the light (the Beer–Lambert law).
    Absorbing,
    /// It reflects, as a metal film does. It renders as the share of light
    /// it reflects, and passes the rest.
    Reflective,
    /// It fluoresces, in fluorescent light only. It renders as the amount of
    /// its dye in each pixel, the same in R, G and B: micrometres of dye at
    /// unit density along the light's path. One, fully excited, gives as
    /// much light as unhindered transmitted light does.
    Fluorescent(Dye),
}

/// One layer of a sample.
#[derive(Clone, Debug, PartialEq)]
pub struct Layer {
    /// Its name.
    pub name: String,
    /// The height of its middle above the slide's top surface.
    pub z: Um,
    /// Its thickness.
    pub thickness: Um,
    /// What it does to light.
    pub kind: LayerKind,
}

/// A sample's model as the GPU's kernels take it (ADR-0062). Only the H&E
/// section has one; other samples render their tiles on the CPU.
#[derive(Clone, Debug, PartialEq)]
pub enum GpuModel {
    /// The H&E section, for `oscope-compute`'s `kernels/tissue.cu`: its
    /// numbers and salted seeds, as [`HeTissue`](crate::HeTissue) lays them
    /// out, how many islands the numbers hold, and its stains' vectors,
    /// hematoxylin's then the second stain's.
    HeTissue {
        /// The numbers.
        numbers: Vec<f64>,
        /// The salted seeds.
        salts: Vec<u64>,
        /// The islands.
        islands: u32,
        /// The vectors.
        vectors: [f32; 6],
    },
}

/// A sample on a slide, in slide coordinates: micrometres from the slide's
/// corner, with z up from its top surface.
pub trait Sample: Send + Sync {
    /// Its name.
    fn name(&self) -> &str;

    /// Its layers, from the slide up.
    fn layers(&self) -> &[Layer];

    /// Where it lies on the slide.
    fn bounds(&self) -> Rect;

    /// Layer `index` over a grid of `width` × `height` square pixels of side
    /// `pixel`, whose top-left corner is at `origin`, as RGB: optical
    /// densities for an absorbing layer, reflectances for a reflective one,
    /// the amounts of its dye for a fluorescent one. A layer it doesn't have
    /// renders as zeros.
    ///
    /// **Each pixel is a mean over its square,** so detail finer than about
    /// two pixels averages away, and a coarse rendering matches a fine one
    /// averaged down: the levels of detail (ADR-0013).
    fn render(&self, index: usize, origin: Point2, pixel: Um, width: u32, height: u32) -> Image;

    /// Its model for the GPU's kernels, if it has one: then the GPU makes
    /// its tiles in transmitted light (ADR-0062). None by default.
    fn gpu_model(&self) -> Option<GpuModel> {
        None
    }

    /// The height above the slide's top surface at which it is sharpest at
    /// `at`, in slide coordinates: the truth autofocus is held to (P34). By
    /// default, the middle of its layers, from the lowest's bottom to the
    /// highest's top; 0 with none.
    fn in_focus(&self, at: Point2) -> Um {
        let _ = at;
        let layers = self.layers();
        let low = layers.iter().map(|l| l.z.0 - l.thickness.0 / 2.0);
        let high = layers.iter().map(|l| l.z.0 + l.thickness.0 / 2.0);
        match (low.reduce(f64::min), high.reduce(f64::max)) {
            (Some(low), Some(high)) => Um((low + high) / 2.0),
            _ => Um(0.0),
        }
    }
}

/// An RGB image of `width` × `height` pixels of side `pixel` from `origin`,
/// with each pixel `f` of its square: `f(x0, y0)` gets the square's
/// top-left corner. Rows are rendered in parallel.
pub fn render_pixels<F>(origin: Point2, pixel: Um, width: u32, height: u32, f: F) -> Image
where
    F: Fn(f64, f64) -> [f32; 3] + Sync,
{
    let mut image = Image::zeros(Shape::new(width, height, 3));
    let row = width as usize * 3;
    if row == 0 {
        return image;
    }
    image
        .data_mut()
        .par_chunks_mut(row)
        .enumerate()
        .for_each(|(y, line)| {
            let y0 = origin.y + y as f64 * pixel.0;
            for (x, rgb) in line.as_chunks_mut::<3>().0.iter_mut().enumerate() {
                *rgb = f(origin.x + x as f64 * pixel.0, y0);
            }
        });
    image
}
