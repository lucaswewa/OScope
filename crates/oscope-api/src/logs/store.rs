//! The log store: the newest records in memory, the daily files, the live
//! tail, and jobs' captures.

use std::collections::VecDeque;
use std::io;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, Weak};

use chrono::Utc;
use serde_json::{Map, Value};
use tokio::sync::broadcast;

use super::files::{self, DailyFiles, LogFile};
use super::{Level, Page, Query, Record};

/// How many of the newest records the store keeps in memory by default.
pub const DEFAULT_RING: usize = 50_000;

/// How many days of files the store keeps by default, today's included.
pub const DEFAULT_KEEP_DAYS: u32 = 30;

/// How many records the live tail buffers for a slow follower before it
/// skips some.
const TAIL_BUFFER: usize = 1024;

/// What a [`LogStore`] keeps.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogConfig {
    /// How many of the newest records to keep in memory, for queries and
    /// the tail.
    pub ring: usize,
    /// The folder for the daily files; none keeps records in memory only.
    pub folder: Option<PathBuf>,
    /// How many days of files to keep, today's included.
    pub keep_days: u32,
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            ring: DEFAULT_RING,
            folder: None,
            keep_days: DEFAULT_KEEP_DAYS,
        }
    }
}

/// Where records go: a ring of the newest, the daily files, the live tail
/// and jobs' captures. Clones share one store.
#[derive(Clone)]
pub struct LogStore {
    inner: Arc<Inner>,
}

struct Inner {
    ring: usize,
    state: Mutex<State>,
    tail: broadcast::Sender<Arc<Record>>,
}

struct State {
    seq: u64,
    records: VecDeque<Arc<Record>>,
    files: Option<DailyFiles>,
    captures: Vec<(String, Weak<Captured>)>,
}

/// A capture's records.
type Captured = Mutex<Vec<Arc<Record>>>;

impl std::fmt::Debug for LogStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LogStore")
            .field("ring", &self.inner.ring)
            .finish_non_exhaustive()
    }
}

impl Default for LogStore {
    /// A store in memory only, of the default size.
    fn default() -> Self {
        Self::new(&LogConfig::default()).expect("no folder, so nothing to fail")
    }
}

/// Locks, carrying on after a panic elsewhere: a record half pushed is
/// still a whole record or none.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

impl LogStore {
    /// A store as `config` says. With a folder, it creates it and deletes
    /// files older than `keep_days`.
    pub fn new(config: &LogConfig) -> io::Result<Self> {
        let today = Utc::now().date_naive();
        let files = match &config.folder {
            Some(folder) => Some(DailyFiles::new(folder, today, config.keep_days)?),
            None => None,
        };
        let state = State {
            seq: 0,
            records: VecDeque::new(),
            files,
            captures: Vec::new(),
        };
        Ok(Self {
            inner: Arc::new(Inner {
                ring: config.ring.max(1),
                state: Mutex::new(state),
                tail: broadcast::channel(TAIL_BUFFER).0,
            }),
        })
    }

    /// Adds a record, numbered and timed now.
    pub(crate) fn push(
        &self,
        level: Level,
        target: &str,
        message: String,
        fields: Map<String, Value>,
        job: Option<String>,
    ) {
        let mut state = lock(&self.inner.state);
        state.seq += 1;
        let record = Arc::new(Record {
            seq: state.seq,
            time: Utc::now(),
            level,
            target: target.to_owned(),
            message,
            fields,
            job,
        });
        if let Some(files) = &mut state.files
            && let Err(error) = files.write(&record)
        {
            // Not logged: that would come back here.
            eprintln!("oscope: the log files stop: {error}");
            state.files = None;
        }
        state
            .captures
            .retain(|(job, capture)| match capture.upgrade() {
                Some(capture) if record.job.as_deref() == Some(job) => {
                    lock(&capture).push(Arc::clone(&record));
                    true
                }
                Some(_) => true,
                None => false,
            });
        if state.records.len() == self.inner.ring {
            state.records.pop_front();
        }
        state.records.push_back(Arc::clone(&record));
        // Under the lock, so that followers get records in order.
        let _ = self.inner.tail.send(record);
    }

    /// A page of the newest records that match `query`, newest first.
    pub fn query(&self, query: &Query) -> Page {
        page(&lock(&self.inner.state), query)
    }

    /// For the live tail: the newest records that match `query`, oldest
    /// first, and a receiver of every record made after them, with no gap.
    pub fn follow(&self, query: &Query) -> (Vec<Record>, broadcast::Receiver<Arc<Record>>) {
        let state = lock(&self.inner.state);
        let receiver = self.inner.tail.subscribe();
        let mut backlog = page(&state, query).records;
        backlog.reverse();
        (backlog, receiver)
    }

    /// Collects the records made in `job` from now on, until it is dropped,
    /// however many the ring drops. A job's records are those made in its
    /// span ([`JOB_SPAN`](super::JOB_SPAN)).
    pub fn capture(&self, job: &str) -> Capture {
        let records = Arc::new(Mutex::new(Vec::new()));
        let mut state = lock(&self.inner.state);
        state
            .captures
            .push((job.to_owned(), Arc::downgrade(&records)));
        Capture { records }
    }

    /// The daily files, newest first; none for a store in memory only.
    pub fn files(&self) -> io::Result<Vec<LogFile>> {
        match self.folder() {
            Some(folder) => files::list(&folder),
            None => Ok(Vec::new()),
        }
    }

    /// The path of the daily file called `name`, if there is one.
    pub fn file(&self, name: &str) -> Option<PathBuf> {
        let listed = self.files().ok()?.into_iter().any(|file| file.name == name);
        listed.then_some(self.folder()?.join(name))
    }

    fn folder(&self) -> Option<PathBuf> {
        let state = lock(&self.inner.state);
        state.files.as_ref().map(|files| files.folder().to_owned())
    }
}

/// A page of `state`'s records that match `query`, newest first.
fn page(state: &State, query: &Query) -> Page {
    let size = query.page_size();
    let mut records: Vec<Record> = state
        .records
        .iter()
        .rev()
        .filter(|record| query.before.is_none_or(|before| record.seq < before))
        .filter(|record| query.matches(record))
        .take(size + 1)
        .map(|record| Record::clone(record))
        .collect();
    let next = if records.len() > size {
        records.pop();
        records.last().map(|record| record.seq)
    } else {
        None
    };
    Page { records, next }
}

/// A job's records, collected by [`LogStore::capture`].
#[derive(Debug)]
pub struct Capture {
    records: Arc<Captured>,
}

impl Capture {
    /// The records so far, oldest first.
    pub fn records(&self) -> Vec<Record> {
        lock(&self.records)
            .iter()
            .map(|r| Record::clone(r))
            .collect()
    }
}
