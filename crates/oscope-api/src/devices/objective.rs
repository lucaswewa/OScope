//! The `objective` Thing: the catalogue, and the objective fitted (plan
//! §5.4; P16).

use oscope_hal::{Objective, ObjectiveCatalogue, ObjectiveMountCapabilities};
use teta_wot::prelude::*;

use super::{Link, action_error, property_error};

/// The objective mount. On the instrument, objectives are swapped by hand
/// (D18): fitting one records which, for every frame's metadata and, on
/// the simulator, for what the camera sees.
#[derive(Thing)]
#[thing(
    title = "Objective",
    semantic_type = "saref:Device",
    context(saref = "https://saref.etsi.org/core/")
)]
pub struct ObjectiveThing {
    /// The objective last fitted, by name.
    ///
    /// Saved, and fitted again when the server starts.
    #[setting(readonly)]
    saved: Prop<Option<String>>,
    link: Link,
}

#[thing_impl]
impl ObjectiveThing {
    #[on_start]
    async fn connect(&self, ctx: ThingCtx) -> anyhow::Result<()> {
        self.link.connect(&ctx)?;
        let devices = self.link.property().map_err(|e| anyhow::anyhow!("{e}"))?;
        if let Some(name) = self.saved.get()
            && let Err(error) = devices.objectives.fit(name).await
        {
            tracing::warn!("the saved objective wasn't fitted: {error}");
        }
        let fitted = devices
            .objectives
            .fitted()
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        devices.acquisition.set_objective(fitted.map(|o| o.name));
        Ok(())
    }

    /// What the mount is: manual, or motorised.
    #[property]
    async fn capabilities(&self) -> Result<ObjectiveMountCapabilities, PropertyError> {
        Ok(self.link.property()?.objectives.capabilities().clone())
    }

    /// The objectives that can be fitted, and the instrument's tube lens.
    #[property]
    async fn catalogue(&self) -> Result<ObjectiveCatalogue, PropertyError> {
        Ok(self.link.property()?.objectives.catalogue().clone())
    }

    /// The objective fitted, if one is said to be.
    #[property]
    async fn fitted(&self) -> Result<Option<Objective>, PropertyError> {
        let devices = self.link.property()?;
        devices.objectives.fitted().await.map_err(property_error)
    }

    /// A camera pixel's side at the sample.
    ///
    /// Through the fitted objective: the camera's pixel over the
    /// magnification on the instrument. Until calibration (P32) measures it,
    /// this is what it should be.
    #[property(unit = "µm")]
    async fn pixel_at_sample(&self) -> Result<Option<f64>, PropertyError> {
        let devices = self.link.property()?;
        let fitted = devices.objectives.fitted().await.map_err(property_error)?;
        let pixel = devices.camera.capabilities().pixel_size.0;
        let catalogue = devices.objectives.catalogue();
        Ok(fitted.map(|o| pixel / catalogue.magnification(&o)))
    }

    /// Records which objective is fitted.
    ///
    /// `name`, from the catalogue. A name the catalogue doesn't have is
    /// refused.
    #[action]
    async fn fit(&self, name: String) -> Result<Objective, ActionError> {
        let devices = self.link.action()?;
        let _turn = devices.gates.objective.lock().await;
        let objective = devices.objectives.fit(name).await.map_err(action_error)?;
        devices
            .acquisition
            .set_objective(Some(objective.name.clone()));
        self.saved
            .set(Some(objective.name.clone()))
            .map_err(|e| ActionError::handled(e.to_string()))?;
        Ok(objective)
    }
}
