//! What a frame records about itself (plan §5.5).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::PixelFormat;
use crate::clock::Timestamp;
use crate::geometry::{PixelRect, Xyz};
use crate::ids::FrameId;
use crate::units::Ms;

/// A light's state when a frame was taken.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LightState {
    /// The channel, such as `transmitted`.
    pub channel: String,
    /// Whether it was on.
    pub on: bool,
    /// Its intensity, from 0 to 1.
    pub intensity: f64,
}

/// What a frame records about itself: its pixels' layout, and when, how and
/// where it was taken.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct FrameMeta {
    /// Its number, in the order frames arrive.
    pub id: FrameId,
    /// How its pixels are stored.
    pub format: PixelFormat,
    /// Its width in pixels.
    pub width: u32,
    /// Its height in pixels.
    pub height: u32,
    /// The sensor's pixels it was read from, in full-sensor coordinates at
    /// full resolution (ADR-0007).
    pub roi: PixelRect,
    /// How many sensor pixels each of its pixels combines, along each axis.
    pub binning: u32,
    /// The camera mode, by name.
    pub mode: String,
    /// When it reached the host.
    pub host_time: Timestamp,
    /// The camera's own timestamp for it, on OScope's clock, if the camera
    /// gives one. The MIChrome 20 doesn't.
    pub sensor_time: Option<Timestamp>,
    /// When the exposure of its first row started.
    pub exposure_start: Timestamp,
    /// How long each row was exposed.
    pub exposure: Ms,
    /// For a rolling shutter, how much later each row starts than the row
    /// above it; `None` for a global shutter.
    pub line_time: Option<Ms>,
    /// The gain, in the camera's units.
    pub gain: f64,
    /// The value of black, in the frame's sample units.
    pub black_level: f64,
    /// The objective fitted, by its catalogue name.
    pub objective: Option<String>,
    /// The lights, as they were.
    pub lights: Vec<LightState>,
    /// The stage's position at the exposure's midpoint, from its position
    /// track.
    pub position: Option<Xyz>,
    /// How many frames the camera dropped just before this one.
    pub dropped_before: u64,
}

impl FrameMeta {
    /// The metadata of a frame of the whole sensor, exposed when it arrived:
    /// the rest is left empty, for the producer to fill in.
    pub fn new(
        id: FrameId,
        format: PixelFormat,
        width: u32,
        height: u32,
        host_time: Timestamp,
    ) -> Self {
        Self {
            id,
            format,
            width,
            height,
            roi: PixelRect::new(0, 0, width, height),
            binning: 1,
            mode: String::new(),
            host_time,
            sensor_time: None,
            exposure_start: host_time,
            exposure: Ms::ZERO,
            line_time: None,
            gain: 0.0,
            black_level: 0.0,
            objective: None,
            lights: Vec::new(),
            position: None,
            dropped_before: 0,
        }
    }

    /// When row `row` started and stopped being exposed. With a rolling
    /// shutter, each row starts a line time after the one above it.
    pub fn row_exposure(&self, row: u32) -> (Timestamp, Timestamp) {
        let delay = self
            .line_time
            .and_then(|line| (line * f64::from(row)).to_duration());
        let start = self.exposure_start + delay.unwrap_or_default();
        (
            start,
            start + self.exposure.to_duration().unwrap_or_default(),
        )
    }

    /// The middle of the exposure: of the middle row, with a rolling
    /// shutter. The stage's position is interpolated to this moment.
    pub fn exposure_midpoint(&self) -> Timestamp {
        let (start, end) = self.row_exposure(self.height / 2);
        start.midpoint(end)
    }
}
