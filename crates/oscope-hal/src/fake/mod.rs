//! Scripted fakes of every HAL trait, for tests and examples (P09b).
//!
//! - [`FakeCamera`] streams frames of a test pattern from the pool it is
//!   given, at its mode's rate, or one for each software trigger.
//! - [`FakeStage`] reaches each target at once, and reports it on its track.
//! - [`FakeIllumination`] and [`FakeObjectiveMount`] behave as the manual
//!   implementations do.
//!
//! **Each fake has a [`Script`],** which records the operations called on
//! it, and makes chosen ones fail. Tests use it to see what a caller did,
//! and to check what the caller does when a device fails. Each operation is
//! named after its trait method, such as `configure` or `move_to`.

use std::sync::{Mutex, MutexGuard, PoisonError};

mod camera;
mod lights;
mod stage;

pub use camera::FakeCamera;
pub use lights::{FakeIllumination, FakeObjectiveMount};
pub use stage::FakeStage;

use crate::{Error, Result};

/// What a fake has been asked to do, and the failures it is to give.
#[derive(Debug, Default)]
pub struct Script {
    inner: Mutex<Inner>,
}

#[derive(Debug, Default)]
struct Inner {
    calls: Vec<&'static str>,
    failures: Vec<(&'static str, Error)>,
}

impl Script {
    fn inner(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Makes the next call of `operation` fail with `error`. Several
    /// failures of one operation are given in the order they were added.
    pub fn fail_next(&self, operation: &'static str, error: Error) {
        self.inner().failures.push((operation, error));
    }

    /// The operations called so far, in order.
    pub fn calls(&self) -> Vec<&'static str> {
        self.inner().calls.clone()
    }

    /// Records a call of `operation`, and gives its scripted failure, if
    /// there is one.
    pub(crate) fn call(&self, operation: &'static str) -> Result<()> {
        let mut inner = self.inner();
        inner.calls.push(operation);
        match inner.failures.iter().position(|(op, _)| *op == operation) {
            Some(i) => Err(inner.failures.remove(i).1),
            None => Ok(()),
        }
    }
}
