//! The simulator as devices (P16): lights and an objective mount that set
//! what the simulated camera sees, and the whole simulator built from the
//! configuration's `[devices.simulator]`.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError, RwLock};

use oscope_core::{LightState, Point2, Um};
use oscope_hal::{
    Camera, HalFuture, Illumination, IlluminationCapabilities, LightChannel, LightKind,
    ManualObjectiveMount, Objective, ObjectiveCatalogue, ObjectiveMount,
    ObjectiveMountCapabilities,
};
use serde::Deserialize;

use crate::camera::{SimCamera, SimCameraConfig};
use crate::catalogue::{SampleCatalogue, SampleEntry, SlideLoader};
use crate::clock::{Rate, SimClock};
use crate::fluorescence::Excitation;
use crate::optics::Optics;
use crate::stage::{SimStage, SimStageConfig};
use crate::world::{Light, World};

/// The channels, in the order the camera looks for one that is on, and the
/// light each gives it.
const CHANNELS: [(&str, LightKind, Light); 6] = [
    ("transmitted", LightKind::Transmitted, Light::Transmitted),
    ("reflected", LightKind::Reflected, Light::Reflected),
    (
        "violet 405",
        LightKind::Other,
        fluorescence(Excitation::Violet),
    ),
    ("blue 488", LightKind::Other, fluorescence(Excitation::Blue)),
    (
        "green 561",
        LightKind::Other,
        fluorescence(Excitation::Green),
    ),
    ("red 640", LightKind::Other, fluorescence(Excitation::Red)),
];

const fn fluorescence(line: Excitation) -> Light {
    Light::Fluorescence {
        line,
        confocal: false,
    }
}

/// The simulator's lights: transmitted and reflected white light, and four
/// excitation lines, each dimmable. The camera sees the first that is on,
/// in that order, at its intensity, and the dark if none is.
pub struct SimLights {
    capabilities: IlluminationCapabilities,
    state: Mutex<Vec<LightState>>,
    camera: Arc<SimCamera>,
    confocal: AtomicBool,
}

impl SimLights {
    /// Lights for `camera`, the transmitted one on at first.
    pub fn new(camera: Arc<SimCamera>) -> Self {
        let lights = Self {
            capabilities: IlluminationCapabilities {
                model: "simulated".to_owned(),
                channels: CHANNELS
                    .iter()
                    .map(|(name, kind, _)| LightChannel {
                        name: (*name).to_owned(),
                        kind: *kind,
                        dimmable: true,
                    })
                    .collect(),
            },
            state: Mutex::new(
                CHANNELS
                    .iter()
                    .enumerate()
                    .map(|(i, (name, _, _))| LightState {
                        channel: (*name).to_owned(),
                        on: i == 0,
                        intensity: 1.0,
                    })
                    .collect(),
            ),
            camera,
            confocal: AtomicBool::new(false),
        };
        lights.show();
        lights
    }

    /// Whether fluorescence is seen through a confocal's pinhole.
    pub fn set_confocal(&self, confocal: bool) {
        self.confocal.store(confocal, Ordering::Relaxed);
        self.show();
    }

    /// Whether it is.
    pub fn confocal(&self) -> bool {
        self.confocal.load(Ordering::Relaxed)
    }

    /// Gives the camera the light it now sees.
    fn show(&self) {
        let state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        let lit = state
            .iter()
            .zip(CHANNELS)
            .find(|(s, _)| s.on && s.intensity > 0.0);
        match lit {
            Some((s, (_, _, light))) => {
                let light = match light {
                    Light::Fluorescence { line, .. } => Light::Fluorescence {
                        line,
                        confocal: self.confocal(),
                    },
                    other => other,
                };
                self.camera.set_light(s.intensity, light);
            }
            None => self.camera.set_light(0.0, Light::Transmitted),
        }
    }
}

impl Illumination for SimLights {
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
            {
                let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
                if let Some(s) = state.iter_mut().find(|s| s.channel == applied.channel) {
                    *s = applied.clone();
                }
            }
            self.show();
            Ok(applied)
        })
    }
}

/// The simulator's objective mount: fitting an objective, by hand as on the
/// instrument, gives the camera its optics.
pub struct SimMount {
    mount: ManualObjectiveMount,
    camera: Arc<SimCamera>,
}

impl SimMount {
    /// A mount for `catalogue`'s objectives over `camera`.
    pub fn new(catalogue: ObjectiveCatalogue, camera: Arc<SimCamera>) -> Self {
        Self {
            mount: ManualObjectiveMount::new(catalogue),
            camera,
        }
    }
}

impl ObjectiveMount for SimMount {
    fn capabilities(&self) -> &ObjectiveMountCapabilities {
        self.mount.capabilities()
    }

    fn catalogue(&self) -> &ObjectiveCatalogue {
        self.mount.catalogue()
    }

    fn fitted(&self) -> HalFuture<'_, Option<Objective>> {
        self.mount.fitted()
    }

    fn fit(&self, name: String) -> HalFuture<'_, Objective> {
        Box::pin(async move {
            let objective = self.mount.fit(name).await?;
            let pixel = self.camera.capabilities().pixel_size;
            self.camera
                .set_optics(Optics::new(self.mount.catalogue(), &objective, pixel));
            Ok(objective)
        })
    }
}

/// The simulator's part of the configuration, `[devices.simulator]`:
///
/// ```toml
/// [devices.simulator]
/// rate = 1.0              # the clock: 1 is real time
/// sensor = "full"         # the MIChrome 20's, or "quarter": 1368 × 912
/// objective = "10x"       # fitted at start
/// sample = "H&E tissue"   # loaded at start; "" for none
/// focus = 5000.0          # the stage z, in µm, at which the slide is sharp
///
/// [[devices.simulator.samples]]   # beyond the built-in ones
/// name = "my section"
/// kind = "image"
/// path = "section.png"
/// um_per_pixel = 0.25
/// ```
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct SimulatorConfig {
    /// The clock's rate: 1 is real time.
    pub rate: f64,
    /// The sensor: `full` or `quarter`.
    pub sensor: String,
    /// The objective fitted at start.
    pub objective: String,
    /// The sample loaded at start, or `""` for none.
    pub sample: String,
    /// The stage z at which the slide's surface is in focus.
    pub focus: f64,
    /// Samples beyond the built-in ones.
    pub samples: Vec<SampleEntry>,
}

impl Default for SimulatorConfig {
    fn default() -> Self {
        Self {
            rate: 1.0,
            sensor: "full".to_owned(),
            objective: "10x".to_owned(),
            sample: "H&E tissue".to_owned(),
            focus: 5_000.0,
            samples: Vec::new(),
        }
    }
}

/// The whole simulator: its clock, stage, camera, lights, objective mount
/// and slide loader, sharing one world.
pub struct Simulator {
    /// The clock every device runs on.
    pub clock: Arc<SimClock>,
    /// The stage, which a live simulator runs ([`SimStage::run`]).
    pub stage: Arc<SimStage>,
    /// The camera.
    pub camera: Arc<SimCamera>,
    /// The lights.
    pub lights: Arc<SimLights>,
    /// The objective mount.
    pub mount: Arc<SimMount>,
    /// The slide loader.
    pub loader: Arc<SlideLoader>,
}

impl Simulator {
    /// The simulator `config` describes, with imports' paths taken from
    /// `base`, the configuration's folder. It fits the starting objective
    /// and loads the starting sample, if any.
    pub async fn new(config: &SimulatorConfig, base: &Path) -> std::result::Result<Self, String> {
        let rate = if config.rate == 1.0 {
            Rate::RealTime
        } else if config.rate > 0.0 && config.rate.is_finite() {
            Rate::Scaled(config.rate)
        } else {
            return Err(format!("a clock rate of {}", config.rate));
        };
        let clock = Arc::new(SimClock::new(rate));
        let stage = Arc::new(SimStage::new(SimStageConfig::default(), Arc::clone(&clock)));
        let world = Arc::new(RwLock::new(World::empty()));
        world.write().unwrap_or_else(PoisonError::into_inner).focus = Um(config.focus);
        let camera_config = match config.sensor.as_str() {
            "full" => SimCameraConfig::default(),
            "quarter" => SimCameraConfig::quarter(),
            other => return Err(format!("a sensor `{other}`: `full` or `quarter`")),
        };
        let catalogue = ObjectiveCatalogue::mitutoyo();
        let first = catalogue
            .get(&config.objective)
            .map_err(|e| e.to_string())?
            .clone();
        let optics = Optics::new(&catalogue, &first, camera_config.pixel_size);
        let camera = Arc::new(SimCamera::new(
            camera_config,
            Arc::clone(&clock),
            Arc::clone(&stage),
            Arc::clone(&world),
            optics,
        ));
        let lights = Arc::new(SimLights::new(Arc::clone(&camera)));
        let mount = Arc::new(SimMount::new(catalogue, Arc::clone(&camera)));
        mount
            .fit(config.objective.clone())
            .await
            .map_err(|e| e.to_string())?;
        let mut samples = SampleCatalogue::builtin();
        for entry in &config.samples {
            samples.samples.retain(|s| s.name != entry.name);
            samples.samples.push(entry.clone());
        }
        let loader = Arc::new(SlideLoader::new(
            world,
            samples,
            base,
            Point2::new(0.0, 0.0),
        ));
        if !config.sample.is_empty() {
            loader.load(&config.sample)?;
        }
        Ok(Self {
            clock,
            stage,
            camera,
            lights,
            mount,
            loader,
        })
    }
}
