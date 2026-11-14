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
use oscope_ext::{Extensions, OSCOPE_FOLDER, PythonRuntime};
use oscope_pipeline::{Engine, Plan, Recipe, Registry, RunOptions, Value, Values};
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

[python]
requires-python = ">=3.12,<3.15"
entry = "testnodes"
"#;

const MODULE: &str = r#""""Nodes for OScope's tests of Python extensions."""

import os
import time

from oscope_ext import node


@node("test.scale", tileable=0)
def scale(image, *, by: float = 2.0):
    """Multiplies each sample."""
    return image * by


@node("test.split", outputs=("first", "rest"), params={"type": "object"})
def split(image):
    return image[:, :, :1], image[:, :, 1:]


@node("test.sleep")
def sleep(image, *, seconds: float = 1.0):
    time.sleep(seconds)
    return image


@node("test.crash")
def crash(image):
    os._exit(3)


@node("test.fail")
def fail(image):
    raise ValueError("no good")
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
    let sleeping = Arc::clone(&engine);
    let call =
        std::thread::spawn(move || run(&sleeping, "test.sleep", json!({ "seconds": 60 }), ramp()));
    std::thread::sleep(Duration::from_millis(500));
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

/// Kills the process `pid` and its children.
fn kill(pid: u32) {
    let status = if cfg!(windows) {
        std::process::Command::new("taskkill")
            .args(["/F", "/T", "/PID", &pid.to_string()])
            .output()
    } else {
        std::process::Command::new("kill")
            .args(["-9", &pid.to_string()])
            .output()
    };
    assert!(status.unwrap().status.success(), "killed {pid}");
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
