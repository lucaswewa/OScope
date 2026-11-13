//! A culture of cells in 3D, labelled in three colours: the ground truth
//! for confocal-style stacks and volume rendering (D8, D23).

use std::collections::HashMap;
use std::f64::consts::PI;

use oscope_compute::Image;
use oscope_core::{Point2, Rect, Um};

use crate::fluorescence::Dye;
use crate::noise::{hash, nth, unit};
use crate::sample::{Layer, LayerKind, Sample, render_pixels};

/// The dyes, in the order of the truth's channels: nuclei, membranes and
/// organelles.
pub const DYES: [Dye; 3] = [Dye::Dapi, Dye::Green, Dye::Orange];

/// The density of each dye where it is, in amount a micrometre
/// (`LayerKind::Fluorescent`).
pub const DENSITY: [f64; 3] = [0.06, 0.1, 0.1];

/// The grid the cells are placed on: one at most in each square, never
/// reaching out of it.
const PITCH: f64 = 32.0;
/// The membrane's thickness.
const MEMBRANE: f64 = 0.3;
/// The places tried for organelles in each cell; those that fall outside
/// the cytoplasm are dropped.
const TRIES: u64 = 120;
const SALT_PLACE: u64 = 11;
const SALT_SHAPE: u64 = 12;
const SALT_ORGANELLES: u64 = 13;

/// An ellipsoid, with its axes along its cell's.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ellipsoid {
    /// Its centre: x and y on the slide, z above it.
    pub centre: [f64; 3],
    /// Its half-lengths along the cell's long and short axes, and in z.
    pub radii: [f64; 3],
}

/// A small, round organelle.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Organelle {
    /// Its centre: x and y on the slide, z above it.
    pub centre: [f64; 3],
    /// Its radius.
    pub radius: f64,
}

/// One cell, spread on the glass as a half-ellipsoid.
#[derive(Clone, Debug, PartialEq)]
pub struct Cell {
    /// Its centre on the slide, on the glass.
    pub centre: Point2,
    /// Its half-lengths along its long and short axes, and its height.
    pub radii: [f64; 3],
    /// The angle of its long axis from x, in radians.
    pub angle: f64,
    /// Its nucleus.
    pub nucleus: Ellipsoid,
    /// Its organelles, in the cytoplasm.
    pub organelles: Vec<Organelle>,
}

impl Cell {
    /// `(x, y)` on the slide, along the cell's axes from its centre.
    fn local(&self, x: f64, y: f64, from: [f64; 2]) -> (f64, f64) {
        let (dx, dy) = (x - from[0], y - from[1]);
        let (sin, cos) = self.angle.sin_cos();
        (dx * cos + dy * sin, -dx * sin + dy * cos)
    }

    /// The membrane's thickness for pixels of side `p`: its own, or as wide
    /// as a pixel, so that it is never missed between pixel centres.
    fn membrane(&self, p: f64) -> f64 {
        MEMBRANE.max(p).min(0.5 * self.radii[1].min(self.radii[2]))
    }

    /// The span in z of the cell's dome, and of the space inside its
    /// membrane `t` thick, above `(u, v)` along its axes.
    fn domes(&self, u: f64, v: f64, t: f64) -> [Option<(f64, f64)>; 2] {
        let [a, b, c] = self.radii;
        let dome = |a: f64, b: f64, c: f64, base: f64| {
            let rho = (u / a).powi(2) + (v / b).powi(2);
            (rho < 1.0).then(|| (base, base + c * (1.0 - rho).sqrt()))
        };
        [dome(a, b, c, 0.0), dome(a - t, b - t, c - 2.0 * t, t)]
    }

    /// The span in z of its nucleus above `(x, y)`.
    fn nucleus_span(&self, x: f64, y: f64) -> Option<(f64, f64)> {
        let n = self.nucleus;
        let (u, v) = self.local(x, y, [n.centre[0], n.centre[1]]);
        let rho = (u / n.radii[0]).powi(2) + (v / n.radii[1]).powi(2);
        let half = n.radii[2] * (1.0 - rho).sqrt();
        (rho < 1.0).then(|| (n.centre[2] - half, n.centre[2] + half))
    }

    /// The volume of the membrane's skin if it were `t` thick, over 2π / 3:
    /// the dome's, less the space inside.
    fn skin(&self, t: f64) -> f64 {
        let [a, b, c] = self.radii;
        a * b * c - (a - t) * (b - t) * (c - 2.0 * t)
    }

    /// The amount of dye `d` of the `DYES` above `(x, y)`, from `z0` to
    /// `z1`, for pixels of side `p`. Exact, but for the membrane and the
    /// organelles at pixels larger than they are: the membrane widened to a
    /// pixel, and each organelle to a pixel across, with their density
    /// lowered to keep their amount of dye.
    fn column(&self, d: usize, x: f64, y: f64, z0: f64, z1: f64, p: f64) -> f64 {
        let overlap =
            |span: Option<(f64, f64)>| span.map_or(0.0, |(a, b)| (b.min(z1) - a.max(z0)).max(0.0));
        match d {
            0 => overlap(self.nucleus_span(x, y)) * DENSITY[0],
            1 => {
                let t = self.membrane(p);
                let (u, v) = self.local(x, y, [self.centre.x, self.centre.y]);
                let [outer, inner] = self.domes(u, v, t);
                (overlap(outer) - overlap(inner)) * DENSITY[1] * self.skin(MEMBRANE) / self.skin(t)
            }
            _ => self
                .organelles
                .iter()
                .map(|o| {
                    let wide = o.radius.max(p);
                    let d2 = (x - o.centre[0]).powi(2) + (y - o.centre[1]).powi(2);
                    if d2 >= wide * wide {
                        return 0.0;
                    }
                    let half = o.radius * (1.0 - d2 / (wide * wide)).sqrt();
                    let span = Some((o.centre[2] - half, o.centre[2] + half));
                    overlap(span) * DENSITY[2] * (o.radius / wide).powi(2)
                })
                .sum(),
        }
    }

    /// The density of each dye at `(x, y, z)`: the truth.
    fn density(&self, x: f64, y: f64, z: f64) -> [f64; 3] {
        let within = |span: Option<(f64, f64)>| span.is_some_and(|(a, b)| a <= z && z < b);
        let (u, v) = self.local(x, y, [self.centre.x, self.centre.y]);
        let [outer, inner] = self.domes(u, v, MEMBRANE);
        let organelle = self.organelles.iter().any(|o| {
            let d = [x - o.centre[0], y - o.centre[1], z - o.centre[2]];
            d.iter().map(|v| v * v).sum::<f64>() < o.radius * o.radius
        });
        [
            if within(self.nucleus_span(x, y)) {
                DENSITY[0]
            } else {
                0.0
            },
            if within(outer) && !within(inner) {
                DENSITY[1]
            } else {
                0.0
            },
            if organelle { DENSITY[2] } else { 0.0 },
        ]
    }
}

/// Cells grown on the slide within `bounds`, 25 to 40 µm apart, labelled in
/// three colours:
///
/// - **nuclei** in [`Dye::Dapi`], ellipsoids in the lower half of the cell;
/// - **membranes** in [`Dye::Green`], a 0.3 µm skin over each cell and
///   under it;
/// - **organelles** in [`Dye::Orange`], spheres 0.5 to 1 µm across, in the
///   cytoplasm.
///
/// Each cell is a half-ellipsoid on the glass, 16 to 22 µm long and 3.5 to
/// 5.5 µm high, at a random angle, and seven squares in ten of its grid hold
/// one. Everything is drawn from hashes of the seed (ADR-0013).
///
/// **It renders as slices** [`CellCulture::SLICE`] thick, a layer for each
/// dye in each, holding each column's exact amount of the dye in the slice.
/// [`CellCulture::truth`] gives the densities themselves, on any grid.
#[derive(Clone, Debug)]
pub struct CellCulture {
    seed: u64,
    bounds: Rect,
    layers: Vec<Layer>,
}

impl CellCulture {
    /// The slices' thickness.
    pub const SLICE: Um = Um(0.5);
    /// The top of the highest slice, above every cell.
    pub const TOP: Um = Um(6.5);

    /// Cells from `seed` within `bounds` on the slide.
    pub fn new(seed: u64, bounds: Rect) -> Self {
        let slices = (Self::TOP.0 / Self::SLICE.0).round() as usize;
        let layers = (0..slices)
            .flat_map(|k| {
                let z = (k as f64 + 0.5) * Self::SLICE.0;
                DYES.map(|dye| Layer {
                    name: format!("{dye:?} at {z:.2} µm"),
                    z: Um(z),
                    thickness: Self::SLICE,
                    kind: LayerKind::Fluorescent(dye),
                })
            })
            .collect();
        Self {
            seed,
            bounds,
            layers,
        }
    }

    /// The grid square holding `(x, y)`.
    fn square(&self, x: f64, y: f64) -> (i64, i64) {
        (
            ((x - self.bounds.origin.x) / PITCH).floor() as i64,
            ((y - self.bounds.origin.y) / PITCH).floor() as i64,
        )
    }

    /// The cell in grid square `(i, j)`, if it has one.
    fn cell(&self, i: i64, j: i64) -> Option<Cell> {
        if unit(hash(self.seed, SALT_PLACE, i, j)) > 0.7 {
            return None;
        }
        let h = hash(self.seed, SALT_SHAPE, i, j);
        let b = self.bounds;
        let centre = Point2::new(
            b.origin.x + (i as f64 + 0.5 + (nth(h, 0) - 0.5) / 4.0) * PITCH,
            b.origin.y + (j as f64 + 0.5 + (nth(h, 1) - 0.5) / 4.0) * PITCH,
        );
        let room = PITCH / 2.0;
        let inside = centre.x - room >= b.origin.x
            && centre.y - room >= b.origin.y
            && centre.x + room <= b.origin.x + b.size.width
            && centre.y + room <= b.origin.y + b.size.height;
        if !inside {
            return None;
        }
        let a = 8.0 + 3.0 * nth(h, 2);
        let radii = [a, a * (0.65 + 0.3 * nth(h, 3)), 3.5 + 2.0 * nth(h, 4)];
        let angle = PI * nth(h, 5);
        let [a, b, c] = radii;
        let (sin, cos) = angle.sin_cos();
        let to_slide =
            |u: f64, v: f64| [centre.x + u * cos - v * sin, centre.y + u * sin + v * cos];

        let n = [
            0.42 * a * (0.9 + 0.2 * nth(h, 6)),
            0.42 * b * (0.9 + 0.2 * nth(h, 7)),
            0.3 * c,
        ];
        let (nu, nv) = ((nth(h, 8) - 0.5) * 0.3 * a, (nth(h, 9) - 0.5) * 0.3 * b);
        let [nx, ny] = to_slide(nu, nv);
        let nucleus = Ellipsoid {
            centre: [nx, ny, 0.15 * c + n[2]],
            radii: n,
        };

        // Organelles where they fit: inside the membrane, clear of the
        // nucleus.
        let o = hash(self.seed, SALT_ORGANELLES, i, j);
        let organelles = (0..TRIES)
            .filter_map(|k| {
                let r = 0.25 + 0.25 * nth(o, 4 * k + 3);
                let u = (2.0 * nth(o, 4 * k) - 1.0) * a;
                let v = (2.0 * nth(o, 4 * k + 1) - 1.0) * b;
                let z = nth(o, 4 * k + 2) * c;
                let room = MEMBRANE + r;
                let rho = (u / (a - room)).powi(2) + (v / (b - room)).powi(2);
                let top = room + (c - 2.0 * room) * (1.0 - rho).max(0.0).sqrt();
                let cytoplasm = rho < 1.0 && z >= room && z <= top;
                let clear = ((u - nu) / (n[0] + r)).powi(2)
                    + ((v - nv) / (n[1] + r)).powi(2)
                    + ((z - nucleus.centre[2]) / (n[2] + r)).powi(2)
                    > 1.0;
                let [x, y] = to_slide(u, v);
                (cytoplasm && clear).then_some(Organelle {
                    centre: [x, y, z],
                    radius: r,
                })
            })
            .collect();
        Some(Cell {
            centre,
            radii,
            angle,
            nucleus,
            organelles,
        })
    }

    /// The cells in the grid squares that `region` touches, by square.
    fn cells_by_square(&self, region: Rect) -> HashMap<(i64, i64), Cell> {
        let (i0, j0) = self.square(region.origin.x, region.origin.y);
        let (i1, j1) = self.square(
            region.origin.x + region.size.width,
            region.origin.y + region.size.height,
        );
        (j0..=j1)
            .flat_map(|j| (i0..=i1).map(move |i| (i, j)))
            .filter_map(|(i, j)| self.cell(i, j).map(|cell| ((i, j), cell)))
            .collect()
    }

    /// The cells whose centres lie in `region`: the ground truth.
    pub fn cells(&self, region: Rect) -> Vec<Cell> {
        let mut cells: Vec<Cell> = self
            .cells_by_square(region)
            .into_values()
            .filter(|c| region.contains(c.centre))
            .collect();
        cells.sort_by(|a, b| {
            (a.centre.y, a.centre.x)
                .partial_cmp(&(b.centre.y, b.centre.x))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        cells
    }

    /// The dyes' densities at height `z` over a grid of `width` × `height`
    /// pixels of side `pixel` from `origin`, at each pixel's centre: a plane
    /// of the ground-truth volume, with the [`DYES`] in its channels.
    pub fn truth(&self, origin: Point2, pixel: Um, width: u32, height: u32, z: Um) -> Image {
        let p = pixel.0;
        let region = Rect::new(
            origin.x,
            origin.y,
            f64::from(width) * p,
            f64::from(height) * p,
        );
        let cells = self.cells_by_square(region);
        render_pixels(origin, pixel, width, height, |x0, y0| {
            let (x, y) = (x0 + p / 2.0, y0 + p / 2.0);
            cells
                .get(&self.square(x, y))
                .map_or([0.0; 3], |cell| cell.density(x, y, z.0).map(|d| d as f32))
        })
    }
}

impl Sample for CellCulture {
    fn name(&self) -> &str {
        "fluorescent cell culture"
    }

    fn layers(&self) -> &[Layer] {
        &self.layers
    }

    fn bounds(&self) -> Rect {
        self.bounds
    }

    fn render(&self, index: usize, origin: Point2, pixel: Um, width: u32, height: u32) -> Image {
        let p = pixel.0;
        let (slice, d) = (index / DYES.len(), index % DYES.len());
        let z0 = slice as f64 * Self::SLICE.0;
        let z1 = z0 + Self::SLICE.0;
        let region = Rect::new(
            origin.x,
            origin.y,
            f64::from(width) * p,
            f64::from(height) * p,
        );
        let cells = if index < self.layers.len() {
            self.cells_by_square(region)
        } else {
            HashMap::new()
        };
        render_pixels(origin, pixel, width, height, |x0, y0| {
            let (x, y) = (x0 + p / 2.0, y0 + p / 2.0);
            let amount = cells
                .get(&self.square(x, y))
                .map_or(0.0, |cell| cell.column(d, x, y, z0, z1, p));
            [amount as f32; 3]
        })
    }
}
