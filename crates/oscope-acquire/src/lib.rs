//! Acquisition metadata, bounded frame fan-out, scratch recording and ingest.
//!
//! Preview and capture services are ported with microscope workflows later.

pub mod acquisition;
pub mod bus;
pub mod ingest;
pub mod recorder;

pub use acquisition::Acquisition;
pub use bus::{FrameBus, LatestReceiver, SubscriberStats, WhenFull};
pub use ingest::{INGEST, ingest_jobs};
pub use recorder::{QUEUE, Recorder, RecorderStats, Take};
