//! Captures (P30): a single raw frame, or a burst of them, into a new
//! acquisition, through the recorder and ingest; with the acquisition's
//! display rendition if asked.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use oscope_core::{Clock, ProjectId};
use oscope_jobs::{Factory, Job, JobCtx, JobSpec, Resource, Stop};
use oscope_store::{RecordingOptions, Rendition, Scratch, Store, ingest, render};
use serde::Deserialize;
use serde_json::json;

use crate::bus::FrameBus;
use crate::recorder::{QUEUE, Recorder, Take};

/// A single capture's kind.
pub const SINGLE: &str = "capture.single";
/// A burst's kind.
pub const BURST: &str = "capture.burst";
/// The most frames a burst takes.
pub const BURST_MAX: u64 = 10_000;
/// How long a capture waits for a frame before it gives up.
const WAIT: Duration = Duration::from_secs(10);

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Params {
    project: ProjectId,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    frames: Option<u64>,
    #[serde(default)]
    rendition: Option<Rendition>,
}

/// What captures need: the bus the camera's frames come on, its clock, and
/// where the frames go.
#[derive(Clone)]
pub struct Rig {
    /// The camera's frames.
    pub bus: Arc<FrameBus>,
    /// The instrument's clock.
    pub clock: Arc<dyn Clock>,
    /// The projects.
    pub store: Arc<Store>,
    /// The scratch space, which a capture's frames pass through.
    pub scratch: Arc<Scratch>,
}

/// Makes capture jobs, single if not `burst`. Their parameters are
/// `project`, and optionally `name` and `rendition` (`png` or `jpeg`); a
/// burst's `frames` too, 1 to [`BURST_MAX`]. Each takes the frames whose
/// exposure starts after it does, into a new acquisition of the project,
/// with its rendition if asked.
pub fn capture_jobs(rig: Rig, burst: bool) -> Factory {
    Arc::new(move |value| {
        let params = Params::deserialize(value).map_err(|e| e.to_string())?;
        let frames = match (burst, params.frames) {
            (false, None) => 1,
            (false, Some(_)) => return Err("a single capture takes one frame".to_owned()),
            (true, Some(n @ 1..=BURST_MAX)) => n,
            (true, n) => {
                return Err(format!("frames: {n:?}, where 1 to {BURST_MAX} may be"));
            }
        };
        let spec = JobSpec {
            kind: (if burst { BURST } else { SINGLE }).to_owned(),
            title: match frames {
                1 => "capture a frame".to_owned(),
                n => format!("capture {n} frames"),
            },
            account: None,
            params: value.clone(),
            resource: Resource::Hardware,
            pausable: false,
            gpu_mib: 0,
        };
        let rig = rig.clone();
        Ok(Job {
            spec,
            run: Box::new(move |ctx| Box::pin(run(rig, params, frames, ctx))),
        })
    })
}

async fn run(rig: Rig, params: Params, frames: u64, ctx: JobCtx) -> Result<(), Stop> {
    let project = params.project;
    let name = params.name.unwrap_or_else(|| match frames {
        1 => "capture".to_owned(),
        n => format!("burst of {n}"),
    });
    // Segments of 1 GiB: a capture is a frame or a few, not minutes.
    let options = RecordingOptions {
        name: name.clone(),
        source: json!({ "capture": frames }),
        segment_bytes: 1 << 30,
        ..RecordingOptions::default()
    };
    let take = Take {
        frames: Some(frames),
        after: Some(rig.clock.now()),
    };
    let recorder =
        Recorder::taking(&rig.bus, &rig.scratch, options, QUEUE, take).map_err(Stop::failed)?;
    let id = recorder.id();
    let (mut seen, mut last) = (0, Instant::now());
    let waited = loop {
        if recorder.finished() {
            break Ok(());
        }
        if ctx.is_cancelled() {
            break Err(Stop::Cancelled);
        }
        let taken = recorder.stats().frames;
        if taken > seen {
            (seen, last) = (taken, Instant::now());
            let share = 0.8 * taken as f64 / frames as f64;
            ctx.progress(Some(share), format!("{taken} of {frames} frames"));
        }
        if last.elapsed() > WAIT {
            break Err(Stop::failed(
                "no frame came in 10 s: is the camera streaming?",
            ));
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    };
    let scratch = Arc::clone(&rig.scratch);
    let stopped = tokio::task::spawn_blocking(move || recorder.stop()).await;
    let recording = stopped.map_err(Stop::failed)?.map_err(Stop::failed)?;
    if let Err(stop) = waited.and(recording.error.map_or(Ok(()), |e| Err(Stop::failed(e)))) {
        let _ = scratch.delete(id);
        return Err(stop);
    }
    ctx.progress(Some(0.8), "ingesting");
    let folder: PathBuf = scratch.folder(id).map_err(Stop::failed)?;
    let store = Arc::clone(&rig.store);
    let made = tokio::task::spawn_blocking(move || {
        let acquisition = ingest(&store, project, &folder, Some(&name), false, &|_, _| true)?;
        let rendition = params
            .rendition
            .map(|rendition| render(&store, project, acquisition.id, rendition))
            .transpose()?;
        Ok::<_, oscope_store::StoreError>((acquisition, rendition))
    });
    let (acquisition, rendition) = made.await.map_err(Stop::failed)?.map_err(Stop::failed)?;
    ctx.add_result(format!(
        "projects/{project}/acquisitions/{}",
        acquisition.id
    ));
    if let Some(path) = rendition {
        ctx.add_result(format!("data/{project}/{path}"));
    }
    ctx.progress(Some(1.0), "captured");
    Ok(())
}
