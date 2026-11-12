//! This crate's errors, and the conventions every OScope crate follows.
//!
//! - **One error type per crate:** an enum named `Error`, derived with
//!   [`thiserror`], beside `pub type Result<T> = std::result::Result<T, Error>`.
//!   It is `#[non_exhaustive]`, so variants can be added without breaking
//!   callers.
//! - **Messages say what went wrong** in a sentence a user can act on: lower
//!   case, no final full stop, as Rust's own errors are. For example: "a fit
//!   needs at least 3 point pairs, and got 2".
//! - **Variants carry the facts** (values, names, limits) as fields, so that
//!   callers can match on them rather than parse the message.
//! - **A wrapped error is the `#[source]`,** not part of the message, so that
//!   it isn't printed twice.
//! - **Library code doesn't panic** on bad input, a failed device or a broken
//!   file: it returns an error. A panic means a bug, a broken invariant.
//! - **`anyhow`** is for binaries, examples and tests only.

/// What can go wrong in `oscope-core`.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// An affine transform with no inverse.
    #[error("the transform can't be inverted: its determinant is {determinant}")]
    Singular {
        /// The determinant of its matrix.
        determinant: f64,
    },
    /// Too few point pairs to fit a transform.
    #[error("a fit needs at least {needed} point pairs, and got {got}")]
    TooFewPoints {
        /// How many the fit needs.
        needed: usize,
        /// How many it got.
        got: usize,
    },
    /// Points that lie on one line, which fix no affine transform.
    #[error("the points lie on one line, so they fix no affine transform")]
    Collinear,
    /// A buffer too small for the frame it should hold.
    #[error("the frame needs {needed} bytes, and its buffer has {got}")]
    BufferTooSmall {
        /// The bytes the frame needs.
        needed: usize,
        /// The bytes the buffer has.
        got: usize,
    },
    /// A rectangle that is empty, or not wholly inside a frame.
    #[error("{rect:?} isn't a non-empty part of the {width} × {height} frame")]
    OutsideFrame {
        /// The rectangle.
        rect: crate::geometry::PixelRect,
        /// The frame's width.
        width: u32,
        /// The frame's height.
        height: u32,
    },
    /// Text that isn't an ID of the expected kind.
    #[error("{text:?} isn't a {kind}")]
    InvalidId {
        /// The kind of ID, such as `JobId`.
        kind: &'static str,
        /// The text.
        text: String,
    },
    /// JSON that isn't a document of the expected schema.
    #[error("this isn't a {schema} document: {reason}")]
    NotADocument {
        /// The expected schema.
        schema: &'static str,
        /// What is wrong with it.
        reason: String,
    },
    /// A document whose fields don't match its type.
    #[error("the {schema} document doesn't match its type")]
    Mismatch {
        /// The schema.
        schema: &'static str,
        /// What serde found.
        #[source]
        source: serde_json::Error,
    },
    /// A document written by a newer OScope.
    #[error("the {schema} document is version {found}, newer than this build's {current}")]
    NewerVersion {
        /// The schema.
        schema: &'static str,
        /// The document's version.
        found: u32,
        /// The version this build writes.
        current: u32,
    },
    /// A document of an old version this build can't upgrade.
    #[error("the {schema} document is version {found}, which this build can't upgrade")]
    CantUpgrade {
        /// The schema.
        schema: &'static str,
        /// The document's version.
        found: u32,
    },
}

/// The result of an `oscope-core` operation.
pub type Result<T> = std::result::Result<T, Error>;
