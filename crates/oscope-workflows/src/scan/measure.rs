//! Measuring a focus map: autofocus at each sample point.

use oscope_core::{Point2, Um, Xyz};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::focus_map::FocusPoint;
use crate::autofocus::{AutofocusError, FocusOptions, FocusRig, autofocus, check, device};
use crate::calibration::Report;

/// What autofocus found at a sample point.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MapSample {
    /// The point, in µm.
    pub at: Point2,
    /// The z found, if one was.
    pub z_um: Option<f64>,
    /// Why none was: autofocus's refusal.
    pub refused: Option<String>,
}

impl MapSample {
    /// The focus found, as a map fits it.
    pub fn point(&self) -> Option<FocusPoint> {
        self.z_um.map(|z_um| FocusPoint {
            x_um: self.at.x,
            y_um: self.at.y,
            z_um,
        })
    }
}

/// A part of a report: from `from`, a `share` of it.
struct Part<'a> {
    report: &'a dyn Report,
    from: f64,
    share: f64,
}

impl Report for Part<'_> {
    fn progress(&self, fraction: f64, stage: &str) {
        self.report
            .progress(self.from + fraction * self.share, stage);
    }

    fn cancelled(&self) -> bool {
        self.report.cancelled()
    }
}

/// Measures focus at `points` for a map: moves to each in turn, at the z
/// last found (where the stage is, for the first), and autofocuses there
/// with `options` (P34). A point where autofocus is refused, on an empty
/// field or with the focus out of reach, keeps the refusal and no z.
pub async fn measure_map(
    rig: &FocusRig,
    points: &[Point2],
    options: &FocusOptions,
    report: &dyn Report,
) -> Result<Vec<MapSample>, AutofocusError> {
    let mut z = rig.stage.position().await.map_err(device)?.position.z.0;
    let share = 1.0 / points.len().max(1) as f64;
    let mut samples = Vec::with_capacity(points.len());
    for (k, &at) in points.iter().enumerate() {
        check(report)?;
        let to = Xyz::new(Um(at.x), Um(at.y), Um(z));
        rig.stage.move_to(to, None).await.map_err(device)?;
        let part = Part {
            report,
            from: k as f64 * share,
            share,
        };
        let found = autofocus(rig, options, &part).await?;
        if found.found {
            z = found.z_um;
        }
        samples.push(MapSample {
            at,
            z_um: found.found.then_some(found.z_um),
            refused: found.refused,
        });
    }
    Ok(samples)
}
