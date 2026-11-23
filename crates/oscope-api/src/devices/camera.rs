//! The `camera` Thing: its settings, its modes, and streaming into
//! acquisition (plan §5.4; P16).

use oscope_core::{FramePool, HeapAllocator, Ms, PixelFormat, PixelRect};
use oscope_hal::{CameraCapabilities, CameraSettings, Trigger};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use teta_wot::prelude::*;

use super::{Link, action_error, property_error};

/// Frames the pool holds while the camera streams.
const SLABS: usize = 8;

/// A mode and a pixel format, set together.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ModeChange {
    /// The mode, by name.
    pub mode: String,
    /// The pixel format.
    pub format: PixelFormat,
}

/// The camera: its settings, and streaming. Frames go to acquisition,
/// which completes them with the stage's position, the objective and the
/// lights, and publishes them on the frame bus.
#[derive(Thing)]
#[thing(
    title = "Camera",
    semantic_type = "saref:Sensor",
    context(saref = "https://saref.etsi.org/core/")
)]
pub struct CameraThing {
    /// The settings as last applied.
    ///
    /// Saved, and applied again when the server starts, if the camera still
    /// takes them.
    #[setting(readonly)]
    saved: Prop<Option<CameraSettings>>,
    /// The mode or the pixel format changed.
    #[event]
    mode_changed: Event<ModeChange>,
    link: Link,
}

impl CameraThing {
    /// The camera's settings, changed by `change`, applied as the camera
    /// applies them, and saved.
    async fn change(&self, change: impl FnOnce(&mut CameraSettings)) -> Result<(), PropertyError> {
        let devices = self.link.property()?;
        let _turn = devices.gates.camera.lock().await;
        let mut settings = devices.camera.settings().await.map_err(property_error)?;
        change(&mut settings);
        let applied = devices
            .camera
            .configure(settings)
            .await
            .map_err(property_error)?;
        self.saved.set(Some(applied))
    }

    async fn read(&self) -> Result<CameraSettings, PropertyError> {
        let devices = self.link.property()?;
        devices.camera.settings().await.map_err(property_error)
    }
}

#[thing_impl]
impl CameraThing {
    #[on_start]
    async fn connect(&self, ctx: ThingCtx) -> anyhow::Result<()> {
        self.link.connect(&ctx)?;
        if let Some(saved) = self.saved.get() {
            let devices = self.link.property().map_err(|e| anyhow::anyhow!("{e}"))?;
            match devices.camera.configure(saved).await {
                Ok(applied) => self
                    .saved
                    .set(Some(applied))
                    .map_err(|e| anyhow::anyhow!("{e}"))?,
                // A camera that no longer takes them keeps its own.
                Err(error) => tracing::warn!("the saved camera settings weren't applied: {error}"),
            }
        }
        Ok(())
    }

    /// What the camera is and offers.
    ///
    /// Its sensor, modes and rates, pixel formats, exposure and gain ranges,
    /// ROI rules and triggers.
    #[property]
    async fn capabilities(&self) -> Result<CameraCapabilities, PropertyError> {
        Ok(self.link.property()?.camera.capabilities().clone())
    }

    /// All its settings.
    ///
    /// The mode, pixel format, exposure, gain, ROI and trigger.
    #[property]
    async fn settings(&self) -> Result<CameraSettings, PropertyError> {
        self.read().await
    }

    /// The exposure of each row.
    #[property(unit = "ms")]
    async fn exposure(&self) -> Result<f64, PropertyError> {
        Ok(self.read().await?.exposure.0)
    }

    #[setter(exposure)]
    async fn set_exposure(&self, value: f64) -> Result<(), PropertyError> {
        self.change(|s| s.exposure = Ms(value)).await
    }

    /// The gain, in the camera's units.
    #[property]
    async fn gain(&self) -> Result<f64, PropertyError> {
        Ok(self.read().await?.gain)
    }

    #[setter(gain)]
    async fn set_gain(&self, value: f64) -> Result<(), PropertyError> {
        self.change(|s| s.gain = value).await
    }

    /// The region of the sensor read out.
    ///
    /// In full-sensor pixels, or none for the whole sensor. The camera rounds
    /// it to its rules.
    #[property]
    async fn roi(&self) -> Result<Option<PixelRect>, PropertyError> {
        Ok(self.read().await?.roi)
    }

    #[setter(roi)]
    async fn set_roi(&self, value: Option<PixelRect>) -> Result<(), PropertyError> {
        self.change(|s| s.roi = value).await
    }

    /// What starts each exposure.
    #[property]
    async fn trigger(&self) -> Result<Trigger, PropertyError> {
        Ok(self.read().await?.trigger)
    }

    #[setter(trigger)]
    async fn set_trigger(&self, value: Trigger) -> Result<(), PropertyError> {
        self.change(|s| s.trigger = value).await
    }

    /// The mode, by name: set with `set_mode`.
    #[property]
    async fn mode(&self) -> Result<String, PropertyError> {
        Ok(self.read().await?.mode)
    }

    /// The pixel format: set with `set_mode`.
    #[property]
    async fn format(&self) -> Result<PixelFormat, PropertyError> {
        Ok(self.read().await?.format)
    }

    /// Whether it is streaming.
    #[property]
    async fn streaming(&self) -> Result<bool, PropertyError> {
        Ok(self.link.property()?.camera.streaming())
    }

    /// The sensor's temperature, if the camera reports one.
    #[property(unit = "°C")]
    async fn temperature(&self) -> Result<f64, PropertyError> {
        let devices = self.link.property()?;
        devices.camera.temperature().await.map_err(property_error)
    }

    /// Sets the mode and the pixel format together.
    ///
    /// Refused while streaming.
    #[action]
    async fn set_mode(
        &self,
        mode: String,
        format: PixelFormat,
    ) -> Result<CameraSettings, ActionError> {
        let devices = self.link.action()?;
        let _turn = devices.gates.camera.lock().await;
        let mut settings = devices.camera.settings().await.map_err(action_error)?;
        settings.mode = mode;
        settings.format = format;
        let applied = devices
            .camera
            .configure(settings)
            .await
            .map_err(action_error)?;
        self.saved
            .set(Some(applied.clone()))
            .map_err(|e| ActionError::handled(e.to_string()))?;
        self.mode_changed.emit(ModeChange {
            mode: applied.mode.clone(),
            format: applied.format,
        });
        Ok(applied)
    }

    /// Starts streaming, as set, into acquisition.
    #[action]
    async fn start_streaming(&self) -> Result<(), ActionError> {
        let devices = self.link.action()?;
        let _turn = devices.gates.camera.lock().await;
        let settings = devices.camera.settings().await.map_err(action_error)?;
        let (_, width, height) = devices
            .camera
            .capabilities()
            .frame_shape(&settings)
            .map_err(action_error)?;
        // Pinned memory when the simulated camera renders on the GPU, which
        // copies each frame back into it by DMA (ADR-0062).
        #[cfg(feature = "cuda")]
        let pinned = devices
            .simulator
            .as_ref()
            .and_then(|s| s.camera.pinned_allocator());
        #[cfg(feature = "cuda")]
        let allocator: &dyn oscope_core::SlabAllocator = match &pinned {
            Some(pinned) => pinned,
            None => &HeapAllocator,
        };
        #[cfg(not(feature = "cuda"))]
        let allocator = &HeapAllocator;
        let pool = FramePool::new(settings.format.frame_bytes(width, height), SLABS, allocator);
        let sink = devices.acquisition.sink();
        devices.camera.start(pool, sink).await.map_err(action_error)
    }

    /// Stops streaming.
    #[action]
    async fn stop_streaming(&self) -> Result<(), ActionError> {
        let devices = self.link.action()?;
        let _turn = devices.gates.camera.lock().await;
        devices.camera.stop().await.map_err(action_error)
    }

    /// Starts one exposure, with the software trigger set.
    #[action]
    async fn trigger_exposure(&self) -> Result<(), ActionError> {
        let devices = self.link.action()?;
        devices.camera.trigger().await.map_err(action_error)
    }
}
