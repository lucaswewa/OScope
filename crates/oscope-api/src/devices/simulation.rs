//! The `simulation` Thing: the simulator's samples, loaded and unloaded,
//! and how it is seen (plan §5.4; P16). Served only with the simulator.

// `import`'s parameters are its action's inputs, each its own field, as
// the Thing Description gives them: the wrapper the Thing's macro makes
// for it takes as many.
#![allow(clippy::too_many_arguments)]

use std::sync::Arc;
use std::sync::atomic::Ordering;

use oscope_sim::{Rate, SampleEntry, Simulator};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use teta_wot::prelude::*;
use uuid::Uuid;

use super::{Link, property_refused};
use crate::server::Roots;
use crate::upload::{Uploads, staged};

/// A sample in the catalogue.
/// A fault to inject into the simulator (P50).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Fault {
    /// The camera, unplugged.
    CameraUnplugged,
    /// The stage in alarm, as a limit switch leaves it.
    StageAlarm,
    /// The scratch disk, full.
    ScratchFull,
}

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

/// The fastest the clock may be written to run.
const MOST_RATE: f64 = 1_000.0;

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
    /// 1 is real time; 0, stepped by tests. It may be written, above 0 and
    /// up to 1,000: the app's end-to-end tests run it fast while the stage
    /// homes and moves (W3.1).
    #[property]
    async fn rate(&self) -> Result<f64, PropertyError> {
        Ok(match self.simulator_property()?.clock.rate() {
            Rate::RealTime => 1.0,
            Rate::Scaled(k) => k,
            Rate::Stepped => 0.0,
        })
    }

    #[setter(rate)]
    async fn set_rate(&self, value: f64) -> Result<(), PropertyError> {
        if !(value > 0.0 && value <= MOST_RATE) {
            let why = format!("the rate must be above 0 and at most {MOST_RATE}, and was {value}");
            return Err(property_refused(400, "out of range", why));
        }
        let rate = if value == 1.0 {
            Rate::RealTime
        } else {
            Rate::Scaled(value)
        };
        self.simulator_property()?.clock.set_rate(rate);
        Ok(())
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

    /// Injects a fault, or takes it away.
    ///
    /// For the status (P50): `camera_unplugged`, `stage_alarm` or `scratch_full`, with `active`
    /// true to inject it and false to take it away. A stage alarm taken
    /// away is cleared, as the stage's `clear_alarm` does.
    #[action]
    async fn inject_fault(&self, fault: Fault, active: bool) -> Result<(), ActionError> {
        let simulator = self.simulator()?;
        match fault {
            Fault::CameraUnplugged => simulator.camera.unplug(active),
            Fault::StageAlarm if active => simulator
                .stage
                .inject_alarm("a limit switch (a fault injected)"),
            Fault::StageAlarm => {
                let cleared = oscope_hal::Stage::clear_alarm(simulator.stage.as_ref()).await;
                cleared.map_err(|e| ActionError::handled(e.to_string()))?;
            }
            Fault::ScratchFull => simulator
                .faults
                .scratch_full
                .store(active, Ordering::Release),
        }
        Ok(())
    }

    /// Takes the slide off the stage, and says which sample it was.
    #[action]
    async fn unload(&self) -> Result<Option<String>, ActionError> {
        Ok(self.simulator()?.loader.unload())
    }

    /// Adds your own image or stack to the catalogue.
    ///
    /// As `name`: a PNG, JPEG or TIFF on the server's disk, its `path`, or
    /// one the upload route staged, its `upload` (W8.4), at `um_per_pixel`.
    /// A stack (a multi-page TIFF) gives its `z_step`. Its values are
    /// `transmitted` light, `reflected` light, or `fluorescent` amounts of
    /// `dye`. An upload is kept in the data's `simulation` folder, which
    /// the simulator reads at each load, and taken off the staged ones.
    #[action]
    async fn import(
        &self,
        ctx: ActionCtx,
        name: String,
        #[param(default)] path: Option<String>,
        #[param(default)] upload: Option<Uuid>,
        um_per_pixel: f64,
        #[param(default)] z_step: Option<f64>,
        #[param(default = "transmitted".to_owned())] light: String,
        #[param(default = "green".to_owned())] dye: String,
    ) -> Result<(), ActionError> {
        let simulator = self.simulator()?;
        let (path, kept) = match (path, upload) {
            (Some(path), None) => (path, None),
            (None, Some(id)) => {
                let kept = keep(&ctx, id).map_err(ActionError::handled)?;
                (kept.display().to_string(), Some((id, kept)))
            }
            _ => {
                let both = "an import reads a `path` or an `upload`, one of them";
                return Err(ActionError::handled(format!("refused (400): {both}")));
            }
        };
        let kind = if z_step.is_some() { "stack" } else { "image" };
        let entry = serde_json::from_value::<SampleEntry>(json!({
            "name": name, "kind": kind, "path": path, "um_per_pixel": um_per_pixel,
            "z_step": z_step, "light": light, "dye": dye,
        }))
        .map_err(|e| format!("the import: {e}"))
        .and_then(|entry| simulator.loader.add(entry));
        // Kept only once it is a sample: the staged file goes; else the copy does.
        match (&entry, kept) {
            (Ok(()), Some((id, _))) => {
                if let Some(uploads) = ctx.server().service::<Uploads>() {
                    uploads.remove(id);
                }
            }
            (Err(_), Some((_, copy))) => {
                let _ = std::fs::remove_file(copy);
            }
            _ => {}
        }
        entry.map_err(ActionError::handled)
    }
}

/// The staged file `id`, copied into the data's `simulation` folder, as
/// `<id>-<its name>`: where the simulator reads it at each load.
fn keep(ctx: &ActionCtx, id: Uuid) -> Result<std::path::PathBuf, String> {
    let staged = staged(ctx.server().service::<Uploads>().as_deref(), id)
        .map_err(|e| format!("refused (400): {e}"))?;
    let roots = ctx.server().service::<Roots>();
    let roots = roots.ok_or_else(|| "the server keeps no data".to_owned())?;
    let folder = roots.data.join("simulation");
    std::fs::create_dir_all(&folder).map_err(|e| format!("{}: {e}", folder.display()))?;
    let name = staged.file_name().map(|n| n.to_string_lossy().into_owned());
    let kept = folder.join(format!("{id}-{}", name.unwrap_or_default()));
    std::fs::copy(&staged, &kept).map_err(|e| format!("{}: {e}", kept.display()))?;
    Ok(kept)
}
