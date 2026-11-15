//! The `scan` Thing (P36; ADR-0025): scans planned, previewed and run as
//! jobs, from the built-in patterns and the extensions' workflows.

use std::sync::{Arc, OnceLock, PoisonError};

use oscope_core::{Point2, ProjectId};
use oscope_ext::Extensions;
use oscope_jobs::JobService;
use oscope_store::{Scratch, Store};
use oscope_workflows::autofocus::FocusRig;
use oscope_workflows::calibration::{Values, geometry_key};
use oscope_workflows::scan::{
    Estimate, Field, FrameSize, LastScan, Pattern, Plan, Position, Region, SCAN, ScanOptions,
    ScanProgress, ScanRig, Stack, Timing, ZPolicy, estimate, positions, raster, scan_jobs, snake,
    tiled_stack, z_scan,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use teta_wot::prelude::*;

use crate::calibration::Calibrations;
use crate::devices::Devices;
use crate::jobs::{JobRef, action_error};
use crate::server::Roots;

/// The built-in patterns, by name.
const PATTERNS: [&str; 5] = ["raster", "snake", "tiled_stack", "z_scan", "positions"];

/// An autofocus's time in the estimates: about one at real time on the
/// simulator, through a region of the sensor (P34).
const AUTOFOCUS_S: f64 = 10.0;

/// A scan, as a client asks for it.
#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ScanRequest {
    /// `raster`, `snake`, `tiled_stack`, `z_scan` or `positions`, or an
    /// extension's workflow by name.
    pub pattern: String,
    /// Where a raster, a snake or a tiled z-stack goes.
    #[serde(default)]
    pub region: Option<Region>,
    /// Where a z-scan is taken, in µm.
    #[serde(default)]
    pub at: Option<Point2>,
    /// The positions to visit, in order.
    #[serde(default)]
    pub positions: Vec<Position>,
    /// The stack at each tile: a z-scan's and a tiled z-stack's.
    #[serde(default)]
    pub stack: Option<Stack>,
    /// How each tile's z is found: the stage's z now, if not given.
    #[serde(default)]
    pub z: Option<ZPolicy>,
    /// The share of a frame its neighbours overlap: a tenth if not given.
    #[serde(default)]
    pub overlap: Option<f64>,
    /// A workflow's parameters, as its schema describes them.
    #[serde(default)]
    pub params: Value,
}

/// A scan's plan, and what it would take.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct Preview {
    /// The plan.
    pub plan: Plan,
    /// Its estimate, at the stage's default speed, the camera's frame rate,
    /// 50 ms of settling and 10 s an autofocus.
    pub estimate: Estimate,
}

/// A pattern a scan may follow.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct PatternInfo {
    /// Its name, as a request gives it.
    pub name: String,
    /// `planner`, built in, or `workflow`, an extension's.
    pub kind: String,
    /// A workflow's parameters' JSON Schema.
    pub schema: Option<Value>,
}

struct State {
    devices: Arc<Devices>,
    roots: Arc<Roots>,
    calibrations: Arc<Calibrations>,
    extensions: Option<Arc<Extensions>>,
    jobs: Option<Arc<JobService>>,
    last: LastScan,
}

/// Scans: planned and previewed with nothing moving, and run as hardware
/// jobs that make an acquisition of a project.
#[derive(Thing)]
#[thing(
    title = "Scan",
    semantic_type = "saref:Function",
    context(saref = "https://saref.etsi.org/core/")
)]
pub struct ScanThing {
    state: OnceLock<State>,
}

/// A request that can't be (ADR-0034).
fn refused(why: impl std::fmt::Display) -> ActionError {
    ActionError::handled(format!("refused (400): {why}"))
}

fn device(error: oscope_hal::Error) -> ActionError {
    crate::devices::action_error(error)
}

impl ScanThing {
    fn state(&self) -> Result<&State, ActionError> {
        self.state
            .get()
            .ok_or_else(|| ActionError::handled("unavailable (503): the server has no devices"))
    }

    /// The field of the camera's frames through the fitted objective, by its
    /// active geometry, or its nominal pixel without one.
    async fn field(&self, overlap: f64) -> Result<Field, ActionError> {
        let state = self.state()?;
        let devices = &state.devices;
        let settings = devices.camera.settings().await.map_err(device)?;
        let capabilities = devices.camera.capabilities();
        let (_, width, height) = capabilities.frame_shape(&settings).map_err(device)?;
        let objective = devices.objectives.fitted().await.map_err(device)?;
        let objective =
            objective.ok_or_else(|| refused("no objective is fitted: fit one first"))?;
        let binning = capabilities
            .mode(&settings.mode)
            .map_err(device)?
            .binning
            .max(1);
        let calibrations = state.calibrations.open(&state.roots).map_err(refused)?;
        let active = calibrations.active(&geometry_key(&objective.name, binning));
        let field = match active.map(|set| set.values) {
            Some(Values::Geometry(g)) => Field::new(g.matrix, width, height, overlap),
            _ => {
                let catalogue = devices.objectives.catalogue();
                let pixel = catalogue.pixel_size(&objective, capabilities.pixel_size).0;
                Field::nominal(pixel * f64::from(binning), width, height, overlap)
            }
        };
        field.map_err(refused)
    }

    /// The plan `request` asks for.
    async fn plan_of(&self, request: &ScanRequest) -> Result<Plan, ActionError> {
        let state = self.state()?;
        let field = self.field(request.overlap.unwrap_or(0.1)).await?;
        let z = match &request.z {
            Some(z) => z.clone(),
            None => {
                let here = state.devices.stage.position().await.map_err(device)?;
                ZPolicy::Fixed {
                    z_um: here.position.z.0,
                }
            }
        };
        let region = || {
            request
                .region
                .as_ref()
                .ok_or_else(|| refused(format!("a {} needs a region", request.pattern)))
        };
        let stack = || {
            request
                .stack
                .ok_or_else(|| refused(format!("a {} needs a stack", request.pattern)))
        };
        let planned = match request.pattern.as_str() {
            "raster" => raster(region()?, &field, z),
            "snake" => snake(region()?, &field, z),
            "tiled_stack" => tiled_stack(region()?, &field, z, stack()?),
            "z_scan" => {
                let at = request
                    .at
                    .ok_or_else(|| refused("a z_scan needs where: `at`"))?;
                z_scan(at, &field, z, stack()?)
            }
            "positions" => positions(&request.positions, &field, z, request.stack),
            name => {
                let tiles = self.workflow(name, &request.params, &field).await?;
                let plan = positions(&tiles, &field, z, request.stack).map_err(refused)?;
                return Ok(Plan {
                    pattern: Pattern::Workflow,
                    ..plan
                });
            }
        };
        planned.map_err(refused)
    }

    /// The tiles the extensions' workflow `name` plans.
    async fn workflow(
        &self,
        name: &str,
        params: &Value,
        field: &Field,
    ) -> Result<Vec<Position>, ActionError> {
        let extensions = self.state()?.extensions.clone();
        let Some(extensions) = extensions else {
            return Err(refused(format!(
                "no pattern {name}: the patterns are {PATTERNS:?}, and the server loads no \
                 extensions"
            )));
        };
        let (name, params) = (name.to_owned(), params.clone());
        let field = serde_json::to_value(field).map_err(refused)?;
        let planned =
            tokio::task::spawn_blocking(move || extensions.workflows().plan(&name, params, field));
        let tiles = planned.await.map_err(refused)?.map_err(refused)?;
        serde_json::from_value(tiles).map_err(|e| refused(format!("the workflow's tiles: {e}")))
    }

    /// What `plan` takes, at the instrument's speeds.
    async fn estimate_of(&self, plan: &Plan) -> Result<Estimate, ActionError> {
        let devices = &self.state()?.devices;
        let settings = devices.camera.settings().await.map_err(device)?;
        let capabilities = devices.camera.capabilities();
        let (_, width, height) = capabilities.frame_shape(&settings).map_err(device)?;
        let fps = capabilities.mode(&settings.mode).map_err(device)?;
        let interval = fps.fps(settings.format).map_or(0.0, |fps| 1e3 / fps);
        let timing = Timing {
            speed_um_s: devices.stage.capabilities().default_speed.0,
            settle_ms: ScanOptions::default().settle_ms,
            frame_ms: settings.exposure.0.max(interval),
            autofocus_s: AUTOFOCUS_S,
        };
        let bytes = settings.format.frame_bytes(width, height) / (width * height).max(1) as usize;
        let frame = FrameSize {
            width,
            height,
            bytes_per_pixel: bytes as u32,
        };
        estimate(plan, &timing, &frame).map_err(refused)
    }
}

#[thing_impl]
impl ScanThing {
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
        let jobs = server.service::<JobService>();
        let last = LastScan::default();
        let places = (server.service::<Store>(), server.service::<Scratch>());
        if let (Some(jobs), (Some(store), Some(scratch))) = (&jobs, places) {
            let focus = FocusRig {
                stage: Arc::clone(&devices.stage),
                camera: Arc::clone(&devices.camera),
                objectives: Arc::clone(&devices.objectives),
                bus: Arc::clone(devices.acquisition.bus()),
                lights: Arc::clone(&devices.lights),
                compute: None,
            };
            let rig = ScanRig {
                focus,
                store,
                scratch,
            };
            jobs.register(SCAN, scan_jobs(rig, Arc::clone(&last)));
        }
        let state = State {
            devices,
            roots,
            calibrations,
            extensions: server.service::<Extensions>(),
            jobs,
            last,
        };
        let _ = self.state.set(state);
        Ok(())
    }

    /// The patterns a scan may follow.
    ///
    /// The built-in planners, and the extensions' workflows, each with its
    /// parameters' JSON Schema.
    #[property]
    async fn patterns(&self) -> Result<Vec<PatternInfo>, PropertyError> {
        let mut patterns: Vec<PatternInfo> = PATTERNS
            .iter()
            .map(|name| PatternInfo {
                name: (*name).to_owned(),
                kind: "planner".to_owned(),
                schema: None,
            })
            .collect();
        let extensions = self.state.get().and_then(|s| s.extensions.clone());
        if let Some(extensions) = extensions {
            let listed = tokio::task::spawn_blocking(move || {
                let workflows = extensions.workflows();
                let names = workflows.names().into_iter();
                names
                    .map(|name| {
                        let schema = workflows.schema(&name).ok();
                        PatternInfo {
                            name,
                            kind: "workflow".to_owned(),
                            schema,
                        }
                    })
                    .collect::<Vec<_>>()
            });
            let listed = listed.await.map_err(|e| anyhow::anyhow!(e.to_string()));
            patterns.extend(listed.map_err(PropertyError::failed)?);
        }
        Ok(patterns)
    }

    /// The latest scan's progress.
    ///
    /// Its plan, the tiles done, each where it was taken, and how it stands.
    /// None before the first.
    #[property]
    async fn progress(&self) -> Result<Option<ScanProgress>, PropertyError> {
        let Some(state) = self.state.get() else {
            return Ok(None);
        };
        Ok(state
            .last
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone())
    }

    /// Plans a scan.
    ///
    /// Its plan and what it would take, as `request` asks: nothing moves.
    #[action]
    async fn plan(&self, request: ScanRequest) -> Result<Preview, ActionError> {
        let plan = self.plan_of(&request).await?;
        let estimate = self.estimate_of(&plan).await?;
        Ok(Preview { plan, estimate })
    }

    /// Runs a scan.
    ///
    /// As `request` asks, into a new acquisition of `project`, named `name`,
    /// as `options` say (settling, approach, autofocus). A job: answered at
    /// once with where to follow it. Each scan ends where it started.
    /// (`start` is every Thing's own starting, in `teta-wot`, as `stop` is:
    /// ADR-0034.)
    #[action]
    async fn run(
        &self,
        ctx: ActionCtx,
        request: ScanRequest,
        project: ProjectId,
        #[param(default)] name: Option<String>,
        #[param(default)] options: Option<ScanOptions>,
    ) -> Result<JobRef, ActionError> {
        let jobs = self.state()?.jobs.clone();
        let unavailable = || ActionError::handled("unavailable (503): the server runs no jobs");
        let jobs = jobs.ok_or_else(unavailable)?;
        let plan = self.plan_of(&request).await?;
        let params = json!({
            "plan": plan,
            "project": project,
            "name": name,
            "options": options.unwrap_or_default(),
            "request": request,
        });
        let account = ctx.principal().id().map(str::to_owned);
        let id = jobs
            .submit_kind(SCAN, params, account)
            .map_err(action_error)?;
        Ok(JobRef::new(id))
    }
}
