//! The HAL conformance suite: checks that a driver does what the traits
//! promise (plan §5.9, ADR-0012). The simulator runs it now, the real
//! drivers in M3, and extension drivers in P27.
//!
//! - **One async function for each trait:** [`camera`], [`stage`],
//!   [`illumination`] and [`objective_mount`]. Each exercises the device,
//!   and returns the first check it failed as a [`Failure`].
//! - **The devices are changed:** the camera's settings, the lights, the
//!   objective fitted, the stage's position. Each suite puts them back as it
//!   found them when every check passes.
//! - **Refusals are checked by kind.** Drivers check settings with their
//!   capabilities' own checks (ADR-0012), so a driver must refuse what they
//!   refuse, with the same [`Error`] variant.
//! - **The camera's suite streams.** It waits for each frame its interval
//!   and exposure plus a generous allowance, so a driver that never
//!   delivers fails rather than hangs. It needs a Tokio runtime with its
//!   timer.

use std::fmt::Debug;

use crate::Error;

mod camera;
mod devices;

pub use camera::camera;
pub use devices::{illumination, objective_mount, stage};

/// A check that a device failed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{check}: {detail}")]
pub struct Failure {
    /// The check, such as `configure every mode and format`.
    pub check: &'static str,
    /// What went wrong.
    pub detail: String,
}

/// Fails `check` with `detail`, unless `ok`.
fn ensure(ok: bool, check: &'static str, detail: impl FnOnce() -> String) -> Result<(), Failure> {
    if ok {
        Ok(())
    } else {
        Err(Failure {
            check,
            detail: detail(),
        })
    }
}

/// The result of an operation that should succeed.
fn expect<T>(result: crate::Result<T>, check: &'static str) -> Result<T, Failure> {
    result.map_err(|error| Failure {
        check,
        detail: format!("it failed: {error}"),
    })
}

/// Checks that an operation was refused, with an error that `right` accepts.
fn refused<T: Debug>(
    result: crate::Result<T>,
    check: &'static str,
    right: fn(&Error) -> bool,
) -> Result<(), Failure> {
    let detail = match result {
        Err(error) if right(&error) => return Ok(()),
        Err(error) => format!("it was refused with the wrong error: {error}"),
        Ok(value) => format!("it was accepted: {value:?}"),
    };
    Err(Failure { check, detail })
}
