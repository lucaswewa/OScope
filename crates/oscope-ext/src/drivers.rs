//! Extensions' drivers, as the HAL's (ADR-0036). OScope does what the HAL
//! promises around an extension's camera, stage and lights: it checks
//! settings, targets, speeds and lights against their capabilities, refuses
//! what can't change while a camera streams, waits for software triggers,
//! times, numbers and pools frames, waits for moves to end, and keeps the
//! track of positions. The driver only applies, starts, reports and
//! captures, through a [`Link`]: a Rust library's, or a Python worker's.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use oscope_core::{Clock, FrameId, FrameMeta, FramePool, LightState, Ms, Timestamp, Um, Xyz};
use oscope_hal::{
    Axis, Camera, CameraCapabilities, CameraSettings, Error, FrameSink, HalFuture, Illumination,
    IlluminationCapabilities, PositionSample, PositionTrack, Result, Stage, StageCapabilities,
    StageStatus, Trigger, UmPerS,
};
use oscope_store::{
    Exporter, Format, ImageInfo, Importer, Result as StoreResult, Samples, StoreError,
};
use serde::de::DeserializeOwned;
use serde_json::{Value as Json, json};

use crate::native::Library;
use crate::worker::Worker;

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// How OScope reaches a driver: a method call with JSON, a camera's capture
/// into OScope's buffer, and an importer's and an exporter's planes. All
/// block.
pub(crate) trait Link: Send + Sync {
    /// Calls its method `method` with `params`: its answer.
    fn call(&self, method: &str, params: Json) -> std::result::Result<Json, String>;
    /// Captures a camera's next frame into `pixels`.
    fn capture(&self, pixels: &mut [u8]) -> std::result::Result<(), String>;
    /// Reads into `pixels` what `params` ask of an importer.
    fn read(&self, params: Json, pixels: &mut [u8]) -> std::result::Result<(), String>;
    /// Gives an exporter `pixels`, as `params` say.
    fn write(&self, params: Json, pixels: &[u8]) -> std::result::Result<(), String>;
}

/// A Rust extension's driver, by its place in the library's description.
pub(crate) struct NativeLink {
    pub(crate) library: Arc<Library>,
    pub(crate) index: u32,
}

impl Link for NativeLink {
    fn call(&self, method: &str, params: Json) -> std::result::Result<Json, String> {
        self.library.driver_call(self.index, method, &params)
    }

    fn capture(&self, pixels: &mut [u8]) -> std::result::Result<(), String> {
        self.library.capture(self.index, pixels)
    }

    fn read(&self, params: Json, pixels: &mut [u8]) -> std::result::Result<(), String> {
        self.library.read(self.index, &params, pixels)
    }

    fn write(&self, params: Json, pixels: &[u8]) -> std::result::Result<(), String> {
        self.library.write(self.index, &params, pixels)
    }
}

/// A Python extension's driver, by name, in its worker.
pub(crate) struct PythonLink {
    pub(crate) worker: Arc<Worker>,
    pub(crate) name: String,
}

impl Link for PythonLink {
    fn call(&self, method: &str, params: Json) -> std::result::Result<Json, String> {
        self.worker.driver(&self.name, method, &params)
    }

    fn capture(&self, pixels: &mut [u8]) -> std::result::Result<(), String> {
        self.worker.capture(&self.name, pixels)
    }

    fn read(&self, params: Json, pixels: &mut [u8]) -> std::result::Result<(), String> {
        self.worker.read(&self.name, &params, pixels)
    }

    fn write(&self, params: Json, pixels: &[u8]) -> std::result::Result<(), String> {
        self.worker.write(&self.name, &params, pixels)
    }
}

/// The importers and exporters of the extensions this start loaded, as
/// `oscope-store` takes them (ADR-0037).
#[derive(Clone, Default)]
pub struct Formats {
    /// The importers.
    pub importers: Vec<Arc<dyn Importer>>,
    /// The exporters.
    pub exporters: Vec<Arc<dyn Exporter>>,
}

impl std::fmt::Debug for Formats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Formats")
            .field("importers", &self.importers.len())
            .field("exporters", &self.exporters.len())
            .finish()
    }
}

impl Formats {
    /// Adds the importer or exporter `name`, reached through `link`.
    pub(crate) fn add(&mut self, name: &str, kind: &str, link: Arc<dyn Link>) {
        let (name, format) = (name.to_owned(), OnceLock::new());
        match kind {
            "importer" => self
                .importers
                .push(Arc::new(ExtImporter { name, link, format })),
            _ => self.exporters.push(Arc::new(ExtExporter {
                name,
                link,
                format,
                busy: Mutex::new(()),
            })),
        }
    }
}

/// The scan workflows of the extensions this start loaded, by name
/// (ADR-0025). Each call blocks, as the extension answers.
#[derive(Clone, Default)]
pub struct Workflows {
    by_name: BTreeMap<String, Arc<dyn Link>>,
}

impl std::fmt::Debug for Workflows {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list().entries(self.by_name.keys()).finish()
    }
}

impl Workflows {
    pub(crate) fn add(&mut self, name: &str, link: Arc<dyn Link>) {
        self.by_name.insert(name.to_owned(), link);
    }

    /// Their names.
    pub fn names(&self) -> Vec<String> {
        self.by_name.keys().cloned().collect()
    }

    fn link(&self, name: &str) -> std::result::Result<&Arc<dyn Link>, String> {
        let names = self.names().join(", ");
        self.by_name
            .get(name)
            .ok_or_else(|| format!("no workflow {name}: the extensions' are [{names}]"))
    }

    /// Workflow `name`'s parameters' JSON Schema.
    pub fn schema(&self, name: &str) -> std::result::Result<Json, String> {
        self.link(name)?.call("schema", Json::Null)
    }

    /// Workflow `name`'s tiles, for `params` with frames of `field`: a list
    /// of `{"x_um", "y_um"}`, each with its `z_um` and `name` if it has them.
    pub fn plan(&self, name: &str, params: Json, field: Json) -> std::result::Result<Json, String> {
        let params = json!({ "params": params, "field": field });
        self.link(name)?.call("plan", params)
    }
}

/// The format of the importer or exporter `name`, which it says when first
/// asked: its ID is its name. One that won't say has no extensions.
fn format_of(name: &str, link: &dyn Link, format: &OnceLock<Format>) -> Format {
    let told = || {
        let answer = link.call("format", Json::Null)?;
        serde_json::from_value::<oscope_ext_sdk::Format>(answer).map_err(|e| e.to_string())
    };
    let made = || match told() {
        Ok(told) => Format {
            id: name.to_owned(),
            name: told.name,
            extensions: told
                .extensions
                .iter()
                .map(|e| e.to_ascii_lowercase())
                .collect(),
        },
        Err(error) => {
            tracing::warn!("{name} gave no format: {error}");
            Format {
                id: name.to_owned(),
                name: name.to_owned(),
                extensions: Vec::new(),
            }
        }
    };
    format.get_or_init(made).clone()
}

fn failed(name: &str, path: &Path, error: String) -> StoreError {
    StoreError::Document {
        path: path.to_owned(),
        reason: format!("{name}: {error}"),
    }
}

/// An extension's importer.
struct ExtImporter {
    name: String,
    link: Arc<dyn Link>,
    format: OnceLock<Format>,
}

impl Importer for ExtImporter {
    fn format(&self) -> Format {
        format_of(&self.name, &*self.link, &self.format)
    }

    fn probe(&self, path: &Path) -> StoreResult<ImageInfo> {
        let answer = self.link.call("probe", json!({ "path": path }));
        let answer = answer.map_err(|e| failed(&self.name, path, e))?;
        let mut info: ImageInfo = serde_json::from_value(answer)
            .map_err(|e| failed(&self.name, path, format!("its probe's answer: {e}")))?;
        info.levels = 0;
        Ok(info)
    }

    fn read(&self, path: &Path, info: &ImageInfo, plane: u32) -> StoreResult<Samples> {
        let len = info.width * info.height * u64::from(info.channels) * info.sample.bytes();
        let mut bytes = vec![0; len as usize];
        let params = json!({ "path": path, "plane": plane });
        self.link
            .read(params, &mut bytes)
            .map_err(|e| failed(&self.name, path, e))?;
        Ok(Samples::from_le_bytes(info.sample, bytes))
    }
}

/// An extension's exporter: one image at a time.
struct ExtExporter {
    name: String,
    link: Arc<dyn Link>,
    format: OnceLock<Format>,
    busy: Mutex<()>,
}

impl Exporter for ExtExporter {
    fn format(&self) -> Format {
        format_of(&self.name, &*self.link, &self.format)
    }

    fn write(
        &self,
        path: &Path,
        info: &ImageInfo,
        name: &str,
        plane: &mut dyn FnMut(u32) -> StoreResult<Samples>,
    ) -> StoreResult<()> {
        let _one_at_a_time = lock(&self.busy);
        let failed = |e: String| failed(&self.name, path, e);
        let begin = json!({ "path": path, "image": info, "name": name });
        self.link.call("begin", begin).map_err(failed)?;
        let written = (0..info.planes).try_for_each(|p| {
            let bytes = plane(p)?.to_le_bytes();
            self.link
                .write(json!({ "plane": p }), &bytes)
                .map_err(failed)
        });
        let ended = self.link.call("end", Json::Null).map_err(failed);
        written.and(ended.map(drop))
    }
}

fn device(message: String) -> Error {
    Error::Device { message }
}

/// Calls `method` on `link`, its answer read as `T`.
fn call<T: DeserializeOwned>(link: &dyn Link, method: &str, params: Json) -> Result<T> {
    let answer = link.call(method, params).map_err(device)?;
    serde_json::from_value(answer).map_err(|e| device(format!("{method}'s answer: {e}")))
}

/// Calls `method` on `link` away from the async threads.
async fn later<T: DeserializeOwned + Send + 'static>(
    link: &Arc<dyn Link>,
    method: &'static str,
    params: Json,
) -> Result<T> {
    let link = Arc::clone(link);
    let done = tokio::task::spawn_blocking(move || call(&*link, method, params)).await;
    done.map_err(|e| device(format!("{method}: {e}")))?
}

/// The names of OScope's own drivers, which no extension's driver may take.
pub const OWN_DRIVERS: [&str; 3] = ["sim", "fake", "manual"];

/// The drivers that the extensions this start loaded provide, by name, as
/// the configuration's `[devices]` names them (ADR-0036).
#[derive(Default)]
pub struct Drivers {
    drivers: BTreeMap<String, (String, Arc<dyn Link>)>,
}

impl std::fmt::Debug for Drivers {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let names: Vec<_> = self.names().collect();
        f.debug_struct("Drivers").field("drivers", &names).finish()
    }
}

impl Drivers {
    pub(crate) fn add(&mut self, name: &str, kind: &str, link: Arc<dyn Link>) {
        self.drivers
            .insert(name.to_owned(), (kind.to_owned(), link));
    }

    /// Each driver's name and kind: `camera`, `stage` or `illumination`.
    pub fn names(&self) -> impl Iterator<Item = (&str, &str)> {
        self.drivers
            .iter()
            .map(|(name, (kind, _))| (name.as_str(), kind.as_str()))
    }

    fn link(&self, name: &str, kind: &str) -> Option<std::result::Result<&Arc<dyn Link>, String>> {
        let (found, link) = self.drivers.get(name)?;
        Some(match found == kind {
            true => Ok(link),
            false => Err(format!("`{name}` is a {found} driver, not a {kind} driver")),
        })
    }

    /// The camera driver `name`, opened, its frames on `clock`; `None` if no
    /// extension provides it.
    pub fn camera(
        &self,
        name: &str,
        clock: Arc<dyn Clock>,
    ) -> Option<std::result::Result<Arc<dyn Camera>, String>> {
        let link = self.link(name, "camera")?;
        Some(link.and_then(|link| {
            let camera =
                ExtCamera::open(Arc::clone(link), clock).map_err(|e| format!("`{name}`: {e}"))?;
            Ok(Arc::new(camera) as Arc<dyn Camera>)
        }))
    }

    /// The stage driver `name`, opened, its reports on `clock`.
    pub fn stage(
        &self,
        name: &str,
        clock: Arc<dyn Clock>,
    ) -> Option<std::result::Result<Arc<dyn Stage>, String>> {
        let link = self.link(name, "stage")?;
        Some(link.and_then(|link| {
            let stage =
                ExtStage::open(Arc::clone(link), clock).map_err(|e| format!("`{name}`: {e}"))?;
            Ok(Arc::new(stage) as Arc<dyn Stage>)
        }))
    }

    /// The illumination driver `name`, opened.
    pub fn illumination(
        &self,
        name: &str,
    ) -> Option<std::result::Result<Arc<dyn Illumination>, String>> {
        let link = self.link(name, "illumination")?;
        Some(link.and_then(|link| {
            let lights =
                ExtIllumination::open(Arc::clone(link)).map_err(|e| format!("`{name}`: {e}"))?;
            Ok(Arc::new(lights) as Arc<dyn Illumination>)
        }))
    }
}

/// An extension's camera, as the HAL's.
struct ExtCamera {
    link: Arc<dyn Link>,
    capabilities: CameraCapabilities,
    clock: Arc<dyn Clock>,
    settings: Arc<Mutex<CameraSettings>>,
    stream: Mutex<Option<Stream>>,
}

struct Stream {
    stop: Arc<AtomicBool>,
    triggers: Sender<()>,
    thread: JoinHandle<()>,
}

impl ExtCamera {
    /// Opens it in its first mode and format, free-running, with a 1 ms
    /// exposure (or the nearest it allows), the least gain, and no ROI.
    fn open(link: Arc<dyn Link>, clock: Arc<dyn Clock>) -> Result<ExtCamera> {
        let capabilities: CameraCapabilities = call(&*link, "capabilities", Json::Null)?;
        let mode = capabilities
            .modes
            .first()
            .ok_or_else(|| device("it has no mode".to_owned()))?;
        let format = mode
            .rates
            .first()
            .ok_or_else(|| device("its first mode has no rate".to_owned()))?
            .format;
        let (least, most) = (capabilities.exposure_min.0, capabilities.exposure_max.0);
        let settings = CameraSettings {
            mode: mode.name.clone(),
            format,
            exposure: Ms(1.0f64.clamp(least, most)),
            gain: capabilities.gain_min,
            roi: None,
            trigger: Trigger::FreeRunning,
        };
        let settings = capabilities.check(&settings)?;
        call::<Json>(&*link, "configure", json!(settings))?;
        Ok(ExtCamera {
            link,
            capabilities,
            clock,
            settings: Arc::new(Mutex::new(settings)),
            stream: Mutex::new(None),
        })
    }

    fn halt(&self) -> bool {
        let Some(stream) = lock(&self.stream).take() else {
            return false;
        };
        stream.stop.store(true, Ordering::Release);
        // A panic on the thread has already been reported.
        let _ = stream.thread.join();
        true
    }
}

impl Drop for ExtCamera {
    fn drop(&mut self) {
        if self.halt() {
            let _ = self.link.call("stop", Json::Null);
        }
    }
}

/// What the capturing thread needs.
struct Run {
    link: Arc<dyn Link>,
    capabilities: CameraCapabilities,
    settings: Arc<Mutex<CameraSettings>>,
    clock: Arc<dyn Clock>,
    pool: FramePool,
    sink: FrameSink,
    stop: Arc<AtomicBool>,
    triggers: Receiver<()>,
}

impl Run {
    fn go(self) {
        let first = lock(&self.settings).clone();
        let (Ok(mode), Ok((roi, width, height))) = (
            self.capabilities.mode(&first.mode),
            self.capabilities.frame_shape(&first),
        ) else {
            return;
        };
        let bytes = first.format.frame_bytes(width, height);
        let (mut id, mut dropped, mut spare) = (FrameId(0), 0, Vec::new());
        while !self.stop.load(Ordering::Acquire) {
            if first.trigger != Trigger::FreeRunning {
                match self.triggers.recv_timeout(Duration::from_millis(5)) {
                    Ok(()) => {}
                    Err(RecvTimeoutError::Timeout) => continue,
                    Err(RecvTimeoutError::Disconnected) => return,
                }
            }
            let mut buffer = self.pool.take();
            let captured = match &mut buffer {
                Some(buffer) => self.link.capture(&mut buffer.bytes_mut()[..bytes]),
                // The frame is taken all the same, and counted lost.
                None => {
                    spare.resize(bytes, 0);
                    self.link.capture(&mut spare)
                }
            };
            if let Err(why) = captured {
                tracing::warn!("an extension's camera didn't capture: {why}");
                thread::sleep(Duration::from_millis(10));
                continue;
            }
            let Some(buffer) = buffer else {
                dropped += 1;
                continue;
            };
            let now = lock(&self.settings).clone();
            let host_time = self.clock.now();
            let exposure = now.exposure.to_duration().unwrap_or_default();
            let mut meta = FrameMeta::new(id, first.format, width, height, host_time);
            meta.roi = roi;
            meta.binning = mode.binning;
            meta.mode = mode.name.clone();
            meta.exposure = now.exposure;
            meta.exposure_start = Timestamp(host_time.0.saturating_sub(exposure.as_nanos() as u64));
            meta.line_time = mode.line_time;
            meta.gain = now.gain;
            meta.dropped_before = dropped;
            dropped = 0;
            (self.sink)(buffer, meta);
            id = id.next();
        }
    }
}

impl Camera for ExtCamera {
    fn capabilities(&self) -> &CameraCapabilities {
        &self.capabilities
    }

    fn settings(&self) -> HalFuture<'_, CameraSettings> {
        Box::pin(async move { Ok(lock(&self.settings).clone()) })
    }

    fn configure(&self, settings: CameraSettings) -> HalFuture<'_, CameraSettings> {
        Box::pin(async move {
            let applied = self.capabilities.check(&settings)?;
            let shape = |s: &CameraSettings| (s.mode.clone(), s.format, s.roi, s.trigger);
            if self.streaming() && shape(&applied) != shape(&lock(&self.settings)) {
                let reason = "the mode, format, ROI and trigger can't change while it streams";
                return Err(Error::Busy {
                    reason: reason.to_owned(),
                });
            }
            later::<Json>(&self.link, "configure", json!(applied)).await?;
            *lock(&self.settings) = applied.clone();
            Ok(applied)
        })
    }

    fn start(&self, pool: FramePool, sink: FrameSink) -> HalFuture<'_, ()> {
        Box::pin(async move {
            if self.streaming() {
                return Err(Error::Busy {
                    reason: "it is already streaming".to_owned(),
                });
            }
            let settings = lock(&self.settings).clone();
            let (_, width, height) = self.capabilities.frame_shape(&settings)?;
            let needed = settings.format.frame_bytes(width, height);
            let slab = pool.stats().slab_len as f64;
            if slab < needed as f64 {
                let what = "the pool's slab size in bytes".to_owned();
                return Err(Error::OutOfRange {
                    what,
                    value: slab,
                    min: needed as f64,
                    max: f64::INFINITY,
                });
            }
            later::<Json>(&self.link, "start", json!(settings)).await?;
            let stop = Arc::new(AtomicBool::new(false));
            let (triggers, received) = mpsc::channel();
            let run = Run {
                link: Arc::clone(&self.link),
                capabilities: self.capabilities.clone(),
                settings: Arc::clone(&self.settings),
                clock: Arc::clone(&self.clock),
                pool,
                sink,
                stop: Arc::clone(&stop),
                triggers: received,
            };
            let thread = thread::Builder::new()
                .name("extension camera".to_owned())
                .spawn(move || run.go());
            let thread = thread.map_err(|e| device(e.to_string()))?;
            *lock(&self.stream) = Some(Stream {
                stop,
                triggers,
                thread,
            });
            Ok(())
        })
    }

    fn stop(&self) -> HalFuture<'_, ()> {
        Box::pin(async move {
            // Waits at most a frame for the capturing thread, as the HAL's
            // fake camera does.
            let link = Arc::clone(&self.link);
            if self.halt() {
                let done =
                    tokio::task::spawn_blocking(move || call::<Json>(&*link, "stop", Json::Null));
                done.await.map_err(|e| device(e.to_string()))??;
            }
            Ok(())
        })
    }

    fn streaming(&self) -> bool {
        lock(&self.stream).is_some()
    }

    fn trigger(&self) -> HalFuture<'_, ()> {
        Box::pin(async move {
            if !self.capabilities.trigger.software {
                return Err(Error::Unsupported {
                    what: "a software trigger".to_owned(),
                });
            }
            let busy = |reason: &str| Error::Busy {
                reason: reason.to_owned(),
            };
            let stream = lock(&self.stream);
            let stream = stream.as_ref().ok_or_else(|| busy("it isn't streaming"))?;
            if lock(&self.settings).trigger != Trigger::Software {
                return Err(busy("it isn't set to a software trigger"));
            }
            stream.triggers.send(()).map_err(|_| Error::Disconnected)
        })
    }

    fn temperature(&self) -> HalFuture<'_, f64> {
        Box::pin(async move {
            if !self.capabilities.temperature {
                return Err(Error::Unsupported {
                    what: "a temperature reading".to_owned(),
                });
            }
            later(&self.link, "temperature", Json::Null).await
        })
    }
}

/// An extension's stage, as the HAL's.
struct ExtStage {
    link: Arc<dyn Link>,
    capabilities: StageCapabilities,
    clock: Arc<dyn Clock>,
    track: Arc<PositionTrack>,
}

impl ExtStage {
    fn open(link: Arc<dyn Link>, clock: Arc<dyn Clock>) -> Result<ExtStage> {
        let capabilities = call(&*link, "capabilities", Json::Null)?;
        let stage = ExtStage {
            link,
            capabilities,
            clock,
            track: Arc::new(PositionTrack::new(1024)),
        };
        let position = call(&*stage.link, "position", Json::Null)?;
        stage.report(position);
        Ok(stage)
    }

    /// Records `position` on the track, at the clock's time.
    fn report(&self, position: Xyz) -> PositionSample {
        let sample = PositionSample {
            time: self.clock.now(),
            position,
        };
        self.track.push(sample);
        sample
    }

    async fn read(&self) -> Result<PositionSample> {
        let position = later(&self.link, "position", Json::Null).await?;
        Ok(self.report(position))
    }

    /// Fails if it is in alarm.
    async fn ready(&self) -> Result<()> {
        match later(&self.link, "status", Json::Null).await? {
            StageStatus::Alarm { reason } => Err(Error::Alarm { reason }),
            _ => Ok(()),
        }
    }

    /// Waits until the motion it started ends, reporting where it is each
    /// report interval: where it came to rest.
    async fn rest(&self) -> Result<PositionSample> {
        let interval = self
            .capabilities
            .report_interval
            .to_duration()
            .unwrap_or_default();
        loop {
            let status = later(&self.link, "status", Json::Null).await?;
            let sample = self.read().await?;
            match status {
                StageStatus::Idle => return Ok(sample),
                StageStatus::Alarm { reason } => return Err(Error::Alarm { reason }),
                _ => tokio::time::sleep(interval.max(Duration::from_millis(1))).await,
            }
        }
    }
}

impl Stage for ExtStage {
    fn capabilities(&self) -> &StageCapabilities {
        &self.capabilities
    }

    fn status(&self) -> HalFuture<'_, StageStatus> {
        Box::pin(later(&self.link, "status", Json::Null))
    }

    fn position(&self) -> HalFuture<'_, PositionSample> {
        Box::pin(self.read())
    }

    fn track(&self) -> Arc<PositionTrack> {
        Arc::clone(&self.track)
    }

    fn move_to(&self, target: Xyz, speed: Option<UmPerS>) -> HalFuture<'_, PositionSample> {
        Box::pin(async move {
            self.ready().await?;
            self.capabilities.check_target(target)?;
            self.capabilities.check_speed(speed)?;
            let speed = speed.unwrap_or(self.capabilities.default_speed);
            later::<Json>(
                &self.link,
                "move_to",
                json!({ "target": target, "speed": speed }),
            )
            .await?;
            self.rest().await
        })
    }

    fn jog(&self, axis: Axis, towards_max: bool, speed: Option<UmPerS>) -> HalFuture<'_, ()> {
        Box::pin(async move {
            self.ready().await?;
            self.capabilities.check_speed(speed)?;
            let speed = speed.unwrap_or(self.capabilities.default_speed);
            let params = json!({ "axis": axis, "towards_max": towards_max, "speed": speed });
            later(&self.link, "jog", params).await
        })
    }

    fn stop(&self) -> HalFuture<'_, ()> {
        Box::pin(async move {
            later::<Json>(&self.link, "stop", Json::Null).await?;
            self.rest().await.map(drop)
        })
    }

    fn home(&self) -> HalFuture<'_, PositionSample> {
        Box::pin(async move {
            if !self.capabilities.homing {
                return Err(Error::Unsupported {
                    what: "homing".to_owned(),
                });
            }
            later::<Json>(&self.link, "home", Json::Null).await?;
            self.rest().await
        })
    }

    fn clear_alarm(&self) -> HalFuture<'_, ()> {
        Box::pin(later(&self.link, "clear_alarm", Json::Null))
    }

    fn arm_trigger(&self, axis: Axis, positions: Vec<Um>) -> HalFuture<'_, ()> {
        Box::pin(async move {
            if !self.capabilities.trigger_output {
                return Err(Error::Unsupported {
                    what: "a trigger output".to_owned(),
                });
            }
            later(
                &self.link,
                "arm_trigger",
                json!({ "axis": axis, "positions": positions }),
            )
            .await
        })
    }
}

/// An extension's lights, as the HAL's.
struct ExtIllumination {
    link: Arc<dyn Link>,
    capabilities: IlluminationCapabilities,
}

impl ExtIllumination {
    fn open(link: Arc<dyn Link>) -> Result<ExtIllumination> {
        let capabilities = call(&*link, "capabilities", Json::Null)?;
        Ok(ExtIllumination { link, capabilities })
    }
}

impl Illumination for ExtIllumination {
    fn capabilities(&self) -> &IlluminationCapabilities {
        &self.capabilities
    }

    fn state(&self) -> HalFuture<'_, Vec<LightState>> {
        Box::pin(later(&self.link, "state", Json::Null))
    }

    fn set(&self, light: LightState) -> HalFuture<'_, LightState> {
        Box::pin(async move {
            let applied = self.capabilities.check(&light)?;
            later::<Json>(&self.link, "set", json!(applied)).await?;
            Ok(applied)
        })
    }
}
