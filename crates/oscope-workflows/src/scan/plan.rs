//! The plan model: tiles at stage positions, in order, with how each one's
//! z is found and the planes taken there, and the field that spaces them.

use oscope_core::{Point2, Rect, Size2};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::focus_map::{FocusMap, Surface};
use super::{ScanError, invalid};
use crate::calibration::{Matrix, apply, inverse};

/// The most overlap a field takes: neighbours sharing nine tenths.
pub const MOST_OVERLAP: f64 = 0.9;

/// The most planes a stack takes.
pub const MOST_PLANES: usize = 10_000;

/// What one frame covers on the stage, and how far apart a grid's tiles
/// are for the overlap asked.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Field {
    /// The frame's corners about its centre, on the stage, in µm: from the
    /// image's top left, round by its top right.
    pub corners_um: [Point2; 4],
    /// The cell: the largest rectangle, square with the stage and of the
    /// frame's proportions (or, the camera turned near a right angle, the
    /// turned frame's), that the frame holds about its centre. A grid tiles
    /// cells, so a region its cells cover, its frames cover.
    pub cell_um: Size2,
    /// The share of a cell's width, and of its height, that its neighbours
    /// along x, and along y, overlap.
    pub overlap: f64,
    /// The grid's steps along x and y, in µm: the cell, less the overlap.
    pub step_um: Size2,
}

impl Field {
    /// The field of frames `width` × `height` pixels that `matrix` (the
    /// calibration's M, in µm a pixel: ADR-0023) takes to the stage, its
    /// neighbours overlapping by `overlap`, from 0 to [`MOST_OVERLAP`].
    pub fn new(matrix: Matrix, width: u32, height: u32, overlap: f64) -> Result<Self, ScanError> {
        let n = inverse(matrix).filter(|_| matrix.iter().flatten().all(|v| v.is_finite()));
        let Some(n) = n.filter(|_| width > 0 && height > 0) else {
            return Err(invalid(format!(
                "a frame of {width} × {height} pixels through {matrix:?}: at least a pixel, and a \
                 matrix that can be inverted, are needed"
            )));
        };
        if !(0.0..=MOST_OVERLAP).contains(&overlap) {
            return Err(invalid(format!(
                "an overlap of {overlap}: from 0 to {MOST_OVERLAP} is needed"
            )));
        }
        let (w, h) = (f64::from(width), f64::from(height));
        let corner = |u: f64, v: f64| {
            let (x, y) = apply(matrix, (u * w / 2.0, v * h / 2.0));
            Point2::new(x, y)
        };
        // A cell of `(cw, ch)` pixels' proportions, scaled by k: its corners,
        // taken back through N = M⁻¹, must stay within the frame, so that
        // |N₀₀| a + |N₀₁| b ≤ w / 2 and |N₁₀| a + |N₁₁| b ≤ h / 2, for half
        // sides a = k cw / 2 and b = k ch / 2 in µm.
        let cell = |(cw, ch): (f64, f64)| {
            let k = (w / (n[0][0].abs() * cw + n[0][1].abs() * ch))
                .min(h / (n[1][0].abs() * cw + n[1][1].abs() * ch));
            Size2::new(k * cw, k * ch)
        };
        let (upright, turned) = (cell((w, h)), cell((h, w)));
        let cell_um = match upright.width * upright.height >= turned.width * turned.height {
            true => upright,
            false => turned,
        };
        let keep = 1.0 - overlap;
        Ok(Self {
            corners_um: [
                corner(-1.0, -1.0),
                corner(1.0, -1.0),
                corner(1.0, 1.0),
                corner(-1.0, 1.0),
            ],
            cell_um,
            overlap,
            step_um: Size2::new(cell_um.width * keep, cell_um.height * keep),
        })
    }

    /// A field with square pixels of `pixel_um` at the sample, square with
    /// the stage: the objective's nominal one, before a calibration.
    pub fn nominal(
        pixel_um: f64,
        width: u32,
        height: u32,
        overlap: f64,
    ) -> Result<Self, ScanError> {
        Self::new([[pixel_um, 0.0], [0.0, pixel_um]], width, height, overlap)
    }

    /// The cell of a tile at `centre`.
    pub fn cell_at(&self, centre: Point2) -> Rect {
        let Size2 { width, height } = self.cell_um;
        Rect::new(
            centre.x - width / 2.0,
            centre.y - height / 2.0,
            width,
            height,
        )
    }

    /// The frame of a tile at `centre`, by its corners on the stage.
    pub fn frame_at(&self, centre: Point2) -> [Point2; 4] {
        self.corners_um.map(|c| c + centre)
    }
}

/// A stack of planes through each tile's z.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Stack {
    /// How far below the tile's z it starts, in µm.
    pub below_um: f64,
    /// How far above the tile's z it may end.
    pub above_um: f64,
    /// The step between planes.
    pub step_um: f64,
}

impl Stack {
    /// The planes' offsets from the tile's z, from the lowest, as they are
    /// taken: moving up, as autofocus approaches (ADR-0024). The last is the
    /// highest that doesn't pass `above_um`.
    pub fn planes(&self) -> Vec<f64> {
        let count = ((self.below_um + self.above_um) / self.step_um + 1e-9).floor() as usize + 1;
        (0..count)
            .map(|i| -self.below_um + i as f64 * self.step_um)
            .collect()
    }

    fn check(&self) -> Result<(), ScanError> {
        let (b, a, s) = (self.below_um, self.above_um, self.step_um);
        let planes = (b + a) / s + 1.0;
        if !(b >= 0.0 && a >= 0.0 && s > 0.0 && planes <= MOST_PLANES as f64) {
            return Err(invalid(format!(
                "a stack from {b} µm below to {a} µm above, every {s} µm: none negative, a step \
                 more than 0, and at most {MOST_PLANES} planes, are needed"
            )));
        }
        Ok(())
    }
}

/// How each tile's z is found.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ZPolicy {
    /// Every tile at one z.
    Fixed {
        /// The z, in µm.
        z_um: f64,
    },
    /// Each tile at a focus map's z at its centre.
    Map {
        /// The map.
        map: FocusMap,
    },
    /// By autofocus, as the scan reaches every `every`-th tile from the
    /// first; the tiles between at the last z found (P36).
    Autofocus {
        /// How many tiles each autofocus serves, at least 1.
        every: u32,
    },
    /// By a focus map the scan measures before its first tile: autofocus
    /// at the tiles nearest the middles of a grid over them, then `surface`
    /// fitted (P37). Each tile then takes the map's z.
    Measure {
        /// The grid's columns, from 1 to 10.
        columns: u32,
        /// Its rows, from 1 to 10.
        rows: u32,
        /// The surface fitted.
        surface: Surface,
    },
}

impl ZPolicy {
    /// The z of a tile at `at`, if it is known before the scan.
    pub fn z_at(&self, at: Point2) -> Option<f64> {
        match self {
            ZPolicy::Fixed { z_um } => Some(*z_um),
            ZPolicy::Map { map } => Some(map.z_at(at)),
            ZPolicy::Autofocus { .. } | ZPolicy::Measure { .. } => None,
        }
    }

    pub(super) fn check(&self) -> Result<(), ScanError> {
        match self {
            ZPolicy::Fixed { z_um } if !z_um.is_finite() => Err(invalid(format!("a z of {z_um}"))),
            ZPolicy::Autofocus { every: 0 } => Err(invalid("an autofocus every 0 tiles")),
            ZPolicy::Measure { columns, rows, .. }
                if !((1..=10).contains(columns) && (1..=10).contains(rows)) =>
            {
                Err(invalid(format!(
                    "a map over a grid of {columns} × {rows}: from 1 to 10 each way"
                )))
            }
            _ => Ok(()),
        }
    }
}

/// How the stage moves through a plan (ADR-0025).
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Motion {
    /// It stops at each tile, settles, and takes its frames: M1's.
    #[default]
    StopAndGo,
    /// Reserved for M3 (D14): each row swept at a constant speed, frames
    /// triggered at the tiles' positions. The engine refuses it until then.
    Continuous {
        /// The sweep's speed, in µm/s.
        speed_um_s: f64,
    },
}

/// The pattern a plan follows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Pattern {
    /// Rows from the least y, each from its least x.
    Raster,
    /// Rows from the least y, each the other way from the last.
    Snake,
    /// One position, a stack through it.
    ZScan,
    /// A snake, a stack at each tile.
    TiledStack,
    /// Positions, in the order given.
    Positions,
    /// Positions an extension's workflow planned (ADR-0025).
    Workflow,
}

/// A position to visit, as a multi-position plan is given it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Position {
    /// Its x, in µm.
    pub x_um: f64,
    /// Its y.
    pub y_um: f64,
    /// Its own z, if it has one: where its focus was set by hand.
    #[serde(default)]
    pub z_um: Option<f64>,
    /// Its name, if it has one.
    #[serde(default)]
    pub name: Option<String>,
}

/// One tile: where one frame, or one stack, is taken.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Tile {
    /// Its place in the order, from 0.
    pub index: u32,
    /// Its row in the grid, from the least y; 0 for a position.
    pub row: u32,
    /// Its column in the grid, from the least x; a position's place in its
    /// list.
    pub column: u32,
    /// The stage's x at its centre, in µm.
    pub x_um: f64,
    /// The stage's y at its centre.
    pub y_um: f64,
    /// The stage's z, if it is known before the scan: fixed, from the map,
    /// or the position's own.
    pub z_um: Option<f64>,
    /// A position's name.
    pub name: Option<String>,
}

/// A scan, planned: its tiles in the order they are taken.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Plan {
    /// The pattern.
    pub pattern: Pattern,
    /// How the stage moves through it.
    #[serde(default)]
    pub motion: Motion,
    /// The field that spaced the tiles.
    pub field: Field,
    /// The grid's rows; 1 for positions.
    pub rows: u32,
    /// The grid's columns; the count of positions.
    pub columns: u32,
    /// How each tile's z is found.
    pub z: ZPolicy,
    /// The stack taken at each tile, or `None` for one plane.
    pub stack: Option<Stack>,
    /// The tiles, in order.
    pub tiles: Vec<Tile>,
}

impl Plan {
    /// The planes taken at each tile, as offsets from its z.
    pub fn planes(&self) -> Vec<f64> {
        self.stack.map_or_else(|| vec![0.0], |s| s.planes())
    }
}

pub(super) fn check_stack(stack: Option<&Stack>) -> Result<(), ScanError> {
    stack.map_or(Ok(()), Stack::check)
}
