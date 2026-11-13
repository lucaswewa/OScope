//! The world: a slide with a coverslip and a sample, placed on the stage,
//! and what the camera sees of it through the optics.

use std::sync::Arc;

use oscope_compute::{Image, Shape};
use oscope_core::{Point2, Rect, Size2, Um};
use rayon::prelude::*;

use crate::blur::gaussian;
use crate::fluorescence::{Excitation, optical_section, sectioning};
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
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Light {
    /// From below, through the slide: brightfield.
    Transmitted,
    /// From above, reflected back into the objective.
    Reflected,
    /// Fluorescence, in the simulator only: dyes excited by `line`, seen in
    /// widefield or through a confocal's pinhole (`fluorescence`).
    Fluorescence {
        /// The excitation line.
        line: Excitation,
        /// Whether a confocal's pinhole sections the light.
        confocal: bool,
    },
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
    /// (`Optics::blur`).
    ///
    /// - **Transmitted:** the layers' transmittances multiply, with a
    ///   reflective layer passing what it doesn't reflect.
    /// - **Reflected:** each reflective layer's reflection passes twice
    ///   through the absorbing layers above it (ADR-0013).
    /// - **Fluorescence:** each fluorescent layer's dye, blurred at its
    ///   emission's wavelength, gives its colour in proportion to how well
    ///   the line excites it; a confocal passes less of it the further it is
    ///   from focus.
    ///
    /// Each light sees only its own layers: dyes in fluorescence, stains and
    /// reflections otherwise.
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

        // The layers this light sees, and how far each is from the focal
        // plane, in pixels: a dye's at its emission's wavelength. A dye the
        // line hardly excites is left out.
        let fluorescence = match light {
            Light::Fluorescence { line, confocal } => Some((line, confocal)),
            _ => None,
        };
        let focal = field.z.0 - self.focus.0;
        let layers = slide.sample.layers();
        let seen: Vec<bool> = layers
            .iter()
            .map(|l| match (l.kind, fluorescence) {
                (LayerKind::Fluorescent(dye), Some((line, _))) => dye.excited_by(line) > 1e-6,
                (LayerKind::Fluorescent(_), None) => false,
                (_, light) => light.is_none(),
            })
            .collect();
        let sigmas: Vec<f64> = layers
            .iter()
            .map(|l| {
                let wavelength = match l.kind {
                    LayerKind::Fluorescent(dye) => dye.peaks().1,
                    _ => optics.wavelength,
                };
                let at = Optics {
                    wavelength,
                    ..*optics
                };
                at.blur(Um(focal - l.z.0)).0 / p
            })
            .collect();
        let margin = sigmas
            .iter()
            .zip(&seen)
            .filter(|(_, seen)| **seen)
            .fold(0.0_f64, |m, (s, _)| m.max(*s))
            .mul_add(3.0, 1.0)
            .ceil() as u32;
        let (we, he) = (w + 2 * margin, h + 2 * margin);
        let origin = Point2::new(
            corner.x - f64::from(margin) * p - slide.origin.x,
            corner.y - f64::from(margin) * p - slide.origin.y,
        );

        let filled = |value: f32| {
            let mut image = Image::zeros(Shape::new(we, he, 3));
            image.data_mut().fill(value);
            image
        };
        // What passes all the layers so far, what passes only their stains,
        // what they reflect, and what their dyes emit.
        let (mut passed, mut absorbed) = (filled(1.0), filled(1.0));
        let (mut reflected, mut emitted) = (filled(0.0), filled(0.0));
        // From the top down, so each reflection is dimmed by what is above.
        let mut order: Vec<usize> = (0..layers.len()).filter(|i| seen[*i]).collect();
        order.sort_by(|a, b| layers[*b].z.0.total_cmp(&layers[*a].z.0));
        for index in order {
            let mut layer = slide.sample.render(index, origin, Um(p), we, he);
            if layer.data().iter().all(|v| *v == 0.0) {
                continue;
            }
            match (layers[index].kind, fluorescence) {
                (LayerKind::Absorbing, _) => {
                    for v in layer.data_mut() {
                        *v = 10f32.powf(-*v);
                    }
                    gaussian(&mut layer, sigmas[index]);
                    multiply(&mut passed, &layer);
                    multiply(&mut absorbed, &layer);
                }
                (LayerKind::Reflective, _) => {
                    gaussian(&mut layer, sigmas[index]);
                    for ((r, l), t) in reflected
                        .data_mut()
                        .iter_mut()
                        .zip(layer.data())
                        .zip(absorbed.data())
                    {
                        *r += l * t * t;
                    }
                    for v in layer.data_mut() {
                        *v = 1.0 - *v;
                    }
                    multiply(&mut passed, &layer);
                }
                (LayerKind::Fluorescent(dye), Some((line, confocal))) => {
                    // The dye's amount, blurred once, then given its colour.
                    let amounts = layer.data().iter().step_by(3).copied().collect();
                    let mut amount = Image::from_vec(Shape::new(we, he, 1), amounts)
                        .expect("one channel of the layer's size");
                    gaussian(&mut amount, sigmas[index]);
                    let kept = if confocal {
                        let section = optical_section(optics, dye.peaks().1);
                        sectioning(Um(focal - layers[index].z.0), section)
                    } else {
                        1.0
                    };
                    let excited = dye.excited_by(line) * kept;
                    let gain = dye.colour().map(|c| (c * excited) as f32);
                    for (out, a) in emitted.data_mut().chunks_mut(3).zip(amount.data()) {
                        for (o, g) in out.iter_mut().zip(gain) {
                            *o += a * g;
                        }
                    }
                }
                (LayerKind::Fluorescent(_), None) => {}
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
                            Light::Fluorescence { .. } => emitted.data()[at + c],
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
