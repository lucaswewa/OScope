//! The ingest job (ADR-0022): a recording into a project, on the job
//! service's compute workers.

use std::path::PathBuf;
use std::sync::Arc;

use oscope_core::{ProjectId, RecordingId};
use oscope_jobs::{Factory, Job, JobCtx, JobSpec, Resource, Stop};
use oscope_store::{Scratch, Store, StoreError, ingest};
use serde::Deserialize;

/// The ingest job's kind.
pub const INGEST: &str = "projects.ingest";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Params {
    recording: RecordingId,
    project: ProjectId,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    keep: bool,
}

/// Makes ingest jobs, whose parameters are `recording`, `project`, and
/// optionally `name` and `keep`. Each writes the recording into a new
/// acquisition of the project, named `name` or as the recording, then
/// deletes the recording, unless `keep`.
pub fn ingest_jobs(store: Arc<Store>, scratch: Arc<Scratch>) -> Factory {
    Arc::new(move |value| {
        let params = Params::deserialize(value).map_err(|e| e.to_string())?;
        let folder = scratch
            .folder(params.recording)
            .map_err(|e| e.to_string())?;
        let spec = JobSpec {
            kind: INGEST.to_owned(),
            title: format!("ingest recording {}", params.recording),
            account: None,
            params: value.clone(),
            resource: Resource::Compute,
            pausable: false,
            gpu_mib: 0,
        };
        let store = Arc::clone(&store);
        Ok(Job {
            spec,
            run: Box::new(move |ctx| Box::pin(run(store, folder, params, ctx))),
        })
    })
}

async fn run(store: Arc<Store>, folder: PathBuf, params: Params, ctx: JobCtx) -> Result<(), Stop> {
    let (job, project) = (ctx.clone(), params.project);
    let work = tokio::task::spawn_blocking(move || {
        // The frames, then the pyramid and the thumbnail: about a tenth.
        let progress = |done: u64, total: u64| {
            let share = 0.9 * done as f64 / total as f64;
            match done < total {
                true => job.progress(Some(share), format!("{done} of {total} frames")),
                false => job.progress(Some(share), "the pyramid and the thumbnail"),
            }
            !job.is_cancelled()
        };
        let name = params.name.as_deref();
        ingest(&store, project, &folder, name, params.keep, &progress)
    });
    match work.await {
        Ok(Ok(acquisition)) => {
            ctx.progress(Some(1.0), "ingested");
            ctx.add_result(format!(
                "projects/{project}/acquisitions/{}",
                acquisition.id
            ));
            Ok(())
        }
        Ok(Err(StoreError::Cancelled)) => Err(Stop::Cancelled),
        Ok(Err(error)) => Err(Stop::failed(error)),
        Err(error) => Err(Stop::failed(error)),
    }
}
