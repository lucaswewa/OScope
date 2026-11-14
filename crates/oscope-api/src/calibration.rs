//! The `calibration` Thing (plan §5.13; P32; ADR-0023): the calibration
//! sets, and the jobs that measure the geometry, the stage and the objective.

use std::sync::{Arc, OnceLock, PoisonError};

use oscope_jobs::JobService;
use oscope_workflows::calibration::{
    CHECK, CalibrationSet, CalibrationStore, CheckReport, GEOMETRY, LastCheck, Rig, STAGE,
    STAGE_KEY, SetSummary, calibration_jobs, geometry_key,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use teta_wot::prelude::*;
use uuid::Uuid;

use crate::devices::{Devices, property_error};
use crate::jobs::{JobRef, action_error};
use crate::server::Roots;

/// The sets the instrument uses now.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ActiveSets {
    /// The fitted objective's geometry at the camera's binning, if one is
    /// active.
    pub geometry: Option<CalibrationSet>,
    /// The stage's, if one is active.
    pub stage: Option<CalibrationSet>,
}

struct Calibration {
    devices: Arc<Devices>,
    store: Arc<CalibrationStore>,
    jobs: Option<Arc<JobService>>,
    last: LastCheck,
}

/// The calibration sets, kept and versioned (ADR-0023), and the jobs that
/// measure them.
#[derive(Thing)]
#[thing(
    title = "Calibration",
    semantic_type = "saref:Function",
    context(saref = "https://saref.etsi.org/core/")
)]
pub struct CalibrationThing {
    state: OnceLock<Calibration>,
}

fn store_error(error: oscope_workflows::calibration::StoreError) -> ActionError {
    ActionError::handled(error.to_string())
}

fn unavailable() -> ActionError {
    ActionError::handled("unavailable (503): the server has no devices to calibrate")
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

impl CalibrationThing {
    fn state(&self) -> Result<&Calibration, ActionError> {
        self.state.get().ok_or_else(unavailable)
    }

    fn submit(&self, ctx: &ActionCtx, kind: &str, params: Value) -> Result<JobRef, ActionError> {
        let jobs =
            self.state()?.jobs.as_ref().ok_or_else(|| {
                ActionError::handled("unavailable (503): the server runs no jobs")
            })?;
        let account = ctx.principal().id().map(str::to_owned);
        let id = jobs
            .submit_kind(kind, params, account)
            .map_err(action_error)?;
        Ok(JobRef::new(id))
    }
}

#[thing_impl]
impl CalibrationThing {
    #[on_start]
    async fn connect(&self, ctx: ThingCtx) -> anyhow::Result<()> {
        let server = ctx.server();
        let (Some(devices), Some(roots)) = (server.service::<Devices>(), server.service::<Roots>())
        else {
            return Ok(());
        };
        let store = Arc::new(CalibrationStore::open(roots.data.join("calibration"))?);
        let jobs = server.service::<JobService>();
        let last = LastCheck::default();
        if let Some(jobs) = &jobs {
            let rig = Rig {
                stage: Arc::clone(&devices.stage),
                camera: Arc::clone(&devices.camera),
                objectives: Arc::clone(&devices.objectives),
                bus: Arc::clone(devices.acquisition.bus()),
                store: Arc::clone(&store),
            };
            for (kind, factory) in calibration_jobs(rig, Arc::clone(&last)) {
                jobs.register(kind, factory);
            }
        }
        let state = Calibration {
            devices,
            store,
            jobs,
            last,
        };
        let _ = self.state.set(state);
        Ok(())
    }

    /// Every calibration set, in brief.
    ///
    /// Newest first, by when each was made (an import, when it was made
    /// where it came from), each with whether it is its key's active one.
    #[property]
    async fn sets(&self) -> Result<Vec<SetSummary>, PropertyError> {
        Ok(self.state.get().map(|s| s.store.list()).unwrap_or_default())
    }

    /// The sets in use.
    ///
    /// The fitted objective's geometry at the camera's binning, and the
    /// stage's: those calibration's users take.
    #[property]
    async fn active(&self) -> Result<ActiveSets, PropertyError> {
        let Some(state) = self.state.get() else {
            return Ok(ActiveSets {
                geometry: None,
                stage: None,
            });
        };
        let devices = &state.devices;
        let fitted = devices.objectives.fitted().await.map_err(property_error)?;
        let settings = devices.camera.settings().await.map_err(property_error)?;
        let modes = &devices.camera.capabilities().modes;
        let binning = modes
            .iter()
            .find(|m| m.name == settings.mode)
            .map_or(1, |m| m.binning);
        let geometry = fitted.and_then(|o| state.store.active(&geometry_key(&o.name, binning)));
        Ok(ActiveSets {
            geometry,
            stage: state.store.active(STAGE_KEY),
        })
    }

    /// The latest objective check.
    ///
    /// Since the server started: whether the objective fitted is the one in
    /// the light path.
    #[property]
    async fn last_check(&self) -> Result<Option<CheckReport>, PropertyError> {
        let state = self.state.get();
        let last = state.map(|s| {
            s.last
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .clone()
        });
        Ok(last.flatten())
    }

    /// Makes a set active.
    ///
    /// The set `id` becomes its key's active one, which calibration's users
    /// take.
    #[action]
    async fn activate(&self, id: Uuid) -> Result<CalibrationSet, ActionError> {
        self.state()?.store.activate(id).map_err(store_error)
    }

    /// A set, whole.
    ///
    /// The set `id`, as another instrument imports it.
    #[action]
    async fn export(&self, id: Uuid) -> Result<CalibrationSet, ActionError> {
        self.state()?.store.get(id).map_err(store_error)
    }

    /// Imports a set.
    ///
    /// A set exported elsewhere: checked, and kept as the next version of its
    /// key, not active.
    #[action]
    async fn import(&self, set: Value) -> Result<CalibrationSet, ActionError> {
        self.state()?.store.import(set).map_err(store_error)
    }

    /// Measures the geometry.
    ///
    /// The fitted objective's, at the camera's binning: a 3 × 3 grid of stage moves `step_um` apart (a quarter of the frame's
    /// middle, if not given), each frame registered, and the camera-to-stage
    /// matrix fitted. Each position is reached from `approach_um` before it
    /// (50), and its frame exposed from `settle_ms` after the stage arrives
    /// (50). The set is made active unless `activate` is false.
    ///
    /// A job on the instrument: answered at once with where to follow it.
    /// Its result is the set, `calibration/<id>`.
    #[action]
    async fn geometry(
        &self,
        ctx: ActionCtx,
        #[param(default)] step_um: Option<f64>,
        #[param(default)] approach_um: Option<f64>,
        #[param(default)] settle_ms: Option<f64>,
        #[param(default)] activate: Option<bool>,
    ) -> Result<JobRef, ActionError> {
        let params = given(&[
            ("step_um", step_um.map(|v| json!(v))),
            ("approach_um", approach_um.map(|v| json!(v))),
            ("settle_ms", settle_ms.map(|v| json!(v))),
            ("activate", activate.map(|v| json!(v))),
        ]);
        self.submit(&ctx, GEOMETRY, params)
    }

    /// Measures the stage.
    ///
    /// It homes the stage unless `home` is false, records its soft limits, and measures x's and y's backlash where it was, `cycles` times
    /// each (3). It needs the active geometry of the fitted objective.
    ///
    /// A job on the instrument: answered at once with where to follow it.
    /// Its result is the set, `calibration/<id>`.
    #[action]
    async fn stage(
        &self,
        ctx: ActionCtx,
        #[param(default)] home: Option<bool>,
        #[param(default)] cycles: Option<u32>,
        #[param(default)] approach_um: Option<f64>,
        #[param(default)] settle_ms: Option<f64>,
        #[param(default)] activate: Option<bool>,
    ) -> Result<JobRef, ActionError> {
        let params = given(&[
            ("home", home.map(|v| json!(v))),
            ("cycles", cycles.map(|v| json!(v))),
            ("approach_um", approach_um.map(|v| json!(v))),
            ("settle_ms", settle_ms.map(|v| json!(v))),
            ("activate", activate.map(|v| json!(v))),
        ]);
        self.submit(&ctx, STAGE, params)
    }

    /// Checks the objective.
    ///
    /// Whether the objective fitted is the one in the light path: two small
    /// moves measure the pixel size, whatever the camera's turn, and
    /// within 20 % of what the objective's should be, it is plausible.
    /// Otherwise the catalogue's nearest objective is named.
    ///
    /// A job on the instrument: its report becomes `last_check`.
    #[action]
    async fn check_objective(
        &self,
        ctx: ActionCtx,
        #[param(default)] approach_um: Option<f64>,
        #[param(default)] settle_ms: Option<f64>,
    ) -> Result<JobRef, ActionError> {
        let params = given(&[
            ("approach_um", approach_um.map(|v| json!(v))),
            ("settle_ms", settle_ms.map(|v| json!(v))),
        ]);
        self.submit(&ctx, CHECK, params)
    }
}
