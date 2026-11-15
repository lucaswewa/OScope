//! What a job's code is given: its ID, and ways to report progress, to stop
//! when cancelled, and to wait while paused.

use std::sync::Arc;

use chrono::Utc;
use oscope_core::JobId;
use tokio::sync::watch;

use crate::model::{JobState, Stop};
use crate::service::{Shared, lock};

/// Asked of a running job: to stop, or to pause.
pub(crate) struct Control {
    cancel: watch::Sender<bool>,
    pause: watch::Sender<bool>,
}

impl Default for Control {
    fn default() -> Self {
        Self {
            cancel: watch::Sender::new(false),
            pause: watch::Sender::new(false),
        }
    }
}

impl Control {
    pub(crate) fn cancel(&self) {
        self.cancel.send_replace(true);
    }

    pub(crate) fn pause(&self, paused: bool) {
        self.pause.send_replace(paused);
    }
}

/// The job was cancelled: what [`JobCtx::checkpoint`] returns then. `?`
/// turns it into [`Stop::Cancelled`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cancelled;

impl From<Cancelled> for Stop {
    fn from(_: Cancelled) -> Stop {
        Stop::Cancelled
    }
}

/// A running job's own: report progress, and call
/// [`checkpoint`](Self::checkpoint) where it may stop or pause.
#[derive(Clone)]
pub struct JobCtx {
    id: JobId,
    control: Arc<Control>,
    shared: Arc<Shared>,
}

impl JobCtx {
    pub(crate) fn new(id: JobId, control: Arc<Control>, shared: Arc<Shared>) -> JobCtx {
        JobCtx {
            id,
            control,
            shared,
        }
    }

    /// The job's ID.
    pub fn id(&self) -> JobId {
        self.id
    }

    /// Who submitted it, if an account did: what it makes is theirs (P32).
    pub fn account(&self) -> Option<String> {
        let entries = lock(&self.shared.entries);
        entries
            .get(&self.id)
            .and_then(|e| e.info.spec.account.clone())
    }

    fn update(&self, change: impl FnOnce(&mut crate::JobInfo)) {
        if let Some(entry) = lock(&self.shared.entries).get_mut(&self.id) {
            change(&mut entry.info);
        }
        self.shared.changed(self.id, false);
    }

    /// Reports how far it has got, `fraction` from 0 to 1 if it knows, and
    /// what it is doing; the time it should take still follows.
    pub fn progress(&self, fraction: Option<f64>, stage: impl Into<String>) {
        let stage = stage.into();
        self.update(|info| {
            let fraction = fraction.map(|f| f.clamp(0.0, 1.0));
            let elapsed = info
                .started
                .map(|started| (Utc::now() - started).as_seconds_f64());
            info.progress.eta_s = match (fraction, elapsed) {
                (Some(f), Some(spent)) if f > 0.0 => Some(spent * (1.0 - f) / f),
                _ => None,
            };
            info.progress.fraction = fraction;
            info.progress.stage = stage;
        });
    }

    /// Reports a count, such as tiles done.
    pub fn count(&self, name: &str, value: u64) {
        self.update(|info| {
            info.progress.counters.insert(name.to_owned(), value);
        });
    }

    /// Records something it made: a reference to a result.
    pub fn add_result(&self, reference: impl Into<String>) {
        let reference = reference.into();
        self.update(|info| info.results.push(reference));
    }

    /// Whether it has been asked to stop.
    pub fn is_cancelled(&self) -> bool {
        *self.control.cancel.borrow()
    }

    /// Completes when it is asked to stop: for a long wait, in a `select!`.
    pub async fn cancelled(&self) {
        let mut cancel = self.control.cancel.subscribe();
        let _ = cancel.wait_for(|cancelled| *cancelled).await;
    }

    /// A place where the job may stop or pause: `Err` if it was cancelled;
    /// if it was paused, it waits here, `paused`, until resumed or
    /// cancelled.
    pub async fn checkpoint(&self) -> Result<(), Cancelled> {
        if self.is_cancelled() {
            return Err(Cancelled);
        }
        if !*self.control.pause.borrow() {
            return Ok(());
        }
        self.set_state(JobState::Paused);
        let (mut pause, mut cancel) = (
            self.control.pause.subscribe(),
            self.control.cancel.subscribe(),
        );
        tokio::select! {
            _ = pause.wait_for(|paused| !*paused) => {}
            _ = cancel.wait_for(|cancelled| *cancelled) => {}
        }
        self.set_state(JobState::Running);
        match self.is_cancelled() {
            true => Err(Cancelled),
            false => Ok(()),
        }
    }

    fn set_state(&self, state: JobState) {
        if let Some(entry) = lock(&self.shared.entries).get_mut(&self.id) {
            entry.info.state = state;
        }
        self.shared.changed(self.id, true);
    }
}
