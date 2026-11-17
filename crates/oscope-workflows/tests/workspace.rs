//! Analyses in the Workspace (P48): the cell-counting recipe on a picture
//! of nuclei and on its camera's raw mosaic, whole and in a region; its
//! outputs; its provenance complete, with the software, the nodes' and an
//! extension's versions, and a hash of what it read; made again, the same;
//! made again with a parameter changed, and the difference; a grey picture;
//! a run tile by tile; refusals and a cancel.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use oscope_compute::{Compute, ComputeConfig};
use oscope_core::{BayerColour, BayerPattern, PixelFormat, ProjectId, ResultId};
use oscope_pipeline::{
    Capabilities, Descriptor, Engine, Inputs, NodeError, PortType, RunOptions, StepCtx,
    Value as Output, Values, node, outputs, stock,
};
use oscope_store::{
    ImageInfo, ImageReader, Input, NewProject, PixelSize, Provenance, ResultRecord, SampleType,
    Samples, Store, StoreError,
};
use oscope_workflows::workspace::{Analysis, Origins, RUN, Roi, again, analyse, difference};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};

struct Made {
    store: Store,
    project: ProjectId,
    slide: ResultId,
    raw: Input,
    _folder: tempfile::TempDir,
}

/// An engine of the stock nodes, its memo off, so each run computes.
fn engine() -> Engine {
    let compute = Compute::new(&ComputeConfig::default()).unwrap();
    Engine::new(stock::registry("references"), Arc::new(compute), 0)
}

fn provenance() -> Provenance {
    Provenance {
        recipe: Value::Null,
        nodes: Vec::new(),
        inputs: Vec::new(),
        hashes: Default::default(),
        software: "a test".to_owned(),
    }
}

/// Keeps `pixels` as a result of `project`.
fn keep(
    store: &Store,
    project: ProjectId,
    name: &str,
    info: &ImageInfo,
    pixels: Samples,
) -> ResultId {
    let draft = store
        .result(project, name, provenance(), Some(info))
        .unwrap();
    let size = (info.width, info.height);
    draft
        .image()
        .unwrap()
        .write(0, (0, 0), size, &pixels)
        .unwrap();
    draft.commit().unwrap().id
}

/// Whether pixel `(x, y)` of the slide is a nucleus's: discs of 3 µm,
/// 20 µm apart.
fn nucleus(x: u64, y: u64) -> bool {
    let (dx, dy) = ((x % 40) as f32 - 19.5, (y % 40) as f32 - 19.5);
    dx * dx + dy * dy <= 36.0
}

/// Pixel `(x, y)`'s colour: hematoxylin's purple, or eosin's pink.
fn colour(x: u64, y: u64) -> [u8; 3] {
    match nucleus(x, y) {
        true => [90, 77, 153],
        false => [217, 153, 191],
    }
}

/// A slide of 160 × 120 pixels of 0.5 µm, twelve nuclei on it; and an
/// acquisition of the same slide, raw, from a camera of half its pixels'
/// size, each 2 × 2 cell of its mosaic one of the slide's pixels.
fn made() -> Made {
    let folder = tempfile::tempdir().unwrap();
    let data = folder.path().join("data");
    let store = Store::open(&data, &data.join("index.sqlite")).unwrap();
    let new = NewProject {
        name: "nuclei".to_owned(),
        ..NewProject::default()
    };
    let project = store.create(new).unwrap().id;
    let mut info = ImageInfo::new(160, 120, 3, SampleType::U8);
    info.pixel_size = Some(PixelSize {
        x: 0.5,
        y: 0.5,
        z: 1.0,
    });
    let pixels: Vec<u8> = (0..120)
        .flat_map(|y| (0..160).flat_map(move |x| colour(x, y)))
        .collect();
    let slide = keep(&store, project, "a slide", &info, Samples::U8(pixels));
    let pattern = BayerPattern::Grbg;
    let mut info = ImageInfo::new(320, 240, 1, SampleType::U16);
    info.channel_names = vec!["raw".to_owned()];
    info.pixel_size = Some(PixelSize {
        x: 0.25,
        y: 0.25,
        z: 1.0,
    });
    let mosaic: Vec<f32> = (0..240)
        .flat_map(|y| (0..320).map(move |x| (x, y)))
        .map(|(x, y)| {
            let rgb = colour(x / 2, y / 2);
            let channel = match pattern.colour_at(x as u32, y as u32) {
                BayerColour::Red => 0,
                BayerColour::Green => 1,
                BayerColour::Blue => 2,
            };
            f32::from(rgb[channel]) / 255.0
        })
        .collect();
    let mut draft = store.acquisition(project, "raw", &info).unwrap();
    let samples = Samples::from_unit(SampleType::U16, &mosaic);
    draft
        .image()
        .unwrap()
        .write(0, (0, 0), (320, 240), &samples)
        .unwrap();
    let format = PixelFormat::Bayer16 {
        pattern,
        significant_bits: 16,
    };
    draft.record().camera = json!({ "format": format });
    let raw = Input::Acquisition(draft.commit().unwrap().id);
    Made {
        store,
        project,
        slide,
        raw,
        _folder: folder,
    }
}

/// The cell-counting recipe: each stain's amount, the hematoxylin's
/// thresholded at Otsu's level, the nuclei split and measured; and the
/// eosin's amount, a second image.
fn counting() -> Value {
    json!({
        "format": 1,
        "name": "nuclei",
        "inputs": [{"name": "image", "type": "image"}],
        "nodes": [
            {"id": "stains", "node": "stain.deconvolve", "params": {"stains": "he"}},
            {"id": "hematoxylin", "node": "convert.format", "params": {"channels": "red"}},
            {"id": "eosin", "node": "convert.format", "params": {"channels": "green"}},
            {"id": "nuclei", "node": "threshold.otsu", "params": {}},
            {"id": "split", "node": "cells.split", "params": {}},
            {"id": "measure", "node": "cells.measure", "params": {"pixel_um": [0.5, 0.5]}},
        ],
        "edges": [
            {"from": "input.image", "to": "stains.image"},
            {"from": "stains.amounts", "to": "hematoxylin.image"},
            {"from": "stains.amounts", "to": "eosin.image"},
            {"from": "hematoxylin.image", "to": "nuclei.image"},
            {"from": "nuclei.mask", "to": "split.mask"},
            {"from": "split.labels", "to": "measure.labels"},
        ],
        "outputs": [
            {"name": "hematoxylin", "from": "hematoxylin.image"},
            {"name": "objects", "from": "measure.objects"},
            {"name": "outlines", "from": "measure.outlines"},
            {"name": "nuclei", "from": "nuclei.mask"},
            {"name": "level", "from": "nuclei.level"},
            {"name": "eosin", "from": "eosin.image"},
        ],
    })
}

fn analysis(recipe: Value, input: Input, roi: Option<Roi>) -> Analysis {
    Analysis {
        recipe: serde_json::from_value(recipe).unwrap(),
        input,
        roi,
        tile: None,
        name: None,
    }
}

fn file(m: &Made, result: &ResultRecord, name: &str) -> Vec<u8> {
    let folder = m.store.folder(m.project).join("results");
    std::fs::read(folder.join(result.id.to_string()).join(name)).unwrap()
}

fn rows(m: &Made, result: &ResultRecord) -> usize {
    let csv = String::from_utf8(file(m, result, "objects.csv")).unwrap();
    csv.split("\r\n").filter(|l| !l.is_empty()).count() - 1
}

fn summary(m: &Made, result: &ResultRecord) -> Value {
    serde_json::from_slice(&file(m, result, "summary.json")).unwrap()
}

#[test]
fn a_recipe_counts_nuclei_and_its_provenance_is_complete() {
    let m = made();
    let origins = Origins::from([(
        "cells.measure".to_owned(),
        "org.example.cells@1.2.0".to_owned(),
    )]);
    let wanted = analysis(counting(), Input::Result(m.slide), None);
    let options = RunOptions::default();
    let result = analyse(&m.store, m.project, &engine(), &origins, &wanted, &options).unwrap();
    assert_eq!(result.name, "nuclei");
    assert_eq!(rows(&m, &result), 12);
    let outlines: Value = serde_json::from_slice(&file(&m, &result, "outlines.geojson")).unwrap();
    assert_eq!(outlines["features"].as_array().unwrap().len(), 12);
    let level: f64 = serde_json::from_slice(&file(&m, &result, "level.json")).unwrap();
    assert!(level > 0.0 && level < 1.0, "{level}");
    // The mask: white where the nuclei are, and only there.
    let png = file(&m, &result, "nuclei.png");
    let mut reader = png::Decoder::new(std::io::Cursor::new(png))
        .read_info()
        .unwrap();
    let mut rgb = vec![0; reader.output_buffer_size().unwrap()];
    reader.next_frame(&mut rgb).unwrap();
    let white: Vec<bool> = rgb.iter().step_by(3).map(|&r| r == 255).collect();
    let truth: Vec<bool> = (0..120)
        .flat_map(|y| (0..160).map(move |x| nucleus(x, y)))
        .collect();
    assert_eq!(white, truth);
    // The hematoxylin's amounts are the result's own image, of the slide's
    // pixels; the eosin's a store of its own.
    let image = result.image.as_ref().unwrap();
    assert_eq!((image.width, image.height, image.channels), (160, 120, 1));
    assert_eq!(image.pixel_size.map(|p| p.x), Some(0.5));
    let folder = m
        .store
        .folder(m.project)
        .join(format!("results/{}", result.id));
    let eosin = ImageReader::open(&folder.join("eosin.ome.zarr")).unwrap();
    assert_eq!((eosin.info().width, eosin.info().channels), (160, 1));
    let summary = summary(&m, &result);
    assert_eq!(summary["pixels"], json!([0, 0, 160, 120]));
    assert_eq!(summary["rows"], json!({"objects": 12}));
    assert_eq!(
        (
            &summary["outputs"]["hematoxylin"],
            &summary["outputs"]["eosin"]
        ),
        (&json!("image.ome.zarr"), &json!("eosin.ome.zarr"))
    );
    // Its provenance: the analysis; each node once, with its version and
    // its extension; what it read, hashed; and the software.
    let p = &result.provenance;
    assert_eq!(p.recipe[RUN], serde_json::to_value(&wanted).unwrap());
    let nodes: Vec<(&str, &str, Option<&str>)> = p
        .nodes
        .iter()
        .map(|n| (n.node.as_str(), n.version.as_str(), n.extension.as_deref()))
        .collect();
    assert_eq!(
        nodes,
        [
            ("cells.measure", "1", Some("org.example.cells@1.2.0")),
            ("cells.split", "1", None),
            ("convert.format", "1", None),
            ("stain.deconvolve", "1", None),
            ("threshold.otsu", "1", None),
        ]
    );
    assert_eq!(p.inputs, [Input::Result(m.slide)]);
    assert!(p.software.starts_with("OScope "), "{}", p.software);
    let path = folder
        .with_file_name(m.slide.to_string())
        .join("image.ome.zarr");
    let read = ImageReader::open(&path)
        .unwrap()
        .read(0, 0, (0, 0), (160, 120))
        .unwrap()
        .to_unit();
    let bytes: Vec<u8> = read.iter().flat_map(|v| v.to_le_bytes()).collect();
    let hex: String = Sha256::digest(&bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    assert_eq!(p.hashes[&m.slide.to_string()], format!("sha256:{hex}"));
}

#[test]
fn a_result_made_again_is_the_same_and_a_change_is_its_difference() {
    let m = made();
    let options = RunOptions::default();
    let first = analyse(
        &m.store,
        m.project,
        &engine(),
        &Origins::new(),
        &analysis(counting(), Input::Result(m.slide), None),
        &options,
    )
    .unwrap();
    // The same again, on an engine of its own: the same files, and the same
    // provenance.
    let same = again(&m.store, m.project, first.id, &BTreeMap::new()).unwrap();
    let second = analyse(
        &m.store,
        m.project,
        &engine(),
        &Origins::new(),
        &same,
        &options,
    )
    .unwrap();
    for name in [
        "objects.csv",
        "outlines.geojson",
        "nuclei.png",
        "level.json",
        "summary.json",
    ] {
        assert_eq!(file(&m, &first, name), file(&m, &second, name), "{name}");
    }
    assert!(difference(&first.provenance, &second.provenance).is_empty());
    // Nuclei of 1000 µm² at least: none, and that is the difference.
    let changes = BTreeMap::from([(
        "measure".to_owned(),
        Map::from_iter([("min_area".to_owned(), json!(1000))]),
    )]);
    let fewer = again(&m.store, m.project, first.id, &changes).unwrap();
    let third = analyse(
        &m.store,
        m.project,
        &engine(),
        &Origins::new(),
        &fewer,
        &options,
    )
    .unwrap();
    assert_eq!(rows(&m, &third), 0);
    let changed = difference(&first.provenance, &third.provenance);
    let changed: Vec<(&str, &Value, &Value)> = changed
        .iter()
        .map(|c| (c.path.as_str(), &c.before, &c.after))
        .collect();
    assert_eq!(
        changed,
        [(
            "/recipe/workspace.run/recipe/nodes/measure/params/min_area",
            &Value::Null,
            &json!(1000)
        )]
    );
    // And back to its default.
    let back = BTreeMap::from([(
        "measure".to_owned(),
        Map::from_iter([("min_area".to_owned(), Value::Null)]),
    )]);
    let restored = again(&m.store, m.project, third.id, &back).unwrap();
    assert_eq!(restored, same);
    // A key with `/` or `~` in it, escaped as RFC 6901 has it.
    let mut odd = first.provenance.clone();
    odd.recipe = json!({"a/b": {"c~d": 1}});
    let changed = difference(&first.provenance, &odd);
    assert!(
        changed
            .iter()
            .any(|c| c.path == "/recipe/a~1b/c~0d" && c.after == 1),
        "{changed:?}"
    );
}

#[test]
fn a_region_a_raw_mosaic_a_grey_picture_and_tiles() {
    let m = made();
    let options = RunOptions::default();
    // The top-left 40 × 20 µm, its edges between pixels: two nuclei.
    let roi = Roi {
        x: 0.2,
        y: 0.3,
        width: 39.7,
        height: 19.6,
    };
    let count = |input: Input, roi: Option<Roi>| {
        let wanted = analysis(counting(), input, roi);
        analyse(
            &m.store,
            m.project,
            &engine(),
            &Origins::new(),
            &wanted,
            &options,
        )
        .unwrap()
    };
    let region = count(Input::Result(m.slide), Some(roi));
    assert_eq!(rows(&m, &region), 2);
    assert_eq!(summary(&m, &region)["pixels"], json!([0, 0, 80, 40]));
    // The raw mosaic: its 2 × 2 cells, the slide's pixels.
    let raw = count(m.raw, None);
    assert_eq!(rows(&m, &raw), 12);
    let cells = raw.image.as_ref().unwrap();
    assert_eq!(
        (cells.width, cells.pixel_size.map(|p| p.x)),
        (160, Some(0.5))
    );
    let raw_region = count(m.raw, Some(roi));
    assert_eq!(rows(&m, &raw_region), 2);
    assert_eq!(summary(&m, &raw_region)["pixels"], json!([0, 0, 80, 40]));
    // A grey picture: one channel, thresholded.
    let info = ImageInfo::new(64, 48, 1, SampleType::U16);
    let pixels: Vec<u16> = (0..64 * 48)
        .map(|i| if i % 64 < 32 { 1000 } else { 30000 })
        .collect();
    let grey = keep(&m.store, m.project, "grey", &info, Samples::U16(pixels));
    let recipe = json!({
        "format": 1,
        "name": "bright",
        "inputs": [{"name": "image", "type": "image"}],
        "nodes": [{"id": "otsu", "node": "threshold.otsu", "params": {}}],
        "edges": [{"from": "input.image", "to": "otsu.image"}],
        "outputs": [{"name": "mask", "from": "otsu.mask"}],
    });
    let bright = analyse(
        &m.store,
        m.project,
        &engine(),
        &Origins::new(),
        &analysis(recipe, Input::Result(grey), None),
        &options,
    )
    .unwrap();
    assert!(file(&m, &bright, "mask.png").starts_with(b"\x89PNG"));
    // The stains, tile by tile, as they are whole.
    let mut stains = counting();
    stains["nodes"] = json!([
        {"id": "stains", "node": "stain.deconvolve", "params": {"stains": "he"}},
        {"id": "hematoxylin", "node": "convert.format", "params": {"channels": "red"}},
    ]);
    stains["edges"] = json!([
        {"from": "input.image", "to": "stains.image"},
        {"from": "stains.amounts", "to": "hematoxylin.image"},
    ]);
    stains["outputs"] = json!([{"name": "hematoxylin", "from": "hematoxylin.image"}]);
    let read = |result: &ResultRecord| {
        let path = m
            .store
            .folder(m.project)
            .join(format!("results/{}/image.ome.zarr", result.id));
        let reader = ImageReader::open(&path).unwrap();
        reader.read(0, 0, (0, 0), (160, 120)).unwrap().to_unit()
    };
    let whole = analysis(stains, Input::Result(m.slide), None);
    let tiled = Analysis {
        tile: Some(32),
        ..whole.clone()
    };
    let on = engine();
    let (a, b) = (
        analyse(&m.store, m.project, &on, &Origins::new(), &whole, &options).unwrap(),
        analyse(&m.store, m.project, &on, &Origins::new(), &tiled, &options).unwrap(),
    );
    assert_eq!(read(&a), read(&b));
}

#[test]
fn what_cannot_be_run_is_refused_and_a_cancel_keeps_nothing() {
    let m = made();
    let (on, origins, options) = (engine(), Origins::new(), RunOptions::default());
    let refused = |wanted: &Analysis, why: &str| {
        let made = analyse(&m.store, m.project, &on, &origins, wanted, &options);
        assert!(
            matches!(&made, Err(StoreError::Refused(w)) if w.contains(why)),
            "{why}: {made:?}"
        );
    };
    let slide = Input::Result(m.slide);
    // A recipe that takes no `image`; an output named as the result's own
    // files are; a recipe that can't run tile by tile, so run; regions
    // outside the image, empty, and not a number.
    let mut blind = counting();
    blind["inputs"][0]["name"] = json!("picture");
    blind["edges"][0]["from"] = json!("input.picture");
    refused(&analysis(blind, slide, None), "takes one input");
    let mut clash = counting();
    clash["outputs"][4]["name"] = json!("summary");
    refused(&analysis(clash, slide, None), "can't be named summary");
    let tiled = Analysis {
        tile: Some(32),
        ..analysis(counting(), slide, None)
    };
    refused(&tiled, "tile");
    for (x, width, why) in [
        (100.0, 10.0, "outside"),
        (0.0, 0.0, "wider"),
        (f64::NAN, 10.0, "finite"),
    ] {
        let roi = Roi {
            x,
            y: 0.0,
            width,
            height: 10.0,
        };
        refused(&analysis(counting(), slide, Some(roi)), why);
    }
    // A stack, which a result doesn't keep, from a node of its own.
    let mut registry = stock::registry("references");
    let descriptor = Descriptor::new("test.stack", 1, "test", "An image as a stack of one")
        .input("image", PortType::Image)
        .output("stack", PortType::Stack)
        .capabilities(Capabilities {
            cpu: true,
            ..Capabilities::default()
        });
    let stacked = node(descriptor, |_: Value| {
        |_: &StepCtx<'_>, inputs: &Inputs<'_>| -> Result<Values, NodeError> {
            let image = inputs.image("image")?.clone();
            Ok(outputs([("stack", Output::stack(vec![image]))]))
        }
    });
    registry.register(stacked).unwrap();
    let compute = Arc::new(Compute::new(&ComputeConfig::default()).unwrap());
    let stacks = Engine::new(registry, compute, 0);
    let recipe = json!({
        "format": 1,
        "inputs": [{"name": "image", "type": "image"}],
        "nodes": [{"id": "stacked", "node": "test.stack"}],
        "edges": [{"from": "input.image", "to": "stacked.image"}],
        "outputs": [{"name": "stack", "from": "stacked.stack"}],
    });
    let wanted = analysis(recipe, slide, None);
    let made = analyse(&m.store, m.project, &stacks, &origins, &wanted, &options);
    assert!(
        matches!(&made, Err(StoreError::Refused(w)) if w.contains("a result keeps")),
        "{made:?}"
    );
    // An image over 25 megapixels, whole: it needs a region.
    let info = ImageInfo::new(5001, 5000, 1, SampleType::U8);
    let draft = m
        .store
        .result(m.project, "large", provenance(), Some(&info));
    let large = draft.unwrap().commit().unwrap().id;
    refused(
        &analysis(counting(), Input::Result(large), None),
        "give it a region",
    );
    // A result that isn't; one not made here; a node that isn't there.
    let missing = analyse(
        &m.store,
        m.project,
        &on,
        &origins,
        &analysis(counting(), Input::Result(ResultId::generate()), None),
        &options,
    );
    assert!(missing.is_err());
    let made = analyse(
        &m.store,
        m.project,
        &on,
        &origins,
        &analysis(counting(), slide, None),
        &options,
    )
    .unwrap();
    let not_here = again(&m.store, m.project, m.slide, &BTreeMap::new());
    assert!(
        matches!(&not_here, Err(StoreError::Refused(w)) if w.contains("wasn't made in the Workspace")),
        "{not_here:?}"
    );
    let nowhere = BTreeMap::from([("blur".to_owned(), Map::new())]);
    let unknown = again(&m.store, m.project, made.id, &nowhere);
    assert!(
        matches!(unknown, Err(StoreError::Refused(_))),
        "{unknown:?}"
    );
    // Cancelled before it starts: nothing kept.
    let before = m.store.contents(m.project).unwrap().results.len();
    let stopped = RunOptions::default();
    stopped.cancel.store(true, Ordering::Release);
    let cancelled = analyse(
        &m.store,
        m.project,
        &on,
        &origins,
        &analysis(counting(), slide, None),
        &stopped,
    );
    assert!(
        matches!(cancelled, Err(StoreError::Cancelled)),
        "{cancelled:?}"
    );
    assert_eq!(m.store.contents(m.project).unwrap().results.len(), before);
}
