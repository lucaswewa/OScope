//! Structures finer than the optics resolve, the super-resolution sample
//! (D23): filaments and hollow tubules, with their fine-scale ground truth.

use std::collections::HashMap;
use std::f64::consts::{PI, TAU};

use oscope_compute::Image;
use oscope_core::{Point2, Rect, Um};

use crate::fluorescence::Dye;
use crate::noise::{hash, nth};
use crate::sample::{Layer, LayerKind, Sample, render_pixels};

/// The grid the structures start on.
const PITCH: f64 = 20.0;
/// Filaments started in each grid square.
const FILAMENTS: u64 = 3;
/// A filament's radius: 25 nm across, as a microtubule is.
const FILAMENT_RADIUS: f64 = 0.0125;
/// A filament's step, and the spread of its turn at each step: a
/// persistence length of about 45 µm.
const STEP: f64 = 0.5;
const TURN: f64 = 0.15;
/// The furthest a filament reaches from its start: 80 steps.
const REACH: f64 = 80.0 * STEP;
/// Tubules started in each grid square.
const TUBULES: u64 = 6;
/// A tubule's wall, as a mitochondrion's outer membrane is.
const WALL: f64 = 0.03;
/// The density of each structure's dye, in amount a micrometre
/// (`LayerKind::Fluorescent`).
const DENSITY: [f64; 2] = [50.0, 1.0];
/// The squares of the lookup grid for rendering.
const CELL: f64 = 2.0;
const SALT_FILAMENT: u64 = 21;
const SALT_TUBULE: u64 = 31;

/// A filament: a line through `points` on the slide, `radius` round.
#[derive(Clone, Debug, PartialEq)]
pub struct Filament {
    /// Its path.
    pub points: Vec<Point2>,
    /// Its radius.
    pub radius: f64,
}

/// A hollow tubule along `points`, with round ends.
#[derive(Clone, Debug, PartialEq)]
pub struct Tubule {
    /// Its axis.
    pub points: Vec<Point2>,
    /// Its outer radius.
    pub outer: f64,
    /// Its inner radius: the outer, less the wall.
    pub inner: f64,
}

/// One straight piece of a structure, for rendering.
#[derive(Clone, Copy, Debug)]
struct Piece {
    from: Point2,
    to: Point2,
    /// The structure it belongs to.
    id: usize,
    /// Whether it starts the structure, and whether it ends it.
    first: bool,
    last: bool,
}

/// The distance from `p` to the segment from `a` to `b`, and where along it
/// `p` falls: 0 at `a`, 1 at `b`, beyond them outside 0 to 1.
fn distance(p: Point2, a: Point2, b: Point2) -> (f64, f64) {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let length2 = dx * dx + dy * dy;
    let along = if length2 > 0.0 {
        ((p.x - a.x) * dx + (p.y - a.y) * dy) / length2
    } else {
        0.0
    };
    let t = along.clamp(0.0, 1.0);
    ((p.x - a.x - t * dx).hypot(p.y - a.y - t * dy), along)
}

/// The path length through a round rod of `radius`, `d` from its axis,
/// seen from above.
fn chord(radius: f64, d: f64) -> f64 {
    2.0 * (radius * radius - d * d).max(0.0).sqrt()
}

/// Filaments and tubules on the slide within `bounds`, labelled in two
/// colours, in a thin layer on the glass:
///
/// - **filaments** in [`Dye::Green`], 25 nm across and 20 to 40 µm long,
///   bending slowly, three started in every 20 µm square;
/// - **tubules** in [`Dye::Orange`], hollow, 0.24 to 0.32 µm across with a
///   30 nm wall, 1 to 4 µm long and gently curved, as mitochondria's outer
///   membranes are; six in every square.
///
/// Both are far finer than the optics resolve: a filament is blurred to the
/// point spread function's width, and a tubule's hollow can't be seen.
/// [`Structures::truth`] gives the true amounts at any pixel, however fine.
///
/// **At pixels wider than a structure,** its profile becomes a tent a pixel
/// wide either side, and each of its round ends a cone a pixel in radius,
/// holding the same amount of dye, so that a pixel's centre never misses
/// it.
#[derive(Clone, Debug)]
pub struct Structures {
    seed: u64,
    bounds: Rect,
    layers: Vec<Layer>,
}

impl Structures {
    /// The height of the structures' layer above the slide.
    pub const HEIGHT: Um = Um(0.1);

    /// Structures from `seed` within `bounds` on the slide.
    pub fn new(seed: u64, bounds: Rect) -> Self {
        let layer = |name: &str, dye| Layer {
            name: name.to_owned(),
            z: Self::HEIGHT,
            thickness: Um(0.2),
            kind: LayerKind::Fluorescent(dye),
        };
        Self {
            seed,
            bounds,
            layers: vec![
                layer("filaments", Dye::Green),
                layer("tubules", Dye::Orange),
            ],
        }
    }

    /// The corner of grid square `(i, j)`.
    fn corner(&self, i: i64, j: i64) -> Point2 {
        Point2::new(
            self.bounds.origin.x + i as f64 * PITCH,
            self.bounds.origin.y + j as f64 * PITCH,
        )
    }

    /// The filaments started in grid square `(i, j)`.
    fn filaments_from(&self, i: i64, j: i64) -> Vec<Filament> {
        let corner = self.corner(i, j);
        (0..FILAMENTS)
            .map(|k| {
                let h = hash(self.seed, SALT_FILAMENT + k, i, j);
                let mut at =
                    Point2::new(corner.x + nth(h, 0) * PITCH, corner.y + nth(h, 1) * PITCH);
                let mut angle = TAU * nth(h, 2);
                let steps = 40 + (nth(h, 3) * 40.0) as u64;
                let mut points = vec![at];
                for s in 0..steps {
                    angle += (nth(h, 10 + 2 * s) + nth(h, 11 + 2 * s) - 1.0) * TURN;
                    at = Point2::new(at.x + STEP * angle.cos(), at.y + STEP * angle.sin());
                    points.push(at);
                }
                Filament {
                    points,
                    radius: FILAMENT_RADIUS,
                }
            })
            .collect()
    }

    /// The tubules started in grid square `(i, j)`.
    fn tubules_from(&self, i: i64, j: i64) -> Vec<Tubule> {
        let corner = self.corner(i, j);
        (0..TUBULES)
            .map(|k| {
                let h = hash(self.seed, SALT_TUBULE + k, i, j);
                let mut at =
                    Point2::new(corner.x + nth(h, 0) * PITCH, corner.y + nth(h, 1) * PITCH);
                let mut angle = TAU * nth(h, 2);
                let (length, bend) = (1.0 + 3.0 * nth(h, 3), (nth(h, 4) - 0.5) * 0.6);
                let mut points = vec![at];
                for _ in 0..3 {
                    at = Point2::new(
                        at.x + length / 3.0 * angle.cos(),
                        at.y + length / 3.0 * angle.sin(),
                    );
                    points.push(at);
                    angle += bend;
                }
                let outer = 0.12 + 0.04 * nth(h, 5);
                Tubule {
                    points,
                    outer,
                    inner: outer - WALL,
                }
            })
            .collect()
    }

    /// The grid squares whose structures can reach `region`.
    fn squares(&self, region: Rect, reach: f64) -> impl Iterator<Item = (i64, i64)> {
        let o = self.bounds.origin;
        let index = |v: f64, from: f64| ((v - from) / PITCH).floor() as i64;
        let (i0, j0) = (
            index(region.origin.x - reach, o.x),
            index(region.origin.y - reach, o.y),
        );
        let (i1, j1) = (
            index(region.origin.x + region.size.width + reach, o.x),
            index(region.origin.y + region.size.height + reach, o.y),
        );
        (j0..=j1).flat_map(move |j| (i0..=i1).map(move |i| (i, j)))
    }

    /// The filaments that can reach `region`: the ground truth.
    pub fn filaments(&self, region: Rect) -> Vec<Filament> {
        self.squares(region, REACH)
            .flat_map(|(i, j)| self.filaments_from(i, j))
            .collect()
    }

    /// The tubules that can reach `region`: the ground truth.
    pub fn tubules(&self, region: Rect) -> Vec<Tubule> {
        self.squares(region, 5.0)
            .flat_map(|(i, j)| self.tubules_from(i, j))
            .collect()
    }

    /// The amounts of structure `kind` (0 for filaments, 1 for tubules)
    /// over a grid of `width` × `height` pixels of side `pixel` from
    /// `origin`, at each pixel's centre: exactly, or, if `widen`, with
    /// structures narrower than a pixel widened to keep their amount.
    fn amounts(
        &self,
        kind: usize,
        origin: Point2,
        pixel: Um,
        width: u32,
        height: u32,
        widen: bool,
    ) -> Image {
        let p = pixel.0;
        let region = Rect::new(
            origin.x,
            origin.y,
            f64::from(width) * p,
            f64::from(height) * p,
        );
        // Each structure's radii, and its pieces, filed by the lookup
        // squares they come near.
        let (radii, pieces): (Vec<(f64, f64)>, Vec<Vec<Point2>>) = if kind == 0 {
            self.filaments(region)
                .into_iter()
                .map(|f| ((f.radius, 0.0), f.points))
                .unzip()
        } else {
            self.tubules(region)
                .into_iter()
                .map(|t| ((t.outer, t.inner), t.points))
                .unzip()
        };
        let mut lookup: HashMap<(i64, i64), Vec<Piece>> = HashMap::new();
        let cell = |v: f64| (v / CELL).floor() as i64;
        for (id, points) in pieces.iter().enumerate() {
            let near = radii[id].0.max(p);
            let pairs = points.len() - 1;
            for (k, pair) in points.windows(2).enumerate() {
                let piece = Piece {
                    from: pair[0],
                    to: pair[1],
                    id,
                    first: k == 0,
                    last: k + 1 == pairs,
                };
                let (x0, x1) = (
                    pair[0].x.min(pair[1].x) - near,
                    pair[0].x.max(pair[1].x) + near,
                );
                let (y0, y1) = (
                    pair[0].y.min(pair[1].y) - near,
                    pair[0].y.max(pair[1].y) + near,
                );
                for cy in cell(y0)..=cell(y1) {
                    for cx in cell(x0)..=cell(x1) {
                        lookup.entry((cx, cy)).or_default().push(piece);
                    }
                }
            }
        }
        let bounds = self.bounds;
        render_pixels(origin, pixel, width, height, |x0, y0| {
            let at = Point2::new(x0 + p / 2.0, y0 + p / 2.0);
            if !bounds.contains(at) {
                return [0.0; 3];
            }
            let Some(near) = lookup.get(&(cell(at.x), cell(at.y))) else {
                return [0.0; 3];
            };
            // Each structure counts once, at its nearest piece. Widened, its
            // body stops at its two ends.
            let widened = |id: usize| widen && radii[id].0 < p;
            let mut nearest: Vec<(usize, f64)> = Vec::new();
            for piece in near {
                let (d, along) = distance(at, piece.from, piece.to);
                let beyond = (piece.first && along < 0.0) || (piece.last && along > 1.0);
                let d = if beyond && widened(piece.id) {
                    f64::INFINITY
                } else {
                    d
                };
                match nearest.iter_mut().find(|(id, _)| *id == piece.id) {
                    Some(entry) => entry.1 = entry.1.min(d),
                    None => nearest.push((piece.id, d)),
                }
            }
            let amount: f64 = nearest
                .iter()
                .map(|(id, d)| {
                    let (outer, inner) = radii[*id];
                    let path = if widened(*id) {
                        // The body: a tent of half-width p across it, with
                        // its cross-section's area. Each end: a cone of
                        // radius p, with its round cap's volume. Summed
                        // over pixels, both are exact on average, at any
                        // angle.
                        let area = PI * (outer * outer - inner * inner);
                        let cap = 2.0 / 3.0 * PI * (outer.powi(3) - inner.powi(3));
                        let body = area * (1.0 - d / p).max(0.0) / p;
                        let points = &pieces[*id];
                        let caps: f64 = [points[0], points[points.len() - 1]]
                            .iter()
                            .map(|end| {
                                let r = (at.x - end.x).hypot(at.y - end.y);
                                cap * 3.0 / (PI * p * p) * (1.0 - r / p).max(0.0)
                            })
                            .sum();
                        body + caps
                    } else {
                        chord(outer, *d) - chord(inner, *d)
                    };
                    path * DENSITY[kind]
                })
                .sum();
            [amount as f32; 3]
        })
    }

    /// The true amounts over a grid of `width` × `height` pixels of side
    /// `pixel` from `origin`, at each pixel's centre: filaments in red,
    /// tubules in green. At pixels of a few nanometres it shows each
    /// filament's 25 nm and each tubule's hollow.
    pub fn truth(&self, origin: Point2, pixel: Um, width: u32, height: u32) -> Image {
        let filaments = self.amounts(0, origin, pixel, width, height, false);
        let mut truth = self.amounts(1, origin, pixel, width, height, false);
        for (t, f) in truth
            .data_mut()
            .chunks_mut(3)
            .zip(filaments.data().chunks(3))
        {
            t[1] = t[0];
            t[0] = f[0];
            t[2] = 0.0;
        }
        truth
    }
}

impl Sample for Structures {
    fn name(&self) -> &str {
        "sub-diffraction structures"
    }

    fn layers(&self) -> &[Layer] {
        &self.layers
    }

    fn bounds(&self) -> Rect {
        self.bounds
    }

    fn render(&self, index: usize, origin: Point2, pixel: Um, width: u32, height: u32) -> Image {
        if index >= self.layers.len() {
            return render_pixels(origin, pixel, width, height, |_, _| [0.0; 3]);
        }
        self.amounts(index, origin, pixel, width, height, true)
    }
}
