//! The `autofocus` and `exposure` Things (P34; ADR-0024): autofocus, auto
//! exposure and auto white balance as hardware jobs, their latest reports,
//! and autofocus's measures and defaults now.

// The actions take each option as its own parameter, as clients send them;
// `thing_impl`'s wrappers count them all.
#![allow(clippy::too_many_arguments)]

use std::sync::{Arc, OnceLock, PoisonError};

use oscope_compute::{Compute, ComputeConfig};
use oscope_jobs::JobService;
use oscope_workflows::autofocus::{
    AUTO_EXPOSURE, AUTOFOCUS, ExposureReport, FocusDefaults, FocusReport, FocusRig, FrameMeasures,
    LastReports, Measure, Sweep, WHITE_BALANCE, WhiteBalanceReport, current_measures,
    focus_defaults, focus_jobs,
};
use oscope_workflows::calibration::Region;
use serde_json::{Map, Value, json};
use teta_wot::prelude::*;

use crate::devices::Devices;
use crate::jobs::{JobRef, action_error};

/// What each Thing runs on: the rig, the job service, and the reports its
/// jobs keep.
struct Focus {
    rig: FocusRig,
    jobs: Option<Arc<JobService>>,
    last: LastReports,
}

impl Focus {
    fn submit(&self, ctx: &ActionCtx, kind: &str, params: Value) -> Result<JobRef, ActionError> {
        let jobs = self
            .jobs
            .as_ref()
            .ok_or_else(|| ActionError::handled("unavailable (503): the server runs no jobs"))?;
        let account = ctx.principal().id().map(str::to_owned);
        let id = jobs
            .submit_kind(kind, params, account)
            .map_err(action_error)?;
        Ok(JobRef::new(id))
    }
}

fn unavailable() -> ActionError {
    ActionError::handled("unavailable (503): the server has no devices")
}

fn no_devices() -> PropertyError {
    PropertyError::failed(anyhow::anyhow!("the server has no devices"))
}

/// Parameters for a job: those given, so the job's defaults stand for the
/// rest.
fn given(pairs: &[(&str, Option<Value>)]) -> Value {
    let map: Map<String, Value> = pairs
        .iter()
        .filter_map(|(k, v)| v.clone().map(|v| ((*k).to_owned(), v)))
        .collect();
    Value::Object(map)
}

/// A Thing's state from the server's devices, with its job `kinds`
/// registered; the compute layer made only if it measures focus. None
/// without devices.
async fn connect(ctx: &ThingCtx, measures: bool, kinds: &[&str]) -> anyhow::Result<Option<Focus>> {
    let server = ctx.server();
    let Some(devices) = server.service::<Devices>() else {
        return Ok(None);
    };
    let compute = match measures {
        true => Some(Arc::new(
            tokio::task::spawn_blocking(|| Compute::new(&ComputeConfig::default())).await??,
        )),
        false => None,
    };
    let rig = FocusRig {
        stage: Arc::clone(&devices.stage),
        camera: Arc::clone(&devices.camera),
        objectives: Arc::clone(&devices.objectives),
        bus: Arc::clone(devices.acquisition.bus()),
        lights: Arc::clone(&devices.lights),
        compute,
    };
    let last = LastReports::default();
    let jobs = server.service::<JobService>();
    if let Some(jobs) = &jobs {
        for (kind, factory) in focus_jobs(rig.clone(), last.clone()) {
            if kinds.contains(&kind) {
                jobs.register(kind, factory);
            }
        }
    }
    Ok(Some(Focus { rig, jobs, last }))
}

fn latest<T: Clone>(slot: &std::sync::Mutex<Option<T>>) -> Option<T> {
    slot.lock().unwrap_or_else(PoisonError::into_inner).clone()
}

/// Autofocus: finds the sharpest z, and leaves the stage there.
#[derive(Thing)]
#[thing(
    title = "Autofocus",
    semantic_type = "saref:Function",
    context(saref = "https://saref.etsi.org/core/")
)]
pub struct AutofocusThing {
    state: OnceLock<Focus>,
}

impl AutofocusThing {
    fn state(&self) -> Result<&Focus, ActionError> {
        self.state.get().ok_or_else(unavailable)
    }
}

#[thing_impl]
impl AutofocusThing {
    #[on_start]
    async fn connect(&self, ctx: ThingCtx) -> anyhow::Result<()> {
        if let Some(state) = connect(&ctx, true, &[AUTOFOCUS]).await? {
            let _ = self.state.set(state);
        }
        Ok(())
    }

    /// The focus measures now.
    ///
    /// Brenner's gradient, Tenengrad and the variance of the Laplacian of
    /// the next frame's middle, where the stage is, without moving.
    #[property]
    async fn measures(&self) -> Result<FrameMeasures, PropertyError> {
        let state = self.state.get().ok_or_else(no_devices)?;
        current_measures(&state.rig, None)
            .await
            .map_err(|e| PropertyError::failed(anyhow::anyhow!(e.to_string())))
    }

    /// Autofocus's defaults now.
    ///
    /// From the fitted objective, the camera's pixel and binning, and the
    /// light on: the measure, the depth of field, and the sweep's reach and
    /// steps.
    #[property]
    async fn defaults(&self) -> Result<FocusDefaults, PropertyError> {
        let state = self.state.get().ok_or_else(no_devices)?;
        focus_defaults(&state.rig)
            .await
            .map_err(|e| PropertyError::failed(anyhow::anyhow!(e.to_string())))
    }

    /// The latest run's report.
    ///
    /// Where it left the stage, whether it found a focus or why not, and
    /// its curves.
    #[property]
    async fn last(&self) -> Result<Option<FocusReport>, PropertyError> {
        Ok(self.state.get().and_then(|s| latest(&s.last.focus)))
    }

    /// Autofocuses where the stage is: a hardware job.
    ///
    /// A coarse sweep then a fine one, each position approached moving up;
    /// or one continuous sweep (`sweep`: `continuous`). Each option is
    /// optional: `measure` (`brenner`, `tenengrad` or `laplacian`; the
    /// light's by default), `range_um` and `step_um` (6 depths of field and
    /// one by default), `region` (the frame's middle half), `approach_um`
    /// and `settle_ms`. The job's result is the z found; it fails, saying
    /// why, on an empty field or a focus out of reach.
    #[action]
    async fn run(
        &self,
        ctx: ActionCtx,
        #[param(default)] measure: Option<Measure>,
        #[param(default)] sweep: Option<Sweep>,
        #[param(default)] range_um: Option<f64>,
        #[param(default)] step_um: Option<f64>,
        #[param(default)] region: Option<Region>,
        #[param(default)] approach_um: Option<f64>,
        #[param(default)] settle_ms: Option<f64>,
    ) -> Result<JobRef, ActionError> {
        let params = given(&[
            ("measure", measure.map(|v| json!(v))),
            ("sweep", sweep.map(|v| json!(v))),
            ("range_um", range_um.map(|v| json!(v))),
            ("step_um", step_um.map(|v| json!(v))),
            ("region", region.map(|v| json!(v))),
            ("approach_um", approach_um.map(|v| json!(v))),
            ("settle_ms", settle_ms.map(|v| json!(v))),
        ]);
        self.state()?.submit(&ctx, AUTOFOCUS, params)
    }
}

/// Exposure: auto exposure, and auto white balance.
#[derive(Thing)]
#[thing(
    title = "Exposure",
    semantic_type = "saref:Function",
    context(saref = "https://saref.etsi.org/core/")
)]
pub struct ExposureThing {
    state: OnceLock<Focus>,
}

impl ExposureThing {
    fn state(&self) -> Result<&Focus, ActionError> {
        self.state.get().ok_or_else(unavailable)
    }
}

#[thing_impl]
impl ExposureThing {
    #[on_start]
    async fn connect(&self, ctx: ThingCtx) -> anyhow::Result<()> {
        if let Some(state) = connect(&ctx, false, &[AUTO_EXPOSURE, WHITE_BALANCE]).await? {
            let _ = self.state.set(state);
        }
        Ok(())
    }

    /// The latest auto exposure's report.
    ///
    /// The exposure it left, whether it converged or why not, and each
    /// exposure it tried.
    #[property]
    async fn last(&self) -> Result<Option<ExposureReport>, PropertyError> {
        Ok(self.state.get().and_then(|s| latest(&s.last.exposure)))
    }

    /// The latest white balance's report.
    ///
    /// The gains, (G / R, 1, G / B), for a client to give the preview's
    /// look, and the white they came from.
    #[property]
    async fn last_white_balance(&self) -> Result<Option<WhiteBalanceReport>, PropertyError> {
        Ok(self.state.get().and_then(|s| latest(&s.last.white)))
    }

    /// Sets the exposure: a hardware job.
    ///
    /// Until the region's `percentile` (99.5 by default) sits at the
    /// `target` (0.75 of full scale) within `tolerance` (5 %), in at most
    /// `steps` (10), within the camera's limits. Each option is optional,
    /// as are `region` and `settle_ms`. The job fails, saying why, if it
    /// doesn't converge; it leaves the best exposure it tried.
    #[action]
    async fn auto(
        &self,
        ctx: ActionCtx,
        #[param(default)] target: Option<f64>,
        #[param(default)] percentile: Option<f64>,
        #[param(default)] tolerance: Option<f64>,
        #[param(default)] steps: Option<u32>,
        #[param(default)] region: Option<Region>,
        #[param(default)] settle_ms: Option<f64>,
    ) -> Result<JobRef, ActionError> {
        let params = given(&[
            ("target", target.map(|v| json!(v))),
            ("percentile", percentile.map(|v| json!(v))),
            ("tolerance", tolerance.map(|v| json!(v))),
            ("steps", steps.map(|v| json!(v))),
            ("region", region.map(|v| json!(v))),
            ("settle_ms", settle_ms.map(|v| json!(v))),
        ]);
        self.state()?.submit(&ctx, AUTO_EXPOSURE, params)
    }

    /// Finds the white balance: a hardware job.
    ///
    /// The gains that make the light's white grey, from the region's
    /// lightest part. Its result is the gains, also in
    /// `last_white_balance`; the preview takes them when a client sets its
    /// look's `gains`.
    #[action]
    async fn white_balance(
        &self,
        ctx: ActionCtx,
        #[param(default)] region: Option<Region>,
        #[param(default)] settle_ms: Option<f64>,
    ) -> Result<JobRef, ActionError> {
        let params = given(&[
            ("region", region.map(|v| json!(v))),
            ("settle_ms", settle_ms.map(|v| json!(v))),
        ]);
        self.state()?.submit(&ctx, WHITE_BALANCE, params)
    }
}
