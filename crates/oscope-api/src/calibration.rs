//! The `calibration` Thing (plan §5.13; P32; ADR-0023): the calibration
//! sets, and the jobs that measure the geometry, the stage and the objective.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

use oscope_jobs::{JobService, JobState};
use oscope_workflows::calibration::{
    CHECK, COLOUR, CalibrationSet, CalibrationStore, CheckReport, DARK, FLAT, GEOMETRY, LastCheck,
    Rig, STAGE, STAGE_KEY, SetSummary, StepKind, Wizard, calibration_jobs, colour_key, dark_key,
    flat_key, geometry_key,
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
    /// The dark frame for the camera's mode, gain and exposure (P33).
    pub dark: Option<CalibrationSet>,
    /// The flat field for the fitted objective, the lights on and the
    /// camera's binning.
    pub flat: Option<CalibrationSet>,
    /// The colour calibration for the lights on.
    pub colour: Option<CalibrationSet>,
}

struct Calibration {
    devices: Arc<Devices>,
    store: Arc<CalibrationStore>,
    jobs: Option<Arc<JobService>>,
    last: LastCheck,
    /// The wizard, and where it is saved.
    wizard: Mutex<Option<Wizard>>,
    wizard_file: PathBuf,
}

impl Calibration {
    /// The wizard's running step, settled if its job has ended: done with
    /// what it made, or failed with why. The objective's step is done only
    /// if the check found the objective plausible.
    fn settle(&self, wizard: &mut Wizard) {
        let (Some(job), Some(jobs)) = (wizard.running(), &self.jobs) else {
            return;
        };
        let outcome = match jobs.get(job) {
            Err(_) => Err("its job was lost, as the server restarted: retry it".to_owned()),
            Ok(info) => match info.state {
                JobState::Queued | JobState::Running | JobState::Paused => return,
                JobState::Completed => match wizard.step().map(|s| s.kind) {
                    Some(StepKind::Objective) => self.verdict(),
                    _ => Ok(info.results.first().cloned().unwrap_or_default()),
                },
                state => Err(info
                    .error
                    .unwrap_or_else(|| format!("its job ended {state:?}"))),
            },
        };
        wizard.ended(job, outcome);
        if let Err(error) = wizard.save(&self.wizard_file) {
            tracing::warn!("the calibration wizard wasn't saved: {error}");
        }
    }

    /// The latest objective check's verdict.
    fn verdict(&self) -> Result<String, String> {
        let last = self
            .last
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        let Some(check) = last else {
            return Err("the check left no report".to_owned());
        };
        match check.plausible {
            true => Ok(format!("plausible: the {}", check.objective)),
            false => Err(format!(
                "the {} is fitted, but the image moves as through the {}: fit the right one, \
                 then retry",
                check.objective, check.nearest
            )),
        }
    }
}

/// The calibration store, which the `calibration` and `scan` Things share:
/// opened once, by whichever starts first, so each sees what the other
/// saves.
#[derive(Default)]
pub(crate) struct Calibrations(Mutex<Option<Arc<CalibrationStore>>>);

impl Calibrations {
    /// The store in the data root's `calibration` folder.
    pub(crate) fn open(
        &self,
        roots: &Roots,
    ) -> Result<Arc<CalibrationStore>, oscope_workflows::calibration::StoreError> {
        let mut store = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(store) = store.as_ref() {
            return Ok(Arc::clone(store));
        }
        let opened = Arc::new(CalibrationStore::open(roots.data.join("calibration"))?);
        *store = Some(Arc::clone(&opened));
        Ok(opened)
    }
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

/// A photometric job's parameters: those given.
fn frames_params(frames: Option<u32>, settle_ms: Option<f64>, activate: Option<bool>) -> Value {
    given(&[
        ("frames", frames.map(|v| json!(v))),
        ("settle_ms", settle_ms.map(|v| json!(v))),
        ("activate", activate.map(|v| json!(v))),
    ])
}

impl CalibrationThing {
    /// `change` on the wizard, its step settled first, saved after.
    fn with_wizard<T>(
        &self,
        change: impl FnOnce(&mut Wizard) -> Result<T, ActionError>,
    ) -> Result<T, ActionError> {
        let state = self.state()?;
        let mut held = state.wizard.lock().unwrap_or_else(PoisonError::into_inner);
        let wizard = held.as_mut().ok_or_else(|| {
            ActionError::handled("not found (404): no calibration wizard: start one")
        })?;
        state.settle(wizard);
        let out = change(wizard)?;
        wizard
            .save(&state.wizard_file)
            .map_err(|e| ActionError::handled(e.to_string()))?;
        Ok(out)
    }

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
        let found = (
            server.service::<Devices>(),
            server.service::<Roots>(),
            server.service::<Calibrations>(),
        );
        let (Some(devices), Some(roots), Some(calibrations)) = found else {
            return Ok(());
        };
        let store = calibrations.open(&roots)?;
        let jobs = server.service::<JobService>();
        let last = LastCheck::default();
        if let Some(jobs) = &jobs {
            let rig = Rig {
                stage: Arc::clone(&devices.stage),
                camera: Arc::clone(&devices.camera),
                objectives: Arc::clone(&devices.objectives),
                bus: Arc::clone(devices.acquisition.bus()),
                store: Arc::clone(&store),
                lights: Arc::clone(&devices.lights),
            };
            for (kind, factory) in calibration_jobs(rig, Arc::clone(&last)) {
                jobs.register(kind, factory);
            }
        }
        let wizard_file = store.wizard_file();
        // A wizard that can't be read is set aside: the next one replaces it.
        let wizard = Wizard::load(&wizard_file).unwrap_or_else(|error| {
            tracing::warn!("the calibration wizard saved wasn't read, and is set aside: {error}");
            None
        });
        let state = Calibration {
            devices,
            store,
            jobs,
            last,
            wizard: Mutex::new(wizard),
            wizard_file,
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
                dark: None,
                flat: None,
                colour: None,
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
        let lights = devices.lights.state().await.map_err(property_error)?;
        let on: Vec<String> = lights
            .into_iter()
            .filter(|l| l.on && l.intensity > 0.0)
            .map(|l| l.channel)
            .collect();
        let light = on.join("+");
        let store = &state.store;
        let geometry = fitted
            .as_ref()
            .and_then(|o| store.active(&geometry_key(&o.name, binning)));
        let flat = fitted
            .as_ref()
            .and_then(|o| store.active(&flat_key(&o.name, &light, binning)));
        let dark = store.active(&dark_key(
            &settings.mode,
            settings.gain,
            settings.exposure.0,
        ));
        Ok(ActiveSets {
            geometry,
            stage: store.active(STAGE_KEY),
            dark,
            flat,
            colour: store.active(&colour_key(&light)),
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

    /// The calibration wizard.
    ///
    /// Its steps, each with what to do before confirming it, and where each
    /// stands: the step in hand settled first if its job has ended.
    #[property]
    async fn wizard(&self) -> Result<Option<Wizard>, PropertyError> {
        let Some(state) = self.state.get() else {
            return Ok(None);
        };
        let mut wizard = state.wizard.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(wizard) = wizard.as_mut() {
            state.settle(wizard);
        }
        Ok(wizard.clone())
    }

    /// Starts the calibration wizard.
    ///
    /// Its `steps`, in order: every one if not given (`objective`,
    /// `geometry`, `dark`, `flat`, `colour`, `stage`). Any wizard before is
    /// set aside.
    #[action]
    async fn wizard_start(
        &self,
        #[param(default)] steps: Option<Vec<StepKind>>,
    ) -> Result<Wizard, ActionError> {
        let state = self.state()?;
        let steps = steps.unwrap_or_else(|| StepKind::ALL.to_vec());
        let wizard = Wizard::new(&steps).map_err(|e| ActionError::handled(e.to_string()))?;
        wizard
            .save(&state.wizard_file)
            .map_err(|e| ActionError::handled(e.to_string()))?;
        *state.wizard.lock().unwrap_or_else(PoisonError::into_inner) = Some(wizard.clone());
        Ok(wizard)
    }

    /// Confirms the wizard's step: its prompt is done, so its job starts.
    #[action]
    async fn wizard_confirm(&self, ctx: ActionCtx) -> Result<Wizard, ActionError> {
        let kind = self.with_wizard(|wizard| match wizard.step() {
            Some(step) => Ok(step.kind),
            None => Err(ActionError::handled(
                "conflict (409): the wizard is finished",
            )),
        })?;
        let job = self.submit(&ctx, kind.job(), json!({}))?;
        self.with_wizard(|wizard| {
            wizard
                .confirm(job.id)
                .map_err(|e| ActionError::handled(e.to_string()))?;
            Ok(wizard.clone())
        })
    }

    /// Skips the wizard's step, prompting or failed.
    #[action]
    async fn wizard_skip(&self) -> Result<Wizard, ActionError> {
        self.with_wizard(|wizard| {
            wizard
                .skip()
                .map_err(|e| ActionError::handled(e.to_string()))?;
            Ok(wizard.clone())
        })
    }

    /// Retries the wizard's failed step: it prompts again.
    #[action]
    async fn wizard_retry(&self) -> Result<Wizard, ActionError> {
        self.with_wizard(|wizard| {
            wizard
                .retry()
                .map_err(|e| ActionError::handled(e.to_string()))?;
            Ok(wizard.clone())
        })
    }

    /// Cancels the wizard where it is.
    #[action]
    async fn wizard_cancel(&self) -> Result<Wizard, ActionError> {
        self.with_wizard(|wizard| {
            wizard
                .cancel()
                .map_err(|e| ActionError::handled(e.to_string()))?;
            Ok(wizard.clone())
        })
    }

    /// Takes a dark frame.
    ///
    /// The lights are turned off, `frames` frames averaged (16) from
    /// `settle_ms` on (50), and the lights turned back on: for the camera's
    /// mode, gain and octave of exposure. The set is made active unless
    /// `activate` is false.
    ///
    /// A job on the instrument: its result is the set, `calibration/<id>`.
    #[action]
    async fn dark(
        &self,
        ctx: ActionCtx,
        #[param(default)] frames: Option<u32>,
        #[param(default)] settle_ms: Option<f64>,
        #[param(default)] activate: Option<bool>,
    ) -> Result<JobRef, ActionError> {
        self.submit(&ctx, DARK, frames_params(frames, settle_ms, activate))
    }

    /// Takes a flat field.
    ///
    /// At a blank part of the slide, through the fitted objective, in the
    /// lights that are on: `frames` frames averaged (16), the dark taken
    /// away, each colour scaled to a mean of 1; with the white balance the
    /// blank field gives. Refused if the field isn't blank.
    ///
    /// A job on the instrument: its result is the set, `calibration/<id>`.
    #[action]
    async fn flat(
        &self,
        ctx: ActionCtx,
        #[param(default)] frames: Option<u32>,
        #[param(default)] settle_ms: Option<f64>,
        #[param(default)] activate: Option<bool>,
    ) -> Result<JobRef, ActionError> {
        self.submit(&ctx, FLAT, frames_params(frames, settle_ms, activate))
    }

    /// Calibrates colour.
    ///
    /// From OScope's colour target, centred in the field of the 5× or the
    /// 10×: its 24 patches measured through the active geometry, the dark
    /// and the flat field taken out, white-balanced on its lightest grey,
    /// and the colour matrix fitted. Refused if the colours fit poorly.
    ///
    /// A job on the instrument: its result is the set, `calibration/<id>`.
    #[action]
    async fn colour(
        &self,
        ctx: ActionCtx,
        #[param(default)] frames: Option<u32>,
        #[param(default)] settle_ms: Option<f64>,
        #[param(default)] activate: Option<bool>,
    ) -> Result<JobRef, ActionError> {
        self.submit(&ctx, COLOUR, frames_params(frames, settle_ms, activate))
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
