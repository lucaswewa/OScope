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
    /// Each move's residual, x and y, in pixels: what the fit leaves
    /// unexplained (W8.2). Empty in a set made before them.
    #[serde(default)]
    pub residuals: Vec<[f64; 2]>,
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

/// A frame's region of the sensor, in its pixels.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Region {
    /// Its left.
    pub x: u32,
    /// Its top.
    pub y: u32,
    /// Its width, in the frame's pixels.
    pub width: u32,
    /// Its height.
    pub height: u32,
}

/// A dark frame (P33): the mean of frames taken without light, for a mode,
/// a gain, and an octave of exposure. Its image, the mosaic in linear
/// units from 0 to 1, is kept beside the set.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Dark {
    /// The camera's mode.
    pub mode: String,
    /// The camera's gain.
    pub gain: f64,
    /// The octave of exposure it serves, in ms: from the first up to the
    /// second.
    pub exposure_ms: [f64; 2],
    /// The exposure it was taken at, in ms.
    pub taken_ms: f64,
    /// The frame's region.
    pub region: Region,
    /// How many frames were averaged.
    pub frames: u32,
    /// Its mean level, in linear units.
    pub mean: f64,
    /// Its spread over the frame, the fixed pattern's.
    pub spread: f64,
}

/// A flat field (P33): a blank field's frames averaged, the dark taken
/// away, each colour's sites scaled to a mean of 1, for an objective, a
/// light and a binning; with the white balance the blank field gives. Its
/// image is kept beside the set.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Flat {
    /// The objective, by name.
    pub objective: String,
    /// The lights on, by channel.
    pub light: String,
    /// The camera's mode.
    pub mode: String,
    /// The camera's binning.
    pub binning: u32,
    /// The frame's region.
    pub region: Region,
    /// How many frames were averaged.
    pub frames: u32,
    /// Whether a dark frame was taken away, or else the black level.
    pub dark: bool,
    /// The white balance: red's, green's and blue's gains, green's 1.
    pub gains: [f64; 3],
    /// How uneven the field was: its green's spread over its mean.
    pub uneven: f64,
}

/// A colour calibration (P33), for a light: the white balance, and the
/// matrix that takes white-balanced colours to linear sRGB, from a colour
/// target.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Colour {
    /// The lights on, by channel.
    pub light: String,
    /// The white balance: red's, green's and blue's gains, green's 1.
    pub gains: [f64; 3],
    /// The matrix, by rows.
    pub matrix: [[f64; 3]; 3],
    /// The target's patches measured.
    pub patches: u32,
    /// ΔE*ab over them, white-balanced only: the mean.
    pub delta_e_before: f64,
    /// And corrected by the matrix: the mean.
    pub delta_e_mean: f64,
    /// And the largest.
    pub delta_e_max: f64,
}

/// What a set holds, by its kind.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Values {
    /// An objective's geometry at a binning.
    Geometry(Geometry),
    /// The stage's.
    Stage(StageValues),
    /// A dark frame.
    Dark(Dark),
    /// A flat field.
    Flat(Flat),
    /// A colour calibration.
    Colour(Colour),
}

/// The octave of exposure that `exposure_ms` lies in: from a power of 2 ms
/// up to the next.
pub fn exposure_octave(exposure_ms: f64) -> [f64; 2] {
    let low = 2f64.powi(exposure_ms.max(1e-6).log2().floor() as i32);
    [low, 2.0 * low]
}

/// The key of a dark frame for `mode`, `gain` and `exposure_ms`.
pub fn dark_key(mode: &str, gain: f64, exposure_ms: f64) -> String {
    format!("dark/{mode}/{gain}/{}", exposure_octave(exposure_ms)[0])
}

/// The key of a flat field for `objective`, `light` and `binning`.
pub fn flat_key(objective: &str, light: &str, binning: u32) -> String {
    format!("flat/{objective}/{light}/{binning}")
}

/// The key of a colour calibration for `light`.
pub fn colour_key(light: &str) -> String {
    format!("colour/{light}")
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
    /// Its kind: `geometry`, `stage`, `dark`, `flat` or `colour`.
    pub fn kind(&self) -> &'static str {
        match self {
            Values::Geometry(_) => "geometry",
            Values::Stage(_) => "stage",
            Values::Dark(_) => "dark",
            Values::Flat(_) => "flat",
            Values::Colour(_) => "colour",
        }
    }

    /// Whether an image is kept beside it: a dark frame's or a flat field's.
    pub fn has_image(&self) -> bool {
        matches!(self, Values::Dark(_) | Values::Flat(_))
    }

    /// Its key: one set of each key is active.
    pub fn key(&self) -> String {
        match self {
            Values::Geometry(g) => geometry_key(&g.objective, g.binning),
            Values::Stage(_) => STAGE_KEY.to_owned(),
            Values::Dark(d) => dark_key(&d.mode, d.gain, d.taken_ms),
            Values::Flat(f) => flat_key(&f.objective, &f.light, f.binning),
            Values::Colour(c) => colour_key(&c.light),
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
                let each: Vec<f64> = g.residuals.iter().flatten().copied().collect();
                finite("the geometry's residuals", &each)?;
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
            Values::Dark(d) => {
                finite("the dark frame", &[d.gain, d.taken_ms, d.mean, d.spread])?;
                Ok(Values::Dark(d))
            }
            Values::Flat(f) => {
                finite(
                    "the flat field",
                    &[f.gains[0], f.gains[1], f.gains[2], f.uneven],
                )?;
                match f.gains.iter().all(|g| *g > 0.0) {
                    true => Ok(Values::Flat(f)),
                    false => Err("a flat field's gains must be more than 0".to_owned()),
                }
            }
            Values::Colour(c) => {
                let numbers = c.gains.iter().chain(c.matrix.iter().flatten());
                let all: Vec<f64> = numbers.copied().collect();
                finite("the colour calibration", &all)?;
                match c.gains.iter().all(|g| *g > 0.0) {
                    true => Ok(Values::Colour(c)),
                    false => Err("a colour calibration's gains must be more than 0".to_owned()),
                }
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
            Values::Dark(d) => format!(
                "dark at {} ms, gain {}: mean {:.5}",
                d.taken_ms, d.gain, d.mean
            ),
            Values::Flat(f) => format!(
                "flat for the {} in {}: {:.1} % uneven",
                f.objective,
                f.light,
                100.0 * f.uneven
            ),
            Values::Colour(c) => format!(
                "colour in {}: ΔE*ab {:.2} on average, from {:.2}",
                c.light, c.delta_e_mean, c.delta_e_before
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
    /// Its kind: `geometry`, `stage`, `dark`, `flat` or `colour`.
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
