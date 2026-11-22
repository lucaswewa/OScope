//! The stage: its status, position samples and capabilities, and the
//! [`Stage`] trait.
//!
//! Positions are x, y and z in micrometres, the controller's machine
//! coordinates after homing, with z the focus axis (ADR-0007).

use std::fmt;
use std::sync::Arc;

use oscope_core::{Ms, Timestamp, Um, Xyz};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::error::unsupported;
use crate::{Error, HalFuture, PositionTrack, Result};

/// A speed, in micrometres per second.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, PartialOrd, Serialize, Deserialize, JsonSchema,
)]
pub struct UmPerS(pub f64);

impl fmt::Display for UmPerS {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)?;
        f.write_str(" µm/s")
    }
}

/// What the stage is doing.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "state")]
pub enum StageStatus {
    /// Still, and ready.
    Idle,
    /// Moving or jogging.
    Moving,
    /// Homing.
    Homing,
    /// Paused by a feed hold. A stop ends the motion.
    Hold,
    /// Stopped by a fault, such as a limit switch, until the alarm is
    /// cleared.
    Alarm {
        /// The controller's reason.
        reason: String,
    },
}

impl StageStatus {
    /// Whether the stage is moving: moving or homing.
    pub fn is_moving(&self) -> bool {
        matches!(self, StageStatus::Moving | StageStatus::Homing)
    }
}

/// Where the stage was, and when: on OScope's clock, after the report's
/// latency is subtracted (ADR-0007).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PositionSample {
    /// When.
    pub time: Timestamp,
    /// Where.
    pub position: Xyz,
}

/// One of the stage's axes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Axis {
    /// x.
    X,
    /// y.
    Y,
    /// z, the focus axis.
    Z,
}

/// One axis's soft limits: the stage moves only from `min` to `max`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Travel {
    /// The least position.
    pub min: Um,
    /// The greatest position.
    pub max: Um,
}

/// What a stage is and offers. It doesn't change while the stage is open.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct StageCapabilities {
    /// The controller's maker and model.
    pub model: String,
    /// x's soft limits.
    pub x: Travel,
    /// y's soft limits.
    pub y: Travel,
    /// z's soft limits.
    pub z: Travel,
    /// The fastest speed along a move's path.
    pub max_speed: UmPerS,
    /// The speed of a move that names none.
    pub default_speed: UmPerS,
    /// Whether it can home, on its switches.
    pub homing: bool,
    /// Whether its positions come from linear encoders, rather than from
    /// counted steps.
    pub encoders: bool,
    /// Whether it has a trigger output, to fire a camera at positions (D20).
    pub trigger_output: bool,
    /// How often it reports its position.
    pub report_interval: Ms,
}

impl StageCapabilities {
    /// An axis's soft limits.
    pub fn travel(&self, axis: Axis) -> Travel {
        match axis {
            Axis::X => self.x,
            Axis::Y => self.y,
            Axis::Z => self.z,
        }
    }

    /// Checks that `target` is within the soft limits.
    pub fn check_target(&self, target: Xyz) -> Result<()> {
        for (axis, value) in [
            (Axis::X, target.x),
            (Axis::Y, target.y),
            (Axis::Z, target.z),
        ] {
            let travel = self.travel(axis);
            let what = format!("the {axis:?} target in µm").to_lowercase();
            Error::check_range(&what, value.0, travel.min.0, travel.max.0)?;
        }
        Ok(())
    }

    /// The speed a move goes at: `speed`, or the default speed. A speed must
    /// be more than 0, and at most the fastest.
    pub fn check_speed(&self, speed: Option<UmPerS>) -> Result<UmPerS> {
        let speed = speed.unwrap_or(self.default_speed);
        Error::check_range("the speed in µm/s", speed.0, 0.0, self.max_speed.0)?;
        if speed.0 == 0.0 {
            return Err(Error::Unsupported {
                what: "a move at 0 µm/s".to_owned(),
            });
        }
        Ok(speed)
    }
}

/// A motorised stage (plan §5.9).
pub trait Stage: Send + Sync {
    /// What it is and offers.
    fn capabilities(&self) -> &StageCapabilities;

    /// What it is doing.
    fn status(&self) -> HalFuture<'_, StageStatus>;

    /// Where it is: its latest report.
    fn position(&self) -> HalFuture<'_, PositionSample>;

    /// Its position track, which its reports feed.
    fn track(&self) -> Arc<PositionTrack>;

    /// Moves to `target` at `speed`, or at the default speed, and finishes
    /// when the stage is still there. A target outside the soft limits fails
    /// with [`Error::OutOfRange`], and a move that a stop ends fails with
    /// [`Error::Stopped`].
    fn move_to(&self, target: Xyz, speed: Option<UmPerS>) -> HalFuture<'_, PositionSample>;

    /// Moves by `delta` from where it is, as [`Stage::move_to`] does.
    fn move_by(&self, delta: Xyz, speed: Option<UmPerS>) -> HalFuture<'_, PositionSample> {
        Box::pin(async move {
            let from = self.position().await?.position;
            self.move_to(from + delta, speed).await
        })
    }

    /// Starts moving along `axis` until a stop or a soft limit, towards its
    /// greatest position if `towards_max`. It returns once the jog has
    /// started.
    fn jog(&self, axis: Axis, towards_max: bool, speed: Option<UmPerS>) -> HalFuture<'_, ()>;

    /// Stops any motion, slowing down as it would at the end of a move, and
    /// returns once the stage is still.
    fn stop(&self) -> HalFuture<'_, ()>;

    /// Finds the homing switches, and sets machine coordinates from them.
    fn home(&self) -> HalFuture<'_, PositionSample> {
        unsupported("homing")
    }

    /// Clears an alarm. Its position may be unknown until it is homed again.
    fn clear_alarm(&self) -> HalFuture<'_, ()>;

    /// Arms the trigger output to pulse once as the stage passes each of
    /// `positions` along `axis`, on the moves that follow; arming again
    /// replaces what was armed. A stage with a trigger output says so in
    /// its capabilities (D20, P11).
    fn arm_trigger(&self, axis: Axis, positions: Vec<Um>) -> HalFuture<'_, ()> {
        let _ = (axis, positions);
        unsupported("a trigger output")
    }
}
