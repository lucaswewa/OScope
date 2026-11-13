//! What a job is (plan §5.11): its description, state, progress and
//! history, as clients see them.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use oscope_core::JobId;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Where a job is in its life.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum JobState {
    /// Waiting its turn.
    Queued,
    /// Running.
    Running,
    /// Stopped at a checkpoint until it is resumed.
    Paused,
    /// Done.
    Completed,
    /// Stopped by an error.
    Failed,
    /// Stopped because it was asked to.
    Cancelled,
    /// Cut short by the server stopping.
    Interrupted,
}

impl JobState {
    /// Whether it has ended, for good.
    pub fn is_finished(self) -> bool {
        matches!(
            self,
            Self::Completed | Self::Failed | Self::Cancelled | Self::Interrupted
        )
    }

    /// Its name, as stored and serialised.
    pub fn name(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Paused => "paused",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::Interrupted => "interrupted",
        }
    }
}

/// What a job needs to run.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Resource {
    /// The instrument: one at a time, holding the hardware lock.
    Hardware,
    /// A worker of the compute pool, and the GPU memory it asks for.
    Compute,
}

/// How far a job has got.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Progress {
    /// How far, from 0 to 1, when it knows.
    pub fraction: Option<f64>,
    /// What it is doing now.
    pub stage: String,
    /// Things counted so far, such as tiles.
    pub counters: BTreeMap<String, u64>,
    /// The seconds it should take still, from the fraction and the time so
    /// far.
    pub eta_s: Option<f64>,
}

/// What a job is, as it is submitted.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct JobSpec {
    /// What kind of job, such as `scan.raster`.
    pub kind: String,
    /// A title for people.
    pub title: String,
    /// Who submitted it.
    pub account: Option<String>,
    /// Its parameters.
    pub params: serde_json::Value,
    /// What it runs on.
    pub resource: Resource,
    /// Whether it can pause at its checkpoints.
    pub pausable: bool,
    /// The GPU memory it holds while it runs, in MiB: for compute jobs.
    pub gpu_mib: u32,
}

/// A job, as it is now.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct JobInfo {
    /// Its ID.
    pub id: JobId,
    /// What it is.
    #[serde(flatten)]
    pub spec: JobSpec,
    /// Where it is in its life.
    pub state: JobState,
    /// How far it has got.
    pub progress: Progress,
    /// What it made: references to its results.
    pub results: Vec<String>,
    /// Why it failed, or how it was cut short.
    pub error: Option<String>,
    /// When it was submitted.
    pub submitted: DateTime<Utc>,
    /// When it started.
    pub started: Option<DateTime<Utc>>,
    /// When it ended.
    pub finished: Option<DateTime<Utc>>,
    /// Its place in its queue, from 0, while it waits.
    pub position: Option<usize>,
}

/// Which jobs a list holds: the newest first.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct JobFilter {
    /// Only those in this state.
    pub state: Option<JobState>,
    /// Only those of this kind.
    pub kind: Option<String>,
    /// At most this many: 50 when not given, and 1000 at most.
    pub limit: Option<usize>,
}

/// What the job service refuses.
#[derive(Clone, Debug, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum JobError {
    /// No job has this ID.
    #[error("no job {0}")]
    Unknown(JobId),
    /// The job isn't in a state that allows it.
    #[error("the job {id} is {}: it can't {what}", state.name())]
    State {
        /// The job.
        id: JobId,
        /// Its state.
        state: JobState,
        /// What was asked.
        what: &'static str,
    },
    /// The job can't pause.
    #[error("the job {0} can't pause")]
    NotPausable(JobId),
    /// The job can't be run as it is described.
    #[error("{0}")]
    Refused(String),
    /// The history couldn't be read or written.
    #[error("the job history: {0}")]
    Store(String),
}

/// Why a job's code stopped before the end.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Stop {
    /// It was cancelled, and stopped at a checkpoint.
    Cancelled,
    /// It failed, for this reason.
    Failed(String),
}

impl Stop {
    /// A failure, for `reason`.
    pub fn failed(reason: impl std::fmt::Display) -> Stop {
        Stop::Failed(reason.to_string())
    }
}
