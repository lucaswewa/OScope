//! OScope's logs (ADR-0009): `tracing` events become records in a
//! [`LogStore`], which keeps the newest in memory, writes daily files and
//! feeds the live tail. The `logs` Thing queries it, and the media plane
//! tails it.
//!
//! - **Levels** run from `trace` to `error`, then `critical`: an error
//!   written with `critical = true`.
//! - **A job's records** are those made in its span ([`JOB_SPAN`]). They
//!   carry its id, and [`LogStore::capture`] collects them.

mod files;
mod layer;
mod record;
mod store;
mod thing;

pub use files::{LogFile, list, prune};
pub use layer::{JOB_SPAN, LogLayer};
pub use record::{Level, Page, Query, Record};
pub use store::{Capture, DEFAULT_KEEP_DAYS, DEFAULT_RING, LogConfig, LogStore};
pub use thing::{EMIT_TARGET, Logs};

use std::io::IsTerminal;

use tracing_subscriber::EnvFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

/// The logs can't be installed.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum LogError {
    /// The filter isn't valid.
    #[error("the log level `{filter}`: {source}")]
    Filter {
        /// The filter.
        filter: String,
        /// What is wrong.
        source: tracing_subscriber::filter::ParseError,
    },
    /// The process already has a `tracing` subscriber.
    #[error("the logs are already installed")]
    Installed,
}

/// Sends the process's logs to `store`, and to the console (stderr). It
/// sets the global `tracing` subscriber, once. `filter` chooses what is
/// recorded, in `tracing`'s `EnvFilter` syntax, such as `info` or
/// `info,oscope_api=debug`.
pub fn install(store: &LogStore, filter: &str) -> Result<(), LogError> {
    let filter = EnvFilter::try_new(filter).map_err(|source| LogError::Filter {
        filter: filter.to_owned(),
        source,
    })?;
    let console = tracing_subscriber::fmt::layer()
        .with_writer(std::io::stderr)
        .with_ansi(std::io::stderr().is_terminal());
    tracing_subscriber::registry()
        .with(filter)
        .with(console)
        .with(store.layer())
        .try_init()
        .map_err(|_| LogError::Installed)
}
