//! The lights: their channels and capabilities, the [`Illumination`] trait,
//! and [`ManualIllumination`].
//!
//! The lights aren't chosen yet (P02b), so the trait stays generic: named
//! channels, each on or off, at an intensity from 0 to 1.

use std::sync::{Mutex, PoisonError};

use oscope_core::LightState;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{Error, HalFuture, Result};

/// Where a light shines from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LightKind {
    /// Through the sample, from below: brightfield.
    Transmitted,
    /// Onto the sample, through the objective or beside it.
    Reflected,
    /// Anything else a controller offers.
    Other,
}

/// One light.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LightChannel {
    /// Its name, unique among the channels, such as `transmitted`.
    pub name: String,
    /// Where it shines from.
    pub kind: LightKind,
    /// Whether its intensity can be set. One that can't is on at full
    /// intensity, or off.
    pub dimmable: bool,
}

/// What the lights are and offer. It doesn't change while they are open.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct IlluminationCapabilities {
    /// The controller's maker and model, or `manual`.
    pub model: String,
    /// Its channels.
    pub channels: Vec<LightChannel>,
}

impl IlluminationCapabilities {
    /// Checks `light` against these capabilities, and returns it as the
    /// lights would apply it: a light that can't dim at full intensity.
    pub fn check(&self, light: &LightState) -> Result<LightState> {
        let channel = self
            .channels
            .iter()
            .find(|c| c.name == light.channel)
            .ok_or_else(|| Error::Unknown {
                kind: "light channel",
                name: light.channel.clone(),
            })?;
        Error::check_range("the intensity", light.intensity, 0.0, 1.0)?;
        let mut applied = light.clone();
        if !channel.dimmable {
            applied.intensity = 1.0;
        }
        Ok(applied)
    }
}

/// The lights (plan §5.9).
pub trait Illumination: Send + Sync {
    /// What they are and offer.
    fn capabilities(&self) -> &IlluminationCapabilities;

    /// Each channel's state, in the order of the capabilities' channels.
    fn state(&self) -> HalFuture<'_, Vec<LightState>>;

    /// Sets one channel, and returns it as applied.
    fn set(&self, light: LightState) -> HalFuture<'_, LightState>;
}

/// Lights that the user sets by hand. OScope can't switch them, so it
/// records what the user says they set, for the frames' metadata.
#[derive(Debug)]
pub struct ManualIllumination {
    capabilities: IlluminationCapabilities,
    state: Mutex<Vec<LightState>>,
}

impl ManualIllumination {
    /// Lights with these channels, all off at first, at full intensity.
    pub fn new(channels: Vec<LightChannel>) -> Self {
        let state = channels
            .iter()
            .map(|c| LightState {
                channel: c.name.clone(),
                on: false,
                intensity: 1.0,
            })
            .collect();
        Self {
            capabilities: IlluminationCapabilities {
                model: "manual".to_owned(),
                channels,
            },
            state: Mutex::new(state),
        }
    }

    /// The brightfield microscope's light: one dimmable transmitted channel,
    /// named `transmitted`.
    pub fn brightfield() -> Self {
        Self::new(vec![LightChannel {
            name: "transmitted".to_owned(),
            kind: LightKind::Transmitted,
            dimmable: true,
        }])
    }
}

impl Illumination for ManualIllumination {
    fn capabilities(&self) -> &IlluminationCapabilities {
        &self.capabilities
    }

    fn state(&self) -> HalFuture<'_, Vec<LightState>> {
        let state = self
            .state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        Box::pin(async move { Ok(state) })
    }

    fn set(&self, light: LightState) -> HalFuture<'_, LightState> {
        Box::pin(async move {
            let applied = self.capabilities.check(&light)?;
            let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
            if let Some(slot) = state.iter_mut().find(|s| s.channel == applied.channel) {
                *slot = applied.clone();
            }
            Ok(applied)
        })
    }
}
