//! The job service (ADR-0016): the hardware queue, the compute pool, and
//! what clients ask of jobs.

use std::collections::{HashMap, VecDeque};
use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use chrono::Utc;
use oscope_core::JobId;
use serde_json::Value;
use tokio::sync::{Notify, Semaphore, broadcast, watch};
use tracing::Instrument;

use crate::ctx::{Control, JobCtx};
use crate::model::{JobError, JobFilter, JobInfo, JobSpec, JobState, Progress, Resource, Stop};
use crate::store::JobStore;

/// A job's code, running.
pub type JobFuture = Pin<Box<dyn Future<Output = Result<(), Stop>> + Send>>;

/// A job to run: what it is, and its code, given its context when it starts.
pub struct Job {
    /// What it is.
    pub spec: JobSpec,
    /// Its code.
    pub run: Box<dyn FnOnce(JobCtx) -> JobFuture + Send>,
}

/// Makes a job of one kind from its parameters, for [`JobService::submit_kind`].
pub type Factory = Arc<dyn Fn(&Value) -> Result<Job, String> + Send + Sync>;

/// How many compute jobs run at once, and the GPU memory they share.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct JobsConfig {
    /// Compute jobs at once.
    pub workers: usize,
    /// The GPU memory compute jobs may hold between them, in MiB: none, and
    /// GPU memory isn't counted.
    pub gpu_mib: Option<u32>,
}

impl Default for JobsConfig {
    /// Two workers; GPU memory not counted.
    fn default() -> Self {
        Self {
            workers: 2,
            gpu_mib: None,
        }
    }
}

/// How often a running job's progress is recorded, at most: state changes
/// are recorded at once.
const RECORD_PROGRESS: Duration = Duration::from_secs(2);

pub(crate) struct Entry {
    pub(crate) info: JobInfo,
    control: Arc<Control>,
    run: Option<Box<dyn FnOnce(JobCtx) -> JobFuture + Send>>,
    recorded: Instant,
}

pub(crate) struct Shared {
    pub(crate) entries: Mutex<HashMap<JobId, Entry>>,
    hardware: Mutex<VecDeque<JobId>>,
    compute: Mutex<VecDeque<JobId>>,
    wake_hardware: Notify,
    wake_compute: Notify,
    holder: watch::Sender<Option<JobId>>,
    workers: Arc<Semaphore>,
    gpu: Option<(Arc<Semaphore>, u32)>,
    events: broadcast::Sender<JobInfo>,
    store: JobStore,
    kinds: Mutex<HashMap<String, Factory>>,
    /// The runtime is stopping: start no more jobs.
    stopping: AtomicBool,
}

pub(crate) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Shared {
    /// Tells subscribers about the job `id`, and records it: at once for a
    /// state change (`record`), at most every 2 s for progress.
    pub(crate) fn changed(&self, id: JobId, record: bool) {
        let info = {
            let mut entries = lock(&self.entries);
            let Some(entry) = entries.get_mut(&id) else {
                return;
            };
            let due = record || entry.recorded.elapsed() >= RECORD_PROGRESS;
            if due {
                entry.recorded = Instant::now();
            }
            (entry.info.clone(), due)
        };
        if info.1
            && let Err(error) = self.store.save(&info.0)
        {
            tracing::warn!("the job {id} couldn't be recorded: {error}");
        }
        let _ = self.events.send(info.0);
    }

    /// Updates each queued job's place, and tells subscribers about those
    /// that moved.
    fn renumber(&self) {
        let mut moved = Vec::new();
        {
            let mut entries = lock(&self.entries);
            for queue in [&self.hardware, &self.compute] {
                for (place, id) in lock(queue).iter().enumerate() {
                    if let Some(entry) = entries.get_mut(id)
                        && entry.info.position != Some(place)
                    {
                        entry.info.position = Some(place);
                        moved.push(*id);
                    }
                }
            }
        }
        for id in moved {
            self.changed(id, false);
        }
    }

    /// Ends the job `id` in `state`.
    fn finish(&self, id: JobId, state: JobState, error: Option<String>) {
        {
            let mut entries = lock(&self.entries);
            let Some(entry) = entries.get_mut(&id) else {
                return;
            };
            entry.info.state = state;
            entry.info.error = error;
            entry.info.finished = Some(Utc::now());
            entry.info.position = None;
            entry.run = None;
        }
        self.changed(id, true);
    }
}

/// The hardware lock: the hardware job running, if any. While it is held,
/// the device Things refuse manual moves.
#[derive(Clone)]
pub struct HardwareLock(watch::Receiver<Option<JobId>>);

impl HardwareLock {
    /// The job that holds it.
    pub fn holder(&self) -> Option<JobId> {
        *self.0.borrow()
    }
}

/// The job service: submit jobs, follow them, and change their course.
/// Cloning shares it.
#[derive(Clone)]
pub struct JobService {
    shared: Arc<Shared>,
}

impl JobService {
    /// The service, its history in the SQLite file at `path`: the jobs a
    /// stop cut short are marked interrupted. Starts its queues on the
    /// current tokio runtime.
    pub fn open(path: &Path, config: JobsConfig) -> Result<JobService, JobError> {
        let store = JobStore::open(path)?;
        let interrupted = store.interrupt(Utc::now())?;
        if !interrupted.is_empty() {
            tracing::warn!(
                "{} jobs were interrupted when the server stopped",
                interrupted.len()
            );
        }
        let gpu = config
            .gpu_mib
            .map(|mib| (Arc::new(Semaphore::new(mib as usize)), mib));
        let shared = Arc::new(Shared {
            entries: Mutex::default(),
            hardware: Mutex::default(),
            compute: Mutex::default(),
            wake_hardware: Notify::new(),
            wake_compute: Notify::new(),
            holder: watch::Sender::new(None),
            workers: Arc::new(Semaphore::new(config.workers.max(1))),
            gpu,
            events: broadcast::channel(1024).0,
            store,
            kinds: Mutex::default(),
            stopping: AtomicBool::new(false),
        });
        tokio::spawn(hardware(Arc::clone(&shared)));
        tokio::spawn(compute(Arc::clone(&shared)));
        Ok(JobService { shared })
    }

    /// Queues `job`: its ID.
    pub fn submit(&self, job: Job) -> Result<JobId, JobError> {
        let Job { spec, run } = job;
        if let (Resource::Compute, Some((_, budget))) = (spec.resource, &self.shared.gpu)
            && spec.gpu_mib > *budget
        {
            return Err(JobError::Refused(format!(
                "it needs {} MiB of GPU memory, of {budget}",
                spec.gpu_mib
            )));
        }
        let id = JobId::generate();
        let resource = spec.resource;
        let info = JobInfo {
            id,
            spec,
            state: JobState::Queued,
            progress: Progress::default(),
            results: Vec::new(),
            error: None,
            submitted: Utc::now(),
            started: None,
            finished: None,
            position: None,
        };
        let entry = Entry {
            info,
            control: Arc::new(Control::default()),
            run: Some(run),
            recorded: Instant::now(),
        };
        lock(&self.shared.entries).insert(id, entry);
        self.shared.changed(id, true);
        let (queue, wake) = match resource {
            Resource::Hardware => (&self.shared.hardware, &self.shared.wake_hardware),
            Resource::Compute => (&self.shared.compute, &self.shared.wake_compute),
        };
        lock(queue).push_back(id);
        self.shared.renumber();
        wake.notify_one();
        Ok(id)
    }

    /// Registers how to make jobs of `kind`, for [`submit_kind`](Self::submit_kind).
    pub fn register(&self, kind: &str, factory: Factory) {
        lock(&self.shared.kinds).insert(kind.to_owned(), factory);
    }

    /// The kinds registered.
    pub fn kinds(&self) -> Vec<String> {
        let mut kinds: Vec<String> = lock(&self.shared.kinds).keys().cloned().collect();
        kinds.sort();
        kinds
    }

    /// Queues a job of a registered `kind`, made from `params`, for
    /// `account`: its ID.
    pub fn submit_kind(
        &self,
        kind: &str,
        params: Value,
        account: Option<String>,
    ) -> Result<JobId, JobError> {
        let factory = lock(&self.shared.kinds)
            .get(kind)
            .cloned()
            .ok_or_else(|| JobError::Refused(format!("no kind of job is called {kind:?}")))?;
        let mut job = factory(&params).map_err(JobError::Refused)?;
        job.spec.kind = kind.to_owned();
        job.spec.params = params;
        job.spec.account = account;
        self.submit(job)
    }

    /// The job with this ID, as it is now.
    pub fn get(&self, id: JobId) -> Result<JobInfo, JobError> {
        if let Some(entry) = lock(&self.shared.entries).get(&id) {
            return Ok(entry.info.clone());
        }
        self.shared.store.get(id)?.ok_or(JobError::Unknown(id))
    }

    /// The jobs `filter` lets through, the newest first, as they are now.
    pub fn list(&self, filter: &JobFilter) -> Result<Vec<JobInfo>, JobError> {
        let mut jobs = self.shared.store.list(filter)?;
        let entries = lock(&self.shared.entries);
        for job in &mut jobs {
            if let Some(entry) = entries.get(&job.id) {
                job.clone_from(&entry.info);
            }
        }
        jobs.retain(|job| filter.state.is_none_or(|state| state == job.state));
        Ok(jobs)
    }

    /// The jobs not yet ended: running first, then each queue in order.
    pub fn queue(&self) -> Vec<JobInfo> {
        let entries = lock(&self.shared.entries);
        let mut jobs: Vec<JobInfo> = entries
            .values()
            .filter(|entry| !entry.info.state.is_finished())
            .map(|entry| entry.info.clone())
            .collect();
        jobs.sort_by_key(|job| {
            (
                job.position.is_some(),
                job.spec.resource == Resource::Compute,
                job.position,
                job.submitted,
            )
        });
        jobs
    }

    /// Every change to every job, from now on.
    pub fn subscribe(&self) -> broadcast::Receiver<JobInfo> {
        self.shared.events.subscribe()
    }

    /// The hardware lock.
    pub fn hardware(&self) -> HardwareLock {
        HardwareLock(self.shared.holder.subscribe())
    }

    /// Cancels the job `id`: at once if it is queued, at its next checkpoint
    /// if it is running.
    pub fn cancel(&self, id: JobId) -> Result<(), JobError> {
        let state = self.state(id)?;
        match state {
            JobState::Queued => {
                lock(&self.shared.hardware).retain(|queued| *queued != id);
                lock(&self.shared.compute).retain(|queued| *queued != id);
                self.shared.finish(id, JobState::Cancelled, None);
                self.shared.renumber();
                Ok(())
            }
            JobState::Running | JobState::Paused => {
                self.control(id)?.cancel();
                Ok(())
            }
            _ => Err(JobError::State {
                id,
                state,
                what: "be cancelled",
            }),
        }
    }

    /// Pauses the running job `id` at its next checkpoint, if it can pause.
    pub fn pause(&self, id: JobId) -> Result<(), JobError> {
        let state = self.state(id)?;
        if state != JobState::Running {
            let what = "pause: only a running job can";
            return Err(JobError::State { id, state, what });
        }
        if !self.get(id)?.spec.pausable {
            return Err(JobError::NotPausable(id));
        }
        self.control(id)?.pause(true);
        Ok(())
    }

    /// Lets the paused job `id` go on.
    pub fn resume(&self, id: JobId) -> Result<(), JobError> {
        let state = self.state(id)?;
        if !matches!(state, JobState::Paused | JobState::Running) {
            let what = "resume: only a paused job can";
            return Err(JobError::State { id, state, what });
        }
        self.control(id)?.pause(false);
        Ok(())
    }

    /// Moves the queued hardware job `id` to `position` in its queue, from
    /// 0; past the end, to the end.
    pub fn reorder(&self, id: JobId, position: usize) -> Result<(), JobError> {
        let state = self.state(id)?;
        {
            let mut queue = lock(&self.shared.hardware);
            let Some(at) = queue.iter().position(|queued| *queued == id) else {
                let what = "move: only a queued hardware job can";
                return Err(JobError::State { id, state, what });
            };
            queue.remove(at);
            let end = queue.len();
            queue.insert(position.min(end), id);
        }
        self.shared.renumber();
        Ok(())
    }

    fn state(&self, id: JobId) -> Result<JobState, JobError> {
        Ok(self.get(id)?.state)
    }

    fn control(&self, id: JobId) -> Result<Arc<Control>, JobError> {
        lock(&self.shared.entries)
            .get(&id)
            .map(|entry| Arc::clone(&entry.control))
            .ok_or(JobError::Unknown(id))
    }
}

/// Runs the hardware jobs, one at a time, in their queue's order, each
/// holding the hardware lock.
async fn hardware(shared: Arc<Shared>) {
    while !shared.stopping.load(Ordering::SeqCst) {
        let next = lock(&shared.hardware).pop_front();
        let Some(id) = next else {
            shared.wake_hardware.notified().await;
            continue;
        };
        shared.renumber();
        shared.holder.send_replace(Some(id));
        let ended = run(&shared, id).await;
        // Free before its end is announced: a client told it has ended may
        // move the stage at once.
        shared.holder.send_replace(None);
        if let Some((state, error)) = ended {
            shared.finish(id, state, error);
        }
    }
}

/// Runs the compute jobs in their queue's order, as workers and GPU memory
/// come free.
async fn compute(shared: Arc<Shared>) {
    while !shared.stopping.load(Ordering::SeqCst) {
        let head = lock(&shared.compute).front().copied();
        let Some(id) = head else {
            shared.wake_compute.notified().await;
            continue;
        };
        let gpu_mib = lock(&shared.entries)
            .get(&id)
            .map_or(0, |entry| entry.info.spec.gpu_mib);
        let Ok(worker) = Arc::clone(&shared.workers).acquire_owned().await else {
            return;
        };
        let gpu = match (&shared.gpu, gpu_mib) {
            (Some((budget, _)), mib) if mib > 0 => {
                let Ok(permits) = Arc::clone(budget).acquire_many_owned(mib).await else {
                    return;
                };
                Some(permits)
            }
            _ => None,
        };
        // Cancelled while it waited: it isn't the head any more.
        let taken = {
            let mut queue = lock(&shared.compute);
            let still = queue.front() == Some(&id);
            if still {
                queue.pop_front();
            }
            still
        };
        if !taken {
            continue;
        }
        shared.renumber();
        let shared = Arc::clone(&shared);
        tokio::spawn(async move {
            let ended = run(&shared, id).await;
            drop((worker, gpu));
            if let Some((state, error)) = ended {
                shared.finish(id, state, error);
            }
        });
    }
}

/// Runs the job `id` to its end, in a `job` span with its ID, so that its
/// records carry it (ADR-0009): how it ended, for the caller to announce
/// once it has freed what the job held; nothing if it didn't start.
async fn run(shared: &Arc<Shared>, id: JobId) -> Option<(JobState, Option<String>)> {
    let started = {
        let mut entries = lock(&shared.entries);
        match entries.get_mut(&id) {
            Some(entry) if entry.info.state == JobState::Queued => {
                entry.info.state = JobState::Running;
                entry.info.started = Some(Utc::now());
                entry.info.position = None;
                entry.run.take().map(|code| {
                    (
                        code,
                        Arc::clone(&entry.control),
                        entry.info.spec.kind.clone(),
                    )
                })
            }
            _ => None,
        }
    };
    let (code, control, kind) = started?;
    shared.changed(id, true);
    let span = tracing::info_span!("job", job_id = %id);
    span.in_scope(|| tracing::info!("the job starts: {kind}"));
    let ctx = JobCtx::new(id, control, Arc::clone(shared));
    let outcome = tokio::spawn(code(ctx).instrument(span.clone())).await;
    let (state, error) = match outcome {
        Ok(Ok(())) => (JobState::Completed, None),
        Ok(Err(Stop::Cancelled)) => (JobState::Cancelled, None),
        Ok(Err(Stop::Failed(reason))) => (JobState::Failed, Some(reason)),
        // Its task cancelled: the runtime, and so the server, is stopping.
        // No more jobs start; those queued stay so, for the next start to
        // mark interrupted.
        Err(stopped) if stopped.is_cancelled() => {
            shared.stopping.store(true, Ordering::SeqCst);
            let why = "the server stopped while it was running";
            (JobState::Interrupted, Some(why.to_owned()))
        }
        Err(panic) => (JobState::Failed, Some(format!("it panicked: {panic}"))),
    };
    span.in_scope(|| tracing::info!("the job ends: {}", state.name()));
    Some((state, error))
}
