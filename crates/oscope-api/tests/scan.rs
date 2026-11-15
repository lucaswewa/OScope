//! The `scan` Thing over HTTP (P36; ADR-0025): its patterns; a plan
//! previewed with its estimate, nothing moving; a snake run as a job into a
//! project, its frames with their positions, its progress; requests that
//! can't be, refused; an overview, binned, finding the tissue that the
//! `tissue` pattern then plans over (P37); and, with `OSCOPE_PYTHON=1`, the
//! sample Python workflow, `example.spiral`, listed with its schema,
//! planned and run.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use oscope_api::devices::{Devices, DevicesConfig};
use oscope_api::{Api, Roots};
use oscope_ext::{Extensions, PythonRuntime};
use oscope_jobs::{JobService, JobsConfig};
use oscope_sim::{HeTissue, Slide};
use oscope_store::{Scratch, ScratchPolicy, Store};
use serde_json::{Value, json};
use teta_wot::testing::TestClient;

/// The simulator ten times as fast as real time, on the quarter sensor;
/// the projects, scratch space and jobs; and `extensions`, if any.
async fn server(folder: &Path, extensions: Option<Extensions>) -> TestClient {
    let mut config = DevicesConfig::default();
    let sim = &mut config.simulator;
    (sim.sensor, sim.focus, sim.rate) = ("quarter".to_owned(), 1_000.0, 10.0);
    let devices = Arc::new(Devices::from_config(&config, folder).await.unwrap());
    let roots = Roots {
        data: folder.join("data"),
        scratch: folder.join("scratch"),
    };
    std::fs::create_dir_all(&roots.data).unwrap();
    let store = Store::open(&roots.data, &roots.data.join("index.sqlite")).unwrap();
    let policy = ScratchPolicy {
        reserve_bytes: 0,
        low_bytes: 0,
    };
    let jobs = JobService::open(&folder.join("jobs.sqlite"), JobsConfig::default()).unwrap();
    let mut api = Api::builder(&roots)
        .devices(devices)
        .projects(store)
        .scratch(Scratch::open(&roots.scratch, policy).unwrap())
        .jobs(jobs);
    if let Some(extensions) = extensions {
        api = api.extensions(extensions);
    }
    TestClient::start(api.build().unwrap().into_thing_server())
        .await
        .unwrap()
}

/// Invokes `action` with `input`, and waits up to 300 s for it to end.
async fn invoke(client: &TestClient, action: &str, input: Value) -> Value {
    let started = client
        .post_json(&format!("/api/v1/{action}"), Some(&input))
        .await;
    assert_eq!(started.status, 201, "{action}: {}", started.text());
    let href = started.json()["href"].as_str().unwrap().to_owned();
    let deadline = Instant::now() + Duration::from_secs(300);
    loop {
        let invocation = client.get(&href).await.json();
        if !["pending", "running"].contains(&invocation["status"].as_str().unwrap_or("")) {
            return invocation;
        }
        assert!(Instant::now() < deadline, "{action}: {invocation}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// The output of `action` with `input`, which must complete.
async fn output(client: &TestClient, action: &str, input: Value) -> Value {
    let done = invoke(client, action, input).await;
    assert_eq!(done["status"], "completed", "{action}: {done}");
    done["output"].clone()
}

/// Waits up to 300 s for the job `href` to end: its JSON.
async fn ended(client: &TestClient, href: &str) -> Value {
    let deadline = Instant::now() + Duration::from_secs(300);
    loop {
        let job = client.get(href).await.json();
        if ["completed", "failed", "cancelled"].contains(&job["state"].as_str().unwrap_or("")) {
            return job;
        }
        assert!(Instant::now() < deadline, "{job}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// Homes the stage, moves over the first island at its focus, and streams
/// through a small region: where a scan starts. The project to scan into.
async fn ready(client: &TestClient) -> String {
    let roi = json!({"x": 556, "y": 360, "width": 256, "height": 192});
    assert!(
        client
            .put_json("/api/v1/camera/roi", &roi)
            .await
            .status
            .is_success()
    );
    output(client, "stage/home", json!({})).await;
    let island = HeTissue::new(1, Slide::COVERSLIP).islands()[0];
    let at = json!({"x": island.x, "y": island.y});
    let truth = output(client, "simulation/in_focus", at)
        .await
        .as_f64()
        .unwrap();
    let over = json!({"x": island.x, "y": island.y, "z": truth});
    output(client, "stage/move_to", over).await;
    output(client, "camera/start_streaming", json!({})).await;
    let made = output(client, "projects/create", json!({"name": "Scans"})).await;
    made["id"].as_str().unwrap().to_owned()
}

/// A square of `side` µm at the first island.
fn square(side: f64) -> Value {
    let island = HeTissue::new(1, Slide::COVERSLIP).islands()[0];
    let (x, y) = (island.x - side / 2.0, island.y - side / 2.0);
    json!({"rect": {"origin": {"x": x, "y": y}, "size": {"width": side, "height": side}}})
}

#[tokio::test(flavor = "multi_thread")]
async fn a_scan_is_planned_previewed_and_run_into_a_project() {
    let folder = tempfile::tempdir().unwrap();
    let client = server(folder.path(), None).await;
    let project = ready(&client).await;
    let patterns = client.get("/api/v1/scan/patterns").await.json();
    let names: Vec<&str> = patterns
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        [
            "raster",
            "snake",
            "tiled_stack",
            "z_scan",
            "positions",
            "tissue"
        ]
    );
    assert_eq!(
        client.get("/api/v1/scan/progress").await.json(),
        Value::Null
    );

    // A preview: 3 × 3 tiles of the region's frames, 252 × 192 pixels (its
    // width kept a multiple of 12, for the mosaic's pattern) of 0.96 µm
    // through the 10×, by the nominal pixel; their data and time.
    let request = json!({"pattern": "snake", "region": square(500.0)});
    let preview = output(&client, "scan/plan", json!({"request": request})).await;
    let tiles = preview["plan"]["tiles"].as_array().unwrap();
    assert_eq!(tiles.len(), 9, "{preview}");
    let cell = preview["plan"]["field"]["cell_um"]["width"]
        .as_f64()
        .unwrap();
    assert!((cell - 252.0 * 0.96).abs() < 1e-6, "{cell}");
    let estimate = &preview["estimate"];
    assert_eq!(estimate["frames"], 9);
    assert_eq!(estimate["bytes"], 9 * 252 * 192 * 2);
    // Each frame takes its settling, 50 ms, and at least the camera's
    // interval: the MIChrome 20's 7.24 frames a second at full resolution in
    // 16 bits, which the simulated camera keeps, its exposure shorter.
    let seconds = estimate["seconds"].as_f64().unwrap();
    assert!(seconds > 9.0 * (0.05 + 1.0 / 7.24), "{seconds}");
    // At the stage's z now, as no z was asked.
    let here = client.get("/api/v1/stage/position").await.json()["position"]["z"]
        .as_f64()
        .unwrap();
    assert!(tiles.iter().all(|t| t["z_um"].as_f64() == Some(here)));

    // Run: one acquisition of the nine frames, each where it was taken.
    let input = json!({"request": request, "project": project, "name": "nine tiles"});
    let job = output(&client, "scan/run", input).await;
    let job = ended(&client, job["href"].as_str().unwrap()).await;
    assert_eq!(job["state"], "completed", "{job}");
    let contents = client
        .get(&format!("/api/v1/projects/project/{project}"))
        .await
        .json();
    let acquisition = &contents["acquisitions"][0];
    assert_eq!(acquisition["name"], "nine tiles", "{contents}");
    let frames = acquisition["frames"].as_array().unwrap();
    assert_eq!(frames.len(), 9);
    for (frame, tile) in frames.iter().zip(tiles) {
        let at = &frame["position"];
        let dx = at["x"].as_f64().unwrap() - tile["x_um"].as_f64().unwrap();
        let dy = at["y"].as_f64().unwrap() - tile["y_um"].as_f64().unwrap();
        assert!(dx.abs() < 2.5 && dy.abs() < 2.5, "{frame} for {tile}");
    }
    assert_eq!(acquisition["plan"]["pattern"], "snake");
    let progress = client.get("/api/v1/scan/progress").await.json();
    assert_eq!(progress["state"], "completed");
    assert_eq!(progress["done"].as_array().unwrap().len(), 9);

    // What can't be is refused as it's asked, saying why.
    for (request, why) in [
        (json!({"pattern": "spiral"}), "no pattern spiral"),
        (json!({"pattern": "raster"}), "needs a region"),
        (
            json!({"pattern": "z_scan", "at": {"x": 0.0, "y": 0.0}}),
            "needs a stack",
        ),
        (
            json!({"pattern": "snake", "region": square(500.0), "overlap": 0.95}),
            "overlap",
        ),
    ] {
        let refused = invoke(&client, "scan/plan", json!({"request": request})).await;
        assert_eq!(refused["status"], "error", "{refused}");
        let detail = refused["error"]["detail"].as_str().unwrap_or("");
        let labelled = detail.starts_with("refused (400)");
        assert!(labelled && detail.contains(why), "{why}: {refused}");
    }
    let stopped = output(&client, "camera/stop_streaming", json!({})).await;
    drop(stopped);
}

/// Whether to run: `OSCOPE_PYTHON=1`, with uv installed (CLAUDE.md).
fn python() -> bool {
    let on = std::env::var("OSCOPE_PYTHON").is_ok_and(|value| value == "1");
    if !on {
        eprintln!("skipped: set OSCOPE_PYTHON=1, with uv installed, to run it");
    }
    on
}

/// The sample workflow, `examples/ext-python-workflow`, packed and
/// installed in `folder`'s extensions.
fn spiral(folder: &Path) -> Extensions {
    let source =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/ext-python-workflow");
    let package = folder.join("spiral.omext");
    oscope_ext::pack(&source, &package).unwrap();
    let root = folder.join("extensions");
    let runtime = PythonRuntime::default();
    let extensions = Extensions::open_with(&root, Some(runtime.clone())).unwrap();
    extensions.install(&package).unwrap();
    Extensions::open_with(&root, Some(runtime)).unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_python_workflow_plans_a_spiral_that_the_engine_runs() {
    if !python() {
        return;
    }
    let folder = tempfile::tempdir().unwrap();
    let extensions = spiral(folder.path());
    let client = server(folder.path(), Some(extensions)).await;
    let project = ready(&client).await;
    let patterns = client.get("/api/v1/scan/patterns").await.json();
    let spiral = patterns
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == "example.spiral")
        .unwrap_or_else(|| panic!("{patterns}"));
    assert_eq!(spiral["kind"], "workflow");
    assert_eq!(
        spiral["schema"]["required"],
        json!(["x_um", "y_um", "rings"])
    );

    // A ring about the island's middle: 9 tiles, a step apart, from the
    // middle round.
    let island = HeTissue::new(1, Slide::COVERSLIP).islands()[0];
    let params = json!({"x_um": island.x, "y_um": island.y, "rings": 1});
    let request = json!({"pattern": "example.spiral", "params": params});
    let preview = output(&client, "scan/plan", json!({"request": request})).await;
    let plan = &preview["plan"];
    assert_eq!(plan["pattern"], "workflow");
    let tiles = plan["tiles"].as_array().unwrap();
    assert_eq!(tiles.len(), 9);
    assert_eq!(
        (tiles[0]["x_um"].as_f64(), tiles[0]["name"].as_str()),
        (Some(island.x), Some("ring 0"))
    );
    let step = plan["field"]["step_um"]["width"].as_f64().unwrap();
    let first = tiles[1]["x_um"].as_f64().unwrap() - island.x;
    assert!((first + step).abs() < 1e-6, "{first} and {step}");

    let input = json!({"request": request, "project": project});
    let job = output(&client, "scan/run", input).await;
    let job = ended(&client, job["href"].as_str().unwrap()).await;
    assert_eq!(job["state"], "completed", "{job}");
    let contents = client
        .get(&format!("/api/v1/projects/project/{project}"))
        .await
        .json();
    let acquisition = &contents["acquisitions"][0];
    assert_eq!(acquisition["frames"].as_array().unwrap().len(), 9);
    assert_eq!(acquisition["plan"]["pattern"], "workflow");

    // Parameters it can't take, it refuses, saying why.
    let wrong = json!({"pattern": "example.spiral", "params": {"rings": 60, "x_um": 0, "y_um": 0}});
    let refused = invoke(&client, "scan/plan", json!({"request": wrong})).await;
    assert!(
        refused["error"].to_string().contains("60 rings"),
        "{refused}"
    );
    output(&client, "camera/stop_streaming", json!({})).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn an_overview_finds_the_tissue_the_tissue_pattern_plans_over() {
    let folder = tempfile::tempdir().unwrap();
    let client = server(folder.path(), None).await;
    let request = json!({"request": {"pattern": "tissue"}});
    let early = invoke(&client, "scan/plan", request.clone()).await;
    let detail = early["error"]["detail"].as_str().unwrap_or("");
    assert!(detail.contains("no overview yet"), "{early}");

    // Binned 3 × 3, the whole quarter sensor, over the first island at its
    // focus; the exposure set for the binned mode.
    let format = client.get("/api/v1/camera/format").await.json();
    output(
        &client,
        "camera/set_mode",
        json!({"mode": "bin3", "format": format}),
    )
    .await;
    output(&client, "stage/home", json!({})).await;
    let island = HeTissue::new(1, Slide::COVERSLIP).islands()[0];
    let at = json!({"x": island.x, "y": island.y});
    let truth = output(&client, "simulation/in_focus", at)
        .await
        .as_f64()
        .unwrap();
    output(
        &client,
        "stage/move_to",
        json!({"x": island.x, "y": island.y, "z": truth}),
    )
    .await;
    output(&client, "camera/start_streaming", json!({})).await;
    let exposed = output(&client, "exposure/auto", json!({})).await;
    let exposed = ended(&client, exposed["href"].as_str().unwrap()).await;
    assert_eq!(exposed["state"], "completed", "{exposed}");

    // An overview of 12 × 12 mm about the island, glass about the tissue,
    // in pixels of 50 µm.
    let input = json!({"region": square(12_000.0)});
    let job = output(&client, "scan/overview", input).await;
    let job = ended(&client, job["href"].as_str().unwrap()).await;
    assert_eq!(job["state"], "completed", "{job}");
    let view = client.get("/api/v1/scan/last_overview").await.json();
    assert_eq!(
        (view["width"].as_u64(), view["height"].as_u64()),
        (Some(240), Some(240))
    );
    assert_eq!(view["image"].as_str().unwrap().len(), 240 * 240 * 4);
    assert!(view["kept"].as_u64().unwrap() >= 1, "{}", view["kept"]);
    assert!(!view["tissue"].as_array().unwrap().is_empty());

    // The tissue's tiles: fewer than a raster over the region takes.
    let tissue = output(&client, "scan/plan", request).await;
    let raster = json!({"request": {"pattern": "raster", "region": square(12_000.0)}});
    let raster = output(&client, "scan/plan", raster).await;
    let count = |preview: &Value| preview["plan"]["tiles"].as_array().unwrap().len();
    assert!(
        count(&tissue) > 0 && count(&tissue) < count(&raster),
        "{} and {}",
        count(&tissue),
        count(&raster)
    );
    // Its z: a map measured over it, before the first tile.
    assert_eq!(tissue["plan"]["z"]["kind"], "measure");
    output(&client, "camera/stop_streaming", json!({})).await;
}
