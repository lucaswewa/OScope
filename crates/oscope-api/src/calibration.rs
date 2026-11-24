//! The `calibration` Thing (plan §5.13; P32; ADR-0023): the calibration
//! sets, and the jobs that measure the geometry, the stage and the objective.

use std::collections::HashMap;
use std::io::ErrorKind;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

use oscope_acquire::preview::{Calibrated, Calibrating, Calibration as Found, Wanted};
use oscope_compute::Image;
use oscope_core::FrameMeta;
use oscope_imaging::color::ColourCorrection;
use oscope_jobs::{JobService, JobState};
use oscope_workflows::calibration::{
    CHECK, COLOUR, CalibrationSet, CalibrationStore, CheckReport, DARK, FLAT, GEOMETRY, LastCheck,
    Rig, STAGE, STAGE_KEY, SetSummary, StepKind, StoreError, Values, Wizard, calibration_jobs,
    colour_key, dark_key, flat_key, geometry_key,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use teta_wot::http::PathParams;
use teta_wot::http::axum::Json;
use teta_wot::http::axum::http::StatusCode;
use teta_wot::http::axum::response::{IntoResponse, Response};
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
    pub(crate) fn open(&self, roots: &Roots) -> Result<Arc<CalibrationStore>, StoreError> {
        let mut store = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(store) = store.as_ref() {
            return Ok(Arc::clone(store));
        }
        let opened = Arc::new(CalibrationStore::open(roots.data.join("calibration"))?);
        *store = Some(Arc::clone(&opened));
        Ok(opened)
    }
}

/// The lights on, as a flat field's and a colour calibration's key names
/// them: their channels, joined by `+`; none if none is.
fn lit(meta: &FrameMeta) -> Option<String> {
    let on: Vec<&str> = meta
        .lights
        .iter()
        .filter(|l| l.on && l.intensity > 0.0)
        .map(|l| l.channel.as_str())
        .collect();
    (!on.is_empty()).then(|| on.join("+"))
}

/// The longest side of a set's image as it is sent to be seen.
const SEEN: u32 = 256;

/// `image` in blocks of `block` × `block` pixels, each channel's mean, so
/// that neither side is longer than `most`: the blocks across and down, and
/// their values by rows, each block's channels together. A block at an edge
/// is the mean of the pixels it has.
pub(crate) fn blocks(image: &Image, most: u32) -> (u32, u32, u32, Vec<f32>) {
    let (width, height, channels) = (image.width(), image.height(), image.channels());
    let block = width.max(height).div_ceil(most.max(1)).max(1);
    let (across, down) = (width.div_ceil(block), height.div_ceil(block));
    let mut values = Vec::with_capacity((across * down) as usize * channels);
    for by in 0..down {
        for bx in 0..across {
            let (xs, ys) = (
                bx * block..((bx + 1) * block).min(width),
                by * block..((by + 1) * block).min(height),
            );
            let n = (xs.len() * ys.len()) as f64;
            for c in 0..channels {
                let mut sum = 0.0;
                for y in ys.clone() {
                    for x in xs.clone() {
                        sum += f64::from(image.data()[(y * width + x) as usize * channels + c]);
                    }
                }
                values.push((sum / n) as f32);
            }
        }
    }
    (across, down, block, values)
}

/// A problem's answer: its status, and why.
fn problem(status: StatusCode, detail: String) -> Response {
    let body =
        json!({"title": status.canonical_reason(), "status": status.as_u16(), "detail": detail});
    (status, Json(body)).into_response()
}

/// How the preview finds a frame's calibration (W8.2): the active sets for
/// its mode, gain and exposure, its objective, lights and binning, and its
/// lights, keyed as the calibration keyed them. A set never changes, so
/// each image is read once, by its set's ID.
pub(crate) fn calibrating(store: Arc<CalibrationStore>) -> Calibrating {
    let images: Mutex<HashMap<Uuid, Option<Arc<Calibrated>>>> = Mutex::default();
    Calibrating(Arc::new(move |meta: &FrameMeta, wanted: Wanted| {
        let image = |key: Option<String>| {
            let set = store.active(&key?)?;
            let region = match &set.values {
                Values::Dark(dark) => dark.region,
                Values::Flat(flat) => flat.region,
                _ => return None,
            };
            let mut images = images.lock().unwrap_or_else(PoisonError::into_inner);
            let read = || store.image(set.id).ok();
            let made = |image| {
                Arc::new(Calibrated {
                    image,
                    x: region.x,
                    y: region.y,
                })
            };
            images
                .entry(set.id)
                .or_insert_with(|| read().map(made))
                .clone()
        };
        let light = lit(meta);
        let dark = wanted
            .dark
            .then(|| image(Some(dark_key(&meta.mode, meta.gain, meta.exposure.0))));
        let flat_of =
            |objective: &String, light: &String| flat_key(objective, light, meta.binning.max(1));
        let flat = wanted.flat.then(|| {
            image(
                meta.objective
                    .as_ref()
                    .zip(light.as_ref())
                    .map(|(o, l)| flat_of(o, l)),
            )
        });
        let colour = wanted.colour.then(|| {
            let set = store.active(&colour_key(light.as_ref()?))?;
            match set.values {
                Values::Colour(colour) => Some(ColourCorrection {
                    gains: colour.gains.map(|g| g as f32),
                    matrix: colour.matrix.map(|row| row.map(|v| v as f32)),
                }),
                _ => None,
            }
        });
        Found {
            dark: dark.flatten(),
            flat: flat.flatten(),
            colour: colour.flatten(),
        }
    }))
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

fn store_error(error: StoreError) -> ActionError {
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

    /// A dark frame's or a flat field's image, to see (W8.2).
    ///
    /// The mean of each block of pixels, so that neither side is longer
    /// than 256: `width` and `height`, the blocks across and down;
    /// `channels`; `block`, the pixels a block spans; and `values`, by rows,
    /// each block's channels together, in linear units.
    #[endpoint(get, "image/{id}")]
    async fn image(&self, params: PathParams) -> Response {
        let Some(state) = self.state.get() else {
            return problem(
                StatusCode::SERVICE_UNAVAILABLE,
                "the Thing hasn't started".into(),
            );
        };
        let text = params.get("id").unwrap_or_default();
        let Ok(id) = text.parse::<Uuid>() else {
            return problem(
                StatusCode::BAD_REQUEST,
                format!("{text:?} isn't a set's ID"),
            );
        };
        let store = Arc::clone(&state.store);
        let read = tokio::task::spawn_blocking(move || store.image(id)).await;
        match read {
            Ok(Ok(image)) => {
                let (width, height, block, values) = blocks(&image, SEEN);
                let body = json!({
                    "width": width, "height": height, "channels": image.channels(),
                    "block": block, "values": values,
                });
                Json(body).into_response()
            }
            // A set that isn't there, or isn't a dark frame or a flat field.
            Ok(Err(StoreError::Io { source, .. })) if source.kind() == ErrorKind::NotFound => {
                problem(StatusCode::NOT_FOUND, format!("no set {id} has an image"))
            }
            Ok(Err(error)) => problem(StatusCode::INTERNAL_SERVER_ERROR, error.to_string()),
            Err(error) => problem(StatusCode::INTERNAL_SERVER_ERROR, error.to_string()),
        }
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

#[cfg(test)]
mod tests {
    //! The preview's calibration found as the routines keyed it (W8.2), and
    //! a set's image in blocks.

    use oscope_compute::Shape;
    use oscope_core::{FrameId, LightState, Ms, PixelFormat, PixelRect, Timestamp};
    use oscope_workflows::calibration::{Colour, Dark, Flat, Instrument, Region, exposure_octave};

    use super::*;

    fn instrument() -> Instrument {
        Instrument {
            camera: "a camera".to_owned(),
            sensor_pixel_um: 2.4,
            binning: 1,
            stage: "a stage".to_owned(),
            objective: None,
            tube_lens_um: 200_000.0,
        }
    }

    const REGION: Region = Region {
        x: 12,
        y: 24,
        width: 4,
        height: 2,
    };

    fn image(value: f32) -> Image {
        Image::from_vec(Shape::new(4, 2, 1), vec![value; 8]).unwrap()
    }

    /// A store with a dark frame for `full` at gain 1 and 10 ms, a flat field
    /// for 10x under `blue 488` at binning 1, and a colour calibration for
    /// that light, each active.
    fn stored(folder: &std::path::Path) -> Arc<CalibrationStore> {
        let store = Arc::new(CalibrationStore::open(folder).unwrap());
        let dark = Values::Dark(Dark {
            mode: "full".to_owned(),
            gain: 1.0,
            exposure_ms: exposure_octave(10.0),
            taken_ms: 10.0,
            region: REGION,
            frames: 4,
            mean: 0.05,
            spread: 0.001,
        });
        store
            .save_with_image(dark, instrument(), None, true, &image(0.05))
            .unwrap();
        let flat = Values::Flat(Flat {
            objective: "10x".to_owned(),
            light: "blue 488".to_owned(),
            mode: "full".to_owned(),
            binning: 1,
            region: REGION,
            frames: 4,
            dark: true,
            gains: [1.2, 1.0, 1.4],
            uneven: 0.1,
        });
        store
            .save_with_image(flat, instrument(), None, true, &image(0.9))
            .unwrap();
        let colour = Values::Colour(Colour {
            light: "blue 488".to_owned(),
            gains: [1.5, 1.0, 2.0],
            matrix: [[1.2, -0.1, -0.1], [0.0, 1.0, 0.0], [0.0, -0.2, 1.2]],
            patches: 24,
            delta_e_before: 6.0,
            delta_e_mean: 2.0,
            delta_e_max: 4.0,
        });
        store.save(colour, instrument(), None, true).unwrap();
        store
    }

    /// A frame's metadata: `full`, gain 1, `exposure` ms, 10x, `lights` on.
    fn meta(gain: f64, exposure: f64, lights: &[&str]) -> FrameMeta {
        let mut meta = FrameMeta::new(FrameId(1), PixelFormat::Mono8, 4, 2, Timestamp(0));
        meta.mode = "full".to_owned();
        meta.gain = gain;
        meta.exposure = Ms(exposure);
        meta.objective = Some("10x".to_owned());
        meta.binning = 1;
        meta.roi = PixelRect {
            x: 12,
            y: 24,
            width: 4,
            height: 2,
        };
        meta.lights = ["transmitted", "blue 488"]
            .into_iter()
            .map(|channel| LightState {
                channel: channel.to_owned(),
                on: lights.contains(&channel),
                intensity: 1.0,
            })
            .collect();
        meta
    }

    const ALL: Wanted = Wanted {
        dark: true,
        flat: true,
        colour: true,
    };

    #[test]
    fn a_frame_finds_the_sets_its_mode_gain_exposure_objective_and_light_name() {
        let folder = tempfile::tempdir().unwrap();
        let find = calibrating(stored(folder.path()));
        // 12 ms is in the dark frame's octave, 8 to 16 ms.
        let found = (find.0)(&meta(1.0, 12.0, &["blue 488"]), ALL);
        let dark = found.dark.expect("the dark frame");
        assert_eq!((dark.x, dark.y, dark.image.data()[0]), (12, 24, 0.05));
        assert_eq!(found.flat.expect("the flat field").image.data()[0], 0.9);
        let colour = found.colour.expect("the colour calibration");
        assert_eq!(colour.gains, [1.5, 1.0, 2.0]);
        assert_eq!(colour.matrix[2], [0.0, -0.2, 1.2]);
        // Only what is asked for.
        let dark_only = (find.0)(
            &meta(1.0, 12.0, &["blue 488"]),
            Wanted {
                dark: true,
                ..Wanted::default()
            },
        );
        assert!(dark_only.dark.is_some() && dark_only.flat.is_none() && dark_only.colour.is_none());
        // Another gain, exposure or light: none of its sets.
        let other_gain = (find.0)(&meta(2.0, 12.0, &["blue 488"]), ALL);
        assert!(other_gain.dark.is_none() && other_gain.flat.is_some());
        assert!(
            (find.0)(&meta(1.0, 40.0, &["blue 488"]), ALL)
                .dark
                .is_none()
        );
        let other_light = (find.0)(&meta(1.0, 12.0, &["transmitted"]), ALL);
        assert!(other_light.flat.is_none() && other_light.colour.is_none());
        let unlit = (find.0)(&meta(1.0, 12.0, &[]), ALL);
        assert!(unlit.flat.is_none() && unlit.colour.is_none() && unlit.dark.is_some());
    }

    #[test]
    fn a_set_s_image_is_sent_in_blocks_each_its_pixels_mean() {
        // 5 × 3, one channel, each pixel its x + 10 y: in blocks of 2 at most 3 across.
        let data = (0..15).map(|i| (i % 5 + 10 * (i / 5)) as f32).collect();
        let image = Image::from_vec(Shape::new(5, 3, 1), data).unwrap();
        let (across, down, block, values) = blocks(&image, 3);
        assert_eq!((across, down, block), (3, 2, 2));
        // The first block: 0, 1, 10, 11; the last, at the corner, 24 alone.
        assert_eq!(values, vec![5.5, 7.5, 9.0, 20.5, 22.5, 24.0]);
        // Small enough already: a block of one.
        let (across, down, block, same) = blocks(&image, 256);
        assert_eq!((across, down, block, same.len()), (5, 3, 1, 15));
    }
}
