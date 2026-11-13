//! The pipeline engine: recipes checked, steps ordered and run in parallel,
//! tiles, memoisation, cancelling, batch jobs, live mode and placement.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use oscope_compute::{Compute, ComputeConfig, Image, Shape};
use oscope_jobs::{JobService, JobState, JobsConfig};
use oscope_pipeline::{
    Batch, Capabilities, Descriptor, Device, Engine, Inputs, Live, Node, NodeError, Plan, PortType,
    Recipe, Registry, RunError, RunOptions, StepCtx, Value, Values, node, outputs, test_nodes,
};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::json;

fn compute() -> Arc<Compute> {
    let config = ComputeConfig {
        live_threads: Some(2),
        batch_threads: Some(2),
        cuda_device: None,
    };
    Arc::new(Compute::new(&config).expect("the compute layer starts"))
}

fn engine_with(registry: Registry) -> Arc<Engine> {
    Arc::new(Engine::new(registry, compute(), 256 << 20))
}

fn recipe(json: serde_json::Value) -> Recipe {
    serde_json::from_value(json).expect("a recipe")
}

/// A picture with structure at every scale: gradients and noise, three
/// channels.
fn picture(width: u32, height: u32, seed: u64) -> Image {
    let shape = Shape::new(width, height, 3);
    let mut state = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
    let data = (0..shape.len()).map(|i| {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let (x, y) = ((i / 3) as u32 % width, (i / 3) as u32 / width);
        let noise = (state >> 40) as f32 / (1_u64 << 24) as f32;
        0.5 * x as f32 / width as f32 + 0.3 * y as f32 / height as f32 + 0.2 * noise
    });
    Image::from_vec(shape, data.collect()).expect("the samples fill the shape")
}

fn image_inputs(image: Image) -> Values {
    Values::from([("image".to_owned(), Value::image(image))])
}

/// Waits for `condition`, for at most 20 s.
fn wait_until(what: &str, mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while !condition() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// A step for nodes that are only planned.
fn never(_: &StepCtx<'_>, _: &Inputs<'_>) -> Result<Values, NodeError> {
    Err("never run".into())
}

/// What the recording nodes saw.
#[derive(Default)]
struct Log {
    events: Mutex<Vec<String>>,
    met: Mutex<usize>,
    wake: Condvar,
}

impl Log {
    fn push(&self, event: String) {
        self.events.lock().unwrap().push(event);
    }

    fn has(&self, event: &str) -> bool {
        self.events.lock().unwrap().iter().any(|e| e == event)
    }

    fn at(&self, event: &str) -> usize {
        let events = self.events.lock().unwrap();
        events
            .iter()
            .position(|e| e == event)
            .unwrap_or_else(|| panic!("no `{event}` in {events:?}"))
    }
}

#[derive(Deserialize, JsonSchema)]
struct Label {
    label: String,
    /// Wait for another step to arrive too, as proof that both run at once.
    #[serde(default)]
    meet: bool,
}

/// `test.record`: its image as it is, its start and end logged.
fn record(log: Arc<Log>) -> Arc<dyn Node> {
    let descriptor = Descriptor::new("test.record", 1, "test", "Logs its start and end")
        .input("image", PortType::Image)
        .optional_input("other", PortType::Image)
        .output("image", PortType::Image)
        .params::<Label>()
        .capabilities(Capabilities {
            cpu: true,
            live: true,
            tileable: true,
            ..Capabilities::default()
        });
    node(descriptor, move |params: Label| {
        let log = Arc::clone(&log);
        move |_: &StepCtx<'_>, inputs: &Inputs<'_>| -> Result<Values, NodeError> {
            log.push(format!("{} start", params.label));
            if params.meet {
                let mut met = log.met.lock().unwrap();
                *met += 1;
                log.wake.notify_all();
                let wait = log
                    .wake
                    .wait_timeout_while(met, Duration::from_secs(10), |met| *met < 2);
                if wait.unwrap().1.timed_out() {
                    return Err(
                        "the other branch never came: the branches ran one at a time".into(),
                    );
                }
            }
            log.push(format!("{} end", params.label));
            Ok(outputs([(
                "image",
                inputs.get("image").cloned().ok_or("no image")?,
            )]))
        }
    })
}

#[test]
fn recipes_are_checked_and_every_problem_is_named() {
    let mut registry = test_nodes::registry();
    let gpu_only = Descriptor::new("test.gpu_only", 1, "test", "Runs only on a GPU")
        .input("image", PortType::Image)
        .output("image", PortType::Image)
        .capabilities(Capabilities {
            cuda: true,
            ..Capabilities::default()
        });
    registry
        .register(node(gpu_only, |_: serde_json::Value| never))
        .unwrap();
    let problems = |json: serde_json::Value, gpu: bool| -> Vec<String> {
        match Plan::new(&registry, &recipe(json), gpu) {
            Ok(_) => Vec::new(),
            Err(error) => error.problems.iter().map(ToString::to_string).collect(),
        }
    };
    let image = json!([{ "name": "image", "type": "image" }]);
    assert_eq!(
        problems(
            json!({
                "format": 2,
                "inputs": image,
                "nodes": [
                    { "id": "blur", "node": "test.box_blur", "version": 2, "params": { "radius": 65 } },
                    { "id": "input", "node": "test.nope" }
                ],
                "edges": [{ "from": "input.image", "to": "blur.image" }],
                "outputs": [{ "name": "out", "from": "blur.image" }]
            }),
            false
        ),
        [
            "format: format 2 isn't the one this OScope reads, 1",
            "nodes[0] (blur): it is for version 2 of `test.box_blur`, and this is 1",
            "nodes[0] (blur) params/radius: 65 is above the maximum, 64",
            "nodes[1] (input): its ID isn't a name, is `input`, or is another node's too",
            "nodes[1] (input): there is no node `test.nope`",
        ]
    );
    assert_eq!(
        problems(
            json!({
                "format": 1,
                "inputs": image,
                "nodes": [
                    { "id": "mask", "node": "test.threshold", "params": { "level": 0.5 } },
                    { "id": "sum", "node": "test.add" },
                    { "id": "blur", "node": "test.box_blur", "params": { "radius": 1 } }
                ],
                "edges": [
                    { "from": "input.image", "to": "mask.image" },
                    { "from": "mask.mask", "to": "sum.first" },
                    { "from": "input.image", "to": "blur.image" },
                    { "from": "input.image", "to": "blur.image" },
                    { "from": "input.picture", "to": "sum.second" },
                    { "from": "blur", "to": "sum.second" },
                    { "from": "blur.image", "to": "sum.third" }
                ],
                "outputs": [{ "name": "out", "from": "sum.result" }, { "name": "out", "from": "blur.image" }]
            }),
            false
        ),
        [
            "edges[1]: `mask.mask` gives mask, and `sum.first` takes image",
            "edges[3]: `blur.image` has another edge too",
            "edges[4]: the recipe has no input `picture`",
            "edges[5]: `blur` isn't `node.port`",
            "edges[6]: `sum` has no input `third`",
            "outputs[0]: `sum` has no output `result`",
            "outputs[1]: its name isn't a name, or is another output's too",
        ]
    );
    // A missing input, and a cycle named in the direction data flows.
    assert_eq!(
        problems(
            json!({
                "format": 1,
                "inputs": image,
                "nodes": [
                    { "id": "a", "node": "test.add" },
                    { "id": "b", "node": "test.box_blur", "params": { "radius": 1 } },
                    { "id": "c", "node": "test.sleep", "params": { "ms": 0 } },
                    { "id": "d", "node": "test.add" }
                ],
                "edges": [
                    { "from": "input.image", "to": "a.first" },
                    { "from": "c.image", "to": "a.second" },
                    { "from": "a.image", "to": "b.image" },
                    { "from": "b.image", "to": "c.image" },
                    { "from": "input.image", "to": "d.first" }
                ],
                "outputs": [{ "name": "out", "from": "b.image" }]
            }),
            false
        ),
        [
            "nodes[3] (d): its input `second` has no edge",
            "edges: they make a cycle: b → c → a → b",
        ]
    );
    let gpu = json!({
        "format": 1,
        "inputs": image,
        "nodes": [{ "id": "g", "node": "test.gpu_only" }],
        "edges": [{ "from": "input.image", "to": "g.image" }],
        "outputs": []
    });
    assert_eq!(
        problems(gpu.clone(), false),
        [
            "nodes[0] (g): `test.gpu_only` runs only on a GPU, and there is none",
            "outputs: the recipe gives nothing",
        ]
    );
    assert_eq!(problems(gpu, true), ["outputs: the recipe gives nothing"]);
    let typo = Recipe::from_json(r#"{ "format": 1, "nodes": [], "edge": [], "outputs": [] }"#);
    assert!(
        typo.unwrap_err()
            .to_string()
            .contains("unknown field `edge`")
    );
}

#[test]
fn malformed_nodes_are_refused_by_the_registry() {
    let mut registry = test_nodes::registry();
    let blur = test_nodes::box_blur();
    assert_eq!(
        registry.register(blur),
        Err("node `test.box_blur` is registered already".into())
    );
    let with = |descriptor: Descriptor| node(descriptor, |_: serde_json::Value| never);
    let base = || Descriptor::new("test.bad", 1, "test", "").input("image", PortType::Image);
    let cpu = Capabilities {
        cpu: true,
        ..Capabilities::default()
    };
    let table = base()
        .output("table", PortType::Table)
        .capabilities(Capabilities {
            tileable: true,
            ..cpu
        });
    assert_eq!(
        registry.register(with(table)),
        Err("node `test.bad`: it is tileable, but gives what isn't a raster".into())
    );
    let mut pattern = base().output("image", PortType::Image).capabilities(cpu);
    pattern.params = json!({ "properties": { "name": { "pattern": "^a" } } });
    assert_eq!(
        registry.register(with(pattern)),
        Err("node `test.bad`: its parameters' schema: #/properties/name/pattern: `pattern` isn't supported".into())
    );
    let twice = base()
        .input("image", PortType::Image)
        .output("image", PortType::Image)
        .capabilities(cpu);
    assert_eq!(
        registry.register(with(twice)),
        Err("node `test.bad`: input `image` isn't a name, or is there twice".into())
    );
    let nowhere = base().output("image", PortType::Image);
    assert_eq!(
        registry.register(with(nowhere)),
        Err("node `test.bad`: it runs on neither the CPU nor the GPU".into())
    );
}

#[test]
fn steps_run_after_what_they_read_and_branches_at_once() {
    let log = Arc::new(Log::default());
    let mut registry = test_nodes::registry();
    registry.register(record(Arc::clone(&log))).unwrap();
    let engine = engine_with(registry);
    let plan = engine
        .plan(&recipe(json!({
            "format": 1,
            "inputs": [{ "name": "image", "type": "image" }],
            "nodes": [
                { "id": "last", "node": "test.record", "params": { "label": "last" } },
                { "id": "join", "node": "test.record", "params": { "label": "join" } },
                { "id": "left", "node": "test.record", "params": { "label": "left", "meet": true } },
                { "id": "right", "node": "test.record", "params": { "label": "right", "meet": true } },
                { "id": "unused", "node": "test.record", "params": { "label": "unused" } }
            ],
            "edges": [
                { "from": "input.image", "to": "left.image" },
                { "from": "input.image", "to": "right.image" },
                { "from": "left.image", "to": "join.image" },
                { "from": "right.image", "to": "join.other" },
                { "from": "join.image", "to": "last.image" },
                { "from": "input.image", "to": "unused.image" }
            ],
            "outputs": [{ "name": "out", "from": "last.image" }]
        })))
        .unwrap();
    // Only what the output needs, each after what it reads, otherwise in
    // the recipe's order.
    assert_eq!(plan.order(), ["left", "right", "join", "last"]);
    let image = picture(64, 48, 1);
    let run = engine
        .run(&plan, &image_inputs(image.clone()), &RunOptions::default())
        .unwrap();
    assert_eq!(run.outputs["out"].as_image(), Some(&image));
    for (before, after) in [("left", "join"), ("right", "join"), ("join", "last")] {
        assert!(log.at(&format!("{before} end")) < log.at(&format!("{after} start")));
    }
    assert!(!log.has("unused start"));
    let ran: Vec<&str> = run
        .report
        .steps
        .iter()
        .map(|step| step.id.as_str())
        .collect();
    assert_eq!(ran, ["left", "right", "join", "last"]);
}

/// Three blurs in a row and one beside them, their difference, and its
/// mask: a halo of 2 + 3 + 4 along the longer path, where the other path's
/// widest is 6.
fn band_pass() -> Recipe {
    recipe(json!({
        "format": 1,
        "name": "band pass",
        "inputs": [{ "name": "image", "type": "image" }],
        "nodes": [
            { "id": "fine", "node": "test.box_blur", "params": { "radius": 2 } },
            { "id": "finer", "node": "test.box_blur", "params": { "radius": 3 } },
            { "id": "finest", "node": "test.box_blur", "params": { "radius": 4 } },
            { "id": "coarse", "node": "test.box_blur", "params": { "radius": 6 } },
            { "id": "detail", "node": "test.add", "params": { "a": 1, "b": -1 } },
            { "id": "mask", "node": "test.threshold", "params": { "level": 0.0 } }
        ],
        "edges": [
            { "from": "input.image", "to": "fine.image" },
            { "from": "fine.image", "to": "finer.image" },
            { "from": "finer.image", "to": "finest.image" },
            { "from": "input.image", "to": "coarse.image" },
            { "from": "finest.image", "to": "detail.first" },
            { "from": "coarse.image", "to": "detail.second" },
            { "from": "detail.image", "to": "mask.image" }
        ],
        "outputs": [
            { "name": "detail", "from": "detail.image" },
            { "name": "mask", "from": "mask.mask" },
            { "name": "raw", "from": "input.image" }
        ]
    }))
}

#[test]
fn tiled_runs_give_exactly_what_whole_runs_do() {
    let engine = engine_with(test_nodes::registry());
    let plan = engine.plan(&band_pass()).unwrap();
    assert_eq!(plan.halo(), 9);
    let options = RunOptions {
        memoise: false,
        ..RunOptions::default()
    };
    for (width, height, tile, tiles) in [(1000, 700, 256, 12), (120, 90, 7, 234)] {
        let inputs = image_inputs(picture(width, height, 2));
        let whole = engine.run(&plan, &inputs, &options).unwrap();
        let mut seen = Vec::new();
        let tiled = engine
            .run_tiled(&plan, &inputs, tile, &options, |done, of| {
                seen.push((done, of))
            })
            .unwrap();
        assert_eq!(tiled.report.tiles, tiles);
        assert_eq!(seen.last(), Some(&(tiles as usize, tiles as usize)));
        for name in ["detail", "raw"] {
            assert_eq!(
                tiled.outputs[name].as_image(),
                whole.outputs[name].as_image(),
                "{name}, tiles of {tile}"
            );
        }
        assert_eq!(
            tiled.outputs["mask"].as_mask(),
            whole.outputs["mask"].as_mask()
        );
        let set = tiled.outputs["mask"]
            .as_mask()
            .unwrap()
            .data()
            .iter()
            .filter(|&&bit| bit == 1)
            .count();
        assert!(
            set > 0 && set < (width * height) as usize,
            "the mask has both values"
        );
    }
    let measured = recipe(json!({
        "format": 1,
        "inputs": [{ "name": "image", "type": "image" }],
        "nodes": [{ "id": "stats", "node": "test.statistics" }],
        "edges": [{ "from": "input.image", "to": "stats.image" }],
        "outputs": [{ "name": "table", "from": "stats.table" }]
    }));
    let plan = engine.plan(&measured).unwrap();
    let refused = engine.run_tiled(
        &plan,
        &image_inputs(picture(64, 64, 3)),
        32,
        &options,
        |_, _| {},
    );
    assert_eq!(
        refused.unwrap_err(),
        RunError::Unsuitable("stats can't run tile by tile".into())
    );
}

#[test]
fn results_are_memoised_and_only_what_changed_runs_again() {
    let runs = Arc::new(AtomicUsize::new(0));
    let mut registry = test_nodes::registry();
    let counter = Descriptor::new("test.count", 1, "test", "Counts its runs")
        .input("image", PortType::Image)
        .output("image", PortType::Image)
        .capabilities(Capabilities {
            cpu: true,
            ..Capabilities::default()
        });
    let counted = Arc::clone(&runs);
    registry
        .register(node(counter, move |_: serde_json::Value| {
            let runs = Arc::clone(&counted);
            move |_: &StepCtx<'_>, inputs: &Inputs<'_>| -> Result<Values, NodeError> {
                runs.fetch_add(1, Ordering::SeqCst);
                Ok(outputs([(
                    "image",
                    inputs.get("image").cloned().ok_or("no image")?,
                )]))
            }
        }))
        .unwrap();
    let engine = engine_with(registry);
    let mut edited = recipe(json!({
        "format": 1,
        "inputs": [{ "name": "image", "type": "image" }],
        "nodes": [
            { "id": "blur", "node": "test.box_blur", "params": { "radius": 2 } },
            { "id": "count", "node": "test.count" },
            { "id": "mask", "node": "test.threshold", "params": { "level": 0.5 } }
        ],
        "edges": [
            { "from": "input.image", "to": "blur.image" },
            { "from": "blur.image", "to": "count.image" },
            { "from": "count.image", "to": "mask.image" }
        ],
        "outputs": [{ "name": "mask", "from": "mask.mask" }, { "name": "blurred", "from": "count.image" }]
    }));
    let inputs = image_inputs(picture(320, 240, 4));
    let run = |recipe: &Recipe, inputs: &Values, options: &RunOptions| {
        let run = engine
            .run(&engine.plan(recipe).unwrap(), inputs, options)
            .unwrap();
        let cached: Vec<bool> = run.report.steps.iter().map(|step| step.cached).collect();
        (run, cached)
    };
    let options = RunOptions::default();
    let (first, cached) = run(&edited, &inputs, &options);
    assert_eq!((cached, runs.load(Ordering::SeqCst)), (vec![false; 3], 1));
    let (again, cached) = run(&edited, &inputs, &options);
    assert_eq!((cached, runs.load(Ordering::SeqCst)), (vec![true; 3], 1));
    assert_eq!(
        again.outputs["mask"].as_mask(),
        first.outputs["mask"].as_mask()
    );
    // A parameter edited: only its step, and those after it, run again.
    edited.set_param("mask", "level", json!(0.6)).unwrap();
    let (_, cached) = run(&edited, &inputs, &options);
    assert_eq!(
        (cached, runs.load(Ordering::SeqCst)),
        (vec![true, true, false], 1)
    );
    edited.set_param("blur", "radius", json!(3)).unwrap();
    let (_, cached) = run(&edited, &inputs, &options);
    assert_eq!((cached, runs.load(Ordering::SeqCst)), (vec![false; 3], 2));
    // Other pixels: everything runs again.
    let (_, cached) = run(&edited, &image_inputs(picture(320, 240, 5)), &options);
    assert_eq!((cached, runs.load(Ordering::SeqCst)), (vec![false; 3], 3));
    assert_eq!(engine.memo().hits, 5);
    // Without memoising, nothing is looked for.
    let off = RunOptions {
        memoise: false,
        ..RunOptions::default()
    };
    let (_, cached) = run(&edited, &inputs, &off);
    assert_eq!((cached, runs.load(Ordering::SeqCst)), (vec![false; 3], 4));
}

fn slow_recipe() -> Recipe {
    recipe(json!({
        "format": 1,
        "inputs": [{ "name": "image", "type": "image" }],
        "nodes": [
            { "id": "before", "node": "test.record", "params": { "label": "before" } },
            { "id": "slow", "node": "test.sleep", "params": { "ms": 60000 } },
            { "id": "after", "node": "test.record", "params": { "label": "after" } }
        ],
        "edges": [
            { "from": "input.image", "to": "before.image" },
            { "from": "before.image", "to": "slow.image" },
            { "from": "slow.image", "to": "after.image" }
        ],
        "outputs": [{ "name": "out", "from": "after.image" }]
    }))
}

#[test]
fn a_cancelled_run_stops_in_its_step_and_starts_no_more() {
    let log = Arc::new(Log::default());
    let mut registry = test_nodes::registry();
    registry.register(record(Arc::clone(&log))).unwrap();
    let engine = engine_with(registry);
    let plan = engine.plan(&slow_recipe()).unwrap();
    let options = RunOptions::default();
    let cancel = Arc::clone(&options.cancel);
    let started = Instant::now();
    let running =
        std::thread::spawn(move || engine.run(&plan, &image_inputs(picture(32, 32, 6)), &options));
    wait_until("the slow step", || log.has("before end"));
    cancel.store(true, Ordering::Release);
    assert_eq!(running.join().unwrap().unwrap_err(), RunError::Cancelled);
    assert!(
        started.elapsed() < Duration::from_secs(30),
        "the slow step stopped early"
    );
    assert!(!log.has("after start"));
}

#[test]
fn failures_and_panics_are_the_step_s() {
    let mut registry = test_nodes::registry();
    let broken = Descriptor::new("test.broken", 1, "test", "Fails, or panics")
        .input("image", PortType::Image)
        .output("image", PortType::Image)
        .params::<serde_json::Value>()
        .capabilities(Capabilities {
            cpu: true,
            ..Capabilities::default()
        });
    registry
        .register(node(broken, |params: serde_json::Value| {
            let panics = params["panic"] == json!(true);
            move |_: &StepCtx<'_>, _: &Inputs<'_>| -> Result<Values, NodeError> {
                assert!(!panics, "a broken node");
                Err("it broke".into())
            }
        }))
        .unwrap();
    let engine = engine_with(registry);
    for (params, message) in [
        (json!({}), "it broke"),
        (json!({ "panic": true }), "it panicked: a broken node"),
    ] {
        let plan = engine
            .plan(&recipe(json!({
                "format": 1,
                "inputs": [{ "name": "image", "type": "image" }],
                "nodes": [{ "id": "b", "node": "test.broken", "params": params }],
                "edges": [{ "from": "input.image", "to": "b.image" }],
                "outputs": [{ "name": "out", "from": "b.image" }]
            })))
            .unwrap();
        let failed = engine.run(
            &plan,
            &image_inputs(picture(8, 8, 7)),
            &RunOptions::default(),
        );
        let step = "b".to_owned();
        assert_eq!(
            failed.unwrap_err(),
            RunError::Step {
                step,
                message: message.into()
            }
        );
    }
    let missing = engine.plan(&band_pass()).unwrap();
    let error = engine
        .run(&missing, &Values::new(), &RunOptions::default())
        .unwrap_err();
    assert_eq!(
        error,
        RunError::Input("image".into(), "it is missing".into())
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_batch_runs_as_a_compute_job_with_progress() {
    let folder = tempfile::tempdir().unwrap();
    let jobs =
        JobService::open(&folder.path().join("index.sqlite"), JobsConfig::default()).unwrap();
    let mut events = jobs.subscribe();
    let engine = engine_with(test_nodes::registry());
    let plan = Arc::new(engine.plan(&band_pass()).unwrap());
    let pictures: Vec<Image> = (0..3).map(|seed| picture(200, 150, seed)).collect();
    let written = Arc::new(Mutex::new(Vec::new()));
    let batch = Batch {
        count: 3,
        read: Box::new({
            let pictures = pictures.clone();
            move |item| Ok(image_inputs(pictures[item].clone()))
        }),
        write: Box::new({
            let written = Arc::clone(&written);
            move |item, run| {
                written.lock().unwrap().push(run.outputs);
                Ok(Some(format!("item-{item}")))
            }
        }),
        tile: Some(64),
    };
    let id = jobs
        .submit(engine.batch_job("band pass, three pictures", plan.clone(), batch, 0))
        .unwrap();
    let mut fractions = Vec::new();
    let ended = tokio::time::timeout(Duration::from_secs(60), async {
        loop {
            let job = match events.recv().await {
                Ok(job) if job.id == id => job,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    panic!("the job service stopped")
                }
                _ => continue,
            };
            fractions.extend(job.progress.fraction);
            if job.state.is_finished() {
                return job;
            }
        }
    })
    .await
    .expect("the job ends");
    assert_eq!(ended.state, JobState::Completed, "{:?}", ended.error);
    assert_eq!(ended.results, ["item-0", "item-1", "item-2"]);
    assert_eq!(ended.progress.counters["items"], 3);
    assert!(
        fractions.windows(2).all(|pair| pair[0] <= pair[1]),
        "{fractions:?}"
    );
    assert_eq!(fractions.last(), Some(&1.0));
    // The tiled batch gives what whole runs do.
    let written = written.lock().unwrap();
    for (outputs, picture) in written.iter().zip(pictures) {
        let whole = engine
            .run(&plan, &image_inputs(picture), &RunOptions::default())
            .unwrap();
        assert_eq!(
            outputs["detail"].as_image(),
            whole.outputs["detail"].as_image()
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_cancelled_batch_stops_in_its_item() {
    let folder = tempfile::tempdir().unwrap();
    let jobs =
        JobService::open(&folder.path().join("index.sqlite"), JobsConfig::default()).unwrap();
    let log = Arc::new(Log::default());
    let mut registry = test_nodes::registry();
    registry.register(record(Arc::clone(&log))).unwrap();
    let engine = engine_with(registry);
    let plan = Arc::new(engine.plan(&slow_recipe()).unwrap());
    let written = Arc::new(AtomicUsize::new(0));
    let batch = Batch {
        count: 5,
        read: Box::new(|_| Ok(image_inputs(picture(32, 32, 8)))),
        write: Box::new({
            let written = Arc::clone(&written);
            move |_, _| {
                written.fetch_add(1, Ordering::SeqCst);
                Ok(None)
            }
        }),
        tile: None,
    };
    let started = Instant::now();
    let id = jobs
        .submit(engine.batch_job("slow", plan, batch, 0))
        .unwrap();
    let in_step = Arc::clone(&log);
    tokio::task::spawn_blocking(move || wait_until("the slow step", || in_step.has("before end")))
        .await
        .unwrap();
    jobs.cancel(id).unwrap();
    let mut ended = jobs.get(id).unwrap();
    while !ended.state.is_finished() {
        assert!(started.elapsed() < Duration::from_secs(30), "the job ends");
        tokio::time::sleep(Duration::from_millis(5)).await;
        ended = jobs.get(id).unwrap();
    }
    assert_eq!(ended.state, JobState::Cancelled);
    assert_eq!(written.load(Ordering::SeqCst), 0);
    assert!(!log.has("after start"));
}

/// `test.gate`: passes its image once the gate is open.
#[derive(Default)]
struct Gate {
    open: Mutex<bool>,
    wake: Condvar,
    entered: AtomicUsize,
}

fn gate(gate: Arc<Gate>) -> Arc<dyn Node> {
    let descriptor = Descriptor::new("test.gate", 1, "test", "Waits for its gate to open")
        .input("image", PortType::Image)
        .output("image", PortType::Image)
        .capabilities(Capabilities {
            cpu: true,
            live: true,
            ..Capabilities::default()
        });
    node(descriptor, move |_: serde_json::Value| {
        let gate = Arc::clone(&gate);
        move |_: &StepCtx<'_>, inputs: &Inputs<'_>| -> Result<Values, NodeError> {
            gate.entered.fetch_add(1, Ordering::SeqCst);
            let open = gate.open.lock().unwrap();
            let wait = gate
                .wake
                .wait_timeout_while(open, Duration::from_secs(20), |open| !*open);
            if wait.unwrap().1.timed_out() {
                return Err("the gate never opened".into());
            }
            Ok(outputs([(
                "image",
                inputs.get("image").cloned().ok_or("no image")?,
            )]))
        }
    })
}

#[test]
fn live_mode_drops_frames_it_can_t_run_in_time() {
    let doors = Arc::new(Gate::default());
    let mut registry = test_nodes::registry();
    registry.register(gate(Arc::clone(&doors))).unwrap();
    let offline = Descriptor::new("test.offline", 1, "test", "Too slow for live mode")
        .input("image", PortType::Image)
        .output("image", PortType::Image)
        .capabilities(Capabilities {
            cpu: true,
            ..Capabilities::default()
        });
    registry
        .register(node(offline, |_: serde_json::Value| never))
        .unwrap();
    let engine = engine_with(registry);
    let plan = engine
        .plan(&recipe(json!({
            "format": 1,
            "inputs": [{ "name": "image", "type": "image" }, { "name": "frame", "type": "json" }],
            "nodes": [{ "id": "gate", "node": "test.gate" }],
            "edges": [{ "from": "input.image", "to": "gate.image" }],
            "outputs": [{ "name": "image", "from": "gate.image" }, { "name": "frame", "from": "input.frame" }]
        })))
        .unwrap();
    let budget = Duration::from_secs(2);
    let (send, results) = std::sync::mpsc::channel();
    let live = Live::start(Arc::clone(&engine), Arc::new(plan), budget, move |result| {
        send.send(result).unwrap();
    })
    .unwrap();
    let image = picture(16, 16, 9);
    let frame = |n: u64| {
        let mut inputs = image_inputs(image.clone());
        inputs.insert("frame".into(), Value::json(json!(n)));
        inputs
    };
    let next = || {
        let result = results
            .recv_timeout(Duration::from_secs(20))
            .expect("a result");
        let run = result.run.expect("the frame ran");
        (run.outputs["frame"].as_json().cloned(), result.latency)
    };
    // Frame 1 waits at the gate, past the budget; frames 2 and 3 are
    // dropped for frame 4 meanwhile.
    let captured = Instant::now();
    live.push(frame(1), captured);
    wait_until("frame 1 at the gate", || {
        doors.entered.load(Ordering::SeqCst) == 1
    });
    wait_until("frame 1 past its budget", || {
        captured.elapsed() > budget + Duration::from_millis(100)
    });
    for n in 2..=4 {
        live.push(frame(n), Instant::now());
    }
    *doors.open.lock().unwrap() = true;
    doors.wake.notify_all();
    let (first, latency) = next();
    assert_eq!(first, Some(json!(1)));
    assert!(latency > budget);
    assert_eq!(next().0, Some(json!(4)));
    // A frame older than the budget when its turn comes is dropped.
    live.push(
        frame(5),
        Instant::now()
            .checked_sub(2 * budget)
            .expect("a clock that has run 4 s"),
    );
    wait_until("frame 5 dropped", || live.stats().late == 1);
    live.push(frame(6), Instant::now());
    assert_eq!(next().0, Some(json!(6)));
    let stats = live.stats();
    assert_eq!(
        (
            stats.delivered,
            stats.superseded,
            stats.late,
            stats.over_budget,
            stats.failed
        ),
        (3, 2, 1, 1, 0)
    );
    // Live mode takes live steps only.
    let slow = engine.plan(&recipe(json!({
        "format": 1,
        "inputs": [{ "name": "image", "type": "image" }],
        "nodes": [{ "id": "o", "node": "test.offline" }, { "id": "b", "node": "test.box_blur", "params": { "radius": 1 } }],
        "edges": [{ "from": "input.image", "to": "o.image" }, { "from": "input.image", "to": "b.image" }],
        "outputs": [{ "name": "o", "from": "o.image" }, { "name": "b", "from": "b.image" }]
    })));
    let refused = Live::start(engine, Arc::new(slow.unwrap()), budget, |_| {}).err();
    assert_eq!(
        refused,
        Some(RunError::Unsuitable("o can't run live".into()))
    );
}

#[test]
fn images_cross_to_the_gpu_and_back_as_seldom_as_they_can() {
    let mut registry = test_nodes::registry();
    let both = Descriptor::new("test.both", 1, "test", "Runs on either")
        .input("image", PortType::Image)
        .output("image", PortType::Image)
        .capabilities(Capabilities {
            cpu: true,
            cuda: true,
            ..Capabilities::default()
        });
    registry
        .register(node(both, |_: serde_json::Value| never))
        .unwrap();
    let plan = |nodes: serde_json::Value,
                edges: serde_json::Value,
                outputs: serde_json::Value,
                gpu: bool| {
        let json = json!({
            "format": 1,
            "inputs": [{ "name": "image", "type": "image" }],
            "nodes": nodes, "edges": edges, "outputs": outputs
        });
        Plan::new(&registry, &recipe(json), gpu).unwrap()
    };
    let devices = |plan: &Plan| -> Vec<Device> {
        plan.order()
            .iter()
            .map(|id| plan.device(id).unwrap())
            .collect()
    };
    let (cpu, cuda) = (Device::Cpu, Device::Cuda);
    // Two GPU steps in a row: up once, back once.
    let chain = plan(
        json!([{ "id": "g1", "node": "test.both" }, { "id": "g2", "node": "test.both" }, { "id": "blur", "node": "test.box_blur", "params": { "radius": 1 } }]),
        json!([{ "from": "input.image", "to": "g1.image" }, { "from": "g1.image", "to": "g2.image" }, { "from": "g2.image", "to": "blur.image" }]),
        json!([{ "name": "out", "from": "blur.image" }]),
        true,
    );
    assert_eq!(
        (devices(&chain), chain.transfers()),
        (vec![cuda, cuda, cpu], 2)
    );
    let shown = chain.to_string();
    let line = shown
        .lines()
        .find(|line| line.trim_start().starts_with("g1 "));
    assert!(
        line.is_some_and(|line| line.contains("test.both v1") && line.contains(" cuda ")),
        "{shown}"
    );
    // A step alone on the GPU stays on the CPU.
    let island = plan(
        json!([{ "id": "b1", "node": "test.box_blur", "params": { "radius": 1 } }, { "id": "g", "node": "test.both" }, { "id": "b2", "node": "test.box_blur", "params": { "radius": 1 } }]),
        json!([{ "from": "input.image", "to": "b1.image" }, { "from": "b1.image", "to": "g.image" }, { "from": "g.image", "to": "b2.image" }]),
        json!([{ "name": "out", "from": "b2.image" }]),
        true,
    );
    assert_eq!(
        (devices(&island), island.transfers()),
        (vec![cpu, cpu, cpu], 0)
    );
    // An image read on both sides crosses once.
    let fan = plan(
        json!([{ "id": "g1", "node": "test.both" }, { "id": "g2", "node": "test.both" }, { "id": "blur", "node": "test.box_blur", "params": { "radius": 1 } }]),
        json!([{ "from": "input.image", "to": "g1.image" }, { "from": "g1.image", "to": "g2.image" }, { "from": "g1.image", "to": "blur.image" }]),
        json!([{ "name": "gpu", "from": "g2.image" }, { "name": "cpu", "from": "blur.image" }]),
        true,
    );
    assert_eq!((devices(&fan), fan.transfers()), (vec![cuda, cuda, cpu], 3));
    // Without a GPU, everything is on the CPU, and such a plan won't run on
    // an engine without one.
    let none = plan(
        json!([{ "id": "g1", "node": "test.both" }]),
        json!([{ "from": "input.image", "to": "g1.image" }]),
        json!([{ "name": "out", "from": "g1.image" }]),
        false,
    );
    assert_eq!((devices(&none), none.transfers()), (vec![cpu], 0));
    let engine = engine_with(Registry::new());
    if !engine.has_gpu() {
        let refused = engine.run(
            &chain,
            &image_inputs(picture(8, 8, 10)),
            &RunOptions::default(),
        );
        let message = "the plan has steps on a GPU, and this engine has none";
        assert_eq!(refused.unwrap_err(), RunError::Unsuitable(message.into()));
    }
}

/// On the owner's machine, with `OSCOPE_CUDA=1`: GPU steps give what the
/// CPU's do, and images cross only at the ends of the GPU's run of steps.
#[cfg(feature = "cuda")]
#[test]
fn gpu_steps_match_the_cpu_s() {
    if std::env::var("OSCOPE_CUDA").as_deref() != Ok("1") {
        eprintln!("skipped: set OSCOPE_CUDA=1 to run it on a GPU");
        return;
    }
    let engine = engine_with(test_nodes::registry());
    assert!(engine.has_gpu(), "OSCOPE_CUDA=1, but no GPU");
    let binned = recipe(json!({
        "format": 1,
        "inputs": [{ "name": "image", "type": "image" }],
        "nodes": [
            { "id": "half", "node": "test.bin2x2" },
            { "id": "quarter", "node": "test.bin2x2" },
            { "id": "stats", "node": "test.statistics" }
        ],
        "edges": [
            { "from": "input.image", "to": "half.image" },
            { "from": "half.image", "to": "quarter.image" },
            { "from": "quarter.image", "to": "stats.image" }
        ],
        "outputs": [{ "name": "image", "from": "quarter.image" }, { "name": "table", "from": "stats.table" }]
    }));
    let on_gpu = engine.plan(&binned).unwrap();
    let on_cpu = Plan::new(engine.registry(), &binned, false).unwrap();
    assert_eq!(on_gpu.device("half"), Some(Device::Cuda));
    assert_eq!(on_gpu.transfers(), 2);
    let inputs = image_inputs(picture(1000, 700, 11));
    let off = RunOptions {
        memoise: false,
        ..RunOptions::default()
    };
    let (gpu, cpu) = (
        engine.run(&on_gpu, &inputs, &off).unwrap(),
        engine.run(&on_cpu, &inputs, &off).unwrap(),
    );
    assert_eq!(gpu.report.transfers, 2);
    let (a, b) = (
        gpu.outputs["image"].as_image().unwrap(),
        cpu.outputs["image"].as_image().unwrap(),
    );
    assert_eq!(a.shape(), b.shape());
    let worst = a
        .data()
        .iter()
        .zip(b.data())
        .map(|(x, y)| (x - y).abs())
        .fold(0.0, f32::max);
    assert!(worst <= 1e-5, "the GPU's differs by {worst}");
}
