//! The scan engine (P36; ADR-0025): a plan run stop and go as a hardware
//! job. Each tile is moved to and focused, and each plane's frame recorded
//! with where the stage was once settled; then the frames are ingested into
//! a project as one acquisition. Every scan ends where it started.

use std::future::Future;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use oscope_core::{Frame, JobId, ProjectId, Timestamp, Um, Xyz};
use oscope_jobs::{Factory, Job, JobCtx, JobSpec, Resource, Stop};
use oscope_store::{RecordingOptions, RecordingWriter, Scratch, Store, ingest};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::plan::{Motion, Plan, ZPolicy, check_stack};
use crate::autofocus::frames::frame_after;
use crate::autofocus::{FocusOptions, FocusRig, autofocus};
use crate::calibration::Report;

/// The scan job's kind.
pub const SCAN: &str = "scan.run";

/// How many more times a step that fails is tried.
const RETRIES: u32 = 2;

/// What a scan drives: autofocus's rig, and where its frames go.
#[derive(Clone)]
pub struct ScanRig {
    /// The stage, the camera's frames, the objective and the lights.
    pub focus: FocusRig,
    /// The projects.
    pub store: Arc<Store>,
    /// The scratch space its frames are recorded to.
    pub scratch: Arc<Scratch>,
}

/// How a scan runs, each with a default.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct ScanOptions {
    /// How long the stage settles after a move before its frame, in ms.
    pub settle_ms: f64,
    /// How far below a z the stage starts, to move up to it, in µm.
    pub approach_um: f64,
    /// How each autofocus runs.
    pub focus: FocusOptions,
}

impl Default for ScanOptions {
    /// 50 ms of settling, from 10 µm below, and autofocus's defaults.
    fn default() -> Self {
        Self {
            settle_ms: 50.0,
            approach_um: 10.0,
            focus: FocusOptions::default(),
        }
    }
}

/// A tile done.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TileDone {
    /// Its place in the plan.
    pub index: u32,
    /// Where the stage was at its first frame, in µm.
    pub x_um: f64,
    /// Its y.
    pub y_um: f64,
    /// The z it was focused at, its stack's planes about it.
    pub z_um: f64,
    /// Its frames.
    pub frames: u32,
}

/// A scan's progress: its plan, the tiles done, and how it stands.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ScanProgress {
    /// The scan's job.
    pub job: JobId,
    /// Its plan.
    pub plan: Plan,
    /// The tiles done, in order.
    pub done: Vec<TileDone>,
    /// Autofocus runs refused, the last z found kept for their tiles.
    pub refused: u32,
    /// `running`, `completed`, `cancelled` or `failed`.
    pub state: String,
}

/// The latest scan's progress, which its job keeps.
pub type LastScan = Arc<Mutex<Option<ScanProgress>>>;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Params {
    plan: Plan,
    project: ProjectId,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    options: ScanOptions,
    /// How the plan was asked for, kept with its frames.
    #[serde(default)]
    request: serde_json::Value,
}

/// Makes scan jobs. Their parameters are `plan`, as P35's planners make
/// it, and `project`; optionally `name`, `options` ([`ScanOptions`]), and
/// `request`, how the plan was asked for, which the acquisition keeps.
/// Each runs its plan stop and go, then ingests its frames into a new
/// acquisition of the project, named `name`.
pub fn scan_jobs(rig: ScanRig, last: LastScan) -> Factory {
    Arc::new(move |value| {
        let params = Params::deserialize(value).map_err(|e| e.to_string())?;
        check(&params)?;
        let n = params.plan.tiles.len();
        let spec = JobSpec {
            kind: SCAN.to_owned(),
            title: format!("scan {n} tile{}", if n == 1 { "" } else { "s" }),
            account: None,
            params: value.clone(),
            resource: Resource::Hardware,
            pausable: false,
            gpu_mib: 0,
        };
        let (rig, last) = (rig.clone(), Arc::clone(&last));
        Ok(Job {
            spec,
            run: Box::new(move |ctx| Box::pin(run(rig, params, ctx, last))),
        })
    })
}

/// Refuses, at submission, a scan that can't be.
fn check(params: &Params) -> Result<(), String> {
    let plan = &params.plan;
    if let Motion::Continuous { .. } = plan.motion {
        return Err("continuous scanning comes in M3: a plan moves stop and go".to_owned());
    }
    let finite = |t: &super::Tile| {
        t.x_um.is_finite() && t.y_um.is_finite() && t.z_um.is_none_or(f64::is_finite)
    };
    if plan.tiles.is_empty() || !plan.tiles.iter().all(finite) {
        return Err(format!(
            "a plan of {} tiles: at least one, each finite, is needed",
            plan.tiles.len()
        ));
    }
    check_stack(plan.stack.as_ref()).map_err(|e| e.to_string())?;
    plan.z.check().map_err(|e| e.to_string())?;
    let unknown = plan.tiles.iter().any(|t| t.z_um.is_none());
    if unknown && !matches!(plan.z, ZPolicy::Autofocus { .. }) {
        return Err("a tile without a z needs the plan's z by autofocus".to_owned());
    }
    let ScanOptions {
        settle_ms,
        approach_um,
        ..
    } = params.options;
    if !((0.0..=10_000.0).contains(&settle_ms) && (0.0..=1_000.0).contains(&approach_um)) {
        return Err(format!(
            "{settle_ms} ms of settling and an approach of {approach_um} µm: from 0 to 10,000 \
             ms, and from 0 to 1,000 µm, are needed"
        ));
    }
    Ok(())
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

fn update(last: &LastScan, change: impl FnOnce(&mut ScanProgress)) {
    if let Some(progress) = lock(last).as_mut() {
        change(progress);
    }
}

/// Cancellation, for autofocus, without its progress, which would hide the
/// scan's.
struct Cancels<'a>(&'a JobCtx);

impl Report for Cancels<'_> {
    fn progress(&self, _: f64, _: &str) {}

    fn cancelled(&self) -> bool {
        self.0.is_cancelled()
    }
}

fn cancelled(ctx: &JobCtx) -> Result<(), Stop> {
    match ctx.is_cancelled() {
        true => Err(Stop::Cancelled),
        false => Ok(()),
    }
}

/// Runs `step`, and again up to [`RETRIES`] times while it fails.
async fn tried<T, F, Fut>(what: &str, ctx: &JobCtx, mut step: F) -> Result<T, Stop>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, String>>,
{
    let mut why = String::new();
    for attempt in 0..=RETRIES {
        if attempt > 0 {
            cancelled(ctx)?;
            tracing::warn!("{what}: {why}; trying again");
        }
        match step().await {
            Ok(value) => return Ok(value),
            Err(error) => why = error,
        }
    }
    Err(Stop::failed(format!(
        "{what} failed {} times: {why}",
        RETRIES + 1
    )))
}

/// Moves to `xy` at `z`, moving up, from `approach` µm below if
/// `from_below`: when the stage may be above. When it arrived.
async fn reach(
    focus: &FocusRig,
    xy: (f64, f64),
    z: f64,
    from_below: Option<f64>,
) -> Result<Timestamp, String> {
    let to = |z: f64| Xyz::new(Um(xy.0), Um(xy.1), Um(z));
    if let Some(approach) = from_below {
        focus
            .stage
            .move_to(to(z - approach), None)
            .await
            .map_err(|e| e.to_string())?;
    }
    let arrived = focus.stage.move_to(to(z), None).await;
    Ok(arrived.map_err(|e| e.to_string())?.time)
}

async fn position(focus: &FocusRig) -> Result<Xyz, String> {
    let sample = focus.stage.position().await;
    Ok(sample.map_err(|e| e.to_string())?.position)
}

/// Records `frame`, as taken at `at`.
async fn append(writer: &Arc<Mutex<RecordingWriter>>, frame: Frame, at: Xyz) -> Result<(), Stop> {
    let writer = Arc::clone(writer);
    let appended = tokio::task::spawn_blocking(move || {
        let mut meta = frame.meta().clone();
        meta.position = Some(at);
        lock(&writer).append(&meta, frame.bytes())
    });
    appended.await.map_err(Stop::failed)?.map_err(Stop::failed)
}

async fn run(rig: ScanRig, params: Params, ctx: JobCtx, last: LastScan) -> Result<(), Stop> {
    let Params {
        plan,
        project,
        name,
        options,
        request,
    } = params;
    let start = position(&rig.focus).await.map_err(Stop::failed)?;
    let name = name.unwrap_or_else(|| format!("scan of {} tiles", plan.tiles.len()));
    let recording = RecordingOptions {
        name: name.clone(),
        source: json!({ "plan": &plan, "request": request }),
        segment_bytes: 1 << 30,
        ..RecordingOptions::default()
    };
    let writer = rig.scratch.record(recording).map_err(Stop::failed)?;
    let id = writer.id();
    let writer = Arc::new(Mutex::new(writer));
    *lock(&last) = Some(ScanProgress {
        job: ctx.id(),
        plan: plan.clone(),
        done: Vec::new(),
        refused: 0,
        state: "running".to_owned(),
    });
    let mut lags = Vec::new();
    let scanned = tiles(&rig, &plan, &options, &ctx, (&writer, &last, &mut lags)).await;
    // Every scan ends where it started. Where the stage says it is lags the
    // z commanded, approached from below, as the planes showed: so the
    // command is the start plus that lag (ADR-0024, 4a).
    let lag = match lags.len() {
        0 => 0.0,
        n => lags.iter().sum::<f64>() / n as f64,
    };
    let below = Some(options.approach_um);
    let back = reach(&rig.focus, (start.x.0, start.y.0), start.z.0 + lag, below).await;
    let ended = scanned.and_then(|()| back.map(drop).map_err(Stop::failed));
    let why = match &ended {
        Ok(()) => None,
        Err(Stop::Cancelled) => Some("cancelled".to_owned()),
        Err(Stop::Failed(why)) => Some(why.clone()),
    };
    let writer = Arc::try_unwrap(writer)
        .map_err(|_| Stop::failed("the recording is still in use"))?
        .into_inner()
        .unwrap_or_else(PoisonError::into_inner);
    let finished = tokio::task::spawn_blocking(move || writer.finish(why)).await;
    finished.map_err(Stop::failed)?.map_err(Stop::failed)?;
    if let Err(stop) = ended {
        let state = if stop == Stop::Cancelled {
            "cancelled"
        } else {
            "failed"
        };
        update(&last, |p| p.state = state.to_owned());
        ctx.add_result(format!("recording {id}, kept in scratch"));
        return Err(stop);
    }
    ctx.progress(Some(0.9), "ingesting");
    let folder = rig.scratch.folder(id).map_err(Stop::failed)?;
    let store = Arc::clone(&rig.store);
    let made = tokio::task::spawn_blocking(move || {
        ingest(&store, project, &folder, Some(&name), false, &|_, _| true)
    });
    let acquisition = made.await.map_err(Stop::failed)?.map_err(Stop::failed)?;
    ctx.add_result(format!(
        "projects/{project}/acquisitions/{}",
        acquisition.id
    ));
    update(&last, |p| p.state = "completed".to_owned());
    ctx.progress(Some(1.0), "scanned");
    Ok(())
}

/// Takes each tile's frames, in order, recording them with `writer`, its
/// progress in `last`, and how far each plane's z lagged the command in
/// `lags`.
async fn tiles(
    rig: &ScanRig,
    plan: &Plan,
    options: &ScanOptions,
    ctx: &JobCtx,
    (writer, last, lags): (&Arc<Mutex<RecordingWriter>>, &LastScan, &mut Vec<f64>),
) -> Result<(), Stop> {
    let focus = &rig.focus;
    let planes = plan.planes();
    let (count, total) = (plan.tiles.len(), (plan.tiles.len() * planes.len()) as f64);
    let every = match plan.z {
        ZPolicy::Autofocus { every } => every as usize,
        _ => 0,
    };
    let settle = Duration::from_secs_f64(options.settle_ms / 1e3);
    let mut z = position(focus).await.map_err(Stop::failed)?.z.0;
    let mut taken = 0_u64;
    for (k, tile) in plan.tiles.iter().enumerate() {
        let (xy, which) = ((tile.x_um, tile.y_um), format!("tile {} of {count}", k + 1));
        match tile.z_um {
            Some(own) => z = own,
            None if every > 0 && k % every == 0 => {
                cancelled(ctx)?;
                tried(&format!("the move to {which}"), ctx, || {
                    reach(focus, xy, z, None)
                })
                .await?;
                let focused = tried(&format!("autofocus at {which}"), ctx, || async {
                    let report = autofocus(focus, &options.focus, &Cancels(ctx)).await;
                    report.map_err(|e| e.to_string())
                })
                .await?;
                match focused.found {
                    true => z = focused.z_um,
                    false => update(last, |p| p.refused += 1),
                }
            }
            None => {}
        }
        let mut first = None;
        for (p, offset) in planes.iter().enumerate() {
            cancelled(ctx)?;
            let below = (p == 0).then_some(options.approach_um);
            let target = z + offset;
            let at = format!("plane {} of {which}", p + 1);
            let arrived = tried(&format!("the move to {at}"), ctx, || {
                reach(focus, xy, target, below)
            })
            .await?;
            let frame = tried(&format!("the frame of {at}"), ctx, || async {
                frame_after(&focus.bus, arrived + settle)
                    .await
                    .map_err(|e| e.to_string())
            })
            .await?;
            let settled = tried(&format!("the position at {at}"), ctx, || position(focus)).await?;
            lags.push(target - settled.z.0);
            first.get_or_insert(settled);
            append(writer, frame, settled).await?;
            taken += 1;
            ctx.progress(Some(0.9 * taken as f64 / total), which.clone());
            ctx.count("frames", taken);
        }
        ctx.count("tiles", k as u64 + 1);
        let first = first.unwrap_or(Xyz::new(Um(xy.0), Um(xy.1), Um(z)));
        update(last, |p| {
            p.done.push(TileDone {
                index: tile.index,
                x_um: first.x.0,
                y_um: first.y.0,
                z_um: z,
                frames: planes.len() as u32,
            });
        });
    }
    Ok(())
}
