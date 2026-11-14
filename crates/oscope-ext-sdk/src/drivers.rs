//! Drivers: cameras, stages and lights an extension drives (ADR-0036).
//!
//! OScope wraps each in a driver of its HAL (ADR-0012). It checks settings,
//! targets, speeds and lights against the driver's capabilities, refuses
//! what can't change while a camera streams, waits for software triggers,
//! times and numbers frames, waits for moves to end, and keeps the track of
//! positions. So a driver only applies, starts, reports and captures.

use oscope_core::{LightState, Um, Xyz};
use oscope_hal::{
    Axis, CameraCapabilities, CameraSettings, IlluminationCapabilities, StageCapabilities,
    StageStatus, UmPerS,
};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

/// A camera, as an extension drives it.
pub trait CameraDriver: Send + Sync + 'static {
    /// What it can do.
    fn capabilities(&self) -> CameraCapabilities;

    /// Takes settings that OScope has checked against its capabilities, as
    /// they round them.
    fn configure(&self, settings: &CameraSettings) -> Result<(), String>;

    /// Starts capturing, with `settings`.
    fn start(&self, settings: &CameraSettings) -> Result<(), String>;

    /// Captures the next frame into `pixels`, laid out as the settings'
    /// format and frame shape say, and blocks until it is exposed.
    fn capture(&self, pixels: &mut [u8]) -> Result<(), String>;

    /// Stops capturing.
    fn stop(&self) -> Result<(), String>;

    /// Its sensor's temperature, in °C, when its capabilities say it has a
    /// thermometer.
    fn temperature(&self) -> Result<f64, String> {
        Err("it has no thermometer".to_owned())
    }
}

/// A stage, as an extension drives it.
pub trait StageDriver: Send + Sync + 'static {
    /// What it can do.
    fn capabilities(&self) -> StageCapabilities;

    /// What it is doing.
    fn status(&self) -> Result<StageStatus, String>;

    /// Where it is.
    fn position(&self) -> Result<Xyz, String>;

    /// Starts a move to `target`, at `speed`: it is `Moving` until it
    /// arrives.
    fn move_to(&self, target: Xyz, speed: UmPerS) -> Result<(), String>;

    /// Starts moving along `axis`, towards its greatest position or its
    /// least, until it is stopped or reaches a soft limit.
    fn jog(&self, axis: Axis, towards_max: bool, speed: UmPerS) -> Result<(), String>;

    /// Stops any motion.
    fn stop(&self) -> Result<(), String>;

    /// Starts homing, when its capabilities say it can.
    fn home(&self) -> Result<(), String> {
        Err("it can't home".to_owned())
    }

    /// Clears an alarm.
    fn clear_alarm(&self) -> Result<(), String> {
        Ok(())
    }

    /// Arms its trigger output to pulse at `positions` along `axis`, when
    /// its capabilities say it has one.
    fn arm_trigger(&self, axis: Axis, positions: Vec<Um>) -> Result<(), String> {
        let _ = (axis, positions);
        Err("it has no trigger output".to_owned())
    }
}

/// Lights, as an extension drives them.
pub trait IlluminationDriver: Send + Sync + 'static {
    /// What they are.
    fn capabilities(&self) -> IlluminationCapabilities;

    /// Each light's state, in the order of their channels.
    fn state(&self) -> Result<Vec<LightState>, String>;

    /// Sets a light, as OScope has checked it against the capabilities.
    fn set(&self, light: &LightState) -> Result<(), String>;
}

/// A driver of one of the kinds.
pub enum Driver {
    /// A camera.
    Camera(Box<dyn CameraDriver>),
    /// A stage.
    Stage(Box<dyn StageDriver>),
    /// Lights.
    Illumination(Box<dyn IlluminationDriver>),
}

/// A driver, and the name the configuration knows it by: one of the
/// manifest's `provides.drivers`.
pub struct NamedDriver {
    /// Its name, a dotted one: `example.fakecam`.
    pub name: String,
    /// The driver.
    pub driver: Driver,
}

/// A camera driver named `name`, for [`export!`](crate::export).
pub fn camera(name: &str, driver: impl CameraDriver) -> NamedDriver {
    let driver = Driver::Camera(Box::new(driver));
    NamedDriver {
        name: name.to_owned(),
        driver,
    }
}

/// A stage driver named `name`, for [`export!`](crate::export).
pub fn stage(name: &str, driver: impl StageDriver) -> NamedDriver {
    let driver = Driver::Stage(Box::new(driver));
    NamedDriver {
        name: name.to_owned(),
        driver,
    }
}

/// An illumination driver named `name`, for [`export!`](crate::export).
pub fn illumination(name: &str, driver: impl IlluminationDriver) -> NamedDriver {
    let driver = Driver::Illumination(Box::new(driver));
    NamedDriver {
        name: name.to_owned(),
        driver,
    }
}

impl Driver {
    /// Its kind, as the description names it.
    pub fn kind(&self) -> &'static str {
        match self {
            Driver::Camera(_) => "camera",
            Driver::Stage(_) => "stage",
            Driver::Illumination(_) => "illumination",
        }
    }

    /// Calls its method `method` with `params`: what OScope's adapters ask
    /// of it, in JSON, as the Python SDK's drivers take it too.
    pub fn call(&self, method: &str, params: Value) -> Result<Value, String> {
        fn arg<T: DeserializeOwned>(params: &Value, key: &str) -> Result<T, String> {
            let value = params.get(key).cloned().unwrap_or(Value::Null);
            serde_json::from_value(value).map_err(|e| format!("`{key}`: {e}"))
        }
        fn all<T: DeserializeOwned>(params: Value) -> Result<T, String> {
            serde_json::from_value(params).map_err(|e| e.to_string())
        }
        fn json(value: impl Serialize) -> Result<Value, String> {
            serde_json::to_value(value).map_err(|e| e.to_string())
        }
        let done = |result: Result<(), String>| result.map(|()| Value::Null);
        match (self, method) {
            (Driver::Camera(c), "capabilities") => json(c.capabilities()),
            (Driver::Camera(c), "configure") => done(c.configure(&all(params)?)),
            (Driver::Camera(c), "start") => done(c.start(&all(params)?)),
            (Driver::Camera(c), "stop") => done(c.stop()),
            (Driver::Camera(c), "temperature") => json(c.temperature()?),
            (Driver::Stage(s), "capabilities") => json(s.capabilities()),
            (Driver::Stage(s), "status") => json(s.status()?),
            (Driver::Stage(s), "position") => json(s.position()?),
            (Driver::Stage(s), "move_to") => {
                done(s.move_to(arg(&params, "target")?, arg(&params, "speed")?))
            }
            (Driver::Stage(s), "jog") => {
                let (axis, towards) = (arg(&params, "axis")?, arg(&params, "towards_max")?);
                done(s.jog(axis, towards, arg(&params, "speed")?))
            }
            (Driver::Stage(s), "stop") => done(s.stop()),
            (Driver::Stage(s), "home") => done(s.home()),
            (Driver::Stage(s), "clear_alarm") => done(s.clear_alarm()),
            (Driver::Stage(s), "arm_trigger") => {
                done(s.arm_trigger(arg(&params, "axis")?, arg(&params, "positions")?))
            }
            (Driver::Illumination(l), "capabilities") => json(l.capabilities()),
            (Driver::Illumination(l), "state") => json(l.state()?),
            (Driver::Illumination(l), "set") => done(l.set(&all(params)?)),
            (driver, method) => Err(format!(
                "a {} driver has no method `{method}`",
                driver.kind()
            )),
        }
    }

    /// Captures a camera's next frame into `pixels`.
    pub fn capture(&self, pixels: &mut [u8]) -> Result<(), String> {
        match self {
            Driver::Camera(camera) => camera.capture(pixels),
            driver => Err(format!("a {} driver captures no frames", driver.kind())),
        }
    }
}
