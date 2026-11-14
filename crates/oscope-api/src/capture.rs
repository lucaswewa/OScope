//! The `capture` Thing (plan §4.4; P30): single raw captures and bursts, as
//! jobs that make acquisitions, with a display rendition if asked.

use std::sync::{Arc, OnceLock};

use oscope_acquire::{BURST, Rig, SINGLE, capture_jobs};
use oscope_core::ProjectId;
use oscope_jobs::JobService;
use oscope_store::{Rendition, Scratch, Store};
use serde_json::{Value, json};
use teta_wot::prelude::*;

use crate::devices::Devices;
use crate::jobs::{JobRef, action_error};

/// Single raw captures and bursts, as jobs. Each makes an acquisition of a
/// project from the camera's frames whose exposure starts after it does.
#[derive(Thing)]
#[thing(
    title = "Capture",
    semantic_type = "saref:Function",
    context(saref = "https://saref.etsi.org/core/")
)]
pub struct CaptureThing {
    jobs: OnceLock<Arc<JobService>>,
}

impl CaptureThing {
    fn submit(&self, ctx: &ActionCtx, kind: &str, params: Value) -> Result<JobRef, ActionError> {
        let jobs = self.jobs.get().ok_or_else(|| {
            ActionError::handled(
                "unavailable (503): the server keeps no projects, scratch space or jobs",
            )
        })?;
        let account = ctx.principal().id().map(str::to_owned);
        let id = jobs
            .submit_kind(kind, params, account)
            .map_err(action_error)?;
        Ok(JobRef::new(id))
    }
}

#[thing_impl]
impl CaptureThing {
    #[on_start]
    async fn connect(&self, ctx: ThingCtx) -> anyhow::Result<()> {
        let server = ctx.server();
        let found = (
            server.service::<Devices>(),
            server.service::<Store>(),
            server.service::<Scratch>(),
            server.service::<JobService>(),
        );
        // Without somewhere to put them, there are no captures: its actions
        // say so.
        let (Some(devices), Some(store), Some(scratch), Some(jobs)) = found else {
            return Ok(());
        };
        let rig = Rig {
            bus: Arc::clone(devices.acquisition.bus()),
            clock: Arc::clone(&devices.clock),
            store,
            scratch,
        };
        jobs.register(SINGLE, capture_jobs(rig.clone(), false));
        jobs.register(BURST, capture_jobs(rig, true));
        let _ = self.jobs.set(jobs);
        Ok(())
    }

    /// Captures the camera's next frame into a new acquisition of
    /// `project`, named `name`, with its display rendition, `png` or `jpeg`,
    /// if asked.
    ///
    /// A job: answered at once with where to follow it. Its results are
    /// the acquisition and the rendition.
    #[action]
    async fn single(
        &self,
        ctx: ActionCtx,
        project: ProjectId,
        #[param(default)] name: Option<String>,
        #[param(default)] rendition: Option<Rendition>,
    ) -> Result<JobRef, ActionError> {
        let params = json!({ "project": project, "name": name, "rendition": rendition });
        self.submit(&ctx, SINGLE, params)
    }

    /// Captures the camera's next `frames` frames, 1 to 10,000, into a new
    /// acquisition of `project`, a plane each, named `name`, with the first
    /// plane's display rendition, `png` or `jpeg`, if asked.
    ///
    /// A job: answered at once with where to follow it.
    #[action]
    async fn burst(
        &self,
        ctx: ActionCtx,
        project: ProjectId,
        frames: u64,
        #[param(default)] name: Option<String>,
        #[param(default)] rendition: Option<Rendition>,
    ) -> Result<JobRef, ActionError> {
        let params = json!({
            "project": project, "frames": frames, "name": name, "rendition": rendition,
        });
        self.submit(&ctx, BURST, params)
    }
}
