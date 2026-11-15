//! A reflective surface with a known height: the ground truth for depth
//! from focus (D23).

use oscope_compute::Image;
use oscope_core::{Point2, Rect, Um};

use crate::noise::{coverage, fractal};
use crate::sample::{Layer, LayerKind, Sample, render_pixels};

/// Silicon's reflectance in red, green and blue: Fresnel's ((n − 1)² + k²)
/// / ((n + 1)² + k²) at 650, 550 and 450 nm, from its published refractive
/// index (Green, 2008).
const SILICON: [f64; 3] = [0.345, 0.367, 0.42];
/// Aluminium's, the same way (Rakić, 1995).
const ALUMINIUM: [f64; 3] = [0.91, 0.92, 0.92];
/// The share of light etched silicon reflects, being rougher.
const ETCHED: f64 = 0.6;

/// The chip's base, above the slide, at its corner.
const BASE: f64 = 2.0;
/// The range of its waviness: up to half of it either way.
const WAVE: f64 = 0.15;
/// Its tilt, in µm a mm, along x and along y.
const TILT: [f64; 2] = [0.2, 0.1];
/// The aluminium film's thickness, and the pits' depth below the base.
const FILM: f64 = 0.5;
const PIT: f64 = -1.0;
/// The stepped pyramid's edges, along a die's quadrant, and their heights.
const TERRACES: [(f64, f64); 6] = [
    (10.0, 1.0),
    (25.0, 2.0),
    (40.0, 3.0),
    (60.0, 2.0),
    (75.0, 1.0),
    (90.0, 0.0),
];

/// Where a surface is at one place: its height, and what it reflects.
#[derive(Clone, Copy, Debug)]
struct Part {
    share: f64,
    height: f64,
    reflectance: [f64; 3],
}

/// A 5 × 5 mm chip of silicon with features on it, seen in reflected light,
/// whose height everywhere is known. It is a grid of 200 µm dies, each with
/// four 100 µm quadrants:
///
/// - **a stepped pyramid:** terraces 1 µm apart, 3 µm high in the middle;
/// - **a dome:** a spherical cap 80 µm across and 3 µm high;
/// - **a ridge:** two slopes, rising 4 µm over 50 µm and falling over 30;
/// - **etched pits:** a film of aluminium 0.5 µm thick, with pits 10 µm
///   square every 20 µm etched through it, 1 µm into the silicon.
///
/// The chip lies 2 µm above the slide at its corner, and is tilted, 0.2 µm a
/// mm along x and 0.1 along y. Its height has a fine waviness, and its
/// reflectance a grain, so that every objective finds detail to focus on.
///
/// **It renders as slabs** 0.5 µm apart, each a reflective layer. A pixel's
/// reflectance goes to the two slabs either side of its height, in
/// proportion to how near each is, so that its blur follows its height.
/// Under them, an opaque layer stops transmitted light.
#[derive(Clone, Debug)]
pub struct Surface {
    seed: u64,
    bounds: Rect,
    layers: Vec<Layer>,
}

impl Surface {
    /// The side of the chip.
    pub const SIDE: Um = Um(5_000.0);
    /// The side of a die.
    pub const DIE: Um = Um(200.0);
    /// The spacing of the slabs.
    pub const SLAB: Um = Um(0.5);
    /// The highest slab, above any height the chip reaches.
    pub const TOP: Um = Um(8.0);

    /// A chip with its corner at `corner` on the slide, its texture from
    /// `seed`.
    pub fn new(seed: u64, corner: Point2) -> Self {
        let mut layers = vec![Layer {
            name: "substrate".to_owned(),
            z: Um(-0.25),
            thickness: Um(0.5),
            kind: LayerKind::Absorbing,
        }];
        let slabs = (Self::TOP.0 / Self::SLAB.0).round() as usize;
        layers.extend((0..=slabs).map(|k| {
            let z = k as f64 * Self::SLAB.0;
            Layer {
                name: format!("surface at {z:.1} µm"),
                z: Um(z),
                thickness: Self::SLAB,
                kind: LayerKind::Reflective,
            }
        }));
        Self {
            seed,
            bounds: Rect::new(corner.x, corner.y, Self::SIDE.0, Self::SIDE.0),
            layers,
        }
    }

    /// The die's centre nearest `at`, on the slide.
    pub fn die_centre(&self, at: Point2) -> Point2 {
        let d = Self::DIE.0;
        let centre = |v: f64, from: f64| from + ((v - from) / d).floor() * d + d / 2.0;
        Point2::new(
            centre(at.x, self.bounds.origin.x),
            centre(at.y, self.bounds.origin.y),
        )
    }

    /// The surface's height at `at` on the slide, above the slide's
    /// surface: the ground truth. `None` off the chip.
    pub fn height(&self, at: Point2) -> Option<Um> {
        let parts = self.parts(at.x, at.y, 1e-3);
        let main = parts.iter().max_by(|a, b| a.share.total_cmp(&b.share))?;
        let height = main.height + self.wave(at.x, at.y, 1e-3);
        (self.chip_share(at.x, at.y, 1e-3) > 0.5).then_some(Um(height))
    }

    /// The heights at the centres of a grid of `width` × `height` pixels of
    /// side `pixel` from `origin`, each its parts' mean: the height map that
    /// depth from focus should find. NaN off the chip.
    pub fn height_map(&self, origin: Point2, pixel: Um, width: u32, height: u32) -> Vec<f32> {
        let p = pixel.0;
        let mut map = Vec::with_capacity(width as usize * height as usize);
        for y in 0..height {
            for x in 0..width {
                let (cx, cy) = (
                    origin.x + (f64::from(x) + 0.5) * p,
                    origin.y + (f64::from(y) + 0.5) * p,
                );
                if self.chip_share(cx, cy, p) < 0.5 {
                    map.push(f32::NAN);
                    continue;
                }
                let mean: f64 = self
                    .parts(cx, cy, p)
                    .iter()
                    .map(|part| part.share * part.height)
                    .sum();
                map.push((mean + self.wave(cx, cy, p)) as f32);
            }
        }
        map
    }

    /// How much of the pixel of side `p` at `(x, y)` the chip covers.
    fn chip_share(&self, x: f64, y: f64, p: f64) -> f64 {
        let b = self.bounds;
        let outside = (b.origin.x - x)
            .max(x - b.origin.x - b.size.width)
            .max(b.origin.y - y)
            .max(y - b.origin.y - b.size.height);
        coverage(outside, p)
    }

    /// The surface's waviness at `(x, y)`: [`WAVE`] of fractal noise, with
    /// features 7.5 to 30 µm across.
    fn wave(&self, x: f64, y: f64, p: f64) -> f64 {
        WAVE * (fractal(self.seed, 1, x, y, 30.0, 3, p) - 0.5)
    }

    /// The share of light the surface's grain leaves, from 0.6 to 1: mostly
    /// detail 0.75 to 1.5 µm across, which focusing needs, and some 6 to
    /// 12 µm across.
    fn grain(&self, x: f64, y: f64, p: f64) -> f64 {
        let fine = fractal(self.seed, 2, x, y, 1.5, 2, p);
        let coarse = fractal(self.seed, 3, x, y, 12.0, 2, p);
        1.0 - 0.4 * (0.65 * fine + 0.35 * coarse)
    }

    /// What the pixel of side `p` centred at `(x, y)` holds: up to three
    /// parts of the surface, at their heights but for the waviness, sharing
    /// it.
    fn parts(&self, x: f64, y: f64, p: f64) -> Vec<Part> {
        let (u0, v0) = (x - self.bounds.origin.x, y - self.bounds.origin.y);
        let base = BASE + TILT[0] * u0 / 1_000.0 + TILT[1] * v0 / 1_000.0;
        let d = Self::DIE.0;
        let (u, v) = (u0.rem_euclid(d), v0.rem_euclid(d));
        let half = d / 2.0;
        let part = |share, height, reflectance| Part {
            share,
            height: base + height,
            reflectance,
        };
        match (u < half, v < half) {
            // The stepped pyramid: the edge nearest the pixel splits it.
            (true, true) => {
                let level = |at: f64| {
                    TERRACES
                        .iter()
                        .take_while(|(edge, _)| at >= *edge)
                        .last()
                        .map_or(0.0, |(_, h)| *h)
                };
                let (edge, _) = TERRACES
                    .iter()
                    .min_by(|a, b| (a.0 - u).abs().total_cmp(&(b.0 - u).abs()))
                    .copied()
                    .unwrap_or((0.0, 0.0));
                let after = coverage(edge - u, p);
                let (below, above) = (level(edge - 1.0), level(edge));
                vec![
                    part(1.0 - after, below, SILICON),
                    part(after, above, SILICON),
                ]
            }
            // The dome: a cap of a sphere of radius (r² + h²) / 2h.
            (false, true) => {
                let (r, h) = (40.0, 3.0);
                let sphere = (r * r + h * h) / (2.0 * h);
                let dist = (u - 150.0).hypot(v - 50.0);
                let top = if dist < r {
                    (sphere * sphere - dist * dist).sqrt() - (sphere - h)
                } else {
                    0.0
                };
                vec![part(1.0, top, SILICON)]
            }
            // The ridge: up from 10 to 60 µm, down to 90.
            (true, false) => {
                let top = if u < 60.0 {
                    4.0 * ((u - 10.0) / 50.0).clamp(0.0, 1.0)
                } else {
                    4.0 * ((90.0 - u) / 30.0).clamp(0.0, 1.0)
                };
                vec![part(1.0, top, SILICON)]
            }
            // The film, from 5 µm inside the quadrant, with its pits.
            (false, false) => {
                let (fu, fv) = (u - 150.0, v - 150.0);
                let film = coverage(fu.abs().max(fv.abs()) - 45.0, p);
                let (pu, pv) = (fu.rem_euclid(20.0) - 10.0, fv.rem_euclid(20.0) - 10.0);
                let pit = coverage(pu.abs().max(pv.abs()) - 5.0, p);
                vec![
                    part(1.0 - film, 0.0, SILICON),
                    part(film * (1.0 - pit), FILM, ALUMINIUM),
                    part(film * pit, PIT, SILICON.map(|r| r * ETCHED)),
                ]
            }
        }
    }
}

impl Sample for Surface {
    fn name(&self) -> &str {
        "reflective surface"
    }

    /// The chip's surface at `at`, or the slide's beside it.
    fn in_focus(&self, at: Point2) -> Um {
        self.height(at).unwrap_or(Um(0.0))
    }

    fn layers(&self) -> &[Layer] {
        &self.layers
    }

    fn bounds(&self) -> Rect {
        self.bounds
    }

    fn render(&self, index: usize, origin: Point2, pixel: Um, width: u32, height: u32) -> Image {
        let p = pixel.0;
        let Some(layer) = self.layers.get(index) else {
            return render_pixels(origin, pixel, width, height, |_, _| [0.0; 3]);
        };
        let (z, slab) = (layer.z.0, Self::SLAB.0);
        let substrate = index == 0;
        render_pixels(origin, pixel, width, height, |x0, y0| {
            let (x, y) = (x0 + p / 2.0, y0 + p / 2.0);
            let chip = self.chip_share(x, y, p);
            if chip == 0.0 {
                return [0.0; 3];
            }
            if substrate {
                return [(4.0 * chip) as f32; 3];
            }
            // Most slabs are far from the pixel's height: they are known so
            // before the noise is computed.
            let parts = self.parts(x, y, p);
            if parts
                .iter()
                .all(|part| (part.height - z).abs() >= slab + WAVE / 2.0)
            {
                return [0.0; 3];
            }
            let (wave, grain) = (self.wave(x, y, p), self.grain(x, y, p));
            let mut rgb = [0.0_f64; 3];
            for part in parts {
                let near = 1.0 - ((part.height + wave - z) / slab).abs();
                if near > 0.0 {
                    for (c, r) in rgb.iter_mut().zip(part.reflectance) {
                        *c += chip * part.share * near * r * grain;
                    }
                }
            }
            rgb.map(|c| c.min(1.0) as f32)
        })
    }
}
