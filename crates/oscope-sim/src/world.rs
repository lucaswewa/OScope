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
    /// The stage's z at which the slide's top surface is in focus, at the
    /// slide's corner.
    pub focus: Um,
    /// The slide's tilt (P35): how much higher its surface is in focus, in
    /// µm, a mm further from its corner along x, and along y. A field
    /// renders at the tilt of its centre.
    pub tilt: [f64; 2],
}

/// The largest blur, in pixels, rendered at the field's own pixel: a layer
/// further from focus renders at a coarser pixel. The margin around a field
/// stops at three times this.
const MOST_BLUR: f64 = 64.0;

/// `image`, rendered at pixels `factor` times the field's from `offset` of
/// the field's pixels above and left of its corner, interpolated to `width`
/// × `height` of the field's pixels.
fn enlarge(image: &Image, factor: f64, offset: f64, width: u32, height: u32) -> Image {
    let (cw, ch, c) = (
        image.width() as usize,
        image.height() as usize,
        image.channels(),
    );
    let data = image.data();
    let mut out = Image::zeros(Shape::new(width, height, c));
    out.data_mut()
        .par_chunks_mut(width as usize * c)
        .enumerate()
        .for_each(|(y, row)| {
            // Each pixel's centre, in the coarse pixels' centres' grid.
            let fy = ((y as f64 + 0.5 + offset) / factor - 0.5).clamp(0.0, (ch - 1) as f64);
            let (y0, ay) = (fy.floor() as usize, fy - fy.floor());
            let y1 = (y0 + 1).min(ch - 1);
            for x in 0..width as usize {
                let fx = ((x as f64 + 0.5 + offset) / factor - 0.5).clamp(0.0, (cw - 1) as f64);
                let (x0, ax) = (fx.floor() as usize, fx - fx.floor());
                let x1 = (x0 + 1).min(cw - 1);
                for k in 0..c {
                    let at = |x: usize, y: usize| f64::from(data[(y * cw + x) * c + k]);
                    let top = at(x0, y0) + (at(x1, y0) - at(x0, y0)) * ax;
                    let bottom = at(x0, y1) + (at(x1, y1) - at(x0, y1)) * ax;
                    row[x * c + k] = (top + (bottom - top) * ay) as f32;
                }
            }
        });
    out
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
            tilt: [0.0; 2],
        }
    }

    /// A world with `slide` loaded, its surface in focus at stage z
    /// `focus`.
    pub fn with_slide(slide: Slide, focus: Um) -> Self {
        Self {
            slide: Some(slide),
            focus,
            tilt: [0.0; 2],
        }
    }

    /// This world, with its slide tilted by `tilt` (see [`World::tilt`]).
    pub fn tilted(self, tilt: [f64; 2]) -> Self {
        Self { tilt, ..self }
    }

    /// The stage's z at which the slide's surface is in focus at stage
    /// point `at`: its focus, plus its tilt that far from its corner.
    pub fn focus_at(&self, at: Point2) -> Um {
        let origin = self.slide.as_ref().map_or(at, |s| s.origin);
        let (dx, dy) = ((at.x - origin.x) / 1_000.0, (at.y - origin.y) / 1_000.0);
        Um(self.focus.0 + self.tilt[0] * dx + self.tilt[1] * dy)
    }

    /// The stage's z at which the slide is sharpest at stage point `at`: its
    /// focus there, tilt and all, plus its sample's height in focus there
    /// (P34's truth). `None` with no slide.
    pub fn in_focus(&self, at: Point2) -> Option<Um> {
        let slide = self.slide.as_ref()?;
        let local = Point2::new(at.x - slide.origin.x, at.y - slide.origin.y);
        Some(Um(self.focus_at(at).0 + slide.sample.in_focus(local).0))
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
        let focal = field.z.0 - self.focus_at(field.centre).0;
        let layers = slide.sample.layers();
        let Blurs {
            seen,
            sigmas,
            margin,
        } = self.blurs(field, optics, light);
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
            // A layer blurred by more than the cap renders at a coarser
            // pixel, blurs there, and is interpolated back: the same light,
            // at a bounded cost however far it is from focus.
            let sigma = sigmas[index];
            let coarse = if sigma > MOST_BLUR {
                (sigma / MOST_BLUR).ceil()
            } else {
                1.0
            };
            // Coarse, it covers the blur's own reach, 3σ around the field:
            // at the coarse pixel, about 6 × 64 pixels more, whatever σ is.
            let extra = if coarse > 1.0 {
                ((3.0 * sigma).ceil() - f64::from(margin)).max(0.0)
            } else {
                0.0
            };
            let (lw, lh) = if coarse > 1.0 {
                (
                    ((f64::from(we) + 2.0 * extra) / coarse).ceil() as u32 + 1,
                    ((f64::from(he) + 2.0 * extra) / coarse).ceil() as u32 + 1,
                )
            } else {
                (we, he)
            };
            let from = Point2::new(origin.x - extra * p, origin.y - extra * p);
            let mut layer = slide.sample.render(index, from, Um(p * coarse), lw, lh);
            if layer.data().iter().all(|v| *v == 0.0) {
                continue;
            }
            let kind = layers[index].kind;
            // What blurs: transmittance, reflectance, or a dye's amount.
            match kind {
                LayerKind::Absorbing => {
                    for v in layer.data_mut() {
                        *v = 10f32.powf(-*v);
                    }
                }
                LayerKind::Fluorescent(_) => {
                    let amounts = layer.data().iter().step_by(3).copied().collect();
                    layer = Image::from_vec(Shape::new(lw, lh, 1), amounts)
                        .expect("one channel of the layer's size");
                }
                LayerKind::Reflective => {}
            }
            gaussian(&mut layer, sigma / coarse);
            if coarse > 1.0 {
                layer = enlarge(&layer, coarse, extra, we, he);
            }
            match (kind, fluorescence) {
                (LayerKind::Absorbing, _) => {
                    multiply(&mut passed, &layer);
                    multiply(&mut absorbed, &layer);
                }
                (LayerKind::Reflective, _) => {
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
                    let kept = if confocal {
                        let section = optical_section(optics, dye.peaks().1);
                        sectioning(Um(focal - layers[index].z.0), section)
                    } else {
                        1.0
                    };
                    let excited = dye.excited_by(line) * kept;
                    let gain = dye.colour().map(|c| (c * excited) as f32);
                    for (out, a) in emitted.data_mut().chunks_mut(3).zip(layer.data()) {
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

impl World {
    /// The slide's layers that `light` sees, each layer's blur for `field`
    /// as a standard deviation in pixels, and the margin the blur needs: as
    /// [`World::render`] blurs them, and the GPU's tiles too (ADR-0062).
    pub(crate) fn blurs(&self, field: &Field, optics: &Optics, light: Light) -> Blurs {
        let p = optics.pixel_size().0;
        let fluorescence = match light {
            Light::Fluorescence { line, confocal } => Some((line, confocal)),
            _ => None,
        };
        let Some(slide) = &self.slide else {
            return Blurs {
                seen: Vec::new(),
                sigmas: Vec::new(),
                margin: 1,
            };
        };
        let focal = field.z.0 - self.focus_at(field.centre).0;
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
        // The margin the blur needs, up to the blur's cap: beyond it, the
        // blur repeats the field's edge.
        let margin = sigmas
            .iter()
            .zip(&seen)
            .filter(|(_, seen)| **seen)
            .fold(0.0_f64, |m, (s, _)| m.max(*s))
            .min(MOST_BLUR)
            .mul_add(3.0, 1.0)
            .ceil() as u32;
        Blurs {
            seen,
            sigmas,
            margin,
        }
    }
}

/// What [`World::blurs`] gives.
pub(crate) struct Blurs {
    /// Whether the light sees each layer.
    pub seen: Vec<bool>,
    /// Each layer's blur, in pixels.
    pub sigmas: Vec<f64>,
    /// The margin the blurs need, in pixels.
    pub margin: u32,
}

/// The largest blur, in pixels, at which a layer renders at the field's own
/// pixel: the GPU's tiles go no further (ADR-0062).
#[cfg(feature = "cuda")]
pub(crate) const FINEST_BLUR: f64 = MOST_BLUR;

/// The glass, as the GPU's tiles compose it (ADR-0062): the slide's
/// rectangle and the coverslip's, on the slide; a surface's transmittance;
/// an edge's; and the width of an edge's dark band.
#[cfg(feature = "cuda")]
pub(crate) fn glass(slide: &Slide) -> [f64; 11] {
    let (size, c) = (Slide::SIZE, slide.coverslip);
    [
        0.0,
        0.0,
        size.width,
        size.height,
        c.origin.x,
        c.origin.y,
        c.size.width,
        c.size.height,
        SURFACE,
        EDGE,
        EDGE_WIDTH,
    ]
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
