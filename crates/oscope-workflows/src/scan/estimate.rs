//! A plan's estimates: how long it takes, and how much it records.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::plan::{Plan, ZPolicy};
use super::{ScanError, invalid};

/// What a scan's time depends on.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Timing {
    /// The stage's speed from tile to tile, in µm/s.
    pub speed_um_s: f64,
    /// How long the stage settles before each frame, in ms.
    pub settle_ms: f64,
    /// How long each frame takes, in ms: its exposure, or the camera's time
    /// from one frame to the next, if longer.
    pub frame_ms: f64,
    /// How long an autofocus takes, in s.
    pub autofocus_s: f64,
}

/// A frame as the scan records it.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct FrameSize {
    /// Its width, in pixels.
    pub width: u32,
    /// Its height.
    pub height: u32,
    /// The bytes a pixel takes: 2 for the raw mosaic in 16 bits.
    pub bytes_per_pixel: u32,
}

/// What a plan takes.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Estimate {
    /// Its tiles.
    pub tiles: u32,
    /// Its frames: each plane at each tile.
    pub frames: u64,
    /// The bytes recorded, before compression.
    pub bytes: u64,
    /// How far the stage goes from the first tile to the last, in µm, in
    /// straight lines, as G-code moves it.
    pub travel_um: f64,
    /// How many times it autofocuses.
    pub autofocus_runs: u32,
    /// How long it takes, in s: the travel at speed; each frame, after its
    /// settling; and each autofocus.
    pub seconds: f64,
}

/// What `plan` takes, at `timing`, recording `frame`s.
pub fn estimate(plan: &Plan, timing: &Timing, frame: &FrameSize) -> Result<Estimate, ScanError> {
    let Timing {
        speed_um_s,
        settle_ms,
        frame_ms,
        autofocus_s,
    } = *timing;
    let times = [settle_ms, frame_ms, autofocus_s];
    if !(speed_um_s > 0.0
        && speed_um_s.is_finite()
        && times.iter().all(|t| *t >= 0.0 && t.is_finite()))
    {
        return Err(invalid(format!(
            "a speed of {speed_um_s} µm/s, and times of {times:?}: a speed more than 0, and \
             times of at least 0, are needed"
        )));
    }
    let tiles = plan.tiles.len() as u64;
    let frames = tiles * plan.planes().len() as u64;
    let pixels = u64::from(frame.width) * u64::from(frame.height);
    // Folded from +0, as a sum of no moves would be −0.
    let travel_um = plan
        .tiles
        .windows(2)
        .map(|w| (w[1].x_um - w[0].x_um).hypot(w[1].y_um - w[0].y_um))
        .fold(0.0, |sum, d| sum + d);
    let autofocus_runs = match plan.z {
        ZPolicy::Autofocus { every } => tiles.div_ceil(u64::from(every)),
        _ => 0,
    };
    let seconds = travel_um / speed_um_s
        + frames as f64 * (settle_ms + frame_ms) / 1e3
        + autofocus_runs as f64 * autofocus_s;
    Ok(Estimate {
        tiles: tiles as u32,
        frames,
        bytes: frames * pixels * u64::from(frame.bytes_per_pixel),
        travel_um,
        autofocus_runs: autofocus_runs as u32,
        seconds,
    })
}
