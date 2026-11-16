//! The procedural H&E tissue: a stained section in islands, with holes,
//! uneven thickness, nuclei, stroma and cytoplasm (D23); and the same
//! section stained for immunohistochemistry, hematoxylin and DAB (P45).

use std::f64::consts::PI;

use oscope_compute::{Image, Shape};
use oscope_core::{Point2, Rect, Um};

use crate::noise::{coverage, fractal, hash, nth, value};
use crate::sample::{Layer, LayerKind, Sample, render_pixels};

/// The stains' optical densities in red, green and blue, for a unit of
/// stain, normalised (Ruifrok and Johnston, 2001).
const HEMATOXYLIN: [f64; 3] = [0.650, 0.704, 0.286];
const EOSIN: [f64; 3] = [0.072, 0.990, 0.105];
const DAB: [f64; 3] = [0.268, 0.570, 0.776];
/// Optical density per µm of section, for an amount of 1 of each stain.
const PER_UM_H: f64 = 0.35;
const PER_UM_E: f64 = 0.12;
const PER_UM_DAB: f64 = 0.25;
/// The share of nuclei with DAB in an H-DAB section, and their amounts:
/// weak, moderate and strong, in equal numbers.
const DAB_POSITIVE: f64 = 0.4;
const DAB_LEVELS: [f64; 3] = [0.5, 1.0, 1.6];

/// What each hash is for.
const ISLANDS: u64 = 1;
const EDGES: u64 = 2;
const HOLES: u64 = 3;
const THICKNESS: u64 = 4;
const NUCLEI: u64 = 5;
const CELLULARITY: u64 = 6;
const CHROMATIN: u64 = 7;
const FIBRE_ANGLE: u64 = 8;
const FIBRES: u64 = 16;

/// A nucleus: the ground truth for detecting them (P46).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Nucleus {
    /// Its centre, on the slide.
    pub centre: Point2,
    /// Half its long axis.
    pub semi_major: Um,
    /// Half its short axis.
    pub semi_minor: Um,
    /// Its long axis's angle from x, in radians.
    pub angle: f64,
    /// Which half of the section it lies in: 0, the lower, or 1.
    pub layer: usize,
    /// Its amount of hematoxylin.
    pub hematoxylin: f64,
    /// Its amount of DAB: none in H&E, or in an H-DAB section unless it is
    /// negative (P45).
    pub dab: f64,
}

impl Nucleus {
    /// The signed distance of `(x, y)` from its edge, negative inside: near
    /// enough to an ellipse's for anti-aliasing.
    fn distance(&self, x: f64, y: f64) -> f64 {
        let (dx, dy) = (x - self.centre.x, y - self.centre.y);
        let (sin, cos) = self.angle.sin_cos();
        let (u, v) = (dx * cos + dy * sin, -dx * sin + dy * cos);
        let (a, b) = (self.semi_major.0, self.semi_minor.0);
        (((u / a).powi(2) + (v / b).powi(2)).sqrt() - 1.0) * b
    }
}

/// An ellipse of tissue, before its edge is roughened.
#[derive(Clone, Copy, Debug)]
struct Island {
    centre: Point2,
    radii: (f64, f64),
    angle: f64,
}

impl Island {
    /// The signed distance of `(x, y)` from its smooth edge, in units of its
    /// short radius.
    fn distance(&self, x: f64, y: f64) -> f64 {
        let (dx, dy) = (x - self.centre.x, y - self.centre.y);
        let (sin, cos) = self.angle.sin_cos();
        let (u, v) = (dx * cos + dy * sin, -dx * sin + dy * cos);
        ((u / self.radii.0).powi(2) + (v / self.radii.1).powi(2)).sqrt() - 1.0
    }
}

/// A section of H&E-stained tissue, 4 µm thick, in 3 to 5 islands.
///
/// - **Islands** have rough edges, holes, and a thickness that varies and
///   thins towards their edges.
/// - **Nuclei,** stained by hematoxylin, lie on a jittered grid at a
///   density that varies across the tissue. Each sits in the lower or the
///   upper half of the section, which are its two layers, so focusing
///   through it at high magnification brings different nuclei into focus.
/// - **Eosin** stains the stroma's fibres, and the cytoplasm around each
///   nucleus.
/// - **Or, stained for immunohistochemistry** ([`HeTissue::ihc`]), the same
///   section has hematoxylin and no eosin, and DAB in 40 % of its nuclei,
///   weak, moderate or strong.
///
/// Everything follows from the seed (ADR-0013).
#[derive(Clone, Debug)]
pub struct HeTissue {
    seed: u64,
    bounds: Rect,
    islands: Vec<Island>,
    layers: Vec<Layer>,
    dab: bool,
}

impl HeTissue {
    /// The section's mean thickness.
    pub const THICKNESS: Um = Um(4.0);
    /// The grid that nuclei are scattered on, at most one in each cell.
    const CELL: f64 = 11.0;
    /// The grid of possible holes.
    const HOLE_CELL: f64 = 300.0;

    /// Tissue from `seed`, in islands within `bounds` on the slide.
    pub fn new(seed: u64, bounds: Rect) -> Self {
        let count = 3 + (nth(hash(seed, ISLANDS, 0, 0), 0) * 3.0) as i64;
        let short = bounds.size.width.min(bounds.size.height);
        let islands = (0..count)
            .map(|k| {
                let h = hash(seed, ISLANDS, k, 1);
                let ry = short * (0.12 + 0.1 * nth(h, 0));
                let rx = ry * (1.0 + 0.6 * nth(h, 1));
                let place =
                    |from: f64, extent: f64, f: f64| from + rx + (extent - 2.0 * rx).max(0.0) * f;
                Island {
                    centre: Point2::new(
                        place(bounds.origin.x, bounds.size.width, nth(h, 2)),
                        place(bounds.origin.y, bounds.size.height, nth(h, 3)),
                    ),
                    radii: (rx, ry),
                    angle: PI * nth(h, 4),
                }
            })
            .collect();
        let half = |name: &str, z: f64| Layer {
            name: name.to_owned(),
            z: Um(z),
            thickness: Um(Self::THICKNESS.0 / 2.0),
            kind: LayerKind::Absorbing,
        };
        Self {
            seed,
            bounds,
            islands,
            layers: vec![half("lower half", 1.0), half("upper half", 3.0)],
            dab: false,
        }
    }

    /// The section from `seed` stained for immunohistochemistry: hematoxylin
    /// and DAB, its nuclei the H&E section's (P45).
    pub fn ihc(seed: u64, bounds: Rect) -> Self {
        Self {
            dab: true,
            ..Self::new(seed, bounds)
        }
    }

    /// The second stain's optical density per µm, and its vector: eosin's,
    /// or DAB's.
    fn second(&self) -> (f64, [f64; 3]) {
        match self.dab {
            true => (PER_UM_DAB, DAB),
            false => (PER_UM_E, EOSIN),
        }
    }

    /// Its islands' centres, to aim a field at tissue.
    pub fn islands(&self) -> Vec<Point2> {
        self.islands.iter().map(|i| i.centre).collect()
    }

    /// Whether there is tissue at `at`, in slide coordinates: inside an
    /// island and not in a hole (P37's truth).
    pub fn is_tissue(&self, at: Point2) -> bool {
        self.tissue(at.x, at.y, 1.0).0 > 0.5
    }

    /// The signed distance of `(x, y)` from the nearest island's rough edge,
    /// in µm, negative inside.
    fn island_distance(&self, x: f64, y: f64, pixel: f64) -> f64 {
        let nearest = self
            .islands
            .iter()
            .map(|i| (i.distance(x, y), i.radii.1))
            .min_by(|a, b| a.0.total_cmp(&b.0));
        let Some((plain, radius)) = nearest else {
            return f64::INFINITY;
        };
        // The roughening moves the edge by at most 0.3 radii.
        if plain > 0.35 {
            return plain * radius;
        }
        let rough = fractal(self.seed, EDGES, x, y, 500.0, 3, pixel) - 0.5;
        (plain + 0.6 * rough) * radius
    }

    /// The hole at grid cell `(i, j)`, if there is one: its centre, radii
    /// and angle.
    fn hole(&self, i: i64, j: i64) -> Option<Nucleus> {
        let h = hash(self.seed, HOLES, i, j);
        if nth(h, 0) >= 0.35 {
            return None;
        }
        let cell = Self::HOLE_CELL;
        let radius = 15.0 + 85.0 * nth(h, 3);
        Some(Nucleus {
            centre: Point2::new(
                (i as f64 + 0.2 + 0.6 * nth(h, 1)) * cell,
                (j as f64 + 0.2 + 0.6 * nth(h, 2)) * cell,
            ),
            semi_major: Um(radius),
            semi_minor: Um(radius * (0.5 + 0.5 * nth(h, 4))),
            angle: PI * nth(h, 5),
            layer: 0,
            hematoxylin: 0.0,
            dab: 0.0,
        })
    }

    /// How much of the pixel around `(x, y)` is tissue, and how deep inside
    /// its island the point is, in µm.
    fn tissue(&self, x: f64, y: f64, pixel: f64) -> (f64, f64) {
        let soft = pixel.max(1.0);
        let edge = self.island_distance(x, y, pixel);
        let inside = coverage(edge, soft);
        if inside == 0.0 {
            return (0.0, 0.0);
        }
        let cell = Self::HOLE_CELL;
        let (i, j) = ((x / cell).floor() as i64, (y / cell).floor() as i64);
        let mut hole: f64 = 0.0;
        for (di, dj) in NEIGHBOURS {
            if let Some(h) = self.hole(i + di, j + dj) {
                hole = hole.max(coverage(h.distance(x, y), soft));
            }
        }
        (inside * (1.0 - hole), -edge)
    }

    /// The nucleus at grid cell `(i, j)`, if there is one.
    fn nucleus(&self, i: i64, j: i64) -> Option<Nucleus> {
        let h = hash(self.seed, NUCLEI, i, j);
        let cell = Self::CELL;
        let centre = Point2::new(
            (i as f64 + 0.15 + 0.7 * nth(h, 1)) * cell,
            (j as f64 + 0.15 + 0.7 * nth(h, 2)) * cell,
        );
        let density = 0.25 + 0.6 * value(self.seed, CELLULARITY, centre.x, centre.y, 250.0);
        if nth(h, 0) >= density {
            return None;
        }
        let semi_major = 2.8 + 1.8 * nth(h, 3);
        Some(Nucleus {
            centre,
            semi_major: Um(semi_major),
            semi_minor: Um(semi_major * (0.55 + 0.4 * nth(h, 4))),
            angle: PI * nth(h, 5),
            layer: (h >> 63) as usize,
            hematoxylin: 0.75 + 0.5 * nth(h, 6),
            dab: match self.dab && nth(h, 7) < DAB_POSITIVE {
                true => DAB_LEVELS[(nth(h, 8) * 3.0) as usize % 3],
                false => 0.0,
            },
        })
    }

    /// The optical density of half-section `layer` over the pixel of side
    /// `pixel` whose corner is at `(x0, y0)`.
    fn density(&self, layer: usize, x0: f64, y0: f64, pixel: f64) -> [f32; 3] {
        let (amount, hematoxylin, second) = self.stains(layer, x0, y0, pixel);
        let (per_um, vector) = self.second();
        let od = |c: usize| {
            (amount * (PER_UM_H * hematoxylin * HEMATOXYLIN[c] + per_um * second * vector[c]))
                as f32
        };
        [od(0), od(1), od(2)]
    }

    /// Each stain's amount at `at`, through the section's depth, over a
    /// pixel of side `pixel` centred there: hematoxylin's, and eosin's or
    /// DAB's, in units of their vectors normalised, as colour deconvolution
    /// gives them (P45's truth).
    pub fn stain_amounts(&self, at: Point2, pixel: f64) -> [f64; 2] {
        let (x0, y0) = (at.x - pixel / 2.0, at.y - pixel / 2.0);
        let (per_um, vector) = self.second();
        let length = |v: [f64; 3]| v.iter().map(|x| x * x).sum::<f64>().sqrt();
        (0..self.layers.len()).fold([0.0; 2], |sum, layer| {
            let (amount, hematoxylin, second) = self.stains(layer, x0, y0, pixel);
            [
                sum[0] + amount * PER_UM_H * hematoxylin * length(HEMATOXYLIN),
                sum[1] + amount * per_um * second * length(vector),
            ]
        })
    }

    /// Half-section `layer`'s stains over the pixel of side `pixel` whose
    /// corner is at `(x0, y0)`: its thickness of tissue, in µm, and its
    /// amounts of hematoxylin and of the second stain, eosin or DAB.
    fn stains(&self, layer: usize, x0: f64, y0: f64, pixel: f64) -> (f64, f64, f64) {
        let (x, y) = (x0 + pixel / 2.0, y0 + pixel / 2.0);
        let (tissue, depth) = self.tissue(x, y, pixel);
        if tissue == 0.0 {
            return (0.0, 0.0, 0.0);
        }
        let thinning = 0.6 + 0.4 * (depth / 150.0).clamp(0.0, 1.0);
        let thickness = Self::THICKNESS.0
            * (0.75 + 0.5 * fractal(self.seed, THICKNESS, x, y, 700.0, 3, pixel))
            * thinning;

        // Nuclei, and the cytoplasm around them.
        let cell = Self::CELL;
        let (i, j) = ((x / cell).floor() as i64, (y / cell).floor() as i64);
        let (mut nucleus, mut cytoplasm, mut dab): (f64, f64, f64) = (0.0, 0.0, 0.0);
        for (di, dj) in NEIGHBOURS {
            let Some(n) = self.nucleus(i + di, j + dj).filter(|n| n.layer == layer) else {
                continue;
            };
            let d = n.distance(x, y);
            nucleus = nucleus.max(coverage(d, pixel) * n.hematoxylin);
            cytoplasm = cytoplasm.max(coverage(d - 2.5, pixel));
            dab = dab.max(coverage(d, pixel) * n.dab);
        }
        if nucleus > 0.0 {
            nucleus *= 0.7 + 0.6 * fractal(self.seed, CHROMATIN, x, y, 1.6, 3, pixel);
        }

        // Stroma: fibres along the seed's direction, bent by a gentle warp.
        // (Turning the slide's coordinates by an angle that varies would
        // ring far from the origin, where a small turn moves a long way.)
        let bend = |salt: u64| 60.0 * (value(self.seed, salt, x, y, 200.0) - 0.5);
        let (sin, cos) = (PI * nth(hash(self.seed, FIBRE_ANGLE, 0, 0), 0)).sin_cos();
        let along = x * cos + y * sin + bend(FIBRE_ANGLE + 1);
        let across = -x * sin + y * cos + bend(FIBRE_ANGLE + 2);
        let fibres = fractal(self.seed, FIBRES, along / 4.0, across, 8.0, 6, pixel);
        let stroma = 0.25 + 0.6 * fibres;
        let eosin = (stroma + (0.45 - stroma) * cytoplasm) * (1.0 - 0.6 * nucleus.min(1.0));
        let hematoxylin = nucleus + 0.04;
        let second = if self.dab { dab } else { eosin };
        (tissue * thickness / 2.0, hematoxylin, second)
    }

    /// The nuclei whose centres lie in `region`, and in tissue.
    pub fn nuclei(&self, region: Rect) -> Vec<Nucleus> {
        let cell = Self::CELL;
        let first = |from: f64| (from / cell).floor() as i64;
        let last = |to: f64| (to / cell).ceil() as i64;
        let mut found = Vec::new();
        for j in first(region.origin.y)..=last(region.origin.y + region.size.height) {
            for i in first(region.origin.x)..=last(region.origin.x + region.size.width) {
                let Some(n) = self.nucleus(i, j) else {
                    continue;
                };
                if region.contains(n.centre) && self.tissue(n.centre.x, n.centre.y, 0.1).0 > 0.5 {
                    found.push(n);
                }
            }
        }
        found
    }
}

/// A grid cell and the eight around it.
const NEIGHBOURS: [(i64, i64); 9] = [
    (-1, -1),
    (0, -1),
    (1, -1),
    (-1, 0),
    (0, 0),
    (1, 0),
    (-1, 1),
    (0, 1),
    (1, 1),
];

impl Sample for HeTissue {
    fn name(&self) -> &str {
        match self.dab {
            true => "H-DAB tissue",
            false => "H&E tissue",
        }
    }

    fn layers(&self) -> &[Layer] {
        &self.layers
    }

    fn bounds(&self) -> Rect {
        self.bounds
    }

    fn render(&self, index: usize, origin: Point2, pixel: Um, width: u32, height: u32) -> Image {
        if index >= self.layers.len() {
            return Image::zeros(Shape::new(width, height, 3));
        }
        render_pixels(origin, pixel, width, height, |x0, y0| {
            self.density(index, x0, y0, pixel.0)
        })
    }
}
