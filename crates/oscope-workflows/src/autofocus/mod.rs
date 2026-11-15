//! Autofocus, auto exposure and auto white balance (P34; ADR-0024): the
//! procedures P34's behaviour spec sets out, and the hardware jobs that run
//! them.
//!
//! - [`autofocus`]: a coarse sweep through z, then a fine one, each position
//!   approached moving up; or one continuous sweep, its frames put at z by
//!   their exposure midpoints. The stage is left at the sharpest z.
//! - [`auto_exposure`]: the exposure scaled until the region's 99.5th
//!   percentile sits at the target.
//! - [`white_balance`]: the region's lightest cells taken as the light's
//!   white.

use std::sync::Arc;

use oscope_acquire::FrameBus;
use oscope_compute::Compute;
use oscope_hal::{Camera, Illumination, ObjectiveMount, Stage};

mod exposure;
mod frames;
mod jobs;
mod sweep;

pub use exposure::{
    ExposureOptions, ExposureReport, ExposureStep, WhiteBalanceOptions, WhiteBalanceReport,
    auto_exposure, white_balance,
};
pub use jobs::{
    AUTO_EXPOSURE, AUTOFOCUS, LastExposure, LastFocus, LastReports, WHITE_BALANCE, focus_jobs,
};
pub use sweep::{
    CurvePoint, FocusDefaults, FocusOptions, FocusReport, FrameMeasures, Measure, Sweep, autofocus,
    current_measures, depth_of_field, focus_defaults,
};

use crate::calibration::Report;

/// What autofocus and exposure drive: the stage, the camera and its
/// frames, the objective and the lights that set the defaults, and the
/// compute layer for the measures.
#[derive(Clone)]
pub struct FocusRig {
    /// The stage.
    pub stage: Arc<dyn Stage>,
    /// The camera, streaming.
    pub camera: Arc<dyn Camera>,
    /// The objective mount, for the depth of field.
    pub objectives: Arc<dyn ObjectiveMount>,
    /// The camera's frames.
    pub bus: Arc<FrameBus>,
    /// The lights, for the defaults.
    pub lights: Arc<dyn Illumination>,
    /// The compute layer, for the measures on the GPU when there is one; on
    /// the CPU without it.
    pub compute: Option<Arc<Compute>>,
}

/// Why a procedure didn't finish.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum AutofocusError {
    /// It was asked to stop.
    #[error("cancelled")]
    Cancelled,
    /// It failed, for this reason.
    #[error("{0}")]
    Failed(String),
}

fn failed(why: impl Into<String>) -> AutofocusError {
    AutofocusError::Failed(why.into())
}

pub(crate) fn device(error: oscope_hal::Error) -> AutofocusError {
    failed(error.to_string())
}

pub(crate) fn check(report: &dyn Report) -> Result<(), AutofocusError> {
    match report.cancelled() {
        true => Err(AutofocusError::Cancelled),
        false => Ok(()),
    }
}
