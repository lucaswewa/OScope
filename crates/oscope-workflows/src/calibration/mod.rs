//! Calibration I (P32; ADR-0023): how the camera sits over the stage, and
//! how the stage moves.
//!
//! - **Sets** ([`CalibrationSet`]): an objective's geometry at a binning,
//!   and the stage's, versioned, one active per key, in the
//!   [`CalibrationStore`].
//! - **The camera-to-stage matrix** ([`fit_matrix`], [`decompose`]): fitted
//!   to stage moves and the image shifts they make, and decomposed into a
//!   turn, a mirror, two scales and a skew.
//! - **The procedures** ([`measure_geometry`], [`measure_stage`],
//!   [`check_objective`]) and their jobs ([`calibration_jobs`]).
//!
//! P32's notes hold the behaviour spec, with its sources.

mod fit;
mod jobs;
mod photometry;
mod procedures;
mod set;
mod store;

pub use fit::{Decomposition, Matrix, apply, decompose, fit_matrix, inverse};
pub use jobs::{CHECK, COLOUR, DARK, FLAT, GEOMETRY, LastCheck, STAGE, calibration_jobs};
pub use photometry::{
    ColourOptions, DarkOptions, FlatOptions, Frames, measure_colour, measure_dark, measure_flat,
};
pub use procedures::{
    CalibrationError, CheckReport, GeometryOptions, Motion, Quiet, Report, Rig, StageOptions,
    check_objective, measure_geometry, measure_stage,
};
pub use set::{
    CalibrationSet, Colour, Dark, FORMAT, Flat, Geometry, Instrument, PerAxis, Region, STAGE_KEY,
    SetSummary, SoftLimits, StageValues, Values, colour_key, dark_key, exposure_octave, flat_key,
    geometry_key,
};
pub use store::{CalibrationStore, StoreError};
