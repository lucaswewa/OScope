//! The procedural H&E tissue: a stained section in islands, with holes,
//! uneven thickness, nuclei, stroma and cytoplasm (D23); and the same
//! section stained for immunohistochemistry, hematoxylin and DAB (P45).

use std::f64::consts::PI;

use oscope_compute::{Image, Shape};
use oscope_core::{Point2, Rect, Um};

use crate::noise::{coverage, fractal, hash, nth, value};
use rayon::prelude::*;

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
    /// enough to an ellipse's for anti-aliasing. `turn` is its angle's.
    fn distance(&self, turn: Turn, x: f64, y: f64) -> f64 {
        let (dx, dy) = (x - self.centre.x, y - self.centre.y);
        let (u, v) = turn.apply(dx, dy);
        let (a, b) = (self.semi_major.0, self.semi_minor.0);
        (((u / a).powi(2) + (v / b).powi(2)).sqrt() - 1.0) * b
    }
}

/// An angle's sine and cosine, worked out once for every pixel near its
/// shape.
#[derive(Clone, Copy, Debug)]
struct Turn {
    sin: f64,
    cos: f64,
}

impl Turn {
    fn of(angle: f64) -> Self {
        let (sin, cos) = angle.sin_cos();
        Self { sin, cos }
    }

    /// `(dx, dy)` along the turned axes.
    fn apply(self, dx: f64, dy: f64) -> (f64, f64) {
        (
            dx * self.cos + dy * self.sin,
            -dx * self.sin + dy * self.cos,
        )
    }
}

/// An ellipse of tissue, before its edge is roughened.
#[derive(Clone, Copy, Debug)]
struct Island {
    centre: Point2,
    radii: (f64, f64),
    turn: Turn,
}

impl Island {
    /// The signed distance of `(x, y)` from its smooth edge, in units of its
    /// short radius.
    fn distance(&self, x: f64, y: f64) -> f64 {
        let (dx, dy) = (x - self.centre.x, y - self.centre.y);
        let (u, v) = self.turn.apply(dx, dy);
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
    /// The stroma's fibres' direction, from the seed.
    fibres: Turn,
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
                    turn: Turn::of(PI * nth(h, 4)),
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
            fibres: Turn::of(PI * nth(hash(seed, FIBRE_ANGLE, 0, 0), 0)),
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
        self.tissue_near(x, y, pixel, &Direct(self))
    }

    /// As [`HeTissue::tissue`], with the holes from `near`.
    fn tissue_near(&self, x: f64, y: f64, pixel: f64, near: &impl Near) -> (f64, f64) {
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
            if let Some((h, turn)) = near.hole(i + di, j + dj) {
                hole = hole.max(coverage(h.distance(turn, x, y), soft));
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
    fn density(&self, layer: usize, x0: f64, y0: f64, pixel: f64, near: &impl Near) -> [f32; 3] {
        self.densities(self.stains_near(layer, x0, y0, pixel, near))
    }

    /// The optical density that stains of `(amount, hematoxylin, second)`
    /// give.
    fn densities(&self, (amount, hematoxylin, second): (f64, f64, f64)) -> [f32; 3] {
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
        self.stains_near(layer, x0, y0, pixel, &Direct(self))
    }

    /// As [`HeTissue::stains`], with the nuclei and holes from `near`.
    fn stains_near(
        &self,
        layer: usize,
        x0: f64,
        y0: f64,
        pixel: f64,
        near: &impl Near,
    ) -> (f64, f64, f64) {
        match self.shared(x0, y0, pixel, near) {
            Some(shared) => self.layer_stains(layer, &shared, pixel, near, &mut None),
            None => (0.0, 0.0, 0.0),
        }
    }

    /// What the pixel of side `pixel` whose corner is at `(x0, y0)` is in
    /// every layer: none if it isn't tissue.
    fn shared(&self, x0: f64, y0: f64, pixel: f64, near: &impl Near) -> Option<Shared> {
        let (x, y) = (x0 + pixel / 2.0, y0 + pixel / 2.0);
        let (tissue, depth) = self.tissue_near(x, y, pixel, near);
        if tissue == 0.0 {
            return None;
        }
        let thinning = 0.6 + 0.4 * (depth / 150.0).clamp(0.0, 1.0);
        let thickness = Self::THICKNESS.0
            * (0.75 + 0.5 * fractal(self.seed, THICKNESS, x, y, 700.0, 3, pixel))
            * thinning;

        // Stroma: fibres along the seed's direction, bent by a gentle warp.
        // (Turning the slide's coordinates by an angle that varies would
        // ring far from the origin, where a small turn moves a long way.)
        // With DAB, eosin isn't the second stain, and they aren't seen.
        let stroma = match self.dab {
            true => 0.0,
            false => {
                let bend = |salt: u64| 60.0 * (value(self.seed, salt, x, y, 200.0) - 0.5);
                let Turn { sin, cos } = self.fibres;
                let along = x * cos + y * sin + bend(FIBRE_ANGLE + 1);
                let across = -x * sin + y * cos + bend(FIBRE_ANGLE + 2);
                let fibres = fractal(self.seed, FIBRES, along / 4.0, across, 8.0, 6, pixel);
                0.25 + 0.6 * fibres
            }
        };
        Some(Shared {
            x,
            y,
            tissue,
            thickness,
            stroma,
        })
    }

    /// Half-section `layer`'s stains, as [`HeTissue::stains`] gives them,
    /// at the pixel `shared` describes; `chromatin` is its chromatin's
    /// texture, found the first time a layer needs it.
    fn layer_stains(
        &self,
        layer: usize,
        shared: &Shared,
        pixel: f64,
        near: &impl Near,
        chromatin: &mut Option<f64>,
    ) -> (f64, f64, f64) {
        let Shared {
            x,
            y,
            tissue,
            thickness,
            stroma,
        } = *shared;

        // Nuclei, and the cytoplasm around them.
        let cell = Self::CELL;
        let (i, j) = ((x / cell).floor() as i64, (y / cell).floor() as i64);
        let (mut nucleus, mut cytoplasm, mut dab): (f64, f64, f64) = (0.0, 0.0, 0.0);
        for (di, dj) in NEIGHBOURS {
            let Some((n, turn)) = near
                .nucleus(i + di, j + dj)
                .filter(|(n, _)| n.layer == layer)
            else {
                continue;
            };
            let d = n.distance(turn, x, y);
            nucleus = nucleus.max(coverage(d, pixel) * n.hematoxylin);
            cytoplasm = cytoplasm.max(coverage(d - 2.5, pixel));
            dab = dab.max(coverage(d, pixel) * n.dab);
        }
        if nucleus > 0.0 {
            let texture = *chromatin
                .get_or_insert_with(|| fractal(self.seed, CHROMATIN, x, y, 1.6, 3, pixel));
            nucleus *= 0.7 + 0.6 * texture;
        }
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
        // The nuclei and holes near the pixels, found once for all of them.
        let near = Found::around(self, origin, pixel.0, width, height);
        render_pixels(origin, pixel, width, height, |x0, y0| {
            self.density(index, x0, y0, pixel.0, &near)
        })
    }

    /// Each of `indices` as [`Sample::render`] renders it, with what the
    /// layers share, the tissue, its thickness and its stroma, worked out
    /// once a pixel for all of them.
    fn render_layers(
        &self,
        indices: &[usize],
        origin: Point2,
        pixel: Um,
        width: u32,
        height: u32,
    ) -> Vec<Image> {
        let near = Found::around(self, origin, pixel.0, width, height);
        let (n, row) = (indices.len(), width as usize * 3);
        let mut all = vec![0.0_f32; n * row * height as usize];
        if n > 0 && row > 0 {
            // Each row's layers side by side: layer k's row at k · row.
            all.par_chunks_mut(n * row)
                .enumerate()
                .for_each(|(y, rows)| {
                    let y0 = origin.y + y as f64 * pixel.0;
                    for x in 0..width as usize {
                        let x0 = origin.x + x as f64 * pixel.0;
                        let Some(shared) = self.shared(x0, y0, pixel.0, &near) else {
                            continue;
                        };
                        let mut chromatin = None;
                        for (k, &layer) in indices.iter().enumerate() {
                            if layer >= self.layers.len() {
                                continue;
                            }
                            let stains =
                                self.layer_stains(layer, &shared, pixel.0, &near, &mut chromatin);
                            let at = k * row + x * 3;
                            rows[at..at + 3].copy_from_slice(&self.densities(stains));
                        }
                    }
                });
        }
        (0..n)
            .map(|k| {
                let data = all
                    .chunks(n * row)
                    .flat_map(|rows| &rows[k * row..(k + 1) * row])
                    .copied()
                    .collect();
                Image::from_vec(Shape::new(width, height, 3), data).expect("a layer's size")
            })
            .collect()
    }
}

/// What a pixel is in every layer of the section: its centre, how much of
/// it is tissue, the section's thickness there, and its stroma.
#[derive(Clone, Copy, Debug)]
struct Shared {
    x: f64,
    y: f64,
    tissue: f64,
    thickness: f64,
    stroma: f64,
}

/// Where a pixel's nuclei and holes come from.
trait Near {
    /// The nucleus at grid cell `(i, j)`, if there is one, with its turn.
    fn nucleus(&self, i: i64, j: i64) -> Option<(Nucleus, Turn)>;
    /// The hole at hole cell `(i, j)`, if there is one, with its turn.
    fn hole(&self, i: i64, j: i64) -> Option<(Nucleus, Turn)>;
}

/// Each found as it is asked for.
struct Direct<'a>(&'a HeTissue);

impl Near for Direct<'_> {
    fn nucleus(&self, i: i64, j: i64) -> Option<(Nucleus, Turn)> {
        self.0.nucleus(i, j).map(|n| (n, Turn::of(n.angle)))
    }

    fn hole(&self, i: i64, j: i64) -> Option<(Nucleus, Turn)> {
        self.0.hole(i, j).map(|h| (h, Turn::of(h.angle)))
    }
}

/// A grid's cells from `(i0, j0)`, `columns` by `rows`, row by row.
struct Cells {
    i0: i64,
    j0: i64,
    columns: i64,
    rows: i64,
    cells: Vec<Option<(Nucleus, Turn)>>,
}

impl Cells {
    /// No cells: every cell is outside the grid.
    fn none() -> Self {
        Self {
            i0: 0,
            j0: 0,
            columns: 0,
            rows: 0,
            cells: Vec::new(),
        }
    }

    /// Cells `i0..=i1` by `j0..=j1`, each from `find`.
    fn new(
        (i0, i1): (i64, i64),
        (j0, j1): (i64, i64),
        find: impl Fn(i64, i64) -> Option<Nucleus>,
    ) -> Self {
        let cells = (j0..=j1)
            .flat_map(|j| (i0..=i1).map(move |i| (i, j)))
            .map(|(i, j)| find(i, j).map(|n| (n, Turn::of(n.angle))))
            .collect();
        Self {
            i0,
            j0,
            columns: i1 - i0 + 1,
            rows: j1 - j0 + 1,
            cells,
        }
    }

    /// Cell `(i, j)`'s, or `None` outside the grid.
    fn get(&self, i: i64, j: i64) -> Option<Option<(Nucleus, Turn)>> {
        let (di, dj) = (i - self.i0, j - self.j0);
        let inside = (0..self.columns).contains(&di) && (0..self.rows).contains(&dj);
        inside.then(|| self.cells[(dj * self.columns + di) as usize])
    }
}

/// The nuclei and holes around a block of pixels, each found once for all
/// of them: a pixel otherwise finds the nine about it, with their hashes,
/// noise and turns, again for each of its own. Outside the block, each is
/// found as it is asked for, so its pixels are as [`Direct`] makes them, to
/// the bit.
struct Found<'a> {
    tissue: &'a HeTissue,
    nuclei: Cells,
    holes: Cells,
}

impl<'a> Found<'a> {
    /// Those of the `width` × `height` pixels of side `pixel` from
    /// `origin`: the cells their centres fall in, and those around.
    ///
    /// Only while the grid has fewer cells than the nine its pixels would
    /// each look up: a pixel much larger than a cell, as far from focus,
    /// would make a grid of millions for a few hundred pixels. Then each
    /// pixel finds its own, as [`Direct`] does.
    fn around(tissue: &'a HeTissue, origin: Point2, pixel: f64, width: u32, height: u32) -> Self {
        // A cell more each way than the centres need, against rounding.
        let span = |from: f64, count: u32, cell: f64| {
            let first = from + pixel / 2.0;
            let last = from + f64::from(count.max(1) - 1) * pixel + pixel / 2.0;
            (
                (first / cell).floor() as i64 - 2,
                (last / cell).floor() as i64 + 2,
            )
        };
        let lookups = 9 * i64::from(width) * i64::from(height);
        let grid = |cell: f64, find: &dyn Fn(i64, i64) -> Option<Nucleus>| {
            let (across, down) = (span(origin.x, width, cell), span(origin.y, height, cell));
            let cells = (across.1 - across.0 + 1) * (down.1 - down.0 + 1);
            match cells <= lookups {
                true => Cells::new(across, down, find),
                false => Cells::none(),
            }
        };
        Self {
            tissue,
            nuclei: grid(HeTissue::CELL, &|i, j| tissue.nucleus(i, j)),
            holes: grid(HeTissue::HOLE_CELL, &|i, j| tissue.hole(i, j)),
        }
    }
}

impl Near for Found<'_> {
    fn nucleus(&self, i: i64, j: i64) -> Option<(Nucleus, Turn)> {
        self.nuclei
            .get(i, j)
            .unwrap_or_else(|| Direct(self.tissue).nucleus(i, j))
    }

    fn hole(&self, i: i64, j: i64) -> Option<(Nucleus, Turn)> {
        self.holes
            .get(i, j)
            .unwrap_or_else(|| Direct(self.tissue).hole(i, j))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Layer `layer`'s block as `render` makes it, with the nuclei and holes
    /// found once, and pixel by pixel as each finds its own: to the bit.
    fn same_as_direct(tissue: &HeTissue, layer: usize, origin: Point2, pixel: f64, side: u32) {
        let image = tissue.render(layer, origin, Um(pixel), side, side);
        let direct = render_pixels(origin, Um(pixel), side, side, |x0, y0| {
            tissue.density(layer, x0, y0, pixel, &Direct(tissue))
        });
        let bits = |image: &Image| image.data().iter().map(|v| v.to_bits()).collect::<Vec<_>>();
        assert_eq!(
            bits(&image),
            bits(&direct),
            "layer {layer} at {origin:?}, {pixel} µm"
        );
    }

    #[test]
    fn the_nuclei_and_holes_found_once_render_as_those_found_by_each_pixel() {
        for tissue in [
            HeTissue::new(1, crate::Slide::COVERSLIP),
            HeTissue::ihc(7, crate::Slide::COVERSLIP),
        ] {
            let island = tissue.islands[0];
            let (rx, _) = island.radii;
            let centre = island.centre;
            // Inside, among nuclei, at 10×'s pixel; over the island's rough
            // edge; and coarse, over holes and edges at a 2 mm field's pixel.
            let edge = Point2::new(
                centre.x + rx * island.turn.cos,
                centre.y + rx * island.turn.sin,
            );
            let cases = [
                (centre, 0.24, 96),
                (edge, 0.24, 96),
                (
                    Point2::new(centre.x - 3_000.0, centre.y - 3_000.0),
                    40.0,
                    160,
                ),
                // Half a millimetre a pixel, as 15 mm from focus: each pixel
                // finds its own.
                (Point2::new(-1_000.0, -1_000.0), 550.0, 26),
            ];
            for (at, pixel, side) in cases {
                for layer in 0..tissue.layers.len() {
                    same_as_direct(&tissue, layer, at, pixel, side);
                }
                same_together(&tissue, at, pixel, side);
            }
        }
    }

    #[test]
    fn a_grid_is_found_only_where_it_is_fewer_cells_than_its_pixels_look_up() {
        // At the 10×'s pixel, a 283-pixel tile and its margin make grids of
        // a few hundred cells; at half a millimetre a pixel, 26 pixels would
        // make one of over a million nuclei's cells, for 6,084 look-ups.
        let tissue = HeTissue::new(1, crate::Slide::COVERSLIP);
        let at = tissue.islands[0].centre;
        let fine = Found::around(&tissue, at, 0.24, 283, 283);
        assert!(!fine.nuclei.cells.is_empty() && !fine.holes.cells.is_empty());
        let coarse = Found::around(&tissue, Point2::new(-1_000.0, -1_000.0), 550.0, 26, 26);
        assert!(
            coarse.nuclei.cells.is_empty(),
            "{} cells",
            coarse.nuclei.cells.len()
        );
        assert!(coarse.holes.cells.len() <= 9 * 26 * 26);
    }

    /// The layers rendered together, with what they share worked out once
    /// a pixel, as each is alone: to the bit, whichever are asked for, and
    /// in whatever order, a layer the tissue lacks as zeros.
    fn same_together(tissue: &HeTissue, origin: Point2, pixel: f64, side: u32) {
        let bits = |image: &Image| image.data().iter().map(|v| v.to_bits()).collect::<Vec<_>>();
        for indices in [vec![0, 1], vec![1, 0], vec![1], vec![0, 5, 1]] {
            let together = tissue.render_layers(&indices, origin, Um(pixel), side, side);
            for (layer, image) in indices.iter().zip(&together) {
                let alone = tissue.render(*layer, origin, Um(pixel), side, side);
                assert_eq!(
                    bits(image),
                    bits(&alone),
                    "layer {layer} of {indices:?} at {origin:?}, {pixel} µm"
                );
            }
        }
    }
}
