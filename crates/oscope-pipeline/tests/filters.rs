//! P23's stock nodes: the filters against `oscope-compute`'s, tiled runs
//! through their halos, Otsu's level, and morphology, distances and labels
//! worked by hand.

use std::sync::Arc;

use oscope_compute::filters::{self, Clahe};
use oscope_compute::{Compute, ComputeConfig, Image, Shape};
use oscope_pipeline::{Engine, Mask, Plan, Recipe, RunOptions, Value, Values, stock};
use serde_json::{Value as Json, json};

fn engine() -> (tempfile::TempDir, Engine) {
    let folder = tempfile::tempdir().unwrap();
    let config = ComputeConfig {
        live_threads: Some(2),
        batch_threads: Some(2),
        cuda_device: None,
    };
    let compute = Arc::new(Compute::new(&config).unwrap());
    let engine = Engine::new(stock::registry(folder.path()), compute, 0);
    (folder, engine)
}

/// A recipe of `nodes` in a chain from input `input`, of `kind`, giving
/// the last's output `output`.
fn chain(kind: &str, nodes: &[(&str, Json)], output: &str) -> Recipe {
    let (first_port, input) = if kind == "mask" {
        ("mask", "mask")
    } else {
        ("image", "image")
    };
    let mut edges = Vec::new();
    let mut from = format!("input.{input}");
    let ports = |node: &str| match node {
        "threshold.manual" | "threshold.otsu" => ("image", "mask"),
        node if node.starts_with("morphology.")
            && node != "morphology.distance"
            && node != "morphology.label" =>
        {
            ("mask", "mask")
        }
        "morphology.distance" => ("mask", "image"),
        "morphology.label" => ("mask", "labels"),
        _ => ("image", "image"),
    };
    let mut steps = Vec::new();
    for (k, (node, params)) in nodes.iter().enumerate() {
        let id = format!("s{k}");
        let (takes, gives) = ports(node);
        edges.push(json!({ "from": from, "to": format!("{id}.{takes}") }));
        from = format!("{id}.{gives}");
        steps.push(json!({ "id": id, "node": node, "params": params }));
    }
    let _ = first_port;
    serde_json::from_value(json!({
        "format": 1,
        "inputs": [{ "name": input, "type": kind }],
        "nodes": steps,
        "edges": edges,
        "outputs": [{ "name": output, "from": from }]
    }))
    .unwrap()
}

fn run(engine: &Engine, recipe: &Recipe, inputs: Values) -> Values {
    let plan = Plan::new(engine.registry(), recipe, false).unwrap();
    let options = RunOptions {
        memoise: false,
        ..RunOptions::default()
    };
    engine.run(&plan, &inputs, &options).unwrap().outputs
}

fn picture(width: u32, height: u32, channels: usize) -> Image {
    let shape = Shape::new(width, height, channels);
    let data = (0..shape.len()).map(|i| {
        let (x, y) = ((i / channels) as u32 % width, (i / channels) as u32 / width);
        let wave = ((x as f32 * 0.21).sin() * (y as f32 * 0.17).cos()).abs();
        0.1 + 0.7 * wave + 0.1 * ((i * 7919) % 101) as f32 / 101.0
    });
    Image::from_vec(shape, data.collect()).unwrap()
}

fn image_in(image: Image) -> Values {
    Values::from([("image".to_owned(), Value::image(image))])
}

fn mask_in(width: u32, height: u32, set: &[(u32, u32)]) -> Values {
    let mut mask = Mask::zeros(width, height);
    for &(x, y) in set {
        mask.data_mut()[(y * width + x) as usize] = 1;
    }
    Values::from([("mask".to_owned(), Value::mask(mask))])
}

fn set_pixels(mask: &Mask) -> Vec<(u32, u32)> {
    let width = mask.width();
    let set = mask.data().iter().enumerate().filter(|(_, bit)| **bit == 1);
    set.map(|(i, _)| (i as u32 % width, i as u32 / width))
        .collect()
}

#[test]
fn filter_nodes_give_what_oscope_compute_s_filters_do() {
    let (_folder, engine) = engine();
    let rgb = picture(97, 61, 3);
    let one = |node: &str, params: Json| -> Image {
        let outputs = run(
            &engine,
            &chain("image", &[(node, params)], "out"),
            image_in(rgb.clone()),
        );
        outputs["out"].as_image().unwrap().clone()
    };
    assert_eq!(
        one("filter.gaussian", json!({ "sigma": 1.5 })),
        filters::gaussian(&rgb, 1.5).unwrap()
    );
    assert_eq!(
        one("filter.median", json!({ "radius": 2 })),
        filters::median(&rgb, 2).unwrap()
    );
    let bilateral = one(
        "filter.bilateral",
        json!({ "sigma_space": 2.0, "sigma_range": 0.1 }),
    );
    assert_eq!(bilateral, filters::bilateral(&rgb, 2.0, 0.1).unwrap());
    let clahe = Clahe {
        tiles: (4, 3),
        clip: 3.0,
        bins: 128,
    };
    let equalised = one(
        "enhance.clahe",
        json!({ "tiles": [4, 3], "clip": 3.0, "bins": 128 }),
    );
    assert_eq!(equalised, filters::clahe(&rgb, &clahe).unwrap());
    let blurred = filters::gaussian(&rgb, 2.0).unwrap();
    let sharpened = one("enhance.unsharp", json!({ "sigma": 2.0, "amount": 0.5 }));
    let expected: Vec<f32> = rgb
        .data()
        .iter()
        .zip(blurred.data())
        .map(|(v, b)| v + 0.5 * (v - b))
        .collect();
    assert_eq!(sharpened.data(), expected);
    // Settings out of range are refused when the plan is made.
    for (node, params, problem) in [
        (
            "filter.median",
            json!({ "radius": 8 }),
            "8 is above the maximum, 7",
        ),
        ("filter.gaussian", json!({ "sigma": 0 }), "must be above 0"),
        (
            "enhance.clahe",
            json!({ "clip": 0.5 }),
            "a clip of at least 1",
        ),
    ] {
        let refused = Plan::new(
            engine.registry(),
            &chain("image", &[(node, params)], "out"),
            false,
        );
        let refused = refused.unwrap_err().to_string();
        assert!(refused.contains(problem), "{node}: {refused}");
    }
}

#[test]
fn tiled_runs_through_the_new_halos_give_what_whole_runs_do() {
    let (_folder, engine) = engine();
    let recipe = chain(
        "image",
        &[
            ("filter.gaussian", json!({ "sigma": 1.2 })),
            ("filter.median", json!({ "radius": 2 })),
            (
                "filter.bilateral",
                json!({ "sigma_space": 1.5, "sigma_range": 0.2 }),
            ),
            ("enhance.unsharp", json!({ "sigma": 1.0, "amount": 0.8 })),
            ("threshold.manual", json!({ "level": 0.45 })),
            ("morphology.open", json!({ "radius": 2, "shape": "disk" })),
            ("morphology.dilate", json!({ "radius": 1 })),
        ],
        "mask",
    );
    let plan = Plan::new(engine.registry(), &recipe, false).unwrap();
    // 4 + 2 + 3 + 3 + 0 + 2 × 2 + 1.
    assert_eq!(plan.halo(), 17);
    let inputs = image_in(picture(300, 200, 3));
    let options = RunOptions {
        memoise: false,
        ..RunOptions::default()
    };
    let whole = engine.run(&plan, &inputs, &options).unwrap();
    let tiled = engine
        .run_tiled(&plan, &inputs, 64, &options, |_, _| {})
        .unwrap();
    assert_eq!(tiled.report.tiles, 20);
    let mask = whole.outputs["mask"].as_mask().unwrap();
    assert_eq!(tiled.outputs["mask"].as_mask(), Some(mask));
    let set = set_pixels(mask).len();
    assert!(set > 1000 && set < 59_000, "{set} of 60,000 pixels set");
}

#[test]
fn otsu_finds_the_level_between_two_populations() {
    let (_folder, engine) = engine();
    // Dark and light halves, each spread a little: the level falls between.
    let data = (0..120 * 80)
        .map(|i| if i % 120 < 60 { 0.2 } else { 0.7 } + ((i * 37) % 11) as f32 / 110.0)
        .collect();
    let image = Image::from_vec(Shape::new(120, 80, 1), data).unwrap();
    let recipe: Recipe = serde_json::from_value(json!({
        "format": 1,
        "inputs": [{ "name": "image", "type": "image" }],
        "nodes": [{ "id": "otsu", "node": "threshold.otsu" }],
        "edges": [{ "from": "input.image", "to": "otsu.image" }],
        "outputs": [{ "name": "mask", "from": "otsu.mask" }, { "name": "level", "from": "otsu.level" }]
    }))
    .unwrap();
    let outputs = run(&engine, &recipe, image_in(image));
    // The dark half reaches 0.2 + 10 / 110: every split in the gap ties, and
    // the first is taken.
    let level = outputs["level"].as_json().unwrap().as_f64().unwrap();
    assert!(
        level > 0.2 + 10.0 / 110.0 && level <= 0.7,
        "Otsu's level: {level}"
    );
    let mask = outputs["mask"].as_mask().unwrap();
    assert!(
        set_pixels(mask).iter().all(|&(x, _)| x >= 60),
        "only the light half is set"
    );
    assert_eq!(set_pixels(mask).len(), 60 * 80);
}

#[test]
fn morphology_distances_and_labels_are_as_worked_by_hand() {
    let (_folder, engine) = engine();
    let morph = |node: &str, params: Json, inputs: Values| -> Mask {
        let outputs = run(&engine, &chain("mask", &[(node, params)], "out"), inputs);
        outputs["out"].as_mask().unwrap().clone()
    };
    // A point dilated: a 3 × 3 square, or the 13 pixels of a disk of 2.
    let point = || mask_in(7, 7, &[(3, 3)]);
    let square = morph("morphology.dilate", json!({ "radius": 1 }), point());
    let block: Vec<(u32, u32)> = (2..5).flat_map(|y| (2..5).map(move |x| (x, y))).collect();
    assert_eq!(set_pixels(&square), block);
    assert_eq!(
        set_pixels(&morph(
            "morphology.dilate",
            json!({ "radius": 2, "shape": "disk" }),
            point()
        ))
        .len(),
        13
    );
    // The block eroded back to its middle; opening removes the point;
    // closing fills a hole; a full mask doesn't wear at the edges.
    let eroded = morph(
        "morphology.erode",
        json!({ "radius": 1 }),
        mask_in(7, 7, &block),
    );
    assert_eq!(set_pixels(&eroded), [(3, 3)]);
    assert!(set_pixels(&morph("morphology.open", json!({ "radius": 1 }), point())).is_empty());
    let ring: Vec<(u32, u32)> = block.iter().copied().filter(|&p| p != (3, 3)).collect();
    assert_eq!(
        set_pixels(&morph(
            "morphology.close",
            json!({ "radius": 1 }),
            mask_in(7, 7, &ring)
        )),
        block
    );
    let all: Vec<(u32, u32)> = (0..5).flat_map(|y| (0..6).map(move |x| (x, y))).collect();
    assert_eq!(
        set_pixels(&morph(
            "morphology.erode",
            json!({ "radius": 2 }),
            mask_in(6, 5, &all)
        ))
        .len(),
        30
    );

    // Distances to the one pixel not set, at (0, 0): √(x² + y²) exactly;
    // and with none, the diagonal.
    let lit: Vec<(u32, u32)> = all.iter().copied().filter(|&p| p != (0, 0)).collect();
    let distances = run(
        &engine,
        &chain("mask", &[("morphology.distance", json!({}))], "out"),
        mask_in(6, 5, &lit),
    );
    let distances = distances["out"].as_image().unwrap();
    for (x, y) in all.iter().copied() {
        let expected = ((x * x + y * y) as f32).sqrt();
        assert_eq!(
            distances.data()[(y * 6 + x) as usize],
            expected,
            "({x}, {y})"
        );
    }
    let none = run(
        &engine,
        &chain("mask", &[("morphology.distance", json!({}))], "out"),
        mask_in(6, 5, &all),
    );
    assert!(
        none["out"]
            .as_image()
            .unwrap()
            .data()
            .iter()
            .all(|&d| d == (61.0_f32).sqrt())
    );

    // Three pixels and a pair: the pair touches the first pixel at a
    // corner, so it joins it with corners, and doesn't with sides only.
    let labels = |connectivity: u8, set: &[(u32, u32)]| {
        let recipe: Recipe = serde_json::from_value(json!({
            "format": 1,
            "inputs": [{ "name": "mask", "type": "mask" }],
            "nodes": [{ "id": "label", "node": "morphology.label", "params": { "connectivity": connectivity } }],
            "edges": [{ "from": "input.mask", "to": "label.mask" }],
            "outputs": [{ "name": "labels", "from": "label.labels" }, { "name": "components", "from": "label.components" }]
        }))
        .unwrap();
        let outputs = run(&engine, &recipe, mask_in(8, 6, set));
        let image = outputs["labels"].as_image().unwrap().clone();
        let table = outputs["components"].as_table().unwrap().clone();
        let numbers: Vec<f32> = set
            .iter()
            .map(|&(x, y)| image.data()[(y * 8 + x) as usize])
            .collect();
        (numbers, table)
    };
    let set = [(1, 1), (2, 2), (3, 2), (6, 1), (6, 4)];
    let (eight, table) = labels(8, &set);
    assert_eq!(eight, [1.0, 1.0, 1.0, 2.0, 3.0]);
    assert_eq!(
        table.columns,
        ["label", "area", "x", "y", "left", "top", "width", "height"]
    );
    assert_eq!(
        table.rows[0],
        [
            json!(1),
            json!(3),
            json!(2.0),
            json!(5.0 / 3.0),
            json!(1),
            json!(1),
            json!(3),
            json!(2)
        ]
    );
    let (four, table) = labels(4, &set);
    assert_eq!(four, [1.0, 3.0, 3.0, 2.0, 4.0]);
    assert_eq!(table.rows.len(), 4);
    // A U: its arms are numbered apart at first, then joined at its foot.
    let u = [(0, 0), (2, 0), (0, 1), (2, 1), (0, 2), (1, 2), (2, 2)];
    for connectivity in [4, 8] {
        let (numbers, table) = labels(connectivity, &u);
        assert_eq!(
            (numbers, table.rows.len()),
            (vec![1.0; 7], 1),
            "connectivity {connectivity}"
        );
    }
}

/// On the owner's machine, with `OSCOPE_CUDA=1`: the filter nodes on the
/// GPU give what they do on the CPU.
#[cfg(feature = "cuda")]
#[test]
fn filter_nodes_on_the_gpu_match_the_cpu_s() {
    use oscope_compute::equivalence::{Tolerance, assert_equivalent};
    use oscope_pipeline::Device;
    if std::env::var("OSCOPE_CUDA").as_deref() != Ok("1") {
        eprintln!("skipped: set OSCOPE_CUDA=1 to run it on a GPU");
        return;
    }
    let (_folder, engine) = engine();
    assert!(engine.has_gpu(), "OSCOPE_CUDA=1, but no GPU");
    let rgb = picture(640, 480, 3);
    let options = RunOptions {
        memoise: false,
        ..RunOptions::default()
    };
    for (node, params, tolerance) in [
        ("filter.gaussian", json!({ "sigma": 2.0 }), Tolerance::EXACT),
        ("filter.median", json!({ "radius": 3 }), Tolerance::EXACT),
        (
            "filter.bilateral",
            json!({ "sigma_space": 2.0, "sigma_range": 0.1 }),
            Tolerance {
                abs: 1e-6,
                rel: 1e-5,
            },
        ),
        ("enhance.clahe", json!({}), Tolerance::EXACT),
    ] {
        // Binning by 1 after it, a second GPU step, keeps it on the GPU.
        let recipe = chain(
            "image",
            &[(node, params), ("resize.bin", json!({ "factor": 1 }))],
            "out",
        );
        let ran = |gpu: bool| {
            let plan = Plan::new(engine.registry(), &recipe, gpu).unwrap();
            assert_eq!(
                plan.device("s0"),
                Some(if gpu { Device::Cuda } else { Device::Cpu }),
                "{node}"
            );
            let outputs = engine
                .run(&plan, &image_in(rgb.clone()), &options)
                .unwrap()
                .outputs;
            outputs["out"].as_image().unwrap().clone()
        };
        assert_equivalent(&ran(false), &ran(true), tolerance);
    }
}
