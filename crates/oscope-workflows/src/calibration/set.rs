//! Calibration sets (ADR-0023): what one calibration measured, with when,
//! by whom, and on what instrument.

use chrono::{DateTime, Utc};
use oscope_core::Xyz;
use oscope_hal::{Objective, Travel};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::fit::{Matrix, decompose};

/// The version of a set's JSON.
pub const FORMAT: u32 = 1;

/// The instrument as it was when a set was made.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Instrument {
    /// The camera's maker and model.
    pub camera: String,
    /// The sensor's pixel, in µm.
    pub sensor_pixel_um: f64,
    /// The camera's binning.
    pub binning: u32,
    /// The stage's maker and model.
    pub stage: String,
    /// The objective fitted, if one was.
    pub objective: Option<Objective>,
    /// The instrument's tube lens, in µm.
    pub tube_lens_um: f64,
}

/// One objective's geometry at one binning: the camera-to-stage matrix M,
/// and what it decomposes into.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Geometry {
    /// The objective, by name.
    pub objective: String,
    /// The camera's binning.
    pub binning: u32,
    /// The pixel size the catalogue gives, in µm.
    pub nominal_pixel_um: f64,
    /// M, in µm per pixel, by rows: the stage move `M · d` brings the
    /// content `d` pixels from the image's centre to the centre.
    pub matrix: Matrix,
    /// The pixel size at the sample, `√|det M|`, in µm.
    pub pixel_um: f64,
    /// The camera's turn on the stage, in degrees.
    pub rotation_deg: f64,
    /// Whether the image is mirrored, its x reversed.
    pub mirrored: bool,
    /// The pixel's width over its height.
    pub aspect: f64,
    /// The skew, over the pixel size.
    pub skew: f64,
    /// The fit's root-mean-square error, in pixels.
    pub residual_px: f64,
    /// The moves' step, in µm.
    pub step_um: f64,
    /// How many moves were fitted.
    pub moves: u32,
}

/// A value for each of x and y.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PerAxis {
    /// x's.
    pub x: f64,
    /// y's.
    pub y: f64,
}

/// Each axis's soft limits.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SoftLimits {
    /// x's.
    pub x: Travel,
    /// y's.
    pub y: Travel,
    /// z's.
    pub z: Travel,
}

/// The stage's calibration.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct StageValues {
    /// x's and y's backlash, in µm: what a target reached from above lies
    /// past one reached from below.
    pub backlash_um: PerAxis,
    /// The soft limits, as the stage reported them.
    pub soft_limits: SoftLimits,
    /// Where homing left the stage, if it was homed.
    pub homed_at: Option<Xyz>,
}

/// What a set holds, by its kind.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Values {
    /// An objective's geometry at a binning.
    Geometry(Geometry),
    /// The stage's.
    Stage(StageValues),
}

/// The key of an objective's geometry at a binning.
pub fn geometry_key(objective: &str, binning: u32) -> String {
    format!("geometry/{objective}/{binning}")
}

/// The stage's key.
pub const STAGE_KEY: &str = "stage";

fn finite(what: &str, values: &[f64]) -> Result<(), String> {
    match values.iter().all(|v| v.is_finite()) {
        true => Ok(()),
        false => Err(format!("{what}: values that aren't finite")),
    }
}

impl Values {
    /// Its kind: `geometry` or `stage`.
    pub fn kind(&self) -> &'static str {
        match self {
            Values::Geometry(_) => "geometry",
            Values::Stage(_) => "stage",
        }
    }

    /// Its key: one set of each key is active.
    pub fn key(&self) -> String {
        match self {
            Values::Geometry(g) => geometry_key(&g.objective, g.binning),
            Values::Stage(_) => STAGE_KEY.to_owned(),
        }
    }

    /// The values, checked: finite, and positive where they must be. A
    /// geometry's decomposition is made again from its matrix, so that the
    /// two can't disagree.
    pub fn checked(self) -> Result<Values, String> {
        match self {
            Values::Geometry(mut g) => {
                if g.objective.is_empty() || g.binning == 0 {
                    return Err("a geometry needs an objective and a binning".to_owned());
                }
                let [[a, b], [c, d]] = g.matrix;
                finite("the geometry", &[a, b, c, d, g.nominal_pixel_um])?;
                finite("the geometry", &[g.residual_px, g.step_um])?;
                if g.nominal_pixel_um <= 0.0 {
                    return Err("a nominal pixel size that isn't positive".to_owned());
                }
                let parts = decompose(g.matrix).ok_or("a matrix that can't be inverted")?;
                g.pixel_um = parts.pixel;
                g.rotation_deg = parts.rotation_deg;
                g.mirrored = parts.mirrored;
                g.aspect = parts.aspect;
                g.skew = parts.skew;
                Ok(Values::Geometry(g))
            }
            Values::Stage(s) => {
                let limits = [s.soft_limits.x, s.soft_limits.y, s.soft_limits.z];
                let ends = limits.iter().flat_map(|t| [t.min.0, t.max.0]);
                let homed = s.homed_at.iter().flat_map(|p| [p.x.0, p.y.0, p.z.0]);
                let all: Vec<f64> = [s.backlash_um.x, s.backlash_um.y]
                    .into_iter()
                    .chain(ends)
                    .chain(homed)
                    .collect();
                finite("the stage", &all)?;
                if limits.iter().any(|t| t.min.0 > t.max.0) {
                    return Err("soft limits whose least is above their greatest".to_owned());
                }
                Ok(Values::Stage(s))
            }
        }
    }

    /// What it says, in a line.
    pub fn headline(&self) -> String {
        match self {
            Values::Geometry(g) => format!(
                "{:.4} µm a pixel, turned {:.2}°{}",
                g.pixel_um,
                g.rotation_deg,
                if g.mirrored { ", mirrored" } else { "" }
            ),
            Values::Stage(s) => format!(
                "backlash {:.2} µm in x, {:.2} µm in y",
                s.backlash_um.x, s.backlash_um.y
            ),
        }
    }
}

/// A calibration set (ADR-0023). It is never changed: a new calibration of
/// the same key is its next version.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CalibrationSet {
    /// The version of its JSON: 1.
    pub format: u32,
    /// Its ID.
    pub id: Uuid,
    /// Its version among the sets of its key, from 1.
    pub version: u32,
    /// When it was made.
    pub created: DateTime<Utc>,
    /// Who made it.
    pub account: Option<String>,
    /// The instrument as it was.
    pub instrument: Instrument,
    /// What it measured.
    pub values: Values,
    /// The ID it had where it came from, if it was imported.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub imported_from: Option<Uuid>,
}

/// A set, in brief.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SetSummary {
    /// Its ID.
    pub id: Uuid,
    /// Its kind: `geometry` or `stage`.
    pub kind: String,
    /// Its key.
    pub key: String,
    /// Its version.
    pub version: u32,
    /// When it was made.
    pub created: DateTime<Utc>,
    /// Who made it.
    pub account: Option<String>,
    /// Whether it is its key's active set.
    pub active: bool,
    /// What it says, in a line.
    pub headline: String,
}
