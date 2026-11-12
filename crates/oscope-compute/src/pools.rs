//! Two rayon pools, so that batch jobs can't hold up live work such as the
//! preview.

use rayon::{ThreadPool, ThreadPoolBuilder};
use serde::{Deserialize, Serialize};

use crate::ComputeError;

/// Which pool work runs in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Lane {
    /// Work that can't wait: the preview, live statistics.
    Live,
    /// Jobs: processing, stitching, analysis.
    Batch,
}

impl Lane {
    /// The prefix of its threads' names: `oscope-live-3`.
    pub fn thread_prefix(self) -> &'static str {
        match self {
            Self::Live => "oscope-live",
            Self::Batch => "oscope-batch",
        }
    }

    /// The lane of the thread this runs on, by its name, if it is one of
    /// the pools'.
    pub fn current() -> Option<Self> {
        let thread = std::thread::current();
        let name = thread.name()?;
        [Self::Live, Self::Batch]
            .into_iter()
            .find(|lane| name.starts_with(&format!("{}-", lane.thread_prefix())))
    }
}

/// The live and batch pools. Each has threads of its own and a queue of its
/// own, so live work never waits behind batch work.
#[derive(Debug)]
pub struct Pools {
    live: ThreadPool,
    batch: ThreadPool,
}

impl Pools {
    /// Pools of `live` and `batch` threads, at least one each.
    pub fn new(live: usize, batch: usize) -> Result<Self, ComputeError> {
        let pool = |lane: Lane, threads: usize| {
            ThreadPoolBuilder::new()
                .num_threads(threads.max(1))
                .thread_name(move |i| format!("{}-{i}", lane.thread_prefix()))
                .build()
                .map_err(|error| ComputeError::Pool(error.to_string()))
        };
        Ok(Self {
            live: pool(Lane::Live, live)?,
            batch: pool(Lane::Batch, batch)?,
        })
    }

    /// A lane's pool.
    pub fn pool(&self, lane: Lane) -> &ThreadPool {
        match lane {
            Lane::Live => &self.live,
            Lane::Batch => &self.batch,
        }
    }

    /// Runs `work` in a lane's pool, and waits for it. Rayon's parallel
    /// iterators inside it use that pool.
    pub fn install<R: Send>(&self, lane: Lane, work: impl FnOnce() -> R + Send) -> R {
        self.pool(lane).install(work)
    }

    /// Starts `work` in a lane's pool, without waiting.
    pub fn spawn(&self, lane: Lane, work: impl FnOnce() + Send + 'static) {
        self.pool(lane).spawn(work);
    }
}
