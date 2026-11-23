//! The instrument's devices, from the drivers the configuration's
//! `[devices]` names (plan §5.9; P16), and how their errors answer over
//! HTTP (ADR-0012, ADR-0034).

use std::path::Path;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use oscope_acquire::{Acquisition, FrameBus};
use oscope_core::{Clock, SystemClock};
use oscope_ext::Drivers;
use oscope_hal::fake::{FakeCamera, FakeIllumination, FakeObjectiveMount, FakeStage};
use oscope_hal::{
    Camera, Error, Illumination, LightChannel, LightKind, ManualIllumination, ManualObjectiveMount,
    ObjectiveCatalogue, ObjectiveMount, Stage,
};
use oscope_jobs::{HardwareLock, JobService};
use oscope_sim::{Simulator, SimulatorConfig};
use serde::Deserialize;
use teta_wot::http::axum::body::{Body, to_bytes};
use teta_wot::http::axum::extract::Request;
use teta_wot::http::axum::http::StatusCode;
use teta_wot::http::axum::middleware::Next;
use teta_wot::http::axum::response::Response;
use teta_wot::prelude::{ActionError, PropertyError, ThingCtx};
use tokio::sync::Mutex;

mod camera;
mod illumination;
mod objective;
mod simulation;
mod stage;

pub use camera::CameraThing;
pub use illumination::IlluminationThing;
pub use objective::ObjectiveThing;
pub use simulation::SimulationThing;
pub use stage::StageThing;

/// How long acquisition waits for the stage to report after a frame's
/// exposure midpoint: a report interval and its latency, with room.
const POSITION_WAIT: Duration = Duration::from_millis(50);

/// The configuration's `[devices]`: which driver each device uses.
///
/// ```toml
/// [devices]
/// camera = "sim"          # sim or fake; the MIChrome 20's driver in M3
/// stage = "sim"           # sim or fake; grblHAL's in M3
/// illumination = "sim"    # sim, manual or fake
/// objective = "sim"       # sim, manual or fake
///
/// [simulator]             # see SimulatorConfig; [devices.simulator] in version 1
/// sample = "H&E tissue"
/// ```
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct DevicesConfig {
    /// The camera's driver.
    pub camera: String,
    /// The stage's.
    pub stage: String,
    /// The lights'.
    pub illumination: String,
    /// The objective mount's.
    pub objective: String,
    /// The simulator, for the drivers that are `sim`.
    pub simulator: SimulatorConfig,
}

impl Default for DevicesConfig {
    /// Every device simulated.
    fn default() -> Self {
        Self {
            camera: "sim".to_owned(),
            stage: "sim".to_owned(),
            illumination: "sim".to_owned(),
            objective: "sim".to_owned(),
            simulator: SimulatorConfig::default(),
        }
    }
}

/// The devices, as the Things use them: each driver, a gate that takes its
/// commands one at a time (the device actors of ADR-0012), acquisition and
/// its frame bus, and the simulator when there is one.
pub struct Devices {
    /// The camera.
    pub camera: Arc<dyn Camera>,
    /// The stage.
    pub stage: Arc<dyn Stage>,
    /// The lights.
    pub lights: Arc<dyn Illumination>,
    /// The objective mount.
    pub objectives: Arc<dyn ObjectiveMount>,
    /// The simulator behind the `sim` drivers.
    pub simulator: Option<Arc<Simulator>>,
    /// The camera's clock, which its frames' times are on.
    pub clock: Arc<dyn Clock>,
    /// Completes the camera's frames and publishes them on its bus.
    pub acquisition: Acquisition,
    pub(crate) gates: Gates,
}

/// One gate a device: commands wait their turn, in order.
#[derive(Default)]
pub(crate) struct Gates {
    pub camera: Mutex<()>,
    pub stage: Mutex<()>,
    pub lights: Mutex<()>,
    pub objective: Mutex<()>,
}

impl Devices {
    /// Devices from these drivers, publishing frames on a new bus. `clock`
    /// is the camera's.
    pub fn new(
        camera: Arc<dyn Camera>,
        stage: Arc<dyn Stage>,
        lights: Arc<dyn Illumination>,
        objectives: Arc<dyn ObjectiveMount>,
        simulator: Option<Arc<Simulator>>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        let acquisition = Acquisition::new(Arc::new(FrameBus::new()), POSITION_WAIT);
        acquisition.set_track(Some(stage.track()));
        Self {
            camera,
            stage,
            lights,
            objectives,
            simulator,
            clock,
            acquisition,
            gates: Gates::default(),
        }
    }

    /// The drivers `config` names, with imports' paths taken from `base`,
    /// the configuration's folder. The simulator, if any device is `sim`,
    /// starts its stage running.
    pub async fn from_config(config: &DevicesConfig, base: &Path) -> Result<Self, String> {
        Self::from_config_with(config, base, &Drivers::default()).await
    }

    /// As [`from_config`](Self::from_config), where `config` may also name
    /// the cameras, stages and lights of the extensions loaded (ADR-0036).
    pub async fn from_config_with(
        config: &DevicesConfig,
        base: &Path,
        drivers: &Drivers,
    ) -> Result<Self, String> {
        let names = [
            &config.camera,
            &config.stage,
            &config.illumination,
            &config.objective,
        ];
        let simulator = if names.iter().any(|n| *n == "sim") {
            let simulator = Arc::new(Simulator::new(&config.simulator, base).await?);
            match &simulator.on_cpu {
                None => tracing::info!("the simulated camera renders on the GPU"),
                Some(why) => tracing::info!("the simulated camera renders on the CPU: {why}"),
            }
            let stage = Arc::clone(&simulator.stage);
            tokio::spawn(async move { stage.run().await });
            Some(simulator)
        } else {
            None
        };
        if config.camera == "sim" && config.stage != "sim" {
            return Err(
                "the simulated camera needs the simulated stage: it renders where that stage is"
                    .to_owned(),
            );
        }
        let clock = Arc::new(SystemClock::new());
        let unknown = |device: &str, name: &str, known: &str| {
            let mut known: Vec<String> = known.split(", ").map(str::to_owned).collect();
            let extensions = drivers.names().filter(|(_, kind)| *kind == device);
            known.extend(extensions.map(|(name, _)| name.to_owned()));
            let last = known.pop().unwrap_or_default();
            let known = match known.is_empty() {
                true => last,
                false => format!("{} and {last}", known.join(", ")),
            };
            Err(format!(
                "`{device} = \"{name}\"`: the {device} drivers are {known}"
            ))
        };
        let sim = simulator.as_ref();
        let (camera, frames_clock): (Arc<dyn Camera>, Arc<dyn Clock>) =
            match (config.camera.as_str(), sim) {
                ("sim", Some(s)) => (s.camera.clone(), s.clock.clone()),
                ("fake", _) => (Arc::new(FakeCamera::new(clock.clone())), clock.clone()),
                (name, _) => match drivers.camera(name, clock.clone()) {
                    Some(camera) => (camera?, clock.clone()),
                    None => return unknown("camera", name, "sim, fake"),
                },
            };
        let stage: Arc<dyn Stage> = match (config.stage.as_str(), sim) {
            ("sim", Some(s)) => Arc::clone(&s.stage) as Arc<dyn Stage>,
            ("fake", _) => Arc::new(FakeStage::new(clock.clone())),
            (name, _) => match drivers.stage(name, clock.clone()) {
                Some(stage) => stage?,
                None => return unknown("stage", name, "sim, fake"),
            },
        };
        let channels = || {
            vec![LightChannel {
                name: "transmitted".to_owned(),
                kind: LightKind::Transmitted,
                dimmable: true,
            }]
        };
        let lights: Arc<dyn Illumination> = match (config.illumination.as_str(), sim) {
            ("sim", Some(s)) => Arc::clone(&s.lights) as Arc<dyn Illumination>,
            ("manual", _) => Arc::new(ManualIllumination::brightfield()),
            ("fake", _) => Arc::new(FakeIllumination::new(channels())),
            (name, _) => match drivers.illumination(name) {
                Some(lights) => lights?,
                None => return unknown("illumination", name, "sim, manual, fake"),
            },
        };
        let objectives: Arc<dyn ObjectiveMount> = match (config.objective.as_str(), sim) {
            ("sim", Some(s)) => Arc::clone(&s.mount) as Arc<dyn ObjectiveMount>,
            ("manual", _) => Arc::new(ManualObjectiveMount::new(ObjectiveCatalogue::mitutoyo())),
            ("fake", _) => Arc::new(FakeObjectiveMount::new(ObjectiveCatalogue::mitutoyo())),
            (name, _) => return unknown("objective", name, "sim, manual, fake"),
        };
        Ok(Self::new(
            camera,
            stage,
            lights,
            objectives,
            simulator,
            frames_clock,
        ))
    }
}

/// A Thing's link to the devices, the server's service, made when the
/// Thing starts.
#[derive(Default)]
pub(crate) struct Link {
    devices: OnceLock<Arc<Devices>>,
    hardware: OnceLock<HardwareLock>,
}

impl Link {
    /// Finds the devices among the server's services, and the job
    /// service's hardware lock if there is one.
    pub fn connect(&self, ctx: &ThingCtx) -> anyhow::Result<()> {
        let devices = ctx
            .server()
            .service::<Devices>()
            .ok_or_else(|| anyhow::anyhow!("the server has no devices"))?;
        let _ = self.devices.set(devices);
        if let Some(jobs) = ctx.server().service::<JobService>() {
            let _ = self.hardware.set(jobs.hardware());
        }
        Ok(())
    }

    /// The devices, for a manual move: refused, as busy, while a hardware
    /// job holds the hardware lock (ADR-0016).
    pub fn manual(&self) -> Result<&Devices, ActionError> {
        if let Some(job) = self.hardware.get().and_then(HardwareLock::holder) {
            let reason = format!("the job {job} has the hardware");
            return Err(action_error(Error::Busy { reason }));
        }
        self.action()
    }

    /// The devices, for a property.
    pub fn property(&self) -> Result<&Devices, PropertyError> {
        self.devices
            .get()
            .map(AsRef::as_ref)
            .ok_or_else(|| PropertyError::failed(anyhow::anyhow!("the Thing hasn't started")))
    }

    /// The devices, for an action.
    pub fn action(&self) -> Result<&Devices, ActionError> {
        self.devices
            .get()
            .map(AsRef::as_ref)
            .ok_or_else(|| ActionError::handled("the Thing hasn't started"))
    }
}

/// The HTTP status a device error answers with (ADR-0012), and a short name
/// for its kind.
pub fn status(error: &Error) -> (u16, &'static str) {
    match error {
        Error::Unsupported { .. } => (400, "unsupported"),
        Error::OutOfRange { .. } => (400, "out of range"),
        Error::Unknown { .. } => (400, "unknown"),
        Error::Busy { .. } => (409, "busy"),
        Error::Stopped => (409, "stopped"),
        Error::Alarm { .. } => (409, "alarm"),
        Error::Disconnected => (503, "disconnected"),
        Error::Timeout { .. } => (504, "timeout"),
        Error::Device { .. } => (502, "device"),
        _ => (500, "internal"),
    }
}

/// The start of a property error's message that [`statuses`] answers with
/// its status: `[device 409 busy] `.
const MARK: &str = "[device ";

/// A device error, as a property's: [`statuses`] answers it with its
/// status.
pub(crate) fn property_error(error: Error) -> PropertyError {
    let (code, kind) = status(&error);
    PropertyError::failed(anyhow::anyhow!("{MARK}{code} {kind}] {error}"))
}

/// A property's refusal that isn't a device's (P52): [`statuses`] answers
/// it with `code`.
pub(crate) fn property_refused(code: u16, kind: &str, why: String) -> PropertyError {
    PropertyError::failed(anyhow::anyhow!("{MARK}{code} {kind}] {why}"))
}

/// A device error, as an action's. The action's request was answered when
/// it started, so its invocation ends `error`, with the kind and the status
/// in its message.
pub(crate) fn action_error(error: Error) -> ActionError {
    let (code, kind) = status(&error);
    ActionError::handled(format!("{kind} ({code}): {error}"))
}

/// A layer around every route: a property's device error, which
/// `teta-wot` answers with 500, is answered with its own status, without
/// its mark (ADR-0034).
pub async fn statuses(request: Request, next: Next) -> Response {
    let response = next.run(request).await;
    if response.status() != StatusCode::INTERNAL_SERVER_ERROR {
        return response;
    }
    let (mut parts, body) = response.into_parts();
    let Ok(bytes) = to_bytes(body, 64 * 1024).await else {
        return Response::from_parts(parts, Body::empty());
    };
    let marked = serde_json::from_slice::<serde_json::Value>(&bytes)
        .ok()
        .and_then(|mut problem| {
            let detail = problem.get("detail")?.as_str()?.to_owned();
            let rest = detail.strip_prefix(MARK)?;
            let (head, message) = rest.split_once("] ")?;
            let (code, kind) = head.split_once(' ')?;
            let code: u16 = code.parse().ok()?;
            problem["detail"] = message.into();
            problem["status"] = code.into();
            problem["title"] = kind.into();
            Some((StatusCode::from_u16(code).ok()?, problem))
        });
    match marked {
        Some((code, problem)) => {
            parts.status = code;
            parts.headers.remove("content-length");
            let body = serde_json::to_vec(&problem).unwrap_or_default();
            Response::from_parts(parts, Body::from(body))
        }
        None => Response::from_parts(parts, Body::from(bytes)),
    }
}
