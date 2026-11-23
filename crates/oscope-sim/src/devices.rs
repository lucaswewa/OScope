//! The simulator as devices (P16): lights and an objective mount that set
//! what the simulated camera sees, and the whole simulator built from the
//! configuration's `[simulator]`.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError, RwLock};

use oscope_core::{LightState, Ms, Point2, Um};
use oscope_hal::{
    Camera, HalFuture, Illumination, IlluminationCapabilities, LightChannel, LightKind,
    ManualObjectiveMount, Objective, ObjectiveCatalogue, ObjectiveMount,
    ObjectiveMountCapabilities,
};
use serde::{Deserialize, Serialize};

use crate::camera::{SimCamera, SimCameraConfig};
use crate::catalogue::{SampleCatalogue, SampleEntry, SlideLoader};
use crate::clock::{Rate, SimClock};
use crate::fluorescence::Excitation;
use crate::optics::Optics;
use crate::stage::{LostSteps, SimStage, SimStageConfig};
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
    scale: f64,
}

impl SimMount {
    /// A mount for `catalogue`'s objectives over `camera`.
    pub fn new(catalogue: ObjectiveCatalogue, camera: Arc<SimCamera>) -> Self {
        Self::scaled(catalogue, camera, 1.0)
    }

    /// A mount whose objectives truly magnify `scale` times what `catalogue`
    /// says, as a tube lens a little off would make them: what calibration
    /// measures (P32).
    pub fn scaled(catalogue: ObjectiveCatalogue, camera: Arc<SimCamera>, scale: f64) -> Self {
        Self {
            mount: ManualObjectiveMount::new(catalogue),
            camera,
            scale,
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
            let mut optics = Optics::new(self.mount.catalogue(), &objective, pixel);
            optics.magnification *= self.scale;
            self.camera.set_optics(optics);
            Ok(objective)
        })
    }
}

/// The simulator's part of the configuration, `[simulator]` (version 1's
/// `[devices.simulator]`):
///
/// ```toml
/// [simulator]
/// rate = 1.0              # the clock: 1 is real time
/// sensor = "full"         # the MIChrome 20's, or "quarter": 1368 × 912
/// objective = "10x"       # fitted at start
/// sample = "H&E tissue"   # loaded at start; "" for none
/// focus = 5000.0          # the stage z, in µm, at which the slide is sharp
/// tilt = [0.0, 0.0]       # how much higher it is sharp, in µm, a mm along x and y
/// rotation = 0.0         # the camera's turn on the stage, in degrees
/// mirrored = false        # whether its image is mirrored
/// scale = 1.0             # the true magnification over the catalogue's
/// backlash = 2.0          # the stage's play when an axis reverses, in µm
/// latency = 5.0           # how late the stage's reports arrive, in ms
/// encoders = true          # whether the stage reports where it is, or where it was sent
/// lost_steps = [0.0, 5.0]  # the chance a move loses steps, and how far behind it falls, in µm
/// vignetting = 0.0        # the optics' falloff, 1 / (1 + s r²)²: s
/// response = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]   # the sensor's colour
/// gpu = true              # the camera on the GPU, when built with CUDA and there is one
///
/// [[simulator.samples]]   # beyond the built-in ones
/// name = "my section"
/// kind = "image"
/// path = "section.png"
/// um_per_pixel = 0.25
/// ```
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
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
    /// The stage z at which the slide's surface is in focus, at its corner.
    pub focus: f64,
    /// The slide's tilt (P35): how much higher its surface is in focus, in
    /// µm, a mm further along x, and along y.
    pub tilt: [f64; 2],
    /// The camera's turn on the stage, in degrees: what calibration
    /// recovers (P32), as are the three below.
    pub rotation: f64,
    /// Whether the camera's image is mirrored.
    pub mirrored: bool,
    /// The true magnification over the catalogue's.
    pub scale: f64,
    /// The stage's play when an axis reverses, in µm.
    pub backlash: f64,
    /// How late the stage's position reports arrive, in ms (P38). Each
    /// still carries the time the stage was there.
    pub latency: f64,
    /// Whether the stage has encoders, and reports where it is; without
    /// them it reports where it was sent, as a stepper stage does (P39).
    pub encoders: bool,
    /// The chance that a move loses steps, and how far behind the stage then
    /// falls along the move's main axis, in µm: the losses add up (P39).
    pub lost_steps: [f64; 2],
    /// The optics' vignetting, s: the light falls off as 1 / (1 + s r²)²
    /// towards the sensor's corners (P33).
    pub vignetting: f64,
    /// The sensor's colour response, with the light's tint, by rows: what
    /// colour calibration corrects (P33).
    pub response: [[f64; 3]; 3],
    /// Samples beyond the built-in ones.
    pub samples: Vec<SampleEntry>,
    /// Whether the camera renders its frames on the GPU, when the build has
    /// CUDA and there is a GPU (ADR-0062).
    pub gpu: bool,
}

impl Default for SimulatorConfig {
    fn default() -> Self {
        Self {
            rate: 1.0,
            sensor: "full".to_owned(),
            objective: "10x".to_owned(),
            sample: "H&E tissue".to_owned(),
            focus: 5_000.0,
            tilt: [0.0; 2],
            rotation: 0.0,
            mirrored: false,
            scale: 1.0,
            backlash: 2.0,
            latency: 5.0,
            encoders: true,
            lost_steps: [0.0, 5.0],
            vignetting: 0.0,
            response: crate::camera::IDEAL,
            samples: Vec::new(),
            gpu: true,
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
    /// Faults injected that aren't a device's (P50).
    pub faults: Arc<SimFaults>,
    /// Why the camera renders on the CPU, if it does: `gpu = false`, a build
    /// without CUDA, or the GPU's error.
    pub on_cpu: Option<String>,
}

/// Faults to inject that aren't a device's (P50): the camera's and the
/// stage's are theirs ([`SimCamera::unplug`], [`SimStage::inject_alarm`]).
#[derive(Debug, Default)]
pub struct SimFaults {
    /// The scratch disk, said to be full.
    pub scratch_full: AtomicBool,
}

impl SimulatorConfig {
    /// Checks it as [`Simulator::new`] does, without making the simulator
    /// (P52): the clock's rate, the truths, the stage's latency and lost
    /// steps, the sensor, the objective, and the sample's name.
    pub fn check(&self) -> std::result::Result<(), String> {
        let config = self;
        if !(config.rate > 0.0 && config.rate.is_finite()) {
            return Err(format!("a clock rate of {}", config.rate));
        }
        let response = config.response.iter().flatten();
        let truth = [
            config.rotation,
            config.scale,
            config.backlash,
            config.vignetting,
            config.tilt[0],
            config.tilt[1],
        ];
        let finite = truth.iter().chain(response).all(|v| v.is_finite());
        if !finite || config.scale <= 0.0 || config.backlash < 0.0 || config.vignetting < 0.0 {
            let (r, s, b, v) = (
                config.rotation,
                config.scale,
                config.backlash,
                config.vignetting,
            );
            return Err(format!(
                "a rotation of {r}°, a scale of {s}, a backlash of {b} µm and a vignetting of \
                 {v}, with a response of {:?} and a tilt of {:?}: all finite, a scale more than \
                 0, and a backlash and a vignetting of at least 0, are needed",
                config.response, config.tilt
            ));
        }
        if !(config.latency >= 0.0 && config.latency.is_finite()) {
            let latency = config.latency;
            return Err(format!(
                "a stage latency of {latency} ms: at least 0 is needed"
            ));
        }
        let [chance, size] = config.lost_steps;
        if !((0.0..=1.0).contains(&chance) && size >= 0.0 && size.is_finite()) {
            return Err(format!(
                "lost steps of a chance of {chance} and {size} µm: a chance from 0 to 1, and a \
                 size of at least 0, are needed"
            ));
        }
        if !["full", "quarter"].contains(&config.sensor.as_str()) {
            let sensor = &config.sensor;
            return Err(format!("a sensor `{sensor}`: `full` or `quarter`"));
        }
        let catalogue = ObjectiveCatalogue::mitutoyo();
        catalogue
            .get(&config.objective)
            .map_err(|e| e.to_string())?;
        let builtin = SampleCatalogue::builtin().samples;
        let mut names = builtin.iter().chain(&config.samples).map(|s| &s.name);
        if !config.sample.is_empty() && !names.any(|name| *name == config.sample) {
            let sample = &config.sample;
            return Err(format!(
                "a sample `{sample}`: not built in, nor among `samples`"
            ));
        }
        Ok(())
    }
}

impl Simulator {
    /// The simulator `config` describes, with imports' paths taken from
    /// `base`, the configuration's folder. It fits the starting objective
    /// and loads the starting sample, if any.
    pub async fn new(config: &SimulatorConfig, base: &Path) -> std::result::Result<Self, String> {
        config.check()?;
        let rate = match config.rate == 1.0 {
            true => Rate::RealTime,
            false => Rate::Scaled(config.rate),
        };
        let [chance, size] = config.lost_steps;
        let clock = Arc::new(SimClock::new(rate));
        let mut stage_config = SimStageConfig {
            backlash: Um(config.backlash),
            latency: Ms(config.latency),
            lost_steps: LostSteps {
                chance,
                size: Um(size),
            },
            ..SimStageConfig::default()
        };
        stage_config.capabilities.encoders = config.encoders;
        let stage = Arc::new(SimStage::new(stage_config, Arc::clone(&clock)));
        let world = Arc::new(RwLock::new(World {
            focus: Um(config.focus),
            tilt: config.tilt,
            ..World::empty()
        }));
        let mut camera_config = match config.sensor.as_str() {
            "full" => SimCameraConfig::default(),
            "quarter" => SimCameraConfig::quarter(),
            other => return Err(format!("a sensor `{other}`: `full` or `quarter`")),
        };
        camera_config.mounting.rotation = config.rotation.to_radians();
        camera_config.mounting.mirrored = config.mirrored;
        camera_config.vignetting = config.vignetting;
        camera_config.response = config.response;
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
        let on_cpu = gpu(&camera, config.gpu);
        let lights = Arc::new(SimLights::new(Arc::clone(&camera)));
        let mount = Arc::new(SimMount::scaled(
            catalogue,
            Arc::clone(&camera),
            config.scale,
        ));
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
            faults: Arc::default(),
            on_cpu,
        })
    }
}

/// The most tiles, and layer tiles, a camera holds on the GPU: 0.9 GB in
/// all, nearly three full frames' worth (ADR-0062).
pub const GPU_SLOTS: usize = 1024;

/// The bytes a tile and a layer tile take on the GPU, in half precision.
pub const SLOT_BYTES: usize = 257 * 257 * 3 * 2 + 256 * 256 * 4 * 2;

/// The tiles a camera holds on the GPU, for a sensor of `width` × `height`
/// with `free` bytes free on the GPU: three frames' worth at full
/// resolution, up to [`GPU_SLOTS`] and a quarter of the free memory, so
/// that servers side by side, as in tests, share the GPU. None if that is
/// less than one frame's worth: the camera then stays on the CPU.
pub fn gpu_slots(width: u32, height: u32, free: usize) -> Option<usize> {
    let tile = crate::tiles::TILE;
    let frame = (width / tile + 2) as usize * (height / tile + 2) as usize;
    let slots = (3 * frame).clamp(64, GPU_SLOTS).min(free / 4 / SLOT_BYTES);
    (slots >= frame).then_some(slots)
}

/// Puts `camera` on the GPU if `wanted` and it can: why not, if it can't.
fn gpu(camera: &SimCamera, wanted: bool) -> Option<String> {
    if !wanted {
        return Some("the configuration says `gpu = false`".to_owned());
    }
    #[cfg(feature = "cuda")]
    {
        let capabilities = oscope_hal::Camera::capabilities(camera);
        let (width, height) = (capabilities.sensor_width, capabilities.sensor_height);
        let put = || -> Result<(), String> {
            let cuda = oscope_compute::cuda::Cuda::open(0).map_err(|e| e.to_string())?;
            let (free, _) = cuda.memory().map_err(|e| e.to_string())?;
            let slots = gpu_slots(width, height, free)
                .ok_or_else(|| format!("the GPU has {} MB free, too little", free >> 20))?;
            camera.use_gpu(cuda, slots)
        };
        put().err()
    }
    #[cfg(not(feature = "cuda"))]
    {
        let _ = camera;
        Some("the build has no CUDA".to_owned())
    }
}
