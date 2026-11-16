//! Python extensions, with uv: run only when `OSCOPE_PYTHON=1`. Installing
//! makes the environment and keeps the nodes its worker describes. The
//! worker starts on first use, and runs nodes on frames through shared
//! memory. A node's failure is said, and its worker goes on. A worker that
//! dies or is killed mid-call, or takes too long, is an error, and the next
//! call starts another; one killed between calls is found dead and replaced.
//! An idle worker is stopped.

use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use oscope_compute::{Compute, ComputeConfig, Image, Shape};
use oscope_core::{
    Clock, FrameBuffer, FramePool, HeapAllocator, PixelFormat, SystemClock, Um, Xyz,
};
use oscope_ext::{Extensions, OSCOPE_FOLDER, PythonRuntime};
use oscope_hal::{Camera, Error, FrameSink, Trigger, conformance};
use oscope_pipeline::{Engine, Plan, Recipe, Registry, RunOptions, Value, Values};
use oscope_store::{ImageInfo, SampleType, Samples};
use serde_json::json;

const ID: &str = "org.example.testnodes";

/// Whether to run: `OSCOPE_PYTHON=1`, with uv installed (CLAUDE.md).
fn enabled() -> bool {
    let on = std::env::var("OSCOPE_PYTHON").is_ok_and(|value| value == "1");
    if !on {
        eprintln!("skipped: set OSCOPE_PYTHON=1, with uv installed, to run it");
    }
    on
}

const MANIFEST: &str = r#"id = "org.example.testnodes"
name = "Test nodes"
version = "1.0.0"
license = "MIT"
publisher = "OScope's tests"
oscope-api = "^1.0"
kind = "python"

[provides]
nodes = ["test.scale", "test.split", "test.sleep", "test.crash", "test.fail"]
drivers = ["test.stage", "test.lights", "test.camera"]
importers = ["test.npy.read"]
exporters = ["test.npy.write"]

[python]
requires-python = ">=3.12,<3.15"
entry = "testnodes"
"#;

const MODULE: &str = r#""""Nodes for OScope's tests of Python extensions."""

import os
import time

import numpy as np
from oscope_ext import driver, node


@node("test.scale", tileable=0)
def scale(image, *, by: float = 2.0):
    """Multiplies each sample."""
    return image * by


@node("test.split", outputs=("first", "rest"), params={"type": "object"})
def split(image):
    return image[:, :, :1], image[:, :, 1:]


@node("test.sleep")
def sleep(image, *, seconds: float = 1.0, started: str = ""):
    if started:
        open(started, "w").close()
    time.sleep(seconds)
    return image


@node("test.crash")
def crash(image):
    os._exit(3)


@node("test.fail")
def fail(image):
    raise ValueError("no good")


@driver("test.stage", kind="stage")
class Stage:
    """A stage that arrives on the second look at its status, and trips a
    limit switch on arriving at x = 12,345 µm."""

    def __init__(self):
        self.at = {"x": 0.0, "y": 0.0, "z": 0.0}
        self.state = {"state": "idle"}
        self.target, self.looks = None, 0

    def capabilities(self):
        travel = lambda mm: {"min": 0.0, "max": mm * 1000.0}
        return {
            "model": "test stage", "x": travel(100), "y": travel(75), "z": travel(20),
            "max_speed": 50000.0, "default_speed": 10000.0, "homing": True,
            "encoders": False, "trigger_output": False, "report_interval": 10.0,
        }

    def status(self):
        if self.target is not None:
            self.looks += 1
            if self.looks == 2:
                self.at, self.target = self.target, None
                tripped = self.at["x"] == 12345.0
                self.state = {"state": "alarm", "reason": "a limit switch"} if tripped else {"state": "idle"}
        return self.state

    def position(self):
        return self.at

    def move_to(self, request):
        self.target, self.looks = dict(request["target"]), 0
        self.state = {"state": "moving"}

    def clear_alarm(self):
        self.state = {"state": "idle"}

    def jog(self, request):
        self.state = {"state": "moving"}

    def stop(self):
        self.state = {"state": "idle"}

    def home(self):
        self.at = {"x": 0.0, "y": 0.0, "z": 0.0}


@driver("test.lights", kind="illumination")
class Lights:
    def __init__(self):
        self.lights = [{"channel": "transmitted", "on": False, "intensity": 0.0}]

    def capabilities(self):
        return {"model": "test lights", "channels": [{"name": "transmitted", "kind": "transmitted", "dimmable": True}]}

    def state(self):
        return self.lights

    def set(self, light):
        self.lights = [light if old["channel"] == light["channel"] else old for old in self.lights]


@driver("test.camera", kind="camera")
class Camera:
    """A 64 × 48 sensor in 8 or 16 bits of grey, its frames counting up."""

    def __init__(self):
        self.wide, self.count = False, 0

    def capabilities(self):
        rate = lambda kind, fps: {"format": {"kind": kind}, "fps": fps}
        mode = {"name": "full", "width": 64, "height": 48, "binning": 1, "line_time": None,
                "rates": [rate("mono8", 100.0), rate("mono16", 50.0)]}
        return {
            "model": "test camera", "sensor_width": 64, "sensor_height": 48, "pixel_size": 5.0,
            "shutter": "global", "modes": [mode], "exposure_min": 0.01, "exposure_max": 1000.0,
            "gain_min": 0.0, "gain_max": 10.0, "roi": None,
            "trigger": {"software": False, "external": False, "output": False},
            "sensor_timestamps": False, "temperature": False,
        }

    def configure(self, settings):
        self.wide = settings["format"]["kind"] == "mono16"

    def start(self, settings):
        self.configure(settings)

    def capture(self):
        time.sleep(0.01)
        self.count += 1
        return np.full((48, 64), self.count % 256, dtype=np.uint16 if self.wide else np.uint8)

    def stop(self):
        pass


@driver("test.npy.read", kind="importer")
class NpyReader:
    """NumPy's .npy: (height, width), (height, width, channels), or
    (planes, height, width, channels)."""

    def format(self):
        return {"name": "NumPy array", "extensions": ["npy"]}

    def _array(self, path):
        array = np.load(path)
        return array[None, :, :, None] if array.ndim == 2 else array[None] if array.ndim == 3 else array

    def probe(self, params):
        array = self._array(params["path"])
        sample = {"uint8": "u8", "uint16": "u16", "float32": "f32"}[array.dtype.name]
        planes, height, width, channels = array.shape
        return {"width": width, "height": height, "channels": channels, "planes": planes, "sample": sample}

    def read(self, params):
        return self._array(params["path"])[params["plane"]]


@driver("test.npy.write", kind="exporter")
class NpyWriter:
    """NumPy's .npy, (planes, height, width, channels)."""

    def format(self):
        return {"name": "NumPy array", "extensions": ["npy"]}

    def begin(self, params):
        self.path, self.image, self.planes = params["path"], params["image"], []

    def write(self, params, pixels):
        image = self.image
        dtype = {"u8": "<u1", "u16": "<u2", "f32": "<f4"}[image["sample"]]
        self.planes.append(pixels.view(dtype).reshape(image["height"], image["width"], image["channels"]))

    def end(self):
        with open(self.path, "wb") as out:
            np.save(out, np.stack(self.planes))
"#;

/// Packs the test extension, with `manifest`, into `folder`.
fn package(folder: &Path, manifest: &str) -> std::path::PathBuf {
    let source = folder.join("source");
    fs::create_dir_all(source.join("testnodes")).unwrap();
    fs::write(source.join("manifest.toml"), manifest).unwrap();
    fs::write(source.join("testnodes").join("__init__.py"), MODULE).unwrap();
    let package = folder.join("testnodes.omext");
    oscope_ext::pack(&source, &package).unwrap();
    package
}

/// Installs the test extension in `folder` with `python`, then starts again.
fn installed(folder: &Path, python: &PythonRuntime) -> Extensions {
    let root = folder.join("extensions");
    let package = package(folder, MANIFEST);
    let extensions = Extensions::open_with(&root, Some(python.clone())).unwrap();
    extensions.install(&package).unwrap();
    Extensions::open_with(&root, Some(python.clone())).unwrap()
}

fn engine(extensions: &Extensions) -> Engine {
    let mut registry = Registry::new();
    for node in extensions.nodes() {
        registry.register(Arc::clone(node)).unwrap();
    }
    let config = ComputeConfig {
        live_threads: Some(2),
        batch_threads: Some(2),
        cuda_device: None,
    };
    Engine::new(registry, Arc::new(Compute::new(&config).unwrap()), 0)
}

/// Runs node `id` on `image`, with `params`: every output.
fn run(
    engine: &Engine,
    id: &str,
    params: serde_json::Value,
    image: Image,
) -> Result<Values, String> {
    let descriptor = engine.registry().get(id).unwrap().descriptor().clone();
    let outputs: Vec<_> = descriptor
        .outputs
        .iter()
        .map(|port| json!({ "name": port.name, "from": format!("n.{}", port.name) }))
        .collect();
    let recipe: Recipe = serde_json::from_value(json!({
        "format": 1,
        "inputs": [{ "name": "image", "type": "image" }],
        "nodes": [{ "id": "n", "node": id, "params": params }],
        "edges": [{ "from": "input.image", "to": "n.image" }],
        "outputs": outputs,
    }))
    .unwrap();
    let plan = Plan::new(engine.registry(), &recipe, false).map_err(|e| e.to_string())?;
    let inputs = Values::from([("image".to_owned(), Value::image(image))]);
    let options = RunOptions {
        memoise: false,
        ..RunOptions::default()
    };
    Ok(engine
        .run(&plan, &inputs, &options)
        .map_err(|e| e.to_string())?
        .outputs)
}

/// A 40 × 30 RGB ramp.
fn ramp() -> Image {
    let data = (0..40 * 30 * 3)
        .map(|i| (i % 97) as f32 / 97.0 - 0.25)
        .collect();
    Image::from_vec(Shape::new(40, 30, 3), data).unwrap()
}

fn image(outputs: &Values, port: &str) -> Image {
    outputs[port].as_image().unwrap().clone()
}

/// Waits up to 30 s for `check`.
fn wait_for(what: &str, check: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(30);
    while !check() {
        assert!(Instant::now() < deadline, "{what}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn an_importer_and_an_exporter_in_python_take_a_stack_through_shared_memory() {
    if !enabled() {
        return;
    }
    let folder = tempfile::tempdir().unwrap();
    let extensions = installed(folder.path(), &PythonRuntime::default());
    let formats = extensions.formats();
    let (reader, writer) = (&formats.importers[0], &formats.exporters[0]);
    let format = reader.format();
    assert_eq!(
        (format.id.as_str(), format.name.as_str()),
        ("test.npy.read", "NumPy array")
    );
    assert_eq!(
        (format.extensions, writer.format().id),
        (vec!["npy".to_owned()], "test.npy.write".to_owned())
    );

    // Two planes of 16-bit RGB out, through the exporter, and in again.
    let mut info = ImageInfo::new(5, 3, 3, SampleType::U16);
    info.planes = 2;
    let planes: Vec<Samples> = (0..2_u16)
        .map(|p| Samples::U16((0..45).map(|i| i * 1000 + p * 7).collect()))
        .collect();
    let path = folder.path().join("stack.npy");
    writer
        .write(&path, &info, "a stack", &mut |p| {
            Ok(planes[p as usize].clone())
        })
        .unwrap();
    let probed = reader.probe(&path).unwrap();
    let shape = (
        probed.width,
        probed.height,
        probed.channels,
        probed.planes,
        probed.sample,
    );
    assert_eq!(shape, (5, 3, 3, 2, SampleType::U16));
    for (p, plane) in planes.iter().enumerate() {
        assert_eq!(&reader.read(&path, &probed, p as u32).unwrap(), plane);
    }
    // A file that isn't there: the importer says why.
    let error = reader
        .probe(&folder.path().join("none.npy"))
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("test.npy.read") && error.contains("No such file"),
        "{error}"
    );
}

#[test]
fn installing_makes_an_environment_whose_worker_runs_nodes_through_shared_memory() {
    if !enabled() {
        return;
    }
    let folder = tempfile::tempdir().unwrap();
    let extensions = installed(folder.path(), &PythonRuntime::default());
    let unpacked = folder.path().join("extensions").join(ID).join("1.0.0");
    assert!(unpacked.join(OSCOPE_FOLDER).join("venv").is_dir());
    assert!(unpacked.join(OSCOPE_FOLDER).join("nodes.json").is_file());
    let ids: Vec<_> = extensions
        .nodes()
        .iter()
        .map(|node| node.descriptor().id.clone())
        .collect();
    assert_eq!(
        ids,
        [
            "test.scale",
            "test.split",
            "test.sleep",
            "test.crash",
            "test.fail"
        ]
    );
    let scale = extensions.nodes()[0].descriptor();
    let by = json!({ "type": "object", "properties": { "by": { "type": "number", "default": 2.0 } }, "additionalProperties": false });
    assert_eq!(
        (&scale.description, &scale.params),
        (&"Multiplies each sample.".to_owned(), &by)
    );
    assert!(scale.capabilities.tileable);
    assert_eq!(extensions.workers()[0].pid, None, "started on first use");

    // Through shared memory both ways, to the bit.
    let engine = engine(&extensions);
    let same = run(&engine, "test.scale", json!({ "by": 1.0 }), ramp()).unwrap();
    assert_eq!(image(&same, "image"), ramp());
    assert!(extensions.workers()[0].pid.is_some());
    let doubled = image(
        &run(&engine, "test.scale", json!({}), ramp()).unwrap(),
        "image",
    );
    let expected: Vec<f32> = ramp().data().iter().map(|s| 2.0 * s).collect();
    assert_eq!(doubled.data(), &expected[..]);
    let split = run(&engine, "test.split", json!({}), ramp()).unwrap();
    let (first, rest) = (image(&split, "first"), image(&split, "rest"));
    assert_eq!(
        (first.shape(), rest.shape()),
        (Shape::new(40, 30, 1), Shape::new(40, 30, 2))
    );
    assert_eq!(
        (first.data()[1], rest.data()[0]),
        (ramp().data()[3], ramp().data()[1])
    );
}

#[test]
fn a_failing_node_says_why_and_its_worker_goes_on() {
    if !enabled() {
        return;
    }
    let folder = tempfile::tempdir().unwrap();
    let extensions = installed(folder.path(), &PythonRuntime::default());
    let engine = engine(&extensions);
    run(&engine, "test.scale", json!({}), ramp()).unwrap();
    let pid = extensions.workers()[0].pid;
    let error = run(&engine, "test.fail", json!({}), ramp()).unwrap_err();
    assert!(error.contains("ValueError: no good"), "{error}");
    assert_eq!(extensions.workers()[0].pid, pid, "the same worker");
    run(&engine, "test.scale", json!({}), ramp()).unwrap();
}

#[test]
fn a_worker_that_dies_or_is_killed_mid_call_is_an_error_and_starts_again() {
    if !enabled() {
        return;
    }
    let folder = tempfile::tempdir().unwrap();
    let extensions = installed(folder.path(), &PythonRuntime::default());
    let engine = Arc::new(engine(&extensions));
    let error = run(&engine, "test.crash", json!({}), ramp()).unwrap_err();
    assert!(error.contains("its worker was stopped"), "{error}");
    assert_eq!(extensions.workers()[0].pid, None);
    run(&engine, "test.scale", json!({}), ramp()).unwrap();

    // Killed from outside, mid-call.
    let first = extensions.workers()[0].pid.unwrap();
    let (sleeping, started) = (Arc::clone(&engine), folder.path().join("started"));
    let params = json!({ "seconds": 60, "started": started });
    let call = std::thread::spawn(move || run(&sleeping, "test.sleep", params, ramp()));
    wait_for("the call in the worker", || started.exists());
    kill(first);
    let error = call.join().unwrap().unwrap_err();
    assert!(error.contains("its worker was stopped"), "{error}");
    run(&engine, "test.scale", json!({}), ramp()).unwrap();
    let second = extensions.workers()[0].pid.unwrap();
    assert_ne!(first, second, "another worker");

    // Killed between calls: found dead, and another started for the call.
    kill(second);
    wait_for("the worker gone", || !alive(second));
    run(&engine, "test.scale", json!({}), ramp()).unwrap();
    assert_ne!(extensions.workers()[0].pid, Some(second));
}

/// Whether the process `pid` runs. On Linux, a killed child stays a zombie
/// until its parent reaps it, as the worker's next call does: not running.
fn alive(pid: u32) -> bool {
    let pid = pid.to_string();
    if cfg!(windows) {
        let filter = format!("PID eq {pid}");
        let listed = std::process::Command::new("tasklist")
            .args(["/FI", &filter, "/NH"])
            .output();
        String::from_utf8_lossy(&listed.unwrap().stdout).contains(&pid)
    } else {
        let listed = std::process::Command::new("ps")
            .args(["-o", "stat=", "-p", &pid])
            .output();
        let state = String::from_utf8_lossy(&listed.unwrap().stdout)
            .trim()
            .to_owned();
        !state.is_empty() && !state.starts_with('Z')
    }
}

/// Kills the process `pid` and its children, and waits for it to be gone.
/// `taskkill /T` can fail for a child that ends meanwhile: what counts is
/// that the process has gone.
fn kill(pid: u32) {
    let _ = if cfg!(windows) {
        std::process::Command::new("taskkill")
            .args(["/F", "/T", "/PID", &pid.to_string()])
            .output()
    } else {
        std::process::Command::new("kill")
            .args(["-9", &pid.to_string()])
            .output()
    };
    wait_for("the worker killed", || !alive(pid));
}

#[test]
fn a_call_that_takes_too_long_stops_the_worker_and_an_idle_worker_is_stopped() {
    if !enabled() {
        return;
    }
    let folder = tempfile::tempdir().unwrap();
    let python = PythonRuntime {
        call_timeout: Duration::from_secs(2),
        idle: Duration::from_secs(3),
        ..PythonRuntime::default()
    };
    let extensions = installed(folder.path(), &python);
    let engine = engine(&extensions);
    let started = Instant::now();
    let error = run(&engine, "test.sleep", json!({ "seconds": 60 }), ramp()).unwrap_err();
    assert!(
        error.contains("it took too long; its worker was stopped"),
        "{error}"
    );
    assert!(
        started.elapsed() < Duration::from_secs(30),
        "{:?}",
        started.elapsed()
    );
    run(&engine, "test.scale", json!({}), ramp()).unwrap();
    assert!(extensions.workers()[0].pid.is_some());
    wait_for("the idle worker stopped", || {
        extensions.workers()[0].pid.is_none()
    });
}

#[test]
fn a_module_whose_nodes_are_not_its_manifest_s_is_refused_at_install() {
    if !enabled() {
        return;
    }
    let folder = tempfile::tempdir().unwrap();
    let fewer = MANIFEST.replace(
        r#"nodes = ["test.scale", "test.split", "test.sleep", "test.crash", "test.fail"]"#,
        r#"nodes = ["test.scale"]"#,
    );
    let root = folder.path().join("extensions");
    let extensions = Extensions::open_with(&root, Some(PythonRuntime::default())).unwrap();
    let error = extensions
        .install(&package(folder.path(), &fewer))
        .unwrap_err()
        .to_string();
    assert!(
        error.starts_with("Python: its module has the nodes `test.crash`"),
        "{error}"
    );
    assert!(
        error.ends_with("and its manifest provides `test.scale`"),
        "{error}"
    );
    assert!(!root.join(ID).join("1.0.0").exists(), "nothing left");
}

#[tokio::test(flavor = "multi_thread")]
async fn python_drivers_pass_the_hal_conformance_suite() {
    if !enabled() {
        return;
    }
    let folder = tempfile::tempdir().unwrap();
    let extensions = installed(folder.path(), &PythonRuntime::default());
    let names: Vec<_> = extensions.drivers().names().collect();
    assert_eq!(
        names,
        [
            ("test.camera", "camera"),
            ("test.lights", "illumination"),
            ("test.stage", "stage")
        ]
    );
    let clock: Arc<dyn Clock> = Arc::new(SystemClock::new());
    let drivers = extensions.drivers();
    let stage = drivers
        .stage("test.stage", Arc::clone(&clock))
        .unwrap()
        .unwrap();
    conformance::stage(&*stage, Um(1.0)).await.unwrap();
    // A move that ends in alarm fails with it, and the next is refused,
    // without moving, until the alarm is cleared.
    let alarm = |result: Result<_, Error>| matches!(result, Err(Error::Alarm { .. }));
    let at = |x: f64| Xyz::new(Um(x), Um(0.0), Um(0.0));
    assert!(alarm(stage.move_to(at(12_345.0), None).await));
    assert!(alarm(stage.move_to(at(1.0), None).await));
    let still = stage.position().await.unwrap().position;
    assert_eq!(still, at(12_345.0));
    stage.clear_alarm().await.unwrap();
    assert_eq!(
        stage.move_to(at(1.0), None).await.unwrap().position,
        at(1.0)
    );
    let lights = drivers.illumination("test.lights").unwrap().unwrap();
    conformance::illumination(&*lights).await.unwrap();
    // Frames through shared memory.
    let camera = drivers.camera("test.camera", clock).unwrap().unwrap();
    conformance::camera(&*camera).await.unwrap();
    // Their pixels come through: each frame is its count, one more each time.
    let [first, second] = two_frames(&*camera).await;
    assert!(first.iter().all(|byte| *byte == first[0]));
    assert_eq!(second, vec![first[0].wrapping_add(1); first.len()]);
}

/// The bytes of `camera`'s next two frames, in its full mode, in 8 bits of
/// grey, free-running.
async fn two_frames(camera: &dyn Camera) -> [Vec<u8>; 2] {
    let mut settings = camera.settings().await.unwrap();
    settings.mode = "full".to_owned();
    settings.format = PixelFormat::Mono8;
    settings.roi = None;
    settings.trigger = Trigger::FreeRunning;
    let settings = camera.configure(settings).await.unwrap();
    let (_, width, height) = camera.capabilities().frame_shape(&settings).unwrap();
    let bytes = settings.format.frame_bytes(width, height);
    let (sender, frames) = std::sync::mpsc::channel();
    let sink: FrameSink = Arc::new(move |buffer: FrameBuffer, _| {
        let _ = sender.send(buffer.bytes()[..bytes].to_vec());
    });
    let pool = FramePool::new(bytes, 4, &HeapAllocator);
    camera.start(pool, sink).await.unwrap();
    let next = || frames.recv_timeout(Duration::from_secs(10)).unwrap();
    let both = [next(), next()];
    camera.stop().await.unwrap();
    both
}

#[test]
fn the_cells_stub_segments_by_its_stand_in() {
    if !enabled() {
        return;
    }
    // P46's stub, packed and installed: without cellpose or StarDist, a
    // mask of the pixels above the image's mean.
    let folder = tempfile::tempdir().unwrap();
    let sample = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/ext-python-cells");
    let package = folder.path().join("cells-stub.omext");
    oscope_ext::pack(&sample, &package).unwrap();
    let root = folder.path().join("extensions");
    let python = PythonRuntime::default();
    Extensions::open_with(&root, Some(python.clone()))
        .unwrap()
        .install(&package)
        .unwrap();
    let extensions = Extensions::open_with(&root, Some(python)).unwrap();
    let engine = engine(&extensions);
    let picture = ramp();
    let labels = image(
        &run(&engine, "example.cells_segment", json!({}), picture.clone()).unwrap(),
        "image",
    );
    assert_eq!(labels.shape(), Shape::new(40, 30, 1));
    let grey: Vec<f32> = picture
        .data()
        .chunks(3)
        .map(|p| p.iter().sum::<f32>() / 3.0)
        .collect();
    let mean = grey.iter().sum::<f32>() / grey.len() as f32;
    let want: Vec<f32> = grey
        .iter()
        .map(|&v| f32::from(u8::from(v > mean)))
        .collect();
    let wrong = labels
        .data()
        .iter()
        .zip(&want)
        .filter(|(a, b)| a != b)
        .count();
    assert!(wrong <= 2, "{wrong} pixels unlike the stand-in's");
}
