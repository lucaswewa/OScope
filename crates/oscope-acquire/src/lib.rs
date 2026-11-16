//! Acquisition: the frame bus, the preview, live statistics, the recorder, the
//! live mosaic, and the shared-memory publisher.
//!
//! Layer L4 of ADR-0005. So far it holds the [`FrameBus`] (P03);
//! [`Acquisition`], which completes a camera's frames and publishes them on
//! it (P16); the [`preview`] (P17); live [`stats`] (P18); and the
//! [`Recorder`], with the job that ingests its recordings into projects
//! ([`ingest_jobs`]) (P29); and captures, single and bursts
//! ([`capture_jobs`]) (P30).

mod acquisition;
mod bus;
mod capture;
mod ingest;
pub mod mosaic;
pub mod preview;
mod recorder;
pub mod stats;

pub use acquisition::Acquisition;
pub use bus::{FrameBus, LatestReceiver, SubscriberStats, WhenFull};
pub use capture::{BURST, BURST_MAX, Rig, SINGLE, capture_jobs};
pub use ingest::{INGEST, ingest_jobs};
pub use mosaic::{Gates, LiveMosaic, MosaicCounts, MosaicError, MosaicSetup, Verdict};
pub use recorder::{QUEUE, Recorder, RecorderStats, Take};
