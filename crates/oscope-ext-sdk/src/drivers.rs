//! Drivers: cameras, stages and lights an extension drives (ADR-0036), its
//! importers and exporters of other file formats (ADR-0037), and its scan
//! workflows (ADR-0025).
//!
//! OScope wraps each device driver in a driver of its HAL (ADR-0012). It
//! checks settings, targets, speeds and lights against the driver's
//! capabilities, refuses what can't change while a camera streams, waits for
//! software triggers, times and numbers frames, waits for moves to end, and
//! keeps the track of positions. So a driver only applies, starts, reports
//! and captures.

use oscope_core::{LightState, Um, Xyz};
use oscope_hal::{
    Axis, CameraCapabilities, CameraSettings, IlluminationCapabilities, StageCapabilities,
    StageStatus, UmPerS,
};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
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

/// A file format, as an importer or an exporter names it (ADR-0037). Its
/// ID is the importer's or exporter's name.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Format {
    /// Its name, for people.
    pub name: String,
    /// Its files' extensions, without the dot and in lower case: an
    /// exporter names its files with the first.
    pub extensions: Vec<String>,
}

/// The type of an image's samples.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Sample {
    /// 8 bits.
    U8,
    /// 16 bits.
    U16,
    /// 32-bit floats.
    F32,
}

/// The size of an image's pixels, in µm.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct PixelSize {
    /// Across.
    pub x: f64,
    /// Down.
    pub y: f64,
    /// Between planes.
    pub z: f64,
}

/// An image, as an importer finds it and an exporter is given it: OScope's
/// image description.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ImageDescription {
    /// Its width, in pixels.
    pub width: u64,
    /// Its height, in pixels.
    pub height: u64,
    /// Its channels, 1 to 4: grey, grey and alpha, RGB, RGBA.
    pub channels: u32,
    /// Its planes: 1, or a stack's.
    pub planes: u32,
    /// Its samples' type.
    pub sample: Sample,
    /// Its pixels' size, if known.
    #[serde(default)]
    pub pixel_size: Option<PixelSize>,
    /// Its channels' names, if any.
    #[serde(default)]
    pub channel_names: Vec<String>,
}

impl ImageDescription {
    /// A plane's bytes: its samples interleaved, little-endian.
    pub fn plane_bytes(&self) -> usize {
        let sample = match self.sample {
            Sample::U8 => 1,
            Sample::U16 => 2,
            Sample::F32 => 4,
        };
        (self.width * self.height) as usize * self.channels as usize * sample
    }
}

/// Reads files of a format into images (ADR-0037).
pub trait Importer: Send + Sync + 'static {
    /// Its format.
    fn format(&self) -> Format;

    /// What the file at `path` holds.
    fn probe(&self, path: &str) -> Result<ImageDescription, String>;

    /// Reads plane `plane` of the file at `path` into `pixels`, of
    /// [`ImageDescription::plane_bytes`]: its samples interleaved, and
    /// little-endian.
    fn read(&self, path: &str, plane: u32, pixels: &mut [u8]) -> Result<(), String>;
}

/// Writes images to files of a format (ADR-0037). OScope gives it one image
/// at a time: `begin`, each plane in order, then `end`.
pub trait Exporter: Send + Sync + 'static {
    /// Its format.
    fn format(&self) -> Format;

    /// Starts the file at `path`, of the image `image` describes, named
    /// `name`.
    fn begin(&self, path: &str, image: &ImageDescription, name: &str) -> Result<(), String>;

    /// Writes plane `plane`: its samples interleaved, and little-endian.
    fn write(&self, plane: u32, pixels: &[u8]) -> Result<(), String>;

    /// Ends the file.
    fn end(&self) -> Result<(), String>;
}

/// A frame's cell on the stage, and a grid's steps, as OScope's plans give
/// them to a workflow (P35), in µm.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct ScanField {
    /// The cell: the largest rectangle, square with the stage, each frame
    /// holds.
    pub cell_um: Extent,
    /// The steps between a grid's tiles, for the overlap asked.
    pub step_um: Extent,
}

/// A width and a height, in µm.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Extent {
    /// Along x.
    pub width: f64,
    /// Along y.
    pub height: f64,
}

/// A tile a workflow plans: where the stage takes its frames, in µm.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlannedTile {
    /// The stage's x at its centre.
    pub x_um: f64,
    /// Its y.
    pub y_um: f64,
    /// Its z, if the workflow knows it; autofocus finds it otherwise.
    #[serde(default)]
    pub z_um: Option<f64>,
    /// Its name, if it has one.
    #[serde(default)]
    pub name: Option<String>,
}

/// Plans scans (ADR-0025): from its parameters and the field, the tiles,
/// which OScope's engine runs in the order given.
pub trait ScanWorkflow: Send + Sync + 'static {
    /// Its parameters' JSON Schema, which OScope checks them against.
    fn schema(&self) -> Value;

    /// The tiles for `params`, as its schema describes them, with frames of
    /// `field`. Parameters it can't take are refused, saying why.
    fn plan(&self, params: Value, field: ScanField) -> Result<Vec<PlannedTile>, String>;
}

/// A driver of one of the kinds.
pub enum Driver {
    /// A camera.
    Camera(Box<dyn CameraDriver>),
    /// A stage.
    Stage(Box<dyn StageDriver>),
    /// Lights.
    Illumination(Box<dyn IlluminationDriver>),
    /// An importer.
    Importer(Box<dyn Importer>),
    /// An exporter.
    Exporter(Box<dyn Exporter>),
    /// A scan workflow.
    Workflow(Box<dyn ScanWorkflow>),
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

/// An importer named `name`, for [`export!`](crate::export).
pub fn importer(name: &str, importer: impl Importer) -> NamedDriver {
    let driver = Driver::Importer(Box::new(importer));
    NamedDriver {
        name: name.to_owned(),
        driver,
    }
}

/// An exporter named `name`, for [`export!`](crate::export).
pub fn exporter(name: &str, exporter: impl Exporter) -> NamedDriver {
    let driver = Driver::Exporter(Box::new(exporter));
    NamedDriver {
        name: name.to_owned(),
        driver,
    }
}

/// A scan workflow named `name`, for [`export!`](crate::export).
pub fn workflow(name: &str, workflow: impl ScanWorkflow) -> NamedDriver {
    let driver = Driver::Workflow(Box::new(workflow));
    NamedDriver {
        name: name.to_owned(),
        driver,
    }
}

/// `params`' `key`, read as `T`.
fn arg<T: DeserializeOwned>(params: &Value, key: &str) -> Result<T, String> {
    let value = params.get(key).cloned().unwrap_or(Value::Null);
    serde_json::from_value(value).map_err(|e| format!("`{key}`: {e}"))
}

impl Driver {
    /// Its kind, as the description names it.
    pub fn kind(&self) -> &'static str {
        match self {
            Driver::Camera(_) => "camera",
            Driver::Stage(_) => "stage",
            Driver::Illumination(_) => "illumination",
            Driver::Importer(_) => "importer",
            Driver::Exporter(_) => "exporter",
            Driver::Workflow(_) => "workflow",
        }
    }

    /// Calls its method `method` with `params`: what OScope's adapters ask
    /// of it, in JSON, as the Python SDK's drivers take it too.
    pub fn call(&self, method: &str, params: Value) -> Result<Value, String> {
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
            (Driver::Importer(i), "format") => json(i.format()),
            (Driver::Importer(i), "probe") => json(i.probe(&arg::<String>(&params, "path")?)?),
            (Driver::Exporter(e), "format") => json(e.format()),
            (Driver::Exporter(e), "begin") => {
                let (path, name): (String, String) = (arg(&params, "path")?, arg(&params, "name")?);
                done(e.begin(&path, &arg(&params, "image")?, &name))
            }
            (Driver::Exporter(e), "end") => done(e.end()),
            (Driver::Workflow(w), "schema") => Ok(w.schema()),
            (Driver::Workflow(w), "plan") => {
                let field = arg(&params, "field")?;
                json(w.plan(params.get("params").cloned().unwrap_or(Value::Null), field)?)
            }
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

    /// Reads into `pixels` what `params` ask: an importer's plane of a file,
    /// `{"path", "plane"}`.
    pub fn read(&self, params: Value, pixels: &mut [u8]) -> Result<(), String> {
        match self {
            Driver::Importer(i) => {
                let path: String = arg(&params, "path")?;
                i.read(&path, arg(&params, "plane")?, pixels)
            }
            driver => Err(format!("a {} driver reads no pixels", driver.kind())),
        }
    }

    /// Gives an exporter a plane's `pixels`, as `params` say: `{"plane"}`.
    pub fn write(&self, params: Value, pixels: &[u8]) -> Result<(), String> {
        match self {
            Driver::Exporter(e) => e.write(arg(&params, "plane")?, pixels),
            driver => Err(format!("a {} driver writes no pixels", driver.kind())),
        }
    }
}
