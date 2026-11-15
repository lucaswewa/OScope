//! Scan planning (P35; plan §5.14): every scan pattern, planned and
//! previewable before the stage moves.
//!
//! - **Where:** a [`Region`], a rectangle, a polygon or a mask, in stage µm.
//! - **The field:** what one frame covers, from the calibration's geometry
//!   ([`Field::new`]), and how far apart tiles are for the overlap asked.
//! - **The planners** lay tiles out in order: [`raster`], [`snake`],
//!   [`tiled_stack`], [`z_scan`] and [`positions`]. Each gives a [`Plan`],
//!   whose tiles take their z as its [`ZPolicy`] says.
//! - **Focus maps:** focus measured at [`sample_points`] by autofocus
//!   ([`measure_map`]), then a plane or a thin-plate spline fitted to it,
//!   outliers rejected ([`FocusMap::fit`]).
//! - **Estimates** of a plan's time and data size ([`estimate()`]).
//! - **The engine** runs a plan as a hardware job, stop and go, and ingests
//!   its frames as one acquisition ([`scan_jobs`]; P36, ADR-0025).
//!
//! Plans, maps and estimates serialise as JSON: a plan's preview.

mod engine;
mod estimate;
mod focus_map;
mod measure;
mod plan;
mod planners;
mod region;

pub use engine::{LastScan, SCAN, ScanOptions, ScanProgress, ScanRig, TileDone, scan_jobs};
pub use estimate::{Estimate, FrameSize, Timing, estimate};
pub use focus_map::{FitOptions, FocusMap, FocusPoint, Surface, sample_points};
pub use measure::{MapSample, measure_map};
pub use plan::{
    Field, MOST_OVERLAP, MOST_PLANES, Motion, Pattern, Plan, Position, Stack, Tile, ZPolicy,
};
pub use planners::{MOST_TILES, positions, raster, snake, tiled_stack, z_scan};
pub use region::{Mask, Region};

/// Why a plan, a region or a fit can't be.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ScanError {
    /// It can't be, for this reason.
    #[error("{0}")]
    Invalid(String),
}

fn invalid(why: impl Into<String>) -> ScanError {
    ScanError::Invalid(why.into())
}
