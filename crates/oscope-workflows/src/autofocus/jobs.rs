//! The autofocus and exposure jobs: hardware jobs that run the procedures
//! and keep their reports. A run that finds no focus, or an exposure that
//! doesn't converge, fails its job with why; its report is kept either way.

use std::sync::{Arc, Mutex, PoisonError};

use oscope_jobs::{Factory, Job, JobCtx, JobSpec, Resource, Stop};
use serde::Deserialize;
use serde_json::Value;

use super::exposure::{
    ExposureOptions, ExposureReport, WhiteBalanceOptions, WhiteBalanceReport, auto_exposure,
    white_balance,
};
use super::sweep::{FocusOptions, FocusReport, autofocus};
use super::{AutofocusError, FocusRig};

/// The autofocus job's kind.
pub const AUTOFOCUS: &str = "autofocus.run";
/// Auto exposure's.
pub const AUTO_EXPOSURE: &str = "exposure.auto";
/// Auto white balance's.
pub const WHITE_BALANCE: &str = "exposure.white_balance";

/// The latest reports, which the jobs set: autofocus's, auto exposure's,
/// and white balance's.
#[derive(Clone, Default)]
pub struct LastReports {
    /// Autofocus's.
    pub focus: Arc<Mutex<Option<FocusReport>>>,
    /// Auto exposure's.
    pub exposure: Arc<Mutex<Option<ExposureReport>>>,
    /// White balance's.
    pub white: Arc<Mutex<Option<WhiteBalanceReport>>>,
}

/// The latest autofocus report.
pub type LastFocus = Arc<Mutex<Option<FocusReport>>>;
/// The latest auto exposure report.
pub type LastExposure = Arc<Mutex<Option<ExposureReport>>>;

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

fn stop(error: AutofocusError) -> Stop {
    match error {
        AutofocusError::Cancelled => Stop::Cancelled,
        AutofocusError::Failed(why) => Stop::Failed(why),
    }
}

fn keep<T>(slot: &Mutex<Option<T>>, value: T) {
    *slot.lock().unwrap_or_else(PoisonError::into_inner) = Some(value);
}

fn options<T: for<'de> Deserialize<'de>>(value: &Value) -> Result<T, String> {
    T::deserialize(value).map_err(|e| e.to_string())
}

/// The jobs' kinds and factories, for the job service:
///
/// - `autofocus.run`, with [`FocusOptions`]: its result is the z found;
/// - `exposure.auto`, with [`ExposureOptions`]: the exposure set;
/// - `exposure.white_balance`, with [`WhiteBalanceOptions`]: the gains,
///   for a client to give the preview.
pub fn focus_jobs(rig: FocusRig, last: LastReports) -> Vec<(&'static str, Factory)> {
    let (focus_rig, focus_last) = (rig.clone(), Arc::clone(&last.focus));
    let focus: Factory = Arc::new(move |value| {
        let options: FocusOptions = options(value)?;
        options.check()?;
        let (rig, last) = (focus_rig.clone(), Arc::clone(&focus_last));
        Ok(Job {
            spec: spec(AUTOFOCUS, "Autofocus", value),
            run: Box::new(move |ctx: JobCtx| {
                Box::pin(async move {
                    let report = autofocus(&rig, &options, &ctx).await.map_err(stop)?;
                    keep(&last, report.clone());
                    match report.found {
                        true => {
                            ctx.add_result(format!("z = {:.3} µm", report.z_um));
                            Ok(())
                        }
                        false => Err(Stop::Failed(report.refused.unwrap_or_default())),
                    }
                })
            }),
        })
    });
    let (exposure_rig, exposure_last) = (rig.clone(), Arc::clone(&last.exposure));
    let exposure: Factory = Arc::new(move |value| {
        let options: ExposureOptions = options(value)?;
        options.check()?;
        let (rig, last) = (exposure_rig.clone(), Arc::clone(&exposure_last));
        Ok(Job {
            spec: spec(AUTO_EXPOSURE, "Auto exposure", value),
            run: Box::new(move |ctx: JobCtx| {
                Box::pin(async move {
                    let report = auto_exposure(&rig, &options, &ctx).await.map_err(stop)?;
                    keep(&last, report.clone());
                    ctx.add_result(format!("exposure = {} ms", report.exposure_ms));
                    match report.converged {
                        true => Ok(()),
                        false => Err(Stop::Failed(report.note.unwrap_or_default())),
                    }
                })
            }),
        })
    });
    let (white_rig, white_last) = (rig, Arc::clone(&last.white));
    let white: Factory = Arc::new(move |value| {
        let options: WhiteBalanceOptions = options(value)?;
        let (rig, last) = (white_rig.clone(), Arc::clone(&white_last));
        Ok(Job {
            spec: spec(WHITE_BALANCE, "Auto white balance", value),
            run: Box::new(move |ctx: JobCtx| {
                Box::pin(async move {
                    let report = white_balance(&rig, &options, &ctx).await.map_err(stop)?;
                    let [r, _, b] = report.gains;
                    keep(&last, report);
                    ctx.add_result(format!("gains = {r:.4}, 1, {b:.4}"));
                    Ok(())
                })
            }),
        })
    });
    vec![
        (AUTOFOCUS, focus),
        (AUTO_EXPOSURE, exposure),
        (WHITE_BALANCE, white),
    ]
}
