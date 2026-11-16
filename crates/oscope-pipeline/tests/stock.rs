//! The stock nodes: each against P08–P08c's function where it runs one,
//! and against what it should give where it is new.

use std::path::Path;
use std::sync::Arc;

use oscope_compute::{Compute, ComputeConfig, Image, Shape, cpu};
use oscope_core::{BayerPattern, PixelRect};
use oscope_imaging::color::{self, ColourCorrection, Curve, Demosaic, Display};
use oscope_pipeline::{Engine, Plan, Recipe, RunOptions, Value, Values, stock};
use serde_json::{Value as Json, json};

fn engine(references: &Path) -> Engine {
    let config = ComputeConfig {
        live_threads: Some(2),
        batch_threads: Some(2),
        cuda_device: None,
    };
    let compute = Arc::new(Compute::new(&config).expect("the compute layer starts"));
    Engine::new(stock::registry(references), compute, 0)
}

/// A recipe of node `id` alone, with `params`, fed `inputs` by its ports'
/// names and giving all its outputs.
fn single(engine: &Engine, id: &str, params: Json, inputs: &Values) -> Recipe {
    let descriptor = engine
        .registry()
        .get(id)
        .expect("a stock node")
        .descriptor();
    let kind = |value: &Value| serde_json::to_value(value.port_type()).unwrap();
    let recipe = json!({
        "format": 1,
        "inputs": inputs.iter().map(|(name, value)| json!({ "name": name, "type": kind(value) })).collect::<Vec<_>>(),
        "nodes": [{ "id": "n", "node": id, "params": params }],
        "edges": inputs.keys().map(|name| json!({ "from": format!("input.{name}"), "to": format!("n.{name}") })).collect::<Vec<_>>(),
        "outputs": descriptor.outputs.iter().map(|port| json!({ "name": port.name, "from": format!("n.{}", port.name) })).collect::<Vec<_>>(),
    });
    serde_json::from_value(recipe).expect("a recipe")
}

/// Runs node `id` alone, on the CPU.
fn run(engine: &Engine, id: &str, params: Json, inputs: Values) -> Values {
    let plan = Plan::new(
        engine.registry(),
        &single(engine, id, params, &inputs),
        false,
    )
    .expect("a plan");
    let options = RunOptions {
        memoise: false,
        ..RunOptions::default()
    };
    engine.run(&plan, &inputs, &options).expect("a run").outputs
}

fn one(image: Image) -> Values {
    Values::from([("image".to_owned(), Value::image(image))])
}

fn image(values: &Values, name: &str) -> Image {
    values[name].as_image().expect("an image").clone()
}

/// The test chart, as P08 measures demosaicing with: 3 × 61 × 41 is an odd
/// size, which the mosaics' edges must take.
fn chart() -> Image {
    color::chart(61, 41)
}

fn picture(width: u32, height: u32, channels: usize) -> Image {
    let shape = Shape::new(width, height, channels);
    let data = (0..shape.len()).map(|i| ((i * 7919) % 1000) as f32 / 1000.0);
    Image::from_vec(shape, data.collect()).unwrap()
}

fn worst(a: &Image, b: &Image) -> f32 {
    assert_eq!(a.shape(), b.shape());
    a.data()
        .iter()
        .zip(b.data())
        .map(|(x, y)| (x - y).abs())
        .fold(0.0, f32::max)
}

#[test]
fn correction_nodes_give_what_p08_s_functions_do() {
    let folder = tempfile::tempdir().unwrap();
    let engine = engine(folder.path());
    let rgb = chart();
    for pattern in [
        BayerPattern::Rggb,
        BayerPattern::Grbg,
        BayerPattern::Gbrg,
        BayerPattern::Bggr,
    ] {
        let mosaic = color::mosaic(&rgb, pattern).unwrap();
        for (name, method) in [
            ("superpixel", Demosaic::Superpixel),
            ("bilinear", Demosaic::Bilinear),
            ("malvar_he_cutler", Demosaic::MalvarHeCutler),
        ] {
            let params = json!({ "pattern": pattern, "method": name });
            let node = image(
                &run(&engine, "correction.demosaic", params, one(mosaic.clone())),
                "image",
            );
            assert_eq!(
                node,
                color::demosaic(&mosaic, pattern, method).unwrap(),
                "{pattern:?}, {name}"
            );
        }
    }
    let mut black = rgb.clone();
    color::subtract_black(&mut black, 0.1).unwrap();
    let node = run(
        &engine,
        "correction.black_level",
        json!({ "black": 0.1 }),
        one(rgb.clone()),
    );
    assert_eq!(image(&node, "image"), black);
    let gains = [1.5, 1.0, 0.8];
    let mut balanced = rgb.clone();
    let correction = ColourCorrection {
        gains,
        ..ColourCorrection::default()
    };
    color::correct(&mut balanced, &correction).unwrap();
    let node = run(
        &engine,
        "correction.white_balance",
        json!({ "gains": gains }),
        one(rgb.clone()),
    );
    assert_eq!(image(&node, "image"), balanced);
    let matrix = [[1.2, -0.1, -0.1], [-0.2, 1.3, -0.1], [0.0, -0.3, 1.3]];
    let mut mixed = rgb.clone();
    let correction = ColourCorrection {
        matrix,
        ..ColourCorrection::default()
    };
    color::correct(&mut mixed, &correction).unwrap();
    let node = run(
        &engine,
        "correction.colour_matrix",
        json!({ "matrix": matrix }),
        one(rgb.clone()),
    );
    assert_eq!(image(&node, "image"), mixed);
}

#[test]
fn display_and_geometry_nodes_give_what_p08b_s_functions_do() {
    let folder = tempfile::tempdir().unwrap();
    let engine = engine(folder.path());
    let rgb = chart();
    // The levels are `to_display` before its 8 bits.
    for (params, curve) in [
        (json!({ "black": 0.05, "white": 0.9 }), Curve::Srgb),
        (
            json!({ "black": 0.0, "white": 0.5, "gamma": 2.2 }),
            Curve::Gamma { gamma: 2.2 },
        ),
    ] {
        let display = Display {
            black: params["black"].as_f64().unwrap() as f32,
            white: params["white"].as_f64().unwrap() as f32,
            curve,
        };
        let eight = color::to_display(&rgb, &display).unwrap();
        let node = image(
            &run(&engine, "display.levels", params, one(rgb.clone())),
            "image",
        );
        let node: Vec<u8> = node
            .data()
            .iter()
            .map(|v| (v * 255.0).round() as u8)
            .collect();
        assert_eq!(node, eight);
    }
    let big = picture(1000, 700, 3);
    let node = run(
        &engine,
        "resize.bin",
        json!({ "factor": 4 }),
        one(big.clone()),
    );
    assert_eq!(image(&node, "image"), cpu::downsample(&big, 4).unwrap());
    // Resizing by a whole factor averages the same blocks.
    let node = run(
        &engine,
        "resize.resize",
        json!({ "width": 250, "height": 175 }),
        one(big.clone()),
    );
    assert!(worst(&image(&node, "image"), &cpu::downsample(&big, 4).unwrap()) < 1e-5);
    let rect = PixelRect::new(100, 50, 300, 200);
    let node = run(
        &engine,
        "geometry.crop",
        serde_json::to_value(rect).unwrap(),
        one(big.clone()),
    );
    assert_eq!(image(&node, "image"), color::crop(&big, rect).unwrap());
}

#[test]
fn new_nodes_give_what_they_should() {
    let folder = tempfile::tempdir().unwrap();
    let engine = engine(folder.path());
    let row = |values: &[f32]| {
        Image::from_vec(Shape::new(values.len() as u32, 1, 1), values.to_vec()).unwrap()
    };
    // Enlarging interpolates between pixel centres.
    let node = run(
        &engine,
        "resize.resize",
        json!({ "width": 4, "height": 1 }),
        one(row(&[0.0, 1.0])),
    );
    assert_eq!(image(&node, "image").data(), [0.0, 0.25, 0.75, 1.0]);
    // A lookup table, and an inverting one.
    let ramp = row(&[0.0, 0.25, 0.5, 1.0, 1.5]);
    let node = run(
        &engine,
        "display.lut",
        json!({ "table": [1.0, 0.0] }),
        one(ramp.clone()),
    );
    assert_eq!(image(&node, "image").data(), [1.0, 0.75, 0.5, 0.0, 0.0]);
    let node = run(
        &engine,
        "display.lut",
        json!({ "table": [0.0, 1.0, 0.0] }),
        one(ramp.clone()),
    );
    assert_eq!(image(&node, "image").data(), [0.0, 0.5, 1.0, 0.0, 0.0]);
    // False colour: the map's stops, and grey for the grey map.
    let stops = row(&[0.0, 0.35, 0.7, 1.0]);
    let node = run(
        &engine,
        "display.false_colour",
        json!({ "map": "fire" }),
        one(stops.clone()),
    );
    let fire = [0.0, 0.0, 0.0, 0.8, 0.0, 0.0, 1.0, 0.8, 0.0, 1.0, 1.0, 1.0];
    assert!(
        image(&node, "image")
            .data()
            .iter()
            .zip(fire)
            .all(|(a, b)| (a - b).abs() < 1e-6)
    );
    let node = run(
        &engine,
        "display.false_colour",
        json!({ "map": "grey", "black": 0.5 }),
        one(stops),
    );
    let grey = [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.4, 0.4, 0.4, 1.0, 1.0, 1.0];
    assert!(
        image(&node, "image")
            .data()
            .iter()
            .zip(grey)
            .all(|(a, b)| (a - b).abs() < 1e-6)
    );
    // Formats: Rec. 709's luminance, grey to RGB, a channel, a bit depth.
    let rgb = Image::from_vec(Shape::new(1, 1, 3), vec![0.2, 0.6, 1.0]).unwrap();
    let grey = image(
        &run(
            &engine,
            "convert.format",
            json!({ "channels": "grey" }),
            one(rgb.clone()),
        ),
        "image",
    );
    assert!((grey.data()[0] - (0.2126 * 0.2 + 0.7152 * 0.6 + 0.0722)).abs() < 1e-6);
    let back = image(
        &run(
            &engine,
            "convert.format",
            json!({ "channels": "rgb" }),
            one(grey.clone()),
        ),
        "image",
    );
    assert_eq!(back.data(), [grey.data()[0]; 3]);
    let green = run(
        &engine,
        "convert.format",
        json!({ "channels": "green", "bits": 2 }),
        one(rgb),
    );
    assert_eq!(image(&green, "image").data(), [2.0 / 3.0]);
    // A histogram: half the pixels at 0.25, half at 0.75.
    let halves = row(&[0.25, 0.25, 0.75, 0.75]);
    let measured = run(
        &engine,
        "measure.histogram",
        json!({ "bins": 4 }),
        one(halves),
    );
    let histogram = measured["histogram"].as_table().unwrap();
    assert_eq!(histogram.columns, ["from", "grey"]);
    let counts: Vec<&Json> = histogram.rows.iter().map(|row| &row[1]).collect();
    assert_eq!(counts, [&json!(0), &json!(2), &json!(0), &json!(2)]);
    let statistics = &measured["statistics"].as_table().unwrap().rows[0];
    assert_eq!(
        statistics[0..5],
        [
            json!("grey"),
            json!(0.5),
            json!(0.25),
            json!(0.25),
            json!(0.75)
        ]
    );
    assert_eq!(statistics[5..8], [json!(0.375), json!(0.375), json!(0.875)]);
}

#[test]
fn dark_frames_and_flat_fields_correct_the_part_of_the_frame_shown() {
    let folder = tempfile::tempdir().unwrap();
    let engine = engine(folder.path());
    let (width, height) = (400, 300);
    // Light falling off from the centre, as uneven illumination does.
    let falloff = |x: f32, y: f32| {
        let (dx, dy) = (x / width as f32 - 0.5, y / height as f32 - 0.5);
        1.0 - 0.8 * (dx * dx + dy * dy)
    };
    let field = |w: u32, h: u32, f: &dyn Fn(f32, f32) -> f32| {
        let (sx, sy) = (width as f32 / w as f32, height as f32 / h as f32);
        let data = (0..w * h).flat_map(|i| {
            [f(
                (i % w) as f32 * sx + sx / 2.0,
                (i / w) as f32 * sy + sy / 2.0,
            ); 3]
        });
        Image::from_vec(Shape::new(w, h, 3), data.collect()).unwrap()
    };
    let scene = |x: f32, y: f32| 0.2 + 0.6 * ((x * 0.05).sin() * (y * 0.07).cos()).abs();
    let seen = field(width, height, &|x, y| scene(x, y) * falloff(x, y));
    let flat = field(width, height, &falloff);
    let inputs = Values::from([
        ("image".to_owned(), Value::image(seen.clone())),
        ("flat".to_owned(), Value::image(flat.clone())),
    ]);
    let corrected = image(
        &run(&engine, "correction.flat_field", json!({}), inputs),
        "image",
    );
    let mean = flat.data().iter().map(|&v| f64::from(v)).sum::<f64>() / flat.data().len() as f64;
    let even = field(width, height, &|x, y| scene(x, y) * mean as f32);
    assert!(worst(&corrected, &even) < 1e-5);
    // A flat at a quarter of the resolution, sampled over the region a
    // zoomed view shows.
    let region = json!({ "x": 200, "y": 0, "width": 200, "height": 150, "frame_width": width, "frame_height": height });
    let shown = field(width, height, &|x, y| scene(x, y) * falloff(x, y));
    let shown = color::crop(&shown, PixelRect::new(200, 0, 200, 150)).unwrap();
    let inputs = Values::from([
        ("image".to_owned(), Value::image(shown)),
        (
            "flat".to_owned(),
            Value::image(field(width / 4, height / 4, &falloff)),
        ),
        ("region".to_owned(), Value::json(region.clone())),
    ]);
    let corrected = image(
        &run(&engine, "correction.flat_field", json!({}), inputs),
        "image",
    );
    let even = color::crop(&even, PixelRect::new(200, 0, 200, 150)).unwrap();
    // The quarter-resolution flat's own mean, and its interpolation.
    assert!(
        worst(&corrected, &even) < 0.01,
        "{}",
        worst(&corrected, &even)
    );
    // One plan, run as a live view zooms from one region to another: each
    // run samples the flat over its own region, as a fresh plan would.
    let flat = Value::image(field(width / 4, height / 4, &falloff));
    let inputs = |x: u32| {
        let image = color::crop(
            &field(width, height, &|x, y| scene(x, y) * falloff(x, y)),
            PixelRect::new(x, 0, 200, 150),
        );
        let region = json!({ "x": x, "y": 0, "width": 200, "height": 150, "frame_width": width, "frame_height": height });
        Values::from([
            ("image".to_owned(), Value::image(image.unwrap())),
            ("flat".to_owned(), flat.clone()),
            ("region".to_owned(), Value::json(region)),
        ])
    };
    let recipe = single(&engine, "correction.flat_field", json!({}), &inputs(0));
    let plan = Plan::new(engine.registry(), &recipe, false).unwrap();
    let options = RunOptions {
        memoise: false,
        ..RunOptions::default()
    };
    for x in [0, 200, 200, 0] {
        let kept = engine.run(&plan, &inputs(x), &options).unwrap();
        let fresh = run(&engine, "correction.flat_field", json!({}), inputs(x));
        assert_eq!(
            image(&kept.outputs, "image"),
            image(&fresh, "image"),
            "the region at x = {x}"
        );
    }
    // A dark frame, taken off: at full size, and sampled at twice the size.
    let dark = picture(width, height, 3)
        .data()
        .iter()
        .map(|v| v * 0.05)
        .collect();
    let dark = Image::from_vec(Shape::new(width, height, 3), dark).unwrap();
    let lit = picture(width, height, 3);
    let inputs = Values::from([
        ("image".to_owned(), Value::image(lit.clone())),
        ("dark".to_owned(), Value::image(dark.clone())),
    ]);
    let taken = image(
        &run(&engine, "correction.dark_frame", json!({}), inputs),
        "image",
    );
    let expected = lit
        .data()
        .iter()
        .zip(dark.data())
        .map(|(v, d)| (v - d).max(0.0));
    assert_eq!(taken.data(), expected.collect::<Vec<_>>());
    let small = cpu::downsample(&lit, 2).unwrap();
    let inputs = Values::from([
        ("image".to_owned(), Value::image(small.clone())),
        ("dark".to_owned(), Value::image(dark.clone())),
    ]);
    let taken = image(
        &run(&engine, "correction.dark_frame", json!({}), inputs),
        "image",
    );
    let binned = cpu::downsample(&dark, 2).unwrap();
    let expected = small
        .data()
        .iter()
        .zip(binned.data())
        .map(|(v, d)| (v - d).max(0.0));
    let expected = Image::from_vec(small.shape(), expected.collect()).unwrap();
    assert!(worst(&taken, &expected) < 1e-6);
}

#[test]
fn references_are_read_from_their_folder_and_keyed_by_content() {
    let folder = tempfile::tempdir().unwrap();
    let engine = engine(folder.path());
    let write = |name: &str, value: f32| {
        let flat = Image::from_vec(Shape::new(8, 6, 3), vec![value; 144]).unwrap();
        let mut file = Vec::new();
        color::encode_tiff16(&mut file, &flat).unwrap();
        std::fs::write(folder.path().join(format!("{name}.tiff")), file).unwrap();
    };
    write("flat", 0.5);
    let recipe = |name: &str| -> Recipe {
        serde_json::from_value(json!({
            "format": 1,
            "nodes": [{ "id": "flat", "node": "io.reference", "params": { "name": name } }],
            "outputs": [{ "name": "flat", "from": "flat.image" }]
        }))
        .unwrap()
    };
    let memo = Engine::new(
        stock::registry(folder.path()),
        Arc::new(Compute::new(&ComputeConfig::default()).unwrap()),
        1 << 20,
    );
    let read = |engine: &Engine| {
        let run = engine
            .run(
                &engine.plan(&recipe("flat")).unwrap(),
                &Values::new(),
                &RunOptions::default(),
            )
            .unwrap();
        (image(&run.outputs, "flat"), run.report.steps[0].cached)
    };
    let (flat, cached) = read(&memo);
    assert_eq!(
        (flat.shape(), flat.data()[0], cached),
        (Shape::new(8, 6, 3), 32768.0 / 65535.0, false)
    );
    assert!(read(&memo).1, "the same file is memoised");
    write("flat", 0.25);
    let (flat, cached) = read(&memo);
    assert_eq!(
        (flat.data()[0], cached),
        (16384.0 / 65535.0, false),
        "a new file isn't the old"
    );
    for (name, problem) in [
        ("../flat", "isn't a reference's name"),
        ("missing", "there is no reference `missing`"),
    ] {
        let refused = engine.plan(&recipe(name)).unwrap_err().to_string();
        assert!(refused.contains(problem), "{refused}");
    }
    let refused = Plan::new(
        engine.registry(),
        &single(
            &engine,
            "display.levels",
            json!({ "black": 0.5, "white": 0.5 }),
            &one(chart()),
        ),
        false,
    );
    assert!(
        refused
            .unwrap_err()
            .to_string()
            .contains("white (0.5) must be above black (0.5)")
    );
}

/// On the owner's machine, with `OSCOPE_CUDA=1`: the nodes with CUDA steps
/// give what their CPU steps do, as P08c's twins do.
#[cfg(feature = "cuda")]
#[test]
fn gpu_steps_match_the_cpu_s() {
    if std::env::var("OSCOPE_CUDA").as_deref() != Ok("1") {
        eprintln!("skipped: set OSCOPE_CUDA=1 to run it on a GPU");
        return;
    }
    use oscope_pipeline::Device;
    let folder = tempfile::tempdir().unwrap();
    let engine = engine(folder.path());
    assert!(engine.has_gpu(), "OSCOPE_CUDA=1, but no GPU");
    let rgb = color::chart(640, 480);
    let mosaic = color::mosaic(&rgb, BayerPattern::Rggb).unwrap();
    let cases = [
        (
            "correction.demosaic",
            json!({ "method": "malvar_he_cutler" }),
            mosaic,
        ),
        (
            "correction.black_level",
            json!({ "black": 0.05 }),
            rgb.clone(),
        ),
        (
            "correction.white_balance",
            json!({ "gains": [1.4, 1.0, 0.9] }),
            rgb.clone(),
        ),
        (
            "correction.colour_matrix",
            json!({ "matrix": [[1.2, -0.1, -0.1], [0.0, 1.0, 0.0], [0.0, -0.2, 1.2]] }),
            rgb.clone(),
        ),
        ("resize.bin", json!({ "factor": 3 }), rgb),
    ];
    let options = RunOptions {
        memoise: false,
        ..RunOptions::default()
    };
    for (id, params, input) in cases {
        // A step alone on the GPU would stay on the CPU: `keep`, binning by
        // 1, is a second GPU step, which copies.
        let recipe: Recipe = serde_json::from_value(json!({
            "format": 1,
            "inputs": [{ "name": "image", "type": "image" }],
            "nodes": [{ "id": "n", "node": id, "params": params }, { "id": "keep", "node": "resize.bin", "params": { "factor": 1 } }],
            "edges": [{ "from": "input.image", "to": "n.image" }, { "from": "n.image", "to": "keep.image" }],
            "outputs": [{ "name": "image", "from": "keep.image" }]
        }))
        .unwrap();
        let ran = |gpu: bool| {
            let plan = Plan::new(engine.registry(), &recipe, gpu).unwrap();
            let device = if gpu { Device::Cuda } else { Device::Cpu };
            assert_eq!(plan.device("n"), Some(device), "{id}");
            image(
                &engine
                    .run(&plan, &one(input.clone()), &options)
                    .unwrap()
                    .outputs,
                "image",
            )
        };
        let (cpu, gpu) = (ran(false), ran(true));
        assert!(worst(&cpu, &gpu) <= 1e-5, "{id}: {}", worst(&cpu, &gpu));
    }
}

#[test]
fn a_dzi_folder_is_the_image_s_tiles_and_never_written_over() {
    use oscope_imaging::zoom::{Dzi, InMemory, tile};
    let folder = tempfile::tempdir().unwrap();
    let engine = engine(folder.path());
    let picture = picture(600, 300, 3);
    let params = json!({
        "folder": folder.path(), "name": "slide", "black": 0.0, "white": 1.0, "gamma": 1.0,
    });
    let written = run(&engine, "export.dzi", params.clone(), one(picture.clone()));
    let written = written["written"].as_json().expect("JSON").clone();
    let dzi = Dzi {
        width: 600,
        height: 300,
    };
    let all: u64 = (0..dzi.levels())
        .map(|l| dzi.tiles(l).0 * dzi.tiles(l).1)
        .sum();
    assert_eq!(
        (&written["name"], &written["tiles"]),
        (&json!("slide"), &json!(all))
    );
    // Its tiles are deep zoom's own, of the image.
    let pyramid = InMemory::new(picture.clone()).unwrap();
    let display = Display {
        black: 0.0,
        white: 1.0,
        curve: Curve::Gamma { gamma: 1.0 },
    };
    let table = color::DisplayTable::new(&display).unwrap();
    let last = dzi.levels() - 1;
    for (level, at) in [(last, (2, 1)), (last - 1, (1, 0)), (3, (0, 0))] {
        let file = folder
            .path()
            .join(format!("slide_files/{level}/{}_{}.jpg", at.0, at.1));
        let want = tile(&pyramid, level, at, &table).unwrap().unwrap();
        assert_eq!(std::fs::read(file).unwrap(), want, "level {level}");
    }
    // Refused: written again over it, a folder not given whole, a name
    // that leaves it, and levels the wrong way round.
    let refused = |params: Json| {
        let inputs = one(picture.clone());
        let plan = Plan::new(
            engine.registry(),
            &single(&engine, "export.dzi", params, &inputs),
            false,
        );
        plan.ok()
            .and_then(|plan| engine.run(&plan, &inputs, &RunOptions::default()).ok())
            .is_none()
    };
    assert!(refused(params.clone()), "written over");
    assert!(refused(json!({"folder": "relative/folder"})));
    assert!(refused(json!({"folder": folder.path(), "name": "../up"})));
    assert!(refused(
        json!({"folder": folder.path(), "name": "other", "black": 0.5, "white": 0.2})
    ));
}

#[test]
fn the_volume_nodes_are_depth_from_focus_step_by_step_and_its_mesh() {
    use oscope_compute::{filters, focus};
    use oscope_volume::{FocusOptions, Planes, all_in_focus, depth_from_focus};
    let folder = tempfile::tempdir().unwrap();
    let engine = engine(folder.path());
    // Five planes of a texture, sharpest at the middle one.
    let sharp = picture(64, 48, 1);
    let stack: Vec<Image> = (0..5)
        .map(|k: i32| filters::gaussian(&sharp, 0.3 + 1.2 * (k - 2).abs() as f32).unwrap())
        .collect();
    let planes = Planes {
        z0_um: 10.0,
        step_um: 0.5,
    };
    let planes_json = Value::json(json!({"z0_um": 10.0, "step_um": 0.5}));
    let options = FocusOptions::default();
    let measured = run(
        &engine,
        "volume.focus",
        json!({}),
        Values::from([("stack".to_owned(), Value::stack(stack.clone()))]),
    );
    let measures = measured["measures"].as_stack().expect("a stack").to_vec();
    for (plane, measure) in stack.iter().zip(&measures) {
        assert_eq!(measure, &focus::sml(plane, 2, 3).unwrap());
    }
    let found = run(
        &engine,
        "volume.height",
        json!({"median": 1}),
        Values::from([
            ("measures".to_owned(), Value::stack(measures)),
            ("planes".to_owned(), planes_json.clone()),
        ]),
    );
    let measure = |p: &Image| focus::sml(p, options.step, options.radius);
    let depth = depth_from_focus(&stack, planes, &options, measure).unwrap();
    let heights = image(&found, "heights");
    assert_eq!(
        (&heights, &image(&found, "confidence")),
        (&depth.heights, &depth.confidence)
    );
    // At plane 2, but where the window is cut at the corners.
    let near = heights
        .data()
        .iter()
        .filter(|z| (*z - 11.0).abs() < 0.25)
        .count();
    assert!(
        near * 100 >= 99 * 64 * 48,
        "{near} of {} near plane 2",
        64 * 48
    );
    let fused = run(
        &engine,
        "volume.all_in_focus",
        json!({}),
        Values::from([
            ("stack".to_owned(), Value::stack(stack.clone())),
            ("heights".to_owned(), Value::image(heights.clone())),
            ("planes".to_owned(), planes_json),
        ]),
    );
    let fused = image(&fused, "image");
    assert_eq!(fused, all_in_focus(&stack, &heights, planes).unwrap());

    // The mesh, as PLY: a vertex for each pixel, two triangles for each
    // square; not written over, nor outside the folder.
    let params = json!({"folder": folder.path(), "format": "ply", "pixel_um": 0.5});
    let inputs = Values::from([
        ("heights".to_owned(), Value::image(heights)),
        ("texture".to_owned(), Value::image(fused)),
    ]);
    let written = run(&engine, "export.mesh", params.clone(), inputs.clone());
    let written = written["written"].as_json().expect("JSON").clone();
    assert_eq!(
        (&written["vertices"], &written["triangles"]),
        (&json!(64 * 48), &json!(2 * 63 * 47))
    );
    assert!(folder.path().join("surface.ply").is_file());
    let refused = |params: Json| {
        let plan = Plan::new(
            engine.registry(),
            &single(&engine, "export.mesh", params, &inputs),
            false,
        );
        plan.ok()
            .and_then(|plan| engine.run(&plan, &inputs, &RunOptions::default()).ok())
            .is_none()
    };
    assert!(refused(params), "written over");
    assert!(refused(
        json!({"folder": "relative", "format": "stl", "pixel_um": 0.5})
    ));
    assert!(refused(
        json!({"folder": folder.path(), "name": "../up", "format": "stl", "pixel_um": 0.5})
    ));
}
