//! Auto exposure and auto white balance (P34's spec).

use std::time::Duration;

use oscope_core::{Frame, Ms, Timestamp};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::frames::{SATURATED, frame_after, percentile, white};
use super::{AutofocusError, FocusRig, check, device, failed};
use crate::calibration::{Region, Report};

/// Below this share of full scale, a frame is dark.
const DARK: f64 = 0.01;
/// The lightest part must be at least this share of full scale.
const WHITE_AT_LEAST: f64 = 0.05;

/// What auto exposure takes, each optional.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct ExposureOptions {
    /// Where the percentile should sit, as a share of full scale.
    pub target: f64,
    /// The percentile, from 0 to 100.
    pub percentile: f64,
    /// How near the target is near enough, as a share of it.
    pub tolerance: f64,
    /// The most exposures it tries.
    pub steps: u32,
    /// The frame's region looked at: its middle half each way by default.
    pub region: Option<Region>,
    /// How long after a change a frame may start, in ms.
    pub settle_ms: f64,
}

impl Default for ExposureOptions {
    /// The 99.5th percentile at 0.75 of full scale, within 5 %, in at most
    /// 10 steps, frames 50 ms on.
    fn default() -> Self {
        Self {
            target: 0.75,
            percentile: 99.5,
            tolerance: 0.05,
            steps: 10,
            region: None,
            settle_ms: 50.0,
        }
    }
}

impl ExposureOptions {
    /// Checks it: a target below saturation, a percentile, a tolerance
    /// under 1, at least a step, a settling time that isn't negative.
    pub fn check(&self) -> Result<(), String> {
        let ok = self.target > 0.0
            && self.target < SATURATED
            && self.percentile > 0.0
            && self.percentile <= 100.0
            && self.tolerance > 0.0
            && self.tolerance < 1.0
            && self.steps >= 1
            && self.settle_ms.is_finite()
            && self.settle_ms >= 0.0
            && self.region.is_none_or(|r| r.width > 0 && r.height > 0);
        match ok {
            true => Ok(()),
            false => Err(format!("these options can't be: {self:?}")),
        }
    }
}

/// One exposure tried.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ExposureStep {
    /// The exposure, in ms.
    pub exposure_ms: f64,
    /// Its frame's percentile, as a share of full scale.
    pub level: f64,
}

/// What auto exposure did.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ExposureReport {
    /// The exposure it left, in ms: the best it tried.
    pub exposure_ms: f64,
    /// That exposure's percentile, as a share of full scale.
    pub level: f64,
    /// The target.
    pub target: f64,
    /// Whether it came within the tolerance.
    pub converged: bool,
    /// Each exposure tried.
    pub steps: Vec<ExposureStep>,
    /// Why it didn't converge, if it didn't.
    pub note: Option<String>,
}

/// The first frame exposed for `exposure_ms` that starts at `after` or later.
async fn frame_with(
    rig: &FocusRig,
    mut after: Timestamp,
    exposure_ms: f64,
) -> Result<Frame, AutofocusError> {
    loop {
        let frame = frame_after(&rig.bus, after).await?;
        if (frame.meta().exposure.0 - exposure_ms).abs() <= 1e-9 * exposure_ms.max(1.0) {
            return Ok(frame);
        }
        after = frame.meta().exposure_start + Duration::from_micros(1);
    }
}

/// Sets the camera's exposure until the region's percentile sits at the
/// target: each step multiplies it by the target over the level, the
/// sensor being linear; halves it when saturated; quadruples it when dark;
/// within the camera's limits. It ends at the best exposure tried, and says
/// why if that isn't within the tolerance.
pub async fn auto_exposure(
    rig: &FocusRig,
    options: &ExposureOptions,
    report: &dyn Report,
) -> Result<ExposureReport, AutofocusError> {
    options.check().map_err(failed)?;
    let camera = rig.camera.capabilities();
    let (shortest, longest) = (camera.exposure_min.0, camera.exposure_max.0);
    let settle = Duration::from_secs_f64(options.settle_ms / 1e3);
    let mut settings = rig.camera.settings().await.map_err(device)?;
    let mut since = rig.stage.position().await.map_err(device)?.time;
    let (mut steps, mut note, mut converged) = (Vec::new(), None, false);
    let target = options.target;
    for step in 0..options.steps {
        check(report)?;
        let exposure = settings.exposure.0;
        let frame = frame_with(rig, since + settle, exposure).await?;
        let level = percentile(&frame, options.region, options.percentile)?;
        drop(frame);
        steps.push(ExposureStep {
            exposure_ms: exposure,
            level,
        });
        report.progress(f64::from(step + 1) / f64::from(options.steps), "exposing");
        if (level / target - 1.0).abs() <= options.tolerance {
            converged = true;
            break;
        }
        let next = match level {
            l if l >= SATURATED => exposure / 2.0,
            l if l < DARK => exposure * 4.0,
            l => exposure * target / l,
        }
        .clamp(shortest, longest);
        if (next - exposure).abs() <= 1e-9 * exposure {
            note = Some(match level < target {
                true => format!(
                    "held at the longest exposure, {longest} ms, where the frame reaches {:.0} % \
                     of the target: open the light, or raise the gain",
                    100.0 * level / target
                ),
                false => format!(
                    "held at the shortest exposure, {shortest} ms, where the frame is still too \
                     bright: dim the light"
                ),
            });
            break;
        }
        settings.exposure = Ms(next);
        settings = rig.camera.configure(settings).await.map_err(device)?;
        since = rig.stage.position().await.map_err(device)?.time;
    }
    // The best tried: nearest the target, unsaturated first.
    let score = |s: &ExposureStep| (s.level >= SATURATED, (s.level / target - 1.0).abs());
    let best = *steps
        .iter()
        .min_by(|a, b| {
            score(a)
                .partial_cmp(&score(b))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .expect("a step");
    if best.exposure_ms != settings.exposure.0 {
        settings.exposure = Ms(best.exposure_ms);
        rig.camera.configure(settings).await.map_err(device)?;
    }
    if !converged && note.is_none() {
        note = Some(format!(
            "it didn't come within {:.0} % of the target in {} steps",
            100.0 * options.tolerance,
            options.steps
        ));
    }
    Ok(ExposureReport {
        exposure_ms: best.exposure_ms,
        level: best.level,
        target,
        converged,
        steps,
        note,
    })
}

/// What auto white balance takes.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct WhiteBalanceOptions {
    /// The frame's region looked at: its middle half each way by default.
    pub region: Option<Region>,
    /// How long from now its frame may start, in ms.
    pub settle_ms: f64,
}

impl Default for WhiteBalanceOptions {
    /// The middle half, a frame 50 ms on.
    fn default() -> Self {
        Self {
            region: None,
            settle_ms: 50.0,
        }
    }
}

/// What auto white balance found.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct WhiteBalanceReport {
    /// Red's, green's and blue's gains: (G / R, 1, G / B).
    pub gains: [f64; 3],
    /// The light's white, red, green and blue, as shares of full scale.
    pub white: [f64; 3],
    /// Its grey.
    pub level: f64,
    /// How many cells it was measured over.
    pub cells: u32,
}

/// Finds the gains that make the light's white grey: from the region's
/// lightest cells, between its 95th percentile and saturation (Land and
/// McCann's white patch).
pub async fn white_balance(
    rig: &FocusRig,
    options: &WhiteBalanceOptions,
    report: &dyn Report,
) -> Result<WhiteBalanceReport, AutofocusError> {
    check(report)?;
    let settle = Duration::from_secs_f64(options.settle_ms.max(0.0) / 1e3);
    let now = rig.stage.position().await.map_err(device)?.time;
    let frame = frame_after(&rig.bus, now + settle).await?;
    let found = white(&frame, options.region)?;
    drop(frame);
    let Some((rgb, level, cells)) = found else {
        return Err(failed(
            "the camera's frames are grey: white balance needs colour",
        ));
    };
    if cells == 0 || level < WHITE_AT_LEAST || rgb.iter().any(|v| *v <= 0.0) {
        return Err(failed(
            "the frame's lightest part is too dark, or saturated: set the exposure first",
        ));
    }
    Ok(WhiteBalanceReport {
        gains: [rgb[1] / rgb[0], 1.0, rgb[1] / rgb[2]],
        white: rgb,
        level,
        cells: cells as u32,
    })
}
