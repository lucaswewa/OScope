//! Acquisition: the frame bus, the preview, live statistics, the recorder, the
//! live mosaic, and the shared-memory publisher.
//!
//! Layer L4 of ADR-0005. So far it holds the [`FrameBus`] (P03);
//! [`Acquisition`], which completes a camera's frames and publishes them on
//! it (P16); and the [`preview`] (P17).

mod acquisition;
mod bus;
pub mod preview;

pub use acquisition::Acquisition;
pub use bus::{FrameBus, LatestReceiver, SubscriberStats, WhenFull};
