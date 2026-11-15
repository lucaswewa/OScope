//! The `simulation` Thing: the simulator's samples, loaded and unloaded,
//! and how it is seen (plan §5.4; P16). Served only with the simulator.

use std::sync::Arc;

use oscope_sim::{Rate, SampleEntry, Simulator};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use teta_wot::prelude::*;

use super::Link;

/// A sample in the catalogue.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SampleInfo {
    /// Its name, which loads it.
    pub name: String,
    /// What it is: tissue, test-target, surface, cells, structures, image
    /// or stack.
    pub kind: String,
    /// Whether it carries ground truth: every procedural sample does, an
    /// import doesn't.
    pub ground_truth: bool,
}

/// How the camera's cache of rendered tiles has done.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TileCacheStats {
    /// Tiles found in the cache.
    pub hits: u64,
    /// Tiles rendered.
    pub misses: u64,
    /// Tiles held.
    pub tiles: usize,
    /// The bytes they hold.
    pub bytes: usize,
}

/// The simulator: its sample catalogue, the slide on the stage, and
/// whether fluorescence is seen through a confocal's pinhole.
#[derive(Thing)]
#[thing(
    title = "Simulation",
    semantic_type = "saref:Device",
    context(saref = "https://saref.etsi.org/core/")
)]
pub struct SimulationThing {
    link: Link,
}

impl SimulationThing {
    fn simulator(&self) -> Result<&Arc<Simulator>, ActionError> {
        self.link
            .action()?
            .simulator
            .as_ref()
            .ok_or_else(|| ActionError::handled("no device is simulated"))
    }

    fn simulator_property(&self) -> Result<&Arc<Simulator>, PropertyError> {
        self.link
            .property()?
            .simulator
            .as_ref()
            .ok_or_else(|| PropertyError::failed(anyhow::anyhow!("no device is simulated")))
    }
}

#[thing_impl]
impl SimulationThing {
    #[on_start]
    async fn connect(&self, ctx: ThingCtx) -> anyhow::Result<()> {
        self.link.connect(&ctx)
    }

    /// The samples that can be loaded, and which carry ground truth.
    #[property]
    async fn samples(&self) -> Result<Vec<SampleInfo>, PropertyError> {
        let simulator = self.simulator_property()?;
        Ok(simulator
            .loader
            .catalogue()
            .samples
            .iter()
            .map(|s| SampleInfo {
                name: s.name.clone(),
                kind: s.kind.as_str().to_owned(),
                ground_truth: s.has_ground_truth(),
            })
            .collect())
    }

    /// The sample on the stage, or none.
    #[property]
    async fn loaded(&self) -> Result<Option<String>, PropertyError> {
        Ok(self.simulator_property()?.loader.loaded())
    }

    /// How fast the simulator's clock runs.
    ///
    /// 1 is real time; 0, stepped by tests.
    #[property]
    async fn rate(&self) -> Result<f64, PropertyError> {
        Ok(match self.simulator_property()?.clock.rate() {
            Rate::RealTime => 1.0,
            Rate::Scaled(k) => k,
            Rate::Stepped => 0.0,
        })
    }

    /// How the camera's cache of rendered tiles has done.
    #[property]
    async fn tile_cache(&self) -> Result<TileCacheStats, PropertyError> {
        let stats = self.simulator_property()?.camera.cache_stats();
        Ok(TileCacheStats {
            hits: stats.hits,
            misses: stats.misses,
            tiles: stats.tiles,
            bytes: stats.bytes,
        })
    }

    /// Whether fluorescence is seen through a confocal's pinhole.
    #[property]
    async fn confocal(&self) -> Result<bool, PropertyError> {
        Ok(self.simulator_property()?.lights.confocal())
    }

    #[setter(confocal)]
    async fn set_confocal(&self, value: bool) -> Result<(), PropertyError> {
        self.simulator_property()?.lights.set_confocal(value);
        Ok(())
    }

    /// Loads a sample onto the stage.
    ///
    /// The one called `name`, in place of any other.
    #[action]
    async fn load(&self, name: String) -> Result<(), ActionError> {
        self.simulator()?
            .loader
            .load(&name)
            .map_err(ActionError::handled)
    }

    /// Where the simulated slide is sharpest.
    ///
    /// The stage's z, in µm, at which the slide is in focus at stage point
    /// (`x`, `y`): the truth autofocus is held to (P34). None with no slide.
    #[action]
    async fn in_focus(&self, x: f64, y: f64) -> Result<Option<f64>, ActionError> {
        let world = self.simulator()?.camera.world();
        let world = world
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        Ok(world.in_focus(oscope_core::Point2::new(x, y)).map(|z| z.0))
    }

    /// Takes the slide off the stage, and says which sample it was.
    #[action]
    async fn unload(&self) -> Result<Option<String>, ActionError> {
        Ok(self.simulator()?.loader.unload())
    }

    /// Adds your own image or stack to the catalogue.
    ///
    /// As `name`: a PNG, JPEG or TIFF on the server's disk, at
    /// `um_per_pixel`. A stack (a multi-page TIFF) gives its `z_step`. Its
    /// values are `transmitted` light, `reflected` light, or `fluorescent`
    /// amounts of `dye`.
    #[action]
    async fn import(
        &self,
        name: String,
        path: String,
        um_per_pixel: f64,
        #[param(default)] z_step: Option<f64>,
        #[param(default = "transmitted".to_owned())] light: String,
        #[param(default = "green".to_owned())] dye: String,
    ) -> Result<(), ActionError> {
        let kind = if z_step.is_some() { "stack" } else { "image" };
        let entry: SampleEntry = serde_json::from_value(json!({
            "name": name, "kind": kind, "path": path, "um_per_pixel": um_per_pixel,
            "z_step": z_step, "light": light, "dye": dye,
        }))
        .map_err(|e| ActionError::handled(format!("the import: {e}")))?;
        self.simulator()?
            .loader
            .add(entry)
            .map_err(ActionError::handled)
    }
}
