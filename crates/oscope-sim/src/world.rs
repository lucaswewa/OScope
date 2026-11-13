//! The world: a slide with a coverslip and a sample, placed on the stage,
//! and what the camera sees of it through the optics.

use std::sync::Arc;

use oscope_compute::{Image, Shape};
use oscope_core::{Point2, Rect, Size2, Um};
use rayon::prelude::*;

use crate::noise::coverage;
use crate::optics::Optics;
use crate::sample::{LayerKind, Sample};

/// The light a glass surface passes at normal incidence: 1 − ((n − 1) /
/// (n + 1))² for glass of index 1.5, Fresnel's 4% lost (Hecht, *Optics*).
const SURFACE: f64 = 0.96;
/// The light a glass surface reflects.
const REFLECTED: f64 = 1.0 - SURFACE;
/// How much light passes at a glass edge, which scatters it.
const EDGE: f64 = 0.35;
/// The width of the dark band at a glass edge.
const EDGE_WIDTH: f64 = 30.0;

/// Which light the sample is seen in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Light {
    /// From below, through the slide: brightfield.
    Transmitted,
    /// From above, reflected back into the objective.
    Reflected,
}

/// A 75 × 25 mm slide carrying a sample under a 50 × 24 mm coverslip.
#[derive(Clone)]
pub struct Slide {
    /// Where the slide's corner is on the stage, in µm.
    pub origin: Point2,
    /// The coverslip, in slide coordinates.
    pub coverslip: Rect,
    /// The sample, in slide coordinates.
    pub sample: Arc<dyn Sample>,
}

impl Slide {
    /// The slide's size.
    pub const SIZE: Size2 = Size2::new(75_000.0, 25_000.0);

    /// The coverslip's place on a slide: past the label, along the slide.
    pub const COVERSLIP: Rect = Rect::new(22_000.0, 500.0, 50_000.0, 24_000.0);

    /// A slide carrying `sample`, with its corner at `origin` on the stage.
    pub fn new(sample: Arc<dyn Sample>, origin: Point2) -> Self {
        Self {
            origin,
            coverslip: Self::COVERSLIP,
            sample,
        }
    }
}

/// A field to render: what a camera sees, axis-aligned with the stage.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Field {
    /// The stage position at the field's centre.
    pub centre: Point2,
    /// The stage's z.
    pub z: Um,
    /// Its width, in pixels.
    pub width: u32,
    /// Its height, in pixels.
    pub height: u32,
}

/// The slide on the stage, if one is loaded, and where it is in focus.
#[derive(Clone)]
pub struct World {
    /// The slide.
    pub slide: Option<Slide>,
    /// The stage's z at which the slide's top surface is in focus.
    pub focus: Um,
}

/// Blurs an RGB image with a Gaussian of standard deviation `sigma`
/// pixels, one axis after the other, repeating the edge pixels.
fn blur(image: &mut Image, sigma: f64) {
    if sigma < 0.2 {
        return;
    }
    let radius = (3.0 * sigma).ceil() as i64;
    let raw: Vec<f64> = (-radius..=radius)
        .map(|k| (-(k * k) as f64 / (2.0 * sigma * sigma)).exp())
        .collect();
    let total: f64 = raw.iter().sum();
    let weights: Vec<f32> = raw.iter().map(|w| (w / total) as f32).collect();
    let (w, h) = (image.width() as i64, image.height() as i64);
    let row = w as usize * 3;
    let clamp = |v: i64, n: i64| v.clamp(0, n - 1) as usize;

    let source = image.data().to_vec();
    image
        .data_mut()
        .par_chunks_mut(row)
        .enumerate()
        .for_each(|(y, out)| {
            let line = &source[y * row..(y + 1) * row];
            for x in 0..w {
                let mut acc = [0.0_f32; 3];
                for (k, weight) in weights.iter().enumerate() {
                    let at = clamp(x + k as i64 - radius, w) * 3;
                    for c in 0..3 {
                        acc[c] += weight * line[at + c];
                    }
                }
                out[x as usize * 3..x as usize * 3 + 3].copy_from_slice(&acc);
            }
        });
    let source = image.data().to_vec();
    image
        .data_mut()
        .par_chunks_mut(row)
        .enumerate()
        .for_each(|(y, out)| {
            out.fill(0.0);
            for (k, weight) in weights.iter().enumerate() {
                let from = clamp(y as i64 + k as i64 - radius, h) * row;
                for (o, s) in out.iter_mut().zip(&source[from..from + row]) {
                    *o += weight * s;
                }
            }
        });
}

/// Multiplies `into` by `by`, sample by sample.
fn multiply(into: &mut Image, by: &Image) {
    for (a, b) in into.data_mut().iter_mut().zip(by.data()) {
        *a *= b;
    }
}

impl World {
    /// No slide, in focus at z = 0.
    pub fn empty() -> Self {
        Self {
            slide: None,
            focus: Um(0.0),
        }
    }

    /// A world with `slide` loaded, its surface in focus at stage z
    /// `focus`.
    pub fn with_slide(slide: Slide, focus: Um) -> Self {
        Self {
            slide: Some(slide),
            focus,
        }
    }

    /// What the camera sees of `field` through `optics` in `light`: linear
    /// RGB, 1 where the light passes or is reflected unhindered.
    ///
    /// Each layer is blurred by its own distance from the focal plane
    /// (`Optics::blur`). In transmitted light, the layers' transmittances
    /// multiply, with a reflective layer passing what it doesn't reflect. In
    /// reflected light, each reflective layer's reflection passes twice
    /// through the absorbing layers above it.
    pub fn render(&self, field: &Field, optics: &Optics, light: Light) -> Image {
        let p = optics.pixel_size().0;
        let (w, h) = (field.width, field.height);
        let corner = Point2::new(
            field.centre.x - f64::from(w) * p / 2.0,
            field.centre.y - f64::from(h) * p / 2.0,
        );
        let Some(slide) = &self.slide else {
            let fill = if light == Light::Transmitted {
                1.0
            } else {
                0.0
            };
            let mut image = Image::zeros(Shape::new(w, h, 3));
            image.data_mut().fill(fill);
            return image;
        };

        // The layers, and how far each is from the focal plane, in pixels.
        let focal = field.z.0 - self.focus.0;
        let layers = slide.sample.layers();
        let sigmas: Vec<f64> = layers
            .iter()
            .map(|l| optics.blur(Um(focal - l.z.0)).0 / p)
            .collect();
        let margin = sigmas
            .iter()
            .fold(0.0_f64, |m, s| m.max(*s))
            .mul_add(3.0, 1.0)
            .ceil() as u32;
        let (we, he) = (w + 2 * margin, h + 2 * margin);
        let origin = Point2::new(
            corner.x - f64::from(margin) * p - slide.origin.x,
            corner.y - f64::from(margin) * p - slide.origin.y,
        );

        let mut passed = Image::zeros(Shape::new(we, he, 3));
        passed.data_mut().fill(1.0);
        let mut reflected = Image::zeros(Shape::new(we, he, 3));
        // From the top down, so each reflection is dimmed by what is above.
        let mut order: Vec<usize> = (0..layers.len()).collect();
        order.sort_by(|a, b| layers[*b].z.0.total_cmp(&layers[*a].z.0));
        for index in order {
            let mut layer = slide.sample.render(index, origin, Um(p), we, he);
            match layers[index].kind {
                LayerKind::Absorbing => {
                    for v in layer.data_mut() {
                        *v = 10f32.powf(-*v);
                    }
                    blur(&mut layer, sigmas[index]);
                    multiply(&mut passed, &layer);
                }
                LayerKind::Reflective => {
                    blur(&mut layer, sigmas[index]);
                    for ((r, l), t) in reflected
                        .data_mut()
                        .iter_mut()
                        .zip(layer.data())
                        .zip(passed.data())
                    {
                        *r += l * t * t;
                    }
                    for v in layer.data_mut() {
                        *v = 1.0 - *v;
                    }
                    multiply(&mut passed, &layer);
                }
            }
        }

        // The glass: the slide's two surfaces, the coverslip's two more, and
        // their edges.
        let size = Slide::SIZE;
        let slide_rect = Rect::new(0.0, 0.0, size.width, size.height);
        let mut out = Image::zeros(Shape::new(w, h, 3));
        let row = w as usize * 3;
        let full = we as usize * 3;
        out.data_mut()
            .par_chunks_mut(row)
            .enumerate()
            .for_each(|(y, line)| {
                let sy = origin.y + (y as f64 + f64::from(margin) + 0.5) * p;
                for x in 0..w as usize {
                    let sx = origin.x + (x as f64 + f64::from(margin) + 0.5) * p;
                    let on_slide = inside(slide_rect, sx, sy, p);
                    let covered = inside(slide.coverslip, sx, sy, p);
                    let edges = 1.0
                        - (1.0 - EDGE)
                            * (band(slide_rect, sx, sy, p) + band(slide.coverslip, sx, sy, p))
                                .min(1.0);
                    let at = (y + margin as usize) * full + (x + margin as usize) * 3;
                    for c in 0..3 {
                        line[x * 3 + c] = match light {
                            Light::Transmitted => {
                                let glass = SURFACE.powf(2.0 * (on_slide + covered));
                                (glass * edges) as f32 * passed.data()[at + c]
                            }
                            Light::Reflected => {
                                (REFLECTED * on_slide.max(covered)) as f32
                                    + reflected.data()[at + c]
                            }
                        };
                    }
                }
            });
        out
    }
}

/// How much of the pixel of side `p` around `(x, y)` lies inside `rect`.
fn inside(rect: Rect, x: f64, y: f64, p: f64) -> f64 {
    let outside = (rect.origin.x - x)
        .max(x - rect.origin.x - rect.size.width)
        .max(rect.origin.y - y)
        .max(y - rect.origin.y - rect.size.height);
    coverage(outside, p)
}

/// How much of the pixel around `(x, y)` lies in the dark band along
/// `rect`'s edge.
fn band(rect: Rect, x: f64, y: f64, p: f64) -> f64 {
    let outside = (rect.origin.x - x)
        .max(x - rect.origin.x - rect.size.width)
        .max(rect.origin.y - y)
        .max(y - rect.origin.y - rect.size.height);
    coverage(outside.abs() - EDGE_WIDTH / 2.0, p)
}
