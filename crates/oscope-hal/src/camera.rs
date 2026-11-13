//! The camera: its modes, settings and capabilities, and the [`Camera`]
//! trait.
//!
//! **A camera streams into a [`FrameSink`].** For each frame it takes a slab
//! from the [`FramePool`] it was given, fills it, and hands it to the sink
//! with the metadata it knows. Acquisition then adds what only it knows (the
//! stage's position, the objective, the lights), seals the frame, and
//! publishes it on the frame bus (ADR-0008, ADR-0012).

use std::sync::Arc;

use oscope_core::{FrameBuffer, FrameMeta, FramePool, Ms, PixelFormat, PixelRect, Um};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::error::unsupported;
use crate::{Error, HalFuture, Result};

/// How the sensor's rows are exposed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Shutter {
    /// Every row at once.
    Global,
    /// Row after row, each a line time after the one above, so motion shears
    /// the frame. The MIChrome 20's is (P02b).
    Rolling,
}

/// The fastest frame rate of a mode in one pixel format, at a short
/// exposure.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ModeRate {
    /// The pixel format.
    pub format: PixelFormat,
    /// Frames per second.
    pub fps: f64,
}

/// One of a camera's modes: a frame size, its binning, and its rates.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CameraMode {
    /// Its name, by which settings choose it.
    pub name: String,
    /// The frame's width in pixels, without an ROI.
    pub width: u32,
    /// The frame's height in pixels, without an ROI.
    pub height: u32,
    /// How many sensor pixels each of its pixels combines, along each axis.
    pub binning: u32,
    /// The pixel formats it offers, each with its fastest rate.
    pub rates: Vec<ModeRate>,
    /// With a rolling shutter, how much later each row starts than the one
    /// above it.
    pub line_time: Option<Ms>,
}

impl CameraMode {
    /// Its fastest rate in `format`, or `None` if it doesn't offer that
    /// format.
    pub fn fps(&self, format: PixelFormat) -> Option<f64> {
        self.rates
            .iter()
            .find(|r| r.format == format)
            .map(|r| r.fps)
    }
}

/// The ROIs a camera accepts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RoiRules {
    /// The camera rounds an ROI's x offset and width down to multiples of
    /// this. The MIChrome 20's is 2 (P02b).
    pub x_step: u32,
    /// The same, for the y offset and height.
    pub y_step: u32,
    /// The least width and height.
    pub min_size: u32,
    /// Whether a smaller ROI raises the frame rate. The MIChrome 20's
    /// doesn't (P02b).
    pub raises_rate: bool,
}

/// What starts each exposure.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum Trigger {
    /// Nothing: the camera runs freely, frame after frame.
    #[default]
    FreeRunning,
    /// Each call to [`Camera::trigger`].
    Software,
    /// An edge on the camera's trigger input, such as a pulse from the
    /// stage's trigger output (D20).
    External {
        /// Which edge.
        edge: Edge,
    },
}

/// An edge of a trigger signal.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Edge {
    /// From low to high.
    Rising,
    /// From high to low.
    Falling,
}

/// How a camera can be triggered. A camera always runs freely.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TriggerCapabilities {
    /// By software.
    pub software: bool,
    /// By its trigger input. The MIChrome 20 has none; the camera that will
    /// replace it has one (P02b).
    pub external: bool,
    /// Whether it has a trigger output, which marks each exposure.
    pub output: bool,
}

/// What a camera is and offers. It doesn't change while the camera is open.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CameraCapabilities {
    /// The camera's maker and model.
    pub model: String,
    /// The sensor's width in pixels.
    pub sensor_width: u32,
    /// The sensor's height in pixels.
    pub sensor_height: u32,
    /// The size of a sensor pixel.
    pub pixel_size: Um,
    /// How its rows are exposed.
    pub shutter: Shutter,
    /// Its modes. The first is the default.
    pub modes: Vec<CameraMode>,
    /// The shortest exposure.
    pub exposure_min: Ms,
    /// The longest exposure.
    pub exposure_max: Ms,
    /// The least gain, in the camera's units.
    pub gain_min: f64,
    /// The most gain.
    pub gain_max: f64,
    /// The ROIs it accepts, or `None` if it accepts none.
    pub roi: Option<RoiRules>,
    /// How it can be triggered.
    pub trigger: TriggerCapabilities,
    /// Whether its frames carry the sensor's own timestamps. The
    /// MIChrome 20's don't (P02b).
    pub sensor_timestamps: bool,
    /// Whether it reports its sensor's temperature.
    pub temperature: bool,
}

impl CameraCapabilities {
    /// The mode named `name`.
    pub fn mode(&self, name: &str) -> Result<&CameraMode> {
        self.modes
            .iter()
            .find(|m| m.name == name)
            .ok_or_else(|| Error::Unknown {
                kind: "camera mode",
                name: name.to_owned(),
            })
    }

    /// An ROI as the camera would apply it: its x offset and width rounded
    /// down to multiples of the x step, and its y offset and height to the y
    /// step. `None` if the camera accepts no ROI.
    pub fn round_roi(&self, roi: PixelRect) -> Option<PixelRect> {
        let rules = self.roi?;
        let down = |value: u32, step: u32| value - value % step.max(1);
        Some(PixelRect::new(
            down(roi.x, rules.x_step),
            down(roi.y, rules.y_step),
            down(roi.width, rules.x_step),
            down(roi.height, rules.y_step),
        ))
    }

    /// Checks `settings` against these capabilities, and returns them as the
    /// camera would apply them, with the ROI rounded. Drivers call it first
    /// in [`Camera::configure`].
    ///
    /// It checks the mode and its pixel format, the exposure, the gain, the
    /// ROI (accepted at all, inside the sensor, at least the least size), and
    /// the trigger.
    pub fn check(&self, settings: &CameraSettings) -> Result<CameraSettings> {
        let mode = self.mode(&settings.mode)?;
        if mode.fps(settings.format).is_none() {
            return Err(Error::Unsupported {
                what: format!("{:?} in mode {:?}", settings.format, mode.name),
            });
        }
        Error::check_range(
            "the exposure in ms",
            settings.exposure.0,
            self.exposure_min.0,
            self.exposure_max.0,
        )?;
        Error::check_range("the gain", settings.gain, self.gain_min, self.gain_max)?;
        let mut applied = settings.clone();
        if let Some(roi) = settings.roi {
            let rounded = self.round_roi(roi).ok_or_else(|| Error::Unsupported {
                what: "an ROI".to_owned(),
            })?;
            let least = self.roi.map_or(1, |rules| rules.min_size.max(1));
            let size = rounded.width.min(rounded.height);
            Error::check_range(
                "the ROI's least side in pixels",
                size.into(),
                least.into(),
                self.sensor_width.min(self.sensor_height).into(),
            )?;
            let sensor = PixelRect::new(0, 0, self.sensor_width, self.sensor_height);
            if !sensor.contains(&rounded) {
                return Err(Error::Unsupported {
                    what: format!("an ROI outside the sensor: {rounded:?}"),
                });
            }
            applied.roi = Some(rounded);
        }
        let offered = match settings.trigger {
            Trigger::FreeRunning => true,
            Trigger::Software => self.trigger.software,
            Trigger::External { .. } => self.trigger.external,
        };
        if !offered {
            return Err(Error::Unsupported {
                what: format!("the trigger {:?}", settings.trigger),
            });
        }
        Ok(applied)
    }
}

/// What a camera is set to.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CameraSettings {
    /// The mode, by name.
    pub mode: String,
    /// The pixel format of its frames.
    pub format: PixelFormat,
    /// How long each row is exposed.
    pub exposure: Ms,
    /// The gain, in the camera's units.
    pub gain: f64,
    /// The sensor's pixels to read, in full-sensor coordinates (ADR-0007),
    /// or `None` for the whole sensor.
    pub roi: Option<PixelRect>,
    /// What starts each exposure.
    pub trigger: Trigger,
}

/// Where a streaming camera delivers each frame: a filled slab, and the
/// metadata the camera knows.
///
/// It is called on the camera's own thread, once for each frame, in order,
/// and it may block (ADR-0008). Acquisition completes the metadata, seals
/// the frame and publishes it.
pub type FrameSink = Arc<dyn Fn(FrameBuffer, FrameMeta) + Send + Sync>;

/// A camera (plan §5.9).
pub trait Camera: Send + Sync {
    /// What it is and offers.
    fn capabilities(&self) -> &CameraCapabilities;

    /// What it is set to.
    fn settings(&self) -> HalFuture<'_, CameraSettings>;

    /// Applies `settings`, all or none, and returns them as the camera
    /// applied them: it may round an ROI, say.
    ///
    /// While it streams, a change of mode, pixel format, ROI or trigger
    /// fails with [`Error::Busy`]. Exposure and gain may change if the
    /// camera allows it.
    fn configure(&self, settings: CameraSettings) -> HalFuture<'_, CameraSettings>;

    /// Starts streaming frames into `sink`, in slabs from `pool`.
    ///
    /// When the pool has no free slab, the frame is lost, and the next
    /// frame's `dropped_before` counts it. Starting while streaming fails
    /// with [`Error::Busy`].
    fn start(&self, pool: FramePool, sink: FrameSink) -> HalFuture<'_, ()>;

    /// Stops streaming. Once it returns, the sink isn't called again.
    /// Stopping a camera that isn't streaming does nothing.
    fn stop(&self) -> HalFuture<'_, ()>;

    /// Whether it is streaming.
    fn streaming(&self) -> bool;

    /// Starts one exposure, under [`Trigger::Software`].
    fn trigger(&self) -> HalFuture<'_, ()> {
        unsupported("a software trigger")
    }

    /// The sensor's temperature, in °C.
    fn temperature(&self) -> HalFuture<'_, f64> {
        unsupported("a temperature reading")
    }
}
