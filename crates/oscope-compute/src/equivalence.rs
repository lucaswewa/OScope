//! Comparing two outputs within a tolerance, such as a CUDA kernel's and its
//! CPU twin's, and saying where they differ most.

use std::fmt;

use crate::{ComputeError, Image, Shape};

/// How close a sample must be: `|actual − expected| ≤ abs + rel × |expected|`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tolerance {
    /// The absolute part.
    pub abs: f64,
    /// The part relative to the expected value.
    pub rel: f64,
}

impl Tolerance {
    /// Equal samples only.
    pub const EXACT: Self = Self { abs: 0.0, rel: 0.0 };

    /// Within `abs` of the expected value.
    pub fn abs(abs: f64) -> Self {
        Self { abs, rel: 0.0 }
    }

    /// Within `rel` times the expected value.
    pub fn rel(rel: f64) -> Self {
        Self { abs: 0.0, rel }
    }

    /// Whether `error` is allowed. An error of 0 always is, even where
    /// `expected` is NaN.
    fn allows(self, expected: f64, error: f64) -> bool {
        error == 0.0 || error <= self.abs + self.rel * expected.abs()
    }
}

/// Where a sample is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Position {
    /// Its column.
    pub x: u32,
    /// Its row.
    pub y: u32,
    /// Its channel.
    pub channel: usize,
}

/// What a comparison found.
#[derive(Debug, Clone, PartialEq)]
pub struct Comparison {
    /// The tolerance it used.
    pub tolerance: Tolerance,
    /// How many samples it compared.
    pub compared: usize,
    /// How many were outside the tolerance.
    pub failed: usize,
    /// The largest error, `|actual − expected|`: infinite where one is NaN
    /// and the other isn't.
    pub max_error: f64,
    /// Where the largest error is, with the two values; `None` if every
    /// sample is equal.
    pub worst: Option<(Position, f64, f64)>,
}

impl Comparison {
    /// Whether every sample is within the tolerance.
    pub fn passed(&self) -> bool {
        self.failed == 0
    }
}

impl fmt::Display for Comparison {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Tolerance { abs, rel } = self.tolerance;
        write!(
            f,
            "{} of {} samples outside {abs:e} + {rel:e} × |expected|",
            self.failed, self.compared
        )?;
        match self.worst {
            Some((at, expected, actual)) => write!(
                f,
                "; the largest error is {:e}, at x {}, y {}, channel {}: expected {expected}, got {actual}",
                self.max_error, at.x, at.y, at.channel
            ),
            None => write!(f, "; every sample is equal"),
        }
    }
}

/// Compares `actual` with `expected`, two buffers of `shape`'s samples.
pub fn compare<T: Copy + Into<f64>>(
    expected: &[T],
    actual: &[T],
    shape: Shape,
    tolerance: Tolerance,
) -> Result<Comparison, ComputeError> {
    for len in [expected.len(), actual.len()] {
        if len != shape.len() {
            return Err(ComputeError::Size {
                expected: shape.len(),
                got: len,
            });
        }
    }
    let mut comparison = Comparison {
        tolerance,
        compared: shape.len(),
        failed: 0,
        max_error: 0.0,
        worst: None,
    };
    for (i, (&e, &a)) in expected.iter().zip(actual).enumerate() {
        let (e, a): (f64, f64) = (e.into(), a.into());
        let error = match (e.is_nan(), a.is_nan()) {
            (true, true) => 0.0,
            (false, false) => (a - e).abs(),
            _ => f64::INFINITY,
        };
        if !tolerance.allows(e, error) {
            comparison.failed += 1;
        }
        if error > comparison.max_error {
            comparison.max_error = error;
            let pixel = i / shape.channels;
            let at = Position {
                x: (pixel % shape.width as usize) as u32,
                y: (pixel / shape.width as usize) as u32,
                channel: i % shape.channels,
            };
            comparison.worst = Some((at, e, a));
        }
    }
    Ok(comparison)
}

/// Compares two images, which must have the same shape.
pub fn compare_images(
    expected: &Image,
    actual: &Image,
    tolerance: Tolerance,
) -> Result<Comparison, ComputeError> {
    if expected.shape() != actual.shape() {
        return Err(ComputeError::Shape {
            expected: expected.shape(),
            got: actual.shape(),
        });
    }
    compare(expected.data(), actual.data(), expected.shape(), tolerance)
}

/// Panics, with the comparison, unless two images are equivalent: for tests.
#[track_caller]
pub fn assert_equivalent(expected: &Image, actual: &Image, tolerance: Tolerance) {
    match compare_images(expected, actual, tolerance) {
        Ok(comparison) if comparison.passed() => {}
        Ok(comparison) => panic!("not equivalent: {comparison}"),
        Err(error) => panic!("not comparable: {error}"),
    }
}
