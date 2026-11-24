//! The `illumination` Thing: the lights' channels, on, off and dimmed
//! (plan §5.4; P16).

use oscope_core::LightState;
use oscope_hal::IlluminationCapabilities;
use teta_wot::prelude::*;

use super::{Link, action_error, property_error};

/// The lights: named channels, each on or off at an intensity. Every frame
/// records them as they were.
#[derive(Thing)]
#[thing(
    title = "Illumination",
    semantic_type = "saref:Actuator",
    context(saref = "https://saref.etsi.org/core/")
)]
pub struct IlluminationThing {
    /// The lights as last set.
    ///
    /// Saved, and set again when the server starts.
    #[setting(readonly)]
    saved: Prop<Vec<LightState>>,
    link: Link,
}

#[thing_impl]
impl IlluminationThing {
    #[on_start]
    async fn connect(&self, ctx: ThingCtx) -> anyhow::Result<()> {
        self.link.connect(&ctx)?;
        let devices = self.link.property().map_err(|e| anyhow::anyhow!("{e}"))?;
        for light in self.saved.get() {
            if let Err(error) = devices.lights.set(light).await {
                tracing::warn!("a saved light wasn't set: {error}");
            }
        }
        let state = devices
            .lights
            .state()
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        devices.acquisition.set_lights(state);
        Ok(())
    }

    /// What the lights are: their channels, and which can dim.
    #[property]
    async fn capabilities(&self) -> Result<IlluminationCapabilities, PropertyError> {
        Ok(self.link.property()?.lights.capabilities().clone())
    }

    /// Each channel's state.
    #[property]
    async fn lights(&self) -> Result<Vec<LightState>, PropertyError> {
        let devices = self.link.property()?;
        devices.lights.state().await.map_err(property_error)
    }

    /// Sets a light.
    ///
    /// `channel` on or off, at `intensity` from 0 to 1. It returns the light
    /// as applied: one that can't dim is at full intensity.
    #[action]
    async fn set_light(
        &self,
        channel: String,
        on: bool,
        #[param(default = 1.0)] intensity: f64,
    ) -> Result<LightState, ActionError> {
        let devices = self.link.action()?;
        let _turn = devices.gates.lights.lock().await;
        let light = LightState {
            channel,
            on,
            intensity,
        };
        let applied = devices.lights.set(light).await.map_err(action_error)?;
        let state = devices.lights.state().await.map_err(action_error)?;
        devices.acquisition.set_lights(state.clone());
        self.saved
            .set(state)
            .map_err(|e| ActionError::handled(e.to_string()))?;
        Ok(applied)
    }

    /// Forgets the lights saved.
    ///
    /// They stay as they are; the next start leaves them as they start.
    #[action]
    async fn forget_saved(&self) -> Result<(), ActionError> {
        self.link.action()?;
        self.saved
            .set(Vec::new())
            .map_err(|e| ActionError::handled(e.to_string()))
    }
}
