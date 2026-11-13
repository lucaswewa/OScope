//! Batch mode: a plan over a dataset, as a compute job (ADR-0016, ADR-0017).

use std::sync::{Arc, Mutex};

use oscope_jobs::{Job, JobCtx, JobSpec, Resource, Stop};
use serde_json::json;

use crate::plan::Plan;
use crate::run::{Engine, Run, RunError, RunOptions, lock};
use crate::value::Values;

/// Reads a dataset's item `i`: the recipe's inputs.
pub type ReadItem = Box<dyn FnMut(usize) -> Result<Values, String> + Send>;

/// Takes item `i`'s run: a reference to its result, for the job to record,
/// if it makes one.
pub type WriteItem = Box<dyn FnMut(usize, Run) -> Result<Option<String>, String> + Send>;

/// A batch run's dataset, and where its results go.
pub struct Batch {
    /// How many items it has.
    pub count: usize,
    /// Reads an item.
    pub read: ReadItem,
    /// Takes an item's run.
    pub write: WriteItem,
    /// Whether each item runs tile by tile, with tiles this many pixels
    /// square.
    pub tile: Option<u32>,
}

/// Why an item stopped the job.
enum Failure {
    Cancelled,
    Failed(String),
}

impl From<RunError> for Failure {
    fn from(error: RunError) -> Self {
        match error {
            RunError::Cancelled => Failure::Cancelled,
            error => Failure::Failed(error.to_string()),
        }
    }
}

impl Engine {
    /// A compute job (ADR-0016) running `plan` on each item of `batch`, in
    /// the batch lane, memoised. Its progress counts items, and tiles
    /// within an item; it may pause between items; cancelled, it stops
    /// before the next step. It holds `gpu_mib` of the GPU's memory.
    pub fn batch_job(
        self: &Arc<Self>,
        title: &str,
        plan: Arc<Plan>,
        batch: Batch,
        gpu_mib: u32,
    ) -> Job {
        let spec = JobSpec {
            kind: "pipeline.batch".into(),
            title: title.into(),
            account: None,
            params: json!({ "recipe": plan.name, "items": batch.count, "tile": batch.tile }),
            resource: Resource::Compute,
            pausable: true,
            gpu_mib,
        };
        let engine = Arc::clone(self);
        Job {
            spec,
            run: Box::new(move |ctx| Box::pin(run_batch(engine, plan, batch, ctx))),
        }
    }
}

async fn run_batch(
    engine: Arc<Engine>,
    plan: Arc<Plan>,
    batch: Batch,
    ctx: JobCtx,
) -> Result<(), Stop> {
    let Batch {
        count,
        read,
        write,
        tile,
    } = batch;
    let io = Arc::new(Mutex::new((read, write)));
    for item in 0..count {
        ctx.checkpoint().await?;
        let share = move |done: f64| Some((item as f64 + done) / count as f64);
        ctx.progress(share(0.0), format!("item {} of {count}", item + 1));
        let options = RunOptions::default();
        let cancel = Arc::clone(&options.cancel);
        let (engine, plan, io, job) = (engine.clone(), plan.clone(), io.clone(), ctx.clone());
        let work = tokio::task::spawn_blocking(move || -> Result<Option<String>, Failure> {
            let (read, write) = &mut *lock(&io);
            let inputs =
                read(item).map_err(|error| Failure::Failed(format!("reading it: {error}")))?;
            let run = match tile {
                Some(size) => engine.run_tiled(&plan, &inputs, size, &options, |done, tiles| {
                    let stage = format!("item {} of {count}: tile {done} of {tiles}", item + 1);
                    job.progress(share(done as f64 / tiles as f64), stage);
                })?,
                None => engine.run(&plan, &inputs, &options)?,
            };
            write(item, run).map_err(|error| Failure::Failed(format!("writing it: {error}")))
        });
        tokio::pin!(work);
        let done = tokio::select! {
            done = &mut work => done,
            () = ctx.cancelled() => {
                cancel.store(true, std::sync::atomic::Ordering::Release);
                work.await
            }
        };
        match done {
            Ok(Ok(reference)) => reference
                .into_iter()
                .for_each(|reference| ctx.add_result(reference)),
            Ok(Err(Failure::Cancelled)) => return Err(Stop::Cancelled),
            Ok(Err(Failure::Failed(why))) => {
                return Err(Stop::failed(format!("item {}: {why}", item + 1)));
            }
            Err(panic) => return Err(Stop::failed(format!("item {}: {panic}", item + 1))),
        }
        ctx.count("items", item as u64 + 1);
    }
    ctx.progress(Some(1.0), format!("{count} items done"));
    Ok(())
}
