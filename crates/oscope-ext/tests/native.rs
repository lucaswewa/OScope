//! Rust extensions through the C ABI, in this process: an extension made
//! with `export!` describes its nodes, which run in a pipeline; a panic, a
//! failure and a missing output become errors; another ABI version, and a
//! description the pipeline can't take, are refused; a package whose library
//! doesn't load says why at the next start, and a node that OScope has is
//! refused.

use std::fs;
use std::io::Write;
use std::sync::Arc;

use oscope_compute::{Compute, ComputeConfig, Image, Shape};
use oscope_ext::{Contribution, Extensions, Library, PLATFORM};
use oscope_ext_sdk::{Descriptor, Outputs, ProcessingNode, abi, export, serde_json};
use oscope_pipeline::{Engine, Plan, Recipe, Registry, RunOptions, Value, Values};
use serde_json::json;

/// Multiplies each sample by `by`, 2 unless given.
struct Scale;

impl ProcessingNode for Scale {
    fn descriptor(&self) -> Descriptor {
        let by = json!({ "type": "object", "properties": { "by": { "type": "number" } }, "additionalProperties": false });
        Descriptor::new("test.scale", 1, "test", "Multiplies each sample.")
            .input("image")
            .output("image")
            .params(by)
            .live()
            .tileable(0)
    }

    fn run(
        &self,
        params: &serde_json::Value,
        inputs: &[oscope_ext_sdk::Image<'_>],
        outputs: &mut Outputs<'_>,
    ) -> Result<(), String> {
        let by = params["by"].as_f64().unwrap_or(2.0) as f32;
        let image = inputs[0];
        let out = outputs.image(0, image.width(), image.height(), image.channels())?;
        for (out, sample) in out.iter_mut().zip(image.data()) {
            *out = by * sample;
        }
        Ok(())
    }
}

/// A node that does `what` instead of its work.
struct Broken {
    id: &'static str,
    what: fn(&mut Outputs<'_>) -> Result<(), String>,
}

impl ProcessingNode for Broken {
    fn descriptor(&self) -> Descriptor {
        Descriptor::new(self.id, 1, "test", "Breaks.")
            .input("image")
            .output("image")
    }

    fn run(
        &self,
        _: &serde_json::Value,
        _: &[oscope_ext_sdk::Image<'_>],
        outputs: &mut Outputs<'_>,
    ) -> Result<(), String> {
        (self.what)(outputs)
    }
}

export!(
    Scale,
    Broken {
        id: "test.panics",
        what: |_| panic!("boom, at sample {}", 3)
    },
    Broken {
        id: "test.fails",
        what: |_| Err("no good".to_owned())
    },
    Broken {
        id: "test.lazy",
        what: |_| Ok(())
    },
    Broken {
        id: "test.greedy",
        what: |outputs| {
            outputs.image(0, 2, 2, 1)?;
            outputs.image(0, 2, 2, 1).map(drop)
        },
    },
);

/// An engine with the nodes of `library`.
fn engine(library: &Arc<Library>) -> Engine {
    let mut registry = Registry::new();
    for node in library.nodes() {
        registry.register(node).unwrap();
    }
    let config = ComputeConfig {
        live_threads: Some(2),
        batch_threads: Some(2),
        cuda_device: None,
    };
    Engine::new(registry, Arc::new(Compute::new(&config).unwrap()), 0)
}

/// Runs node `id` on `image`, with `params`.
fn run(
    engine: &Engine,
    id: &str,
    params: serde_json::Value,
    image: Image,
) -> Result<Image, String> {
    let recipe: Recipe = serde_json::from_value(json!({
        "format": 1,
        "inputs": [{ "name": "image", "type": "image" }],
        "nodes": [{ "id": "n", "node": id, "params": params }],
        "edges": [{ "from": "input.image", "to": "n.image" }],
        "outputs": [{ "name": "image", "from": "n.image" }],
    }))
    .unwrap();
    let plan = Plan::new(engine.registry(), &recipe, false).map_err(|e| e.to_string())?;
    let inputs = Values::from([("image".to_owned(), Value::image(image))]);
    let options = RunOptions {
        memoise: false,
        ..RunOptions::default()
    };
    let ran = engine
        .run(&plan, &inputs, &options)
        .map_err(|e| e.to_string())?;
    Ok(ran.outputs["image"].as_image().unwrap().clone())
}

/// A 64 × 48 RGB ramp.
fn ramp() -> Image {
    let data = (0..64 * 48 * 3).map(|i| (i % 251) as f32 / 251.0).collect();
    Image::from_vec(Shape::new(64, 48, 3), data).unwrap()
}

#[test]
fn an_extension_describes_its_nodes_which_run_in_a_pipeline() {
    let library = Library::from_entry(oscope_extension).unwrap();
    let ids: Vec<_> = library
        .descriptors()
        .iter()
        .map(|d| d.id.as_str())
        .collect();
    assert_eq!(
        ids,
        [
            "test.scale",
            "test.panics",
            "test.fails",
            "test.lazy",
            "test.greedy"
        ]
    );
    let scale = &library.descriptors()[0];
    assert!(scale.capabilities.cpu && scale.capabilities.live && scale.capabilities.tileable);
    let engine = engine(&library);
    let image = ramp();
    let doubled = run(&engine, "test.scale", json!({}), image.clone()).unwrap();
    let expected: Vec<f32> = image.data().iter().map(|s| 2.0 * s).collect();
    assert_eq!(
        (doubled.shape(), doubled.data()),
        (image.shape(), &expected[..])
    );
    let tripled = run(&engine, "test.scale", json!({ "by": 3.0 }), image.clone()).unwrap();
    assert_eq!(tripled.data()[3], 3.0 * image.data()[3]);
    // The node's schema is the pipeline's to check.
    let refused = run(&engine, "test.scale", json!({ "by": "three" }), image).unwrap_err();
    assert!(refused.contains("params/by"), "{refused}");
}

#[test]
fn a_panic_a_failure_and_a_missing_output_become_errors() {
    let library = Library::from_entry(oscope_extension).unwrap();
    let engine = engine(&library);
    let cases = [
        ("test.panics", "it panicked: boom, at sample 3"),
        ("test.fails", "no good"),
        ("test.lazy", "it made no output `image`"),
        ("test.greedy", "output 0 can't be made"),
    ];
    for (id, expected) in cases {
        let error = run(&engine, id, json!({}), ramp()).unwrap_err();
        assert!(error.contains(expected), "{id}: {error}");
    }
    // The extension still works.
    assert!(run(&engine, "test.scale", json!({}), ramp()).is_ok());
}

/// Describes the node `node`, given the host's table.
fn describe_as(
    host: *const abi::Host,
    call: *mut abi::Call,
    node: serde_json::Value,
) -> abi::Status {
    // SAFETY: the host lends its table for the call.
    let host = unsafe { &*host };
    let described = json!({ "nodes": [node], "drivers": [] });
    (host.describe)(call, abi::Str::new(&described.to_string()));
    abi::OK
}

/// A node of `inputs` and `params`, giving an image.
fn node(id: &str, inputs: serde_json::Value, params: serde_json::Value) -> serde_json::Value {
    json!({
        "id": id, "version": 1, "category": "test", "description": "",
        "inputs": inputs,
        "outputs": [{ "name": "image", "type": "image" }],
        "params": params,
        "capabilities": { "cpu": true, "cuda": false, "live": false, "tileable": false, "halo": 0 },
    })
}

/// Describes a node with a mask for its input, which ABI 2 doesn't carry.
extern "C" fn describe_mask(host: *const abi::Host, call: *mut abi::Call) -> abi::Status {
    let inputs = json!([{ "name": "mask", "type": "mask" }]);
    describe_as(
        host,
        call,
        node("test.mask", inputs, json!({ "type": "object" })),
    )
}

/// Describes a node whose parameters' schema uses what the pipeline's
/// checks don't support.
extern "C" fn describe_unchecked(host: *const abi::Host, call: *mut abi::Call) -> abi::Status {
    let inputs = json!([{ "name": "image", "type": "image" }]);
    let params = json!({ "type": "object", "properties": { "name": { "pattern": "^a" } } });
    describe_as(host, call, node("test.unchecked", inputs, params))
}

extern "C" fn never(
    _: *const abi::Host,
    _: *mut abi::Call,
    _: u32,
    _: abi::Str,
    _: *const abi::ImageRef,
    _: u32,
) -> abi::Status {
    abi::FAILED
}

extern "C" fn no_driver_call(
    _: *const abi::Host,
    _: *mut abi::Call,
    _: u32,
    _: abi::Str,
    _: abi::Str,
) -> abi::Status {
    abi::FAILED
}

extern "C" fn no_capture(
    _: *const abi::Host,
    _: *mut abi::Call,
    _: u32,
    _: *mut u8,
    _: usize,
) -> abi::Status {
    abi::FAILED
}

extern "C" fn no_read(
    _: *const abi::Host,
    _: *mut abi::Call,
    _: u32,
    _: abi::Str,
    _: *mut u8,
    _: usize,
) -> abi::Status {
    abi::FAILED
}

extern "C" fn no_write(
    _: *const abi::Host,
    _: *mut abi::Call,
    _: u32,
    _: abi::Str,
    _: *const u8,
    _: usize,
) -> abi::Status {
    abi::FAILED
}

/// A table of `describe`, speaking ABI `version`.
const fn table(
    version: u32,
    describe: extern "C" fn(*const abi::Host, *mut abi::Call) -> abi::Status,
) -> abi::Extension {
    abi::Extension {
        abi_version: version,
        describe,
        run: never,
        driver_call: no_driver_call,
        capture: no_capture,
        read: no_read,
        write: no_write,
    }
}

static NEXT_VERSION: abi::Extension = table(abi::ABI_VERSION + 1, describe_mask);
static MASKED: abi::Extension = table(abi::ABI_VERSION, describe_mask);
static UNCHECKED: abi::Extension = table(abi::ABI_VERSION, describe_unchecked);

extern "C" fn next_version() -> *const abi::Extension {
    &NEXT_VERSION
}

extern "C" fn masked() -> *const abi::Extension {
    &MASKED
}

extern "C" fn unchecked() -> *const abi::Extension {
    &UNCHECKED
}

extern "C" fn nothing() -> *const abi::Extension {
    std::ptr::null()
}

#[test]
fn another_abi_version_and_a_description_the_pipeline_cannot_take_are_refused() {
    let cases: [(abi::Entry, &str); 4] = [
        (
            next_version,
            "it was built for the extension ABI 4, and this OScope speaks 3",
        ),
        (
            masked,
            "node `test.mask`: in ABI 3, every port is an image, and none is optional",
        ),
        (nothing, "its entry gave no table"),
        (unchecked, "node `test.unchecked`: its parameters' schema: "),
    ];
    for (entry, expected) in cases {
        let error = Library::from_entry(entry).unwrap_err();
        assert!(error.starts_with(expected), "{error}");
    }
}

/// A package of a Rust extension providing `nodes`, whose library for this
/// platform is `library`.
fn package(id: &str, nodes: &str, library: &[u8]) -> Vec<u8> {
    let platform = PLATFORM.expect("OScope's platforms");
    let manifest = format!(
        "id = \"{id}\"\nname = \"Native\"\nversion = \"1.0.0\"\nlicense = \"MIT\"\n\
         publisher = \"Example Lab\"\noscope-api = \"^1.0\"\nkind = \"rust\"\n\n\
         [provides]\nnodes = [{nodes}]\n\n[rust.libraries]\n{platform} = \"lib/native\"\n"
    );
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default();
    zip.start_file("manifest.toml", options).unwrap();
    zip.write_all(manifest.as_bytes()).unwrap();
    zip.start_file("lib/native", options).unwrap();
    zip.write_all(library).unwrap();
    zip.finish().unwrap().into_inner()
}

#[test]
fn a_library_that_does_not_load_says_why_and_stock_nodes_are_taken() {
    let folder = tempfile::tempdir().unwrap();
    let root = folder.path().join("extensions");
    let extensions = Extensions::open(&root).unwrap();
    let cases = [
        (
            "org.example.broken",
            "\"broken.node\"",
            &b"not a library"[..],
        ),
        (
            "org.example.stock",
            "\"filter.gaussian\"",
            &b"never loaded"[..],
        ),
    ];
    for (id, nodes, library) in cases {
        let path = folder.path().join(format!("{id}.omext"));
        fs::write(&path, package(id, nodes, library)).unwrap();
        extensions.install(&path).unwrap();
    }
    let stock = extensions.get("org.example.stock").unwrap();
    let said = "OScope's stock nodes have `filter.gaussian` already";
    assert_eq!((stock.next, stock.problem.as_deref()), (None, Some(said)));

    // Started again, the library doesn't load: not loaded, and why, at once
    // and for the next start too.
    let extensions = Extensions::open(&root).unwrap();
    assert!(extensions.loaded().is_empty() && extensions.nodes().is_empty());
    assert!(!extensions.registry().contains_key(&Contribution::Nodes));
    let broken = extensions.get("org.example.broken").unwrap();
    assert_eq!((&broken.loaded, &broken.next), (&None, &None));
    let problem = broken.problem.unwrap();
    assert!(problem.starts_with("the library "), "{problem}");
    assert!(problem.contains("native"), "{problem}");
    assert!(!extensions.restart_needed());
}
