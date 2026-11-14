//! OScope's shared vocabulary: units, geometry, the clock and timestamps, IDs,
//! the frame and metadata types, and errors. It does no I/O and nothing async.
//!
//! Layer L0 of ADR-0005. Its units, coordinates and clock follow ADR-0007.
//!
//! - [`units`]: lengths in micrometres ([`Um`]) and durations in milliseconds ([`Ms`]).
//! - [`clock`]: the [`Clock`] and its [`Timestamp`]s.
//! - [`geometry`]: points, sizes, rectangles, stage positions, and [`Affine2`].
//! - [`ids`]: [`FrameId`], [`JobId`], [`ProjectId`], [`AcquisitionId`], [`ResultId`],
//!   [`ReportId`] and [`RecordingId`].
//! - [`error`]: this crate's [`Error`], and the error conventions of every crate.
//! - [`version`]: documents that carry their schema's version.
//! - [`frame`]: frames in pooled memory, their metadata, and views of them.

pub mod clock;
pub mod error;
pub mod frame;
pub mod geometry;
pub mod ids;
pub mod units;
pub mod version;

pub use clock::{Clock, FakeClock, SystemClock, Timestamp};
pub use error::{Error, Result};
pub use frame::{
    BayerColour, BayerPattern, Frame, FrameBuffer, FrameMeta, FramePool, FrameView, HeapAllocator,
    LightState, PixelFormat, PoolStats, Slab, SlabAllocator,
};
pub use geometry::{Affine2, PixelRect, Point2, Rect, Size2, Xyz};
pub use ids::{AcquisitionId, FrameId, JobId, ProjectId, RecordingId, ReportId, ResultId};
pub use units::{Ms, Um};
pub use version::{Versioned, from_document, to_document};
