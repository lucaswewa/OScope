//! The HAL's errors. They follow `oscope-core`'s conventions.

use oscope_core::Ms;

use crate::HalFuture;

/// What can go wrong in a device, or in the objective catalogue.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// Something the device doesn't offer, such as a trigger on the
    /// MIChrome 20.
    #[error("the device doesn't support {what}")]
    Unsupported {
        /// What was asked for.
        what: String,
    },
    /// A value outside what the device accepts.
    #[error("{what} must be from {min} to {max}, and was {value}")]
    OutOfRange {
        /// What the value is.
        what: String,
        /// The value given.
        value: f64,
        /// The least it may be.
        min: f64,
        /// The most it may be.
        max: f64,
    },
    /// A name the device doesn't know: a mode, a light, an objective.
    #[error("there is no {kind} named {name:?}")]
    Unknown {
        /// What was named.
        kind: &'static str,
        /// The name given.
        name: String,
    },
    /// Something the device can't do now, such as change its frame size
    /// while it streams.
    #[error("the device is busy: {reason}")]
    Busy {
        /// Why.
        reason: String,
    },
    /// A move that a stop ended before it reached its target.
    #[error("the move was stopped before it reached its target")]
    Stopped,
    /// The stage is in alarm, and won't move until the alarm is cleared.
    #[error("the stage is in alarm: {reason}")]
    Alarm {
        /// The controller's reason.
        reason: String,
    },
    /// The device isn't connected, or stopped answering.
    #[error("the device isn't connected")]
    Disconnected,
    /// The device didn't answer in time.
    #[error("the device didn't answer within {after}")]
    Timeout {
        /// How long it was given.
        after: Ms,
    },
    /// Any other failure, in the device's own words.
    #[error("the device reported an error: {message}")]
    Device {
        /// What the device said.
        message: String,
    },
    /// An objective catalogue that breaks one of its rules.
    #[error("the objective catalogue is invalid: {reason}")]
    InvalidCatalogue {
        /// The rule it breaks.
        reason: String,
    },
    /// A catalogue file that isn't TOML, or has the wrong fields.
    #[error("the objective catalogue couldn't be read")]
    CatalogueFile(#[source] toml::de::Error),
}

/// A result with this crate's [`Error`].
pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    /// [`Error::OutOfRange`], unless `min <= value <= max`.
    pub(crate) fn check_range(what: &str, value: f64, min: f64, max: f64) -> Result<()> {
        if (min..=max).contains(&value) {
            Ok(())
        } else {
            Err(Error::OutOfRange {
                what: what.to_owned(),
                value,
                min,
                max,
            })
        }
    }
}

/// An operation that fails with [`Error::Unsupported`]: what a trait's
/// optional operations do unless a driver offers them.
pub(crate) fn unsupported<'a, T: Send + 'a>(what: &str) -> HalFuture<'a, T> {
    let what = what.to_owned();
    Box::pin(async move { Err(Error::Unsupported { what }) })
}
