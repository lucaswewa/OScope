//! Acquisition: the frame bus, the preview, live statistics, the recorder, the
//! live mosaic, and the shared-memory publisher.
//!
//! Layer L4 of ADR-0005. So far it holds the [`FrameBus`] (P03);
//! [`Acquisition`], which completes a camera's frames and publishes them on
//! it (P16); the [`preview`] (P17); live [`stats`] (P18); and the
//! [`Recorder`], with the job that ingests its recordings into projects
//! ([`ingest_jobs`]) (P29).

mod acquisition;
mod bus;
mod ingest;
pub mod preview;
mod recorder;
pub mod stats;

pub use acquisition::Acquisition;
pub use bus::{FrameBus, LatestReceiver, SubscriberStats, WhenFull};
pub use ingest::{INGEST, ingest_jobs};
pub use recorder::{QUEUE, Recorder, RecorderStats};
