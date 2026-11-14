//! The calibration jobs: hardware jobs that run the procedures and keep
//! what they measure.

use std::sync::{Arc, Mutex, PoisonError};

use oscope_jobs::{Factory, Job, JobCtx, JobSpec, Resource, Stop};
use serde::Deserialize;
use serde_json::Value;

use super::procedures::{
    CalibrationError, CheckReport, GeometryOptions, Motion, Report, Rig, StageOptions,
    check_objective, measure_geometry, measure_stage,
};
use super::set::Values;

/// The geometry calibration's job kind.
pub const GEOMETRY: &str = "calibration.geometry";
/// The stage calibration's.
pub const STAGE: &str = "calibration.stage";
/// The objective check's.
pub const CHECK: &str = "calibration.check_objective";

/// The latest objective check's report, which the check job sets.
pub type LastCheck = Arc<Mutex<Option<CheckReport>>>;

impl Report for JobCtx {
    fn progress(&self, fraction: f64, stage: &str) {
        JobCtx::progress(self, Some(fraction), stage);
    }

    fn cancelled(&self) -> bool {
        self.is_cancelled()
    }
}

fn stop(error: CalibrationError) -> Stop {
    match error {
        CalibrationError::Cancelled => Stop::Cancelled,
        CalibrationError::Failed(why) => Stop::Failed(why),
    }
}

fn spec(kind: &str, title: &str, params: &Value) -> JobSpec {
    JobSpec {
        kind: kind.to_owned(),
        title: title.to_owned(),
        account: None,
        params: params.clone(),
        resource: Resource::Hardware,
        pausable: false,
        gpu_mib: 0,
    }
}

/// A measurement's parameters: its options, and whether the set it makes
/// becomes active.
#[derive(Deserialize)]
struct Params<T> {
    #[serde(flatten)]
    options: T,
    #[serde(default = "yes")]
    activate: bool,
}

fn yes() -> bool {
    true
}

/// The calibration jobs' kinds and factories, for the job service:
///
/// - `calibration.geometry`, with `step_um`, `approach_um`, `settle_ms` and
///   `activate`, all optional;
/// - `calibration.stage`, with `home`, `cycles`, `approach_um`, `settle_ms`
///   and `activate`;
/// - `calibration.check_objective`, with `approach_um` and `settle_ms`; its
///   report goes to `last`.
///
/// A measurement's result is the set it made, `calibration/<id>`.
pub fn calibration_jobs(rig: Rig, last: LastCheck) -> Vec<(&'static str, Factory)> {
    let geometry_rig = rig.clone();
    let geometry: Factory = Arc::new(move |value| {
        let params: Params<GeometryOptions> =
            Params::deserialize(value).map_err(|e| e.to_string())?;
        let rig = geometry_rig.clone();
        Ok(Job {
            spec: spec(GEOMETRY, "Calibrate the geometry", value),
            run: Box::new(move |ctx| {
                Box::pin(async move {
                    let measured = measure_geometry(&rig, &params.options, &ctx).await;
                    let (geometry, instrument) = measured.map_err(stop)?;
                    let values = Values::Geometry(geometry);
                    let set = rig
                        .store
                        .save(values, instrument, ctx.account(), params.activate)
                        .map_err(Stop::failed)?;
                    ctx.add_result(format!("calibration/{}", set.id));
                    Ok(())
                })
            }),
        })
    });
    let stage_rig = rig.clone();
    let stage: Factory = Arc::new(move |value| {
        let params: Params<StageOptions> = Params::deserialize(value).map_err(|e| e.to_string())?;
        let rig = stage_rig.clone();
        Ok(Job {
            spec: spec(STAGE, "Calibrate the stage", value),
            run: Box::new(move |ctx| {
                Box::pin(async move {
                    let measured = measure_stage(&rig, &params.options, &ctx).await;
                    let (values, instrument) = measured.map_err(stop)?;
                    let set = rig
                        .store
                        .save(
                            Values::Stage(values),
                            instrument,
                            ctx.account(),
                            params.activate,
                        )
                        .map_err(Stop::failed)?;
                    ctx.add_result(format!("calibration/{}", set.id));
                    Ok(())
                })
            }),
        })
    });
    let check: Factory = Arc::new(move |value| {
        let motion = Motion::deserialize(value).map_err(|e| e.to_string())?;
        let (rig, last) = (rig.clone(), Arc::clone(&last));
        Ok(Job {
            spec: spec(CHECK, "Check the objective", value),
            run: Box::new(move |ctx| {
                Box::pin(async move {
                    let report = check_objective(&rig, &motion, &ctx).await.map_err(stop)?;
                    let verdict = match report.plausible {
                        true => "plausible".to_owned(),
                        false => format!("flagged: nearest the {}", report.nearest),
                    };
                    *last.lock().unwrap_or_else(PoisonError::into_inner) = Some(report);
                    ctx.progress(Some(1.0), verdict);
                    Ok(())
                })
            }),
        })
    });
    vec![(GEOMETRY, geometry), (STAGE, stage), (CHECK, check)]
}
