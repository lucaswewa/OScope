//! The hardware abstraction: device traits for the camera, the stage, the
//! lights and the objective mount, each with a report of its capabilities
//! (plan §5.9, ADR-0012).
//!
//! Layer L2 of ADR-0005. Drivers implement the traits: the simulator
//! (`oscope-sim`) now, the real drivers in M3, and extension drivers (P27).
//!
//! - [`camera`]: the [`Camera`] trait, with its modes, settings and
//!   capabilities.
//! - [`stage`]: the [`Stage`] trait, with its status, position samples and
//!   capabilities.
//! - [`track`]: the [`PositionTrack`], which says where the stage was at any
//!   moment.
//! - [`illumination`]: the [`Illumination`] trait, and
//!   [`ManualIllumination`].
//! - [`objective`]: the [`ObjectiveCatalogue`], the [`ObjectiveMount`] trait,
//!   and [`ManualObjectiveMount`].
//! - [`error`]: this crate's [`Error`].
//!
//! **The traits are async, and dyn-compatible.** Every operation that talks
//! to a device returns a [`HalFuture`], a boxed future, so that a driver
//! chosen at run time works behind `Arc<dyn Camera>`. A driver writes
//! `Box::pin(async move { … })`.
//!
//! **Each device reports its capabilities,** which don't change while it is
//! open. The capability types also check settings against them, so that
//! every driver rejects the same things with the same errors.

use std::future::Future;
use std::pin::Pin;

pub mod camera;
pub mod error;
pub mod illumination;
pub mod objective;
pub mod stage;
pub mod track;

pub use camera::{
    Camera, CameraCapabilities, CameraMode, CameraSettings, Edge, FrameSink, ModeRate, RoiRules,
    Shutter, Trigger, TriggerCapabilities,
};
pub use error::{Error, Result};
pub use illumination::{
    Illumination, IlluminationCapabilities, LightChannel, LightKind, ManualIllumination,
};
pub use objective::{
    ManualObjectiveMount, Objective, ObjectiveCatalogue, ObjectiveMount, ObjectiveMountCapabilities,
};
pub use stage::{Axis, PositionSample, Stage, StageCapabilities, StageStatus, Travel, UmPerS};
pub use track::PositionTrack;

/// What a device operation returns: a boxed future of its [`Result`], which
/// keeps the traits dyn-compatible.
pub type HalFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T>> + Send + 'a>>;
