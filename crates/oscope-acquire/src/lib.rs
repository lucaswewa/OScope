//! Acquisition metadata completion and bounded frame fan-out.
//!
//! Recording, preview and capture services are ported with their dependencies
//! in later migration phases.

pub mod acquisition;
pub mod bus;

pub use acquisition::Acquisition;
pub use bus::{FrameBus, LatestReceiver, SubscriberStats, WhenFull};
