//! The device Things over HTTP: driver errors answered with their statuses,
//! the simulator's samples, the stage moving, streaming into acquisition,
//! settings kept across restarts, and the Things' descriptions.

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use oscope_api::devices::{Devices, DevicesConfig};
use oscope_api::{Api, Roots};
use oscope_compute::{Image, Shape};
use oscope_core::{Ms, SystemClock};
use oscope_hal::fake::{FakeCamera, FakeIllumination, FakeObjectiveMount, FakeStage};
use oscope_hal::{Error, LightChannel, LightKind, ObjectiveCatalogue};
use oscope_imaging::color::encode_tiff16;
use serde_json::{Value, json};
use teta_wot::testing::TestClient;

fn roots(folder: &Path) -> Roots {
    let roots = Roots {
        data: folder.join("data"),
        scratch: folder.join("scratch"),
    };
    std::fs::create_dir_all(&roots.data).unwrap();
    roots
}

/// The fakes, for scripting failures.
struct Fakes {
    camera: Arc<FakeCamera>,
    stage: Arc<FakeStage>,
}

/// Devices of fakes, served with their settings in `folder`.
async fn fake_server(folder: &Path) -> (TestClient, Fakes) {
    let clock = Arc::new(SystemClock::new());
    let camera = Arc::new(FakeCamera::new(clock.clone()));
    let stage = Arc::new(FakeStage::new(clock.clone()));
    let lights = Arc::new(FakeIllumination::new(vec![LightChannel {
        name: "transmitted".to_owned(),
        kind: LightKind::Transmitted,
        dimmable: true,
    }]));
    let mount = Arc::new(FakeObjectiveMount::new(ObjectiveCatalogue::mitutoyo()));
    let devices = Devices::new(camera.clone(), stage.clone(), lights, mount, None, clock);
    let roots = roots(folder);
    let api = Api::builder(&roots)
        .settings(roots.data.join("settings"))
        .devices(Arc::new(devices))
        .build()
        .unwrap();
    let client = TestClient::start(api.into_thing_server()).await.unwrap();
    (client, Fakes { camera, stage })
}

/// The simulator, on the scaled-down sensor, its clock at `rate`, served;
/// and its devices.
async fn sim_server(folder: &Path, rate: f64) -> (TestClient, Arc<Devices>) {
    let mut config = DevicesConfig::default();
    config.simulator.sensor = "quarter".to_owned();
    config.simulator.focus = 1_000.0;
    config.simulator.rate = rate;
    let devices = Arc::new(Devices::from_config(&config, folder).await.unwrap());
    let roots = roots(folder);
    let api = Api::builder(&roots)
        .devices(Arc::clone(&devices))
        .build()
        .unwrap();
    (
        TestClient::start(api.into_thing_server()).await.unwrap(),
        devices,
    )
}

/// Invokes `action` with `input`, and waits for its invocation to end:
/// its final JSON.
async fn invoke(client: &TestClient, action: &str, input: Value) -> Value {
    let started = client
        .post_json(&format!("/api/v1/{action}"), Some(&input))
        .await;
    assert_eq!(started.status, 201, "{action}: {}", started.text());
    let href = started.json()["href"].as_str().unwrap().to_owned();
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        let invocation = client.get(&href).await.json();
        if !["pending", "running"].contains(&invocation["status"].as_str().unwrap_or("")) {
            return invocation;
        }
        assert!(
            Instant::now() < deadline,
            "{action} didn't end: {invocation}"
        );
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}

#[tokio::test]
async fn driver_errors_answer_with_their_statuses() {
    // A property write that the camera refuses answers with ADR-0012's
    // status, and the kind as the problem's title.
    let folder = tempfile::tempdir().unwrap();
    let (client, fakes) = fake_server(folder.path()).await;
    let cases = [
        (
            Error::Busy {
                reason: "the camera is streaming".into(),
            },
            409,
            "busy",
        ),
        (
            Error::OutOfRange {
                what: "the exposure".into(),
                value: 0.0,
                min: 0.01,
                max: 1e4,
            },
            400,
            "out of range",
        ),
        (Error::Disconnected, 503, "disconnected"),
        (Error::Timeout { after: Ms(100.0) }, 504, "timeout"),
        (
            Error::Device {
                message: "sensor fault".into(),
            },
            502,
            "device",
        ),
    ];
    for (error, status, title) in cases {
        let message = error.to_string();
        fakes.camera.script().fail_next("configure", error);
        let answer = client
            .put_json("/api/v1/camera/exposure", &json!(5.0))
            .await;
        assert_eq!(answer.status, status, "{title}: {}", answer.text());
        let problem = answer.json();
        assert_eq!(
            (problem["title"].as_str(), problem["status"].as_u64()),
            (Some(title), Some(u64::from(status)))
        );
        assert_eq!(problem["detail"], message.as_str());
    }
    // Other failures keep their 500, unmarked; a good write succeeds.
    assert_eq!(
        client
            .put_json("/api/v1/camera/exposure", &json!(5.0))
            .await
            .status,
        201
    );
    assert_eq!(
        client.get("/api/v1/camera/exposure").await.json(),
        json!(5.0)
    );
    // An action's error ends its invocation, with the kind and status.
    fakes.stage.script().fail_next(
        "move_to",
        Error::Alarm {
            reason: "hard limit".into(),
        },
    );
    let invocation = invoke(
        &client,
        "stage/move_to",
        json!({"x": 1.0, "y": 2.0, "z": 3.0}),
    )
    .await;
    assert_eq!(invocation["status"], "error");
    let error = invocation["error"].to_string();
    assert!(
        error.contains("alarm (409)") && error.contains("hard limit"),
        "{error}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn the_simulation_lists_loads_unloads_and_imports_samples() {
    let folder = tempfile::tempdir().unwrap();
    let (client, _) = sim_server(folder.path(), 1.0).await;
    let samples = client.get("/api/v1/simulation/samples").await.json();
    let samples = samples.as_array().unwrap();
    assert_eq!(samples.len(), 6);
    assert!(samples.iter().all(|s| s["ground_truth"] == true));
    assert_eq!(
        client.get("/api/v1/simulation/loaded").await.json(),
        "H&E tissue"
    );
    let load = invoke(&client, "simulation/load", json!({"name": "test target"})).await;
    assert_eq!(load["status"], "completed", "{load}");
    assert_eq!(
        client.get("/api/v1/simulation/loaded").await.json(),
        "test target"
    );
    let unload = invoke(&client, "simulation/unload", json!({})).await;
    assert_eq!(unload["output"], "test target");
    assert_eq!(
        client.get("/api/v1/simulation/loaded").await.json(),
        Value::Null
    );
    // Your own image, imported, without ground truth; then loaded.
    let image = folder.path().join("mine.tif");
    let pattern = Image::from_vec(Shape::new(40, 30, 3), vec![0.5; 40 * 30 * 3]).unwrap();
    let mut file = Vec::new();
    encode_tiff16(&mut file, &pattern).unwrap();
    std::fs::write(&image, file).unwrap();
    let path = image.display().to_string();
    let import = invoke(
        &client,
        "simulation/import",
        json!({"name": "mine", "path": path, "um_per_pixel": 0.5}),
    )
    .await;
    assert_eq!(import["status"], "completed", "{import}");
    let samples = client.get("/api/v1/simulation/samples").await.json();
    assert!(
        samples
            .as_array()
            .unwrap()
            .contains(&json!({"name": "mine", "kind": "image", "ground_truth": false}))
    );
    assert_eq!(
        invoke(&client, "simulation/load", json!({"name": "mine"})).await["status"],
        "completed"
    );
    // A file that isn't there is refused, and nothing is added.
    let missing = folder.path().join("missing.png").display().to_string();
    let refused = invoke(
        &client,
        "simulation/import",
        json!({"name": "gone", "path": missing, "um_per_pixel": 1.0}),
    )
    .await;
    assert_eq!(refused["status"], "error");
    assert_eq!(
        client
            .get("/api/v1/simulation/samples")
            .await
            .json()
            .as_array()
            .unwrap()
            .len(),
        7
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn the_stage_homes_moves_and_says_so() {
    // The simulator at 50 times real time: home, then a move over the
    // slide, each `moved` event saying where.
    let folder = tempfile::tempdir().unwrap();
    let (client, _) = sim_server(folder.path(), 50.0).await;
    let mut moved = client.events("/api/v1/stage/moved").await;
    let home = invoke(&client, "stage/home", json!({})).await;
    assert_eq!(home["status"], "completed", "{home}");
    let target = json!({"x": 40_000.0, "y": 12_000.0, "z": 1_000.0});
    let travel = invoke(&client, "stage/move_to", target.clone()).await;
    assert_eq!(travel["status"], "completed", "{travel}");
    let at = |sample: &Value| ["x", "y", "z"].map(|a| sample["position"][a].as_f64().unwrap());
    let arrived = at(&travel["output"]);
    for (got, want) in arrived.iter().zip([40_000.0, 12_000.0, 1_000.0]) {
        // Within the simulated stage's 2 µm of backlash.
        assert!((got - want).abs() < 2.5, "{arrived:?}");
    }
    let first = moved.next().await.unwrap();
    let second = moved.next().await.unwrap();
    assert_eq!(at(&second), arrived, "{first} then {second}");
    assert_eq!(client.get("/api/v1/stage/moving").await.json(), false);
    let position = client.get("/api/v1/stage/position").await.json();
    assert!((at(&position)[0] - 40_000.0).abs() < 2.5, "{position}");
}

#[tokio::test(flavor = "multi_thread")]
async fn streaming_publishes_frames_with_position_objective_and_lights() {
    let folder = tempfile::tempdir().unwrap();
    let (client, devices) = sim_server(folder.path(), 1.0).await;
    let fit = invoke(&client, "objective/fit", json!({"name": "20x"})).await;
    assert_eq!(fit["status"], "completed", "{fit}");
    let light = invoke(
        &client,
        "illumination/set_light",
        json!({"channel": "transmitted", "on": true, "intensity": 0.5}),
    )
    .await;
    assert_eq!(light["output"]["intensity"], 0.5);
    let mut frames = devices.acquisition.bus().subscribe_latest("test");
    let start = invoke(&client, "camera/start_streaming", json!({})).await;
    assert_eq!(start["status"], "completed", "{start}");
    let deadline = Instant::now() + Duration::from_secs(60);
    let frame = loop {
        if let Some(frame) = frames.try_recv() {
            break frame;
        }
        assert!(Instant::now() < deadline, "no frame");
        tokio::time::sleep(Duration::from_millis(5)).await;
    };
    assert_eq!(client.get("/api/v1/camera/streaming").await.json(), true);
    invoke(&client, "camera/stop_streaming", json!({})).await;
    let meta = frame.meta();
    assert_eq!(meta.objective.as_deref(), Some("20x"));
    let transmitted = meta
        .lights
        .iter()
        .find(|l| l.channel == "transmitted")
        .unwrap();
    assert!(transmitted.on && transmitted.intensity == 0.5);
    assert!(meta.position.is_some(), "no position");
    assert_eq!((meta.width, meta.height), (1368, 912));
}

#[tokio::test]
async fn settings_are_kept_across_a_restart() {
    // The exposure, the objective and a light, set through one server, are
    // set again by the next, on new drivers.
    let folder = tempfile::tempdir().unwrap();
    {
        let (client, _) = fake_server(folder.path()).await;
        assert_eq!(
            client
                .put_json("/api/v1/camera/exposure", &json!(7.5))
                .await
                .status,
            201
        );
        invoke(&client, "objective/fit", json!({"name": "50x"})).await;
        invoke(
            &client,
            "illumination/set_light",
            json!({"channel": "transmitted", "on": true, "intensity": 0.25}),
        )
        .await;
        client.stop().await;
    }
    let (client, _) = fake_server(folder.path()).await;
    assert_eq!(
        client.get("/api/v1/camera/exposure").await.json(),
        json!(7.5)
    );
    assert_eq!(
        client.get("/api/v1/objective/fitted").await.json()["name"],
        "50x"
    );
    let lights = client.get("/api/v1/illumination/lights").await.json();
    assert_eq!(
        lights[0],
        json!({"channel": "transmitted", "on": true, "intensity": 0.25})
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn the_things_answer_on_a_test_port() {
    // Served on 8221, with plain HTTP/1.1 requests.
    let folder = tempfile::tempdir().unwrap();
    let (_, devices) = sim_server(folder.path(), 1.0).await;
    let roots = roots(folder.path());
    let api = Api::builder(&roots).devices(devices).build().unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:8221")
        .await
        .unwrap();
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    let serving = tokio::spawn(api.serve(listener, async {
        let _ = stopped.await;
    }));
    let (status, body) = http("GET", "/api/v1/stage/capabilities", "").await;
    assert_eq!(status, 200);
    assert_eq!(body["model"], "simulated grblHAL stage");
    let (status, body) = http("PUT", "/api/v1/camera/gain", "999").await;
    assert_eq!(status, 400, "{body}");
    assert_eq!(body["title"], "out of range");
    let (status, body) = http("GET", "/api/v1/objective/fitted", "").await;
    assert_eq!((status, body["name"].as_str()), (200, Some("10x")));
    let _ = stop.send(());
    serving.await.unwrap().unwrap();
}

/// One HTTP/1.1 request to 127.0.0.1:8221: the status, and the body as JSON.
async fn http(method: &str, path: &str, body: &str) -> (u16, Value) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut stream = tokio::net::TcpStream::connect("127.0.0.1:8221")
        .await
        .unwrap();
    let request = format!(
        "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:8221\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut answer = String::new();
    stream.read_to_string(&mut answer).await.unwrap();
    let status = answer[9..12].parse().unwrap();
    let json = answer
        .split_once("\r\n\r\n")
        .and_then(|(_, b)| serde_json::from_str(b).ok())
        .unwrap_or(Value::Null);
    (status, json)
}

#[tokio::test(flavor = "multi_thread")]
async fn the_things_describe_themselves() {
    // Their Thing Descriptions, snapshotted, and the OpenAPI document's
    // routes for them.
    let folder = tempfile::tempdir().unwrap();
    let (_, devices) = sim_server(folder.path(), 1.0).await;
    let api = Api::builder(&roots(folder.path()))
        .devices(devices)
        .build()
        .unwrap();
    let tds = api.thing_descriptions("http://127.0.0.1:8220/").unwrap();
    for name in [
        "camera",
        "stage",
        "illumination",
        "objective",
        "simulation",
        "preview",
        "capture",
        "calibration",
        "autofocus",
        "exposure",
    ] {
        // The ID differs from computer to computer: its form, then the rest.
        let mut td = tds[name].clone();
        let id = td.as_object_mut().unwrap().remove("id").unwrap();
        assert!(id.as_str().unwrap().starts_with("urn:uuid:"), "{id}");
        insta::assert_json_snapshot!(name, td);
    }
    let openapi = api.openapi();
    let paths = openapi["paths"].as_object().unwrap();
    for path in [
        "/api/v1/camera/exposure",
        "/api/v1/stage/move_to",
        "/api/v1/stage/halt",
        "/api/v1/illumination/set_light",
        "/api/v1/objective/fit",
        "/api/v1/simulation/import",
        "/api/v1/preview/defaults",
    ] {
        assert!(
            paths.contains_key(path),
            "{path} isn't in the OpenAPI document"
        );
    }
}

/// Starts a slow move without waiting for it, and waits until the stage
/// moves: the invocation's href.
async fn start_slow_move(client: &TestClient) -> String {
    let input = json!({"x": 40_000.0, "y": 12_000.0, "z": 1_000.0, "speed": 100.0});
    let started = client
        .post_json("/api/v1/stage/move_to", Some(&input))
        .await;
    assert_eq!(started.status, 201);
    let href = started.json()["href"].as_str().unwrap().to_owned();
    let deadline = Instant::now() + Duration::from_secs(30);
    while client.get("/api/v1/stage/moving").await.json() != true {
        assert!(Instant::now() < deadline, "the stage never moved");
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    href
}

/// The invocation at `href`, once it has ended.
async fn ended(client: &TestClient, href: &str) -> Value {
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        let invocation = client.get(href).await.json();
        if !["pending", "running"].contains(&invocation["status"].as_str().unwrap_or("")) {
            return invocation;
        }
        assert!(Instant::now() < deadline, "{invocation}");
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_move_stops_when_halted_or_cancelled() {
    // At 50 times real time, a move at 0.1 mm/s across 40 mm would take
    // 8 s: halted, it ends at once, `stopped`; cancelled, the stage stops.
    let folder = tempfile::tempdir().unwrap();
    let (client, _) = sim_server(folder.path(), 50.0).await;
    assert_eq!(
        invoke(&client, "stage/home", json!({})).await["status"],
        "completed"
    );
    let href = start_slow_move(&client).await;
    let halted = Instant::now();
    assert_eq!(
        invoke(&client, "stage/halt", json!({})).await["status"],
        "completed"
    );
    assert!(
        halted.elapsed() < Duration::from_secs(4),
        "halt waited for the move"
    );
    let travel = ended(&client, &href).await;
    assert_eq!(travel["status"], "error", "{travel}");
    assert!(
        travel["error"].to_string().contains("stopped (409)"),
        "{travel}"
    );
    let href = start_slow_move(&client).await;
    assert!(client.delete(&href).await.status.is_success());
    assert_eq!(ended(&client, &href).await["status"], "cancelled");
    // It stops at once, far short of 40 mm, which the move would reach in
    // seconds more.
    let deadline = Instant::now() + Duration::from_secs(2);
    while client.get("/api/v1/stage/moving").await.json() != false {
        assert!(Instant::now() < deadline, "the stage kept moving");
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    let x = client.get("/api/v1/stage/position").await.json()["position"]["x"]
        .as_f64()
        .unwrap();
    assert!(x < 35_000.0, "the stage went on to x = {x}");
}

/// Waits until the stage is moving, or isn't: up to 30 s.
async fn until_moving(client: &TestClient, moving: bool) {
    let deadline = Instant::now() + Duration::from_secs(30);
    while client.get("/api/v1/stage/moving").await.json() != moving {
        assert!(
            Instant::now() < deadline,
            "the stage never became moving: {moving}"
        );
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn halts_and_jogs_say_where_the_stage_came_to_rest() {
    // In real time: a homing halted on the way, a jog halted, and a jog
    // that reached its soft limit by itself before its halt each say where
    // the stage came to rest; a halt while it is still says nothing.
    let folder = tempfile::tempdir().unwrap();
    let (client, _) = sim_server(folder.path(), 1.0).await;
    let mut moved = client.events("/api/v1/stage/moved").await;
    let mut next = async || {
        moved
            .next_within(Duration::from_secs(30))
            .await
            .expect("a moved event")
    };
    let x = |sample: &Value| sample["position"]["x"].as_f64().unwrap();

    let started = client
        .post_json("/api/v1/stage/home", Some(&json!({})))
        .await;
    let homing = started.json()["href"].as_str().unwrap().to_owned();
    until_moving(&client, true).await;
    invoke(&client, "stage/halt", json!({})).await;
    let ended = ended(&client, &homing).await;
    assert!(
        ended["error"].to_string().contains("stopped (409)"),
        "{ended}"
    );
    let halted = next().await;
    let position = client.get("/api/v1/stage/position").await.json();
    assert_eq!(x(&halted), x(&position), "where the homing stopped");

    assert_eq!(
        invoke(&client, "stage/home", json!({})).await["status"],
        "completed"
    );
    let home = x(&next().await);
    let jog = json!({"axis": "x", "towards_max": true, "speed": 2000.0});
    assert_eq!(
        invoke(&client, "stage/jog", jog).await["status"],
        "completed"
    );
    // Halted once it has gone a tenth of a millimetre.
    let deadline = Instant::now() + Duration::from_secs(30);
    while x(&client.get("/api/v1/stage/position").await.json()) < home + 100.0 {
        assert!(Instant::now() < deadline, "the jog never went 100 µm");
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    invoke(&client, "stage/halt", json!({})).await;
    let jogged = x(&next().await);
    assert!(
        jogged > home,
        "a jog towards x's maximum, from {home} to {jogged}"
    );

    // Back towards x's minimum, at 10 mm/s, until the soft limit stops it.
    let jog = json!({"axis": "x", "towards_max": false});
    assert_eq!(
        invoke(&client, "stage/jog", jog).await["status"],
        "completed"
    );
    until_moving(&client, true).await;
    until_moving(&client, false).await;
    invoke(&client, "stage/halt", json!({})).await;
    let limit = x(&next().await);
    assert!(
        limit < jogged && limit.abs() < 2.5,
        "at the soft limit, within its backlash: {limit}"
    );

    // Still, a halt says nothing: the next event is the move's.
    invoke(&client, "stage/halt", json!({})).await;
    let target = json!({"x": 3_000.0, "y": 2_000.0, "z": 1_000.0});
    assert_eq!(
        invoke(&client, "stage/move_to", target).await["status"],
        "completed"
    );
    assert!(
        (x(&next().await) - 3_000.0).abs() < 2.5,
        "the move's arrival, and nothing before it"
    );
}
