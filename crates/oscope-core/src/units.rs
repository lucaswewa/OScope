//! Lengths in micrometres and durations in milliseconds: the units at every
//! API (ADR-0007).
//!
//! Both are `f64` newtypes, so a length can't be passed for a duration, nor a
//! bare number for either. They serialise as plain numbers in their unit.

use std::fmt;
use std::ops::{Add, AddAssign, Div, Mul, Neg, Sub, SubAssign};
use std::time::Duration;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

macro_rules! unit {
    ($(#[$doc:meta])* $name:ident, $symbol:literal) => {
        $(#[$doc])*
        #[derive(Clone, Copy, Debug, Default, PartialEq, PartialOrd, Serialize, Deserialize, JsonSchema)]
        #[serde(transparent)]
        pub struct $name(pub f64);

        impl $name {
            /// Zero.
            pub const ZERO: Self = Self(0.0);

            /// The absolute value.
            pub fn abs(self) -> Self {
                Self(self.0.abs())
            }
        }

        /// The number, with its unit: `12.5 µm`. A precision applies to the number.
        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                fmt::Display::fmt(&self.0, f)?;
                f.write_str(concat!(" ", $symbol))
            }
        }

        impl Add for $name {
            type Output = Self;
            fn add(self, other: Self) -> Self {
                Self(self.0 + other.0)
            }
        }

        impl Sub for $name {
            type Output = Self;
            fn sub(self, other: Self) -> Self {
                Self(self.0 - other.0)
            }
        }

        impl AddAssign for $name {
            fn add_assign(&mut self, other: Self) {
                self.0 += other.0;
            }
        }

        impl SubAssign for $name {
            fn sub_assign(&mut self, other: Self) {
                self.0 -= other.0;
            }
        }

        impl Neg for $name {
            type Output = Self;
            fn neg(self) -> Self {
                Self(-self.0)
            }
        }

        impl Mul<f64> for $name {
            type Output = Self;
            fn mul(self, factor: f64) -> Self {
                Self(self.0 * factor)
            }
        }

        impl Mul<$name> for f64 {
            type Output = $name;
            fn mul(self, value: $name) -> $name {
                $name(self * value.0)
            }
        }

        impl Div<f64> for $name {
            type Output = Self;
            fn div(self, divisor: f64) -> Self {
                Self(self.0 / divisor)
            }
        }

        /// The ratio of two values in the same unit.
        impl Div for $name {
            type Output = f64;
            fn div(self, other: Self) -> f64 {
                self.0 / other.0
            }
        }
    };
}

unit!(
    /// A length in micrometres (µm): every position and distance.
    Um,
    "µm"
);

unit!(
    /// A duration in milliseconds (ms), such as an exposure time.
    Ms,
    "ms"
);

impl Um {
    /// A length in millimetres.
    pub fn from_mm(mm: f64) -> Self {
        Self(mm * 1e3)
    }

    /// A length in nanometres.
    pub fn from_nm(nm: f64) -> Self {
        Self(nm / 1e3)
    }

    /// This length in millimetres.
    pub fn mm(self) -> f64 {
        self.0 / 1e3
    }

    /// This length in nanometres.
    pub fn nm(self) -> f64 {
        self.0 * 1e3
    }
}

impl Ms {
    /// A duration in seconds.
    pub fn from_secs(secs: f64) -> Self {
        Self(secs * 1e3)
    }

    /// This duration in seconds.
    pub fn secs(self) -> f64 {
        self.0 / 1e3
    }

    /// This duration as a [`Duration`], or `None` if it is negative, not a
    /// number, or too long.
    pub fn to_duration(self) -> Option<Duration> {
        Duration::try_from_secs_f64(self.secs()).ok()
    }
}

impl From<Duration> for Ms {
    fn from(duration: Duration) -> Self {
        Self(duration.as_nanos() as f64 / 1e6)
    }
}
