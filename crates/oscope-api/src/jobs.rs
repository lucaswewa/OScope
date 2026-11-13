//! The `jobs` Thing (plan §5.4, §5.11; ADR-0016): the job service's queue
//! and history, and what clients can ask of a job.

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use oscope_core::JobId;
use oscope_jobs::{JobError, JobFilter, JobInfo, JobService, JobState};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use teta_wot::http::PathParams;
use teta_wot::http::axum::Json;
use teta_wot::http::axum::extract::Query;
use teta_wot::http::axum::http::StatusCode;
use teta_wot::http::axum::response::{IntoResponse, Response};
use teta_wot::prelude::*;
use tokio::sync::broadcast::{self, error::RecvError};

use crate::API_PREFIX;

/// Where to follow a job an action has started: what such an action
/// answers with at once (the `JobRef` pattern, ADR-0016).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct JobRef {
    /// The job.
    pub id: JobId,
    /// Where it is, as it is now.
    pub href: String,
}

impl JobRef {
    /// The reference to the job `id`.
    pub fn new(id: JobId) -> JobRef {
        JobRef {
            id,
            href: format!("{API_PREFIX}/jobs/job/{id}"),
        }
    }
}

/// A job service error, as an action's: its kind and status, as ADR-0034
/// has them.
fn action_error(error: JobError) -> ActionError {
    let (status, kind) = status(&error);
    ActionError::handled(format!("{kind} ({status}): {error}"))
}

/// The HTTP status and kind a job service error answers with.
fn status(error: &JobError) -> (u16, &'static str) {
    match error {
        JobError::Unknown(_) => (404, "unknown"),
        JobError::State { .. } | JobError::NotPausable(_) => (409, "conflict"),
        JobError::Refused(_) => (400, "refused"),
        _ => (500, "internal"),
    }
}

/// A job service error, as an endpoint's answer: problem details.
fn problem(error: &JobError) -> Response {
    let (status, kind) = status(error);
    let status = StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    let body = json!({"title": kind, "status": status.as_u16(), "detail": error.to_string()});
    (status, Json(body)).into_response()
}

/// How often a job's progress may be sent as an event, at most.
const PROGRESS_EVERY: Duration = Duration::from_millis(100);

/// Sends the job service's changes as `changed` events: each change of
/// state at once, and progress at most every 100 ms a job, the latest.
/// `teta-wot` gives each subscriber a buffer of only 5 events, and drops
/// the rest; a job's burst of progress would push its end out (ADR-0016).
async fn forward(mut changes: broadcast::Receiver<JobInfo>, changed: Event<JobInfo>) {
    let mut states: HashMap<JobId, JobState> = HashMap::new();
    let mut held: HashMap<JobId, JobInfo> = HashMap::new();
    let mut tick = tokio::time::interval(PROGRESS_EVERY);
    loop {
        tokio::select! {
            change = changes.recv() => match change {
                Ok(job) => {
                    if states.insert(job.id, job.state) == Some(job.state) {
                        held.insert(job.id, job);
                    } else {
                        held.remove(&job.id);
                        if job.state.is_finished() {
                            states.remove(&job.id);
                        }
                        changed.emit(job);
                    }
                }
                Err(RecvError::Lagged(_)) => continue,
                Err(RecvError::Closed) => break,
            },
            _ = tick.tick() => {
                for (_, job) in held.drain() {
                    changed.emit(job);
                }
            }
        }
    }
}

/// The jobs: every long piece of work, queued, followed and recorded.
/// Hardware jobs run one at a time and hold the hardware lock; compute jobs
/// share a pool. A job's own log is the `logs` Thing's records with its ID
/// (`?job=…`).
#[derive(Thing)]
#[thing(
    title = "Jobs",
    semantic_type = "saref:Function",
    context(saref = "https://saref.etsi.org/core/")
)]
pub struct JobsThing {
    /// A job changed: queued, moved, started, progressed, paused or ended.
    #[event]
    changed: Event<JobInfo>,
    service: OnceLock<Arc<JobService>>,
}

impl JobsThing {
    fn service(&self) -> Result<&JobService, ActionError> {
        self.service
            .get()
            .map(AsRef::as_ref)
            .ok_or_else(|| ActionError::handled("the Thing hasn't started"))
    }

    fn reading(&self) -> Result<&JobService, PropertyError> {
        self.service
            .get()
            .map(AsRef::as_ref)
            .ok_or_else(|| PropertyError::failed(anyhow::anyhow!("the Thing hasn't started")))
    }
}

#[thing_impl]
impl JobsThing {
    #[on_start]
    async fn connect(&self, ctx: ThingCtx) -> anyhow::Result<()> {
        let service = ctx
            .server()
            .service::<JobService>()
            .ok_or_else(|| anyhow::anyhow!("the server has no job service"))?;
        tokio::spawn(forward(service.subscribe(), self.changed.clone()));
        let _ = self.service.set(service);
        Ok(())
    }

    /// The jobs not yet ended.
    ///
    /// Running first, then each queue in order.
    #[property]
    async fn queue(&self) -> Result<Vec<JobInfo>, PropertyError> {
        Ok(self.reading()?.queue())
    }

    /// The kinds of job `submit` can queue.
    #[property]
    async fn kinds(&self) -> Result<Vec<String>, PropertyError> {
        Ok(self.reading()?.kinds())
    }

    /// The hardware job holding the hardware lock, if any.
    ///
    /// Manual moves are refused while one does.
    #[property]
    async fn hardware(&self) -> Result<Option<JobId>, PropertyError> {
        Ok(self.reading()?.hardware().holder())
    }

    /// The jobs, the newest first.
    ///
    /// Filtered by `?state=…`, `&kind=…`, and at most `&limit=…` (50, and
    /// 1000 at most).
    #[endpoint(get, "list")]
    async fn list(&self, Query(filter): Query<JobFilter>) -> Response {
        let listed = self.reading().map_err(|e| JobError::Store(e.to_string()));
        match listed.and_then(|service| service.list(&filter)) {
            Ok(jobs) => Json(jobs).into_response(),
            Err(error) => problem(&error),
        }
    }

    /// One job, as it is now.
    #[endpoint(get, "job/{id}")]
    async fn job(&self, params: PathParams) -> Response {
        let Some(id) = params.parse::<JobId>("id") else {
            let text = params.get("id").unwrap_or_default();
            return problem(&JobError::Refused(format!("{text:?} isn't a job's ID")));
        };
        let found = self.reading().map_err(|e| JobError::Store(e.to_string()));
        match found.and_then(|service| service.get(id)) {
            Ok(job) => Json(job).into_response(),
            Err(error) => problem(&error),
        }
    }

    /// Queues a job of one of the `kinds`, with its parameters.
    ///
    /// Answers at once with where to follow it.
    #[action]
    async fn submit(
        &self,
        ctx: ActionCtx,
        kind: String,
        #[param(default)] params: serde_json::Value,
    ) -> Result<JobRef, ActionError> {
        let account = ctx.principal().id().map(str::to_owned);
        let service = self.service()?;
        let id = service
            .submit_kind(&kind, params, account)
            .map_err(action_error)?;
        Ok(JobRef::new(id))
    }

    /// Cancels a job.
    ///
    /// A queued job at once; a running one at its next checkpoint.
    #[action]
    async fn cancel(&self, id: JobId) -> Result<(), ActionError> {
        self.service()?.cancel(id).map_err(action_error)
    }

    /// Pauses a running job at its next checkpoint, if it can pause.
    #[action]
    async fn pause(&self, id: JobId) -> Result<(), ActionError> {
        self.service()?.pause(id).map_err(action_error)
    }

    /// Lets a paused job go on.
    #[action]
    async fn resume(&self, id: JobId) -> Result<(), ActionError> {
        self.service()?.resume(id).map_err(action_error)
    }

    /// Moves a queued hardware job to a place in its queue.
    ///
    /// `position` counts from 0; past the end is the end.
    #[action]
    async fn reorder(&self, id: JobId, position: usize) -> Result<(), ActionError> {
        self.service()?.reorder(id, position).map_err(action_error)
    }
}
