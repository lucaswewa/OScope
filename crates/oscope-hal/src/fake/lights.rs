//! The fake lights and objective mount.

use oscope_core::LightState;

use super::Script;
use crate::{
    HalFuture, Illumination, IlluminationCapabilities, LightChannel, LightKind, ManualIllumination,
    ManualObjectiveMount, Objective, ObjectiveCatalogue, ObjectiveMount,
    ObjectiveMountCapabilities,
};

/// Lights that behave as [`ManualIllumination`] does, with a script.
pub struct FakeIllumination {
    capabilities: IlluminationCapabilities,
    lights: ManualIllumination,
    script: Script,
}

impl FakeIllumination {
    /// Fake lights with these channels, all off at first.
    pub fn new(channels: Vec<LightChannel>) -> Self {
        let lights = ManualIllumination::new(channels);
        let capabilities = IlluminationCapabilities {
            model: "fake lights".to_owned(),
            ..lights.capabilities().clone()
        };
        Self {
            capabilities,
            lights,
            script: Script::default(),
        }
    }

    /// A dimmable transmitted light, `transmitted`, and a reflected one
    /// that can't dim, `ring`.
    pub fn two_lights() -> Self {
        Self::new(vec![
            LightChannel {
                name: "transmitted".to_owned(),
                kind: LightKind::Transmitted,
                dimmable: true,
            },
            LightChannel {
                name: "ring".to_owned(),
                kind: LightKind::Reflected,
                dimmable: false,
            },
        ])
    }

    /// Its script.
    pub fn script(&self) -> &Script {
        &self.script
    }
}

impl Illumination for FakeIllumination {
    fn capabilities(&self) -> &IlluminationCapabilities {
        &self.capabilities
    }

    fn state(&self) -> HalFuture<'_, Vec<LightState>> {
        Box::pin(async move {
            self.script.call("state")?;
            self.lights.state().await
        })
    }

    fn set(&self, light: LightState) -> HalFuture<'_, LightState> {
        Box::pin(async move {
            self.script.call("set")?;
            self.lights.set(light).await
        })
    }
}

/// A motorised objective mount that turns to any objective at once, and
/// otherwise behaves as [`ManualObjectiveMount`] does, with a script.
pub struct FakeObjectiveMount {
    capabilities: ObjectiveMountCapabilities,
    mount: ManualObjectiveMount,
    script: Script,
}

impl FakeObjectiveMount {
    /// A mount for the objectives of `catalogue`, with none fitted yet.
    pub fn new(catalogue: ObjectiveCatalogue) -> Self {
        Self {
            capabilities: ObjectiveMountCapabilities {
                model: "fake mount".to_owned(),
                motorised: true,
            },
            mount: ManualObjectiveMount::new(catalogue),
            script: Script::default(),
        }
    }

    /// Its script.
    pub fn script(&self) -> &Script {
        &self.script
    }
}

impl ObjectiveMount for FakeObjectiveMount {
    fn capabilities(&self) -> &ObjectiveMountCapabilities {
        &self.capabilities
    }

    fn catalogue(&self) -> &ObjectiveCatalogue {
        self.mount.catalogue()
    }

    fn fitted(&self) -> HalFuture<'_, Option<Objective>> {
        Box::pin(async move {
            self.script.call("fitted")?;
            self.mount.fitted().await
        })
    }

    fn fit(&self, name: String) -> HalFuture<'_, Objective> {
        Box::pin(async move {
            self.script.call("fit")?;
            self.mount.fit(name).await
        })
    }
}
