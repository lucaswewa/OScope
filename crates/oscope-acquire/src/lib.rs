//! Acquisition: the frame bus, the preview, live statistics, the recorder, the
//! live mosaic, and the shared-memory publisher.
//!
//! Layer L4 of ADR-0005. So far it holds the [`FrameBus`] (P03).

mod bus;

pub use bus::{FrameBus, LatestReceiver, SubscriberStats, WhenFull};
