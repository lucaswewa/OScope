//! The `calibration` Thing over HTTP (P32): its jobs on the simulator, with
//! a turned camera, objectives magnifying 2 % less than the catalogue says,
//! and 3 µm of backlash; the sets listed, active, exported, imported and
//! activated; the objective check; and what is refused. The wizard (P33),
//! through every step, and through a wrong magnification.

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use oscope_api::devices::{Devices, DevicesConfig};
use oscope_api::{Api, Roots};
use oscope_jobs::{JobService, JobsConfig};
use oscope_sim::{HeTissue, Slide};
use serde_json::{Value, json};
use teta_wot::testing::TestClient;
use tokio::sync::Mutex;

/// One server at a time: on two cores, three simulators streaming at once
/// starve each other's frames.
static TURN: Mutex<()> = Mutex::const_new(());

/// The simulator ten times as fast as real time, on the quarter sensor
/// (9.6 µm pixels), with the truth calibration recovers.
async fn server(folder: &Path) -> TestClient {
    server_scaled(folder, 0.98).await
}

/// `server`, its objectives magnifying `scale` times the catalogue's.
async fn server_scaled(folder: &Path, scale: f64) -> TestClient {
    let mut config = DevicesConfig::default();
    let sim = &mut config.simulator;
    (sim.sensor, sim.focus, sim.rate) = ("quarter".to_owned(), 1_000.0, 10.0);
    (sim.rotation, sim.scale, sim.backlash) = (-20.0, scale, 3.0);
    // Vignetting, and a sensor whose colours cross, for photometry (P33).
    sim.vignetting = 0.2;
    sim.response = [[0.85, 0.2, 0.05], [0.1, 0.8, 0.1], [0.02, 0.18, 0.6]];
    let devices = Arc::new(Devices::from_config(&config, folder).await.unwrap());
    let roots = Roots {
        data: folder.join("data"),
        scratch: folder.join("scratch"),
    };
    std::fs::create_dir_all(&roots.data).unwrap();
    let jobs = JobService::open(&folder.join("jobs.sqlite"), JobsConfig::default()).unwrap();
    let api = Api::builder(&roots)
        .devices(devices)
        .jobs(jobs)
        .build()
        .unwrap();
    TestClient::start(api.into_thing_server()).await.unwrap()
}

/// Stops the camera, its last frame rendered, before the next test's turn:
/// on two cores, a frame after homing takes the simulator tens of seconds.
async fn stop_streaming(client: &TestClient) {
    let stopped = invoke(client, "camera/stop_streaming", json!({})).await;
    assert_eq!(stopped["status"], "completed", "{stopped}");
}

/// Invokes `action` with `input`, and waits for its invocation to end.
async fn invoke(client: &TestClient, action: &str, input: Value) -> Value {
    let started = client
        .post_json(&format!("/api/v1/{action}"), Some(&input))
        .await;
    assert_eq!(started.status, 201, "{action}: {}", started.text());
    let href = started.json()["href"].as_str().unwrap().to_owned();
    // Up to 300 s: on two cores, stopping the camera after homing waits
    // more than a minute for the simulator's last frame.
    let deadline = Instant::now() + Duration::from_secs(300);
    loop {
        let invocation = client.get(&href).await.json();
        if !["pending", "running"].contains(&invocation["status"].as_str().unwrap_or("")) {
            return invocation;
        }
        assert!(Instant::now() < deadline, "{action}: {invocation}");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

/// Starts the job `action` asks for, and waits for it to end: its JSON.
async fn job(client: &TestClient, action: &str, input: Value) -> Value {
    let submitted = invoke(client, action, input).await;
    assert_eq!(submitted["status"], "completed", "{submitted}");
    let href = submitted["output"]["href"].as_str().unwrap().to_owned();
    let deadline = Instant::now() + Duration::from_secs(120);
    loop {
        let job = client.get(&href).await.json();
        if ["completed", "failed", "cancelled"].contains(&job["state"].as_str().unwrap_or("")) {
            return job;
        }
        assert!(Instant::now() < deadline, "{job}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// The set a job made, from its result.
fn made(job: &Value) -> String {
    assert_eq!(job["state"], "completed", "{job}");
    let result = job["results"][0].as_str().unwrap();
    result.strip_prefix("calibration/").unwrap().to_owned()
}

#[tokio::test(flavor = "multi_thread")]
async fn the_jobs_measure_and_the_sets_are_kept_listed_and_moved() {
    let _turn = TURN.lock().await;
    let folder = tempfile::tempdir().unwrap();
    let client = server(folder.path()).await;
    // Homed, over the tissue's first island, its section's middle in focus,
    // and streaming.
    assert_eq!(
        invoke(&client, "stage/home", json!({})).await["status"],
        "completed"
    );
    let island = HeTissue::new(1, Slide::COVERSLIP).islands()[0];
    let over = json!({"x": island.x, "y": island.y, "z": 1_002.0});
    assert_eq!(
        invoke(&client, "stage/move_to", over).await["status"],
        "completed"
    );
    let streaming = invoke(&client, "camera/start_streaming", json!({})).await;
    assert_eq!(streaming["status"], "completed", "{streaming}");

    // Before any calibration: nothing active, nothing checked.
    let active = client.get("/api/v1/calibration/active").await.json();
    let none = json!({"geometry": null, "stage": null, "dark": null, "flat": null, "colour": null});
    assert_eq!(active, none);
    assert_eq!(
        client.get("/api/v1/calibration/last_check").await.json(),
        Value::Null
    );

    let geometry = made(&job(&client, "calibration/geometry", json!({})).await);
    let active = client.get("/api/v1/calibration/active").await.json();
    let values = &active["geometry"]["values"];
    assert_eq!(active["geometry"]["id"], geometry.as_str());
    assert_eq!(
        (values["kind"].as_str(), values["objective"].as_str()),
        (Some("geometry"), Some("10x"))
    );
    let pixel = values["pixel_um"].as_f64().unwrap();
    assert!(
        (pixel / (9.6 / (10.0 * 0.98)) - 1.0).abs() < 0.002,
        "{values}"
    );
    assert!(
        (values["rotation_deg"].as_f64().unwrap() + 20.0).abs() < 0.1,
        "{values}"
    );
    assert_eq!(values["mirrored"], false);
    // Each move's residual, as the fit left it (W8.2): a fraction of a pixel.
    let residuals = values["residuals"].as_array().unwrap();
    assert!(residuals.len() >= 4, "{values}");
    for residual in residuals {
        let (x, y) = (residual[0].as_f64().unwrap(), residual[1].as_f64().unwrap());
        assert!(x.hypot(y) < 0.5, "{residual}");
    }

    let check = job(&client, "calibration/check_objective", json!({})).await;
    assert_eq!(check["state"], "completed", "{check}");
    let last = client.get("/api/v1/calibration/last_check").await.json();
    assert_eq!(
        (last["plausible"].as_bool(), last["calibrated"].as_bool()),
        (Some(true), Some(true))
    );

    let stage = made(&job(&client, "calibration/stage", json!({"cycles": 1})).await);
    let active = client.get("/api/v1/calibration/active").await.json();
    assert_eq!(active["stage"]["id"], stage.as_str());
    let backlash = &active["stage"]["values"]["backlash_um"];
    for axis in ["x", "y"] {
        let found = backlash[axis].as_f64().unwrap();
        assert!((found - 3.0).abs() < 0.2, "{axis}: {backlash}");
    }

    // Exported, imported as the next version, not active; then activated.
    let exported = invoke(&client, "calibration/export", json!({"id": geometry})).await;
    let imported = invoke(
        &client,
        "calibration/import",
        json!({"set": exported["output"]}),
    )
    .await;
    let imported = &imported["output"];
    assert_eq!(
        (
            imported["version"].as_u64(),
            imported["imported_from"].as_str()
        ),
        (Some(2), Some(geometry.as_str()))
    );
    let listed = client.get("/api/v1/calibration/sets").await.json();
    let flags: Vec<(String, bool)> = listed
        .as_array()
        .unwrap()
        .iter()
        .map(|s| {
            (
                s["id"].as_str().unwrap().to_owned(),
                s["active"].as_bool().unwrap(),
            )
        })
        .collect();
    let new = imported["id"].as_str().unwrap().to_owned();
    // Newest first, by when each was made: the import when it was made where
    // it came from, just after the set it copies.
    assert_eq!(
        flags,
        [
            (stage.clone(), true),
            (new.clone(), false),
            (geometry, true)
        ]
    );
    let activated = invoke(&client, "calibration/activate", json!({"id": new})).await;
    assert_eq!(activated["status"], "completed", "{activated}");
    let active = client.get("/api/v1/calibration/active").await.json();
    assert_eq!(active["geometry"]["id"], new.as_str());

    // Refused: a set that isn't there, and one that isn't a set.
    let nil = "00000000-0000-0000-0000-000000000000";
    let missing = invoke(&client, "calibration/activate", json!({"id": nil})).await;
    assert!(
        missing["error"].to_string().contains("not found (404)"),
        "{missing}"
    );
    let broken = invoke(&client, "calibration/import", json!({"set": {"format": 1}})).await;
    assert!(
        broken["error"].to_string().contains("refused (400)"),
        "{broken}"
    );
    // Binned, the camera has no geometry in use until its binning's is
    // calibrated.
    let stopped = invoke(&client, "camera/stop_streaming", json!({})).await;
    assert_eq!(stopped["status"], "completed", "{stopped}");
    let format = client.get("/api/v1/camera/format").await.json();
    let binned = json!({"mode": "bin2", "format": format});
    assert_eq!(
        invoke(&client, "camera/set_mode", binned).await["status"],
        "completed"
    );
    let active = client.get("/api/v1/calibration/active").await.json();
    assert_eq!(active["geometry"], Value::Null, "{active}");
    assert_eq!(active["stage"]["id"], stage.as_str());
    let full = json!({"mode": "full", "format": format});
    assert_eq!(
        invoke(&client, "camera/set_mode", full).await["status"],
        "completed"
    );
    let active = client.get("/api/v1/calibration/active").await.json();
    assert_eq!(active["geometry"]["id"], new.as_str());
    // Parameters that can't be are refused as the job is submitted.
    for wrong in [json!({"step_um": -5.0}), json!({"settle_ms": -1.0})] {
        let refused = invoke(&client, "calibration/geometry", wrong).await;
        assert!(
            refused["error"].to_string().contains("refused (400)"),
            "{refused}"
        );
    }
    let none = invoke(&client, "calibration/stage", json!({"cycles": 0})).await;
    assert!(none["error"].to_string().contains("no cycles"), "{none}");
    stop_streaming(&client).await;
}

/// Waits up to 120 s for the wizard's step `index` to stop running: the
/// wizard then.
async fn settled(client: &TestClient, index: usize) -> Value {
    let deadline = Instant::now() + Duration::from_secs(120);
    loop {
        let wizard = client.get("/api/v1/calibration/wizard").await.json();
        if wizard["steps"][index]["state"]["state"] != "running" {
            return wizard;
        }
        assert!(Instant::now() < deadline, "{wizard}");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// Confirms the wizard's step `index`, and waits for it to settle: its
/// state.
async fn step(client: &TestClient, index: usize) -> Value {
    let confirmed = invoke(client, "calibration/wizard_confirm", json!({})).await;
    assert_eq!(confirmed["status"], "completed", "{confirmed}");
    settled(client, index).await["steps"][index]["state"].clone()
}

#[tokio::test(flavor = "multi_thread")]
async fn the_wizard_walks_every_step_and_a_failed_one_is_retried() {
    let _turn = TURN.lock().await;
    let folder = tempfile::tempdir().unwrap();
    // A saved wizard that can't be read is set aside, and the server starts.
    let saved = folder.path().join("data").join("calibration");
    std::fs::create_dir_all(&saved).unwrap();
    std::fs::write(saved.join("wizard.json"), "not a wizard").unwrap();
    let client = server(folder.path()).await;
    assert_eq!(
        invoke(&client, "stage/home", json!({})).await["status"],
        "completed"
    );
    let island = HeTissue::new(1, Slide::COVERSLIP).islands()[0];
    let over = json!({"x": island.x, "y": island.y, "z": 1_002.0});
    assert_eq!(
        invoke(&client, "stage/move_to", over).await["status"],
        "completed"
    );
    let streaming = invoke(&client, "camera/start_streaming", json!({})).await;
    assert_eq!(streaming["status"], "completed", "{streaming}");
    assert_eq!(
        client.get("/api/v1/calibration/wizard").await.json(),
        Value::Null
    );
    let skipped = invoke(&client, "calibration/wizard_skip", json!({})).await;
    assert!(
        skipped["error"].to_string().contains("not found (404)"),
        "{skipped}"
    );

    let started = invoke(&client, "calibration/wizard_start", json!({})).await;
    let wizard = &started["output"];
    let kinds: Vec<&str> = wizard["steps"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["kind"].as_str().unwrap())
        .collect();
    assert_eq!(
        kinds,
        ["objective", "geometry", "dark", "flat", "colour", "stage"]
    );
    assert_eq!(wizard["steps"][0]["state"]["state"], "prompting");
    assert!(
        wizard["steps"][0]["prompt"]
            .as_str()
            .unwrap()
            .contains("Fit the objective")
    );
    // Over the tissue: the objective, the geometry, the dark frame.
    for index in 0..3 {
        let state = step(&client, index).await;
        assert_eq!(state["state"], "done", "step {index}: {state}");
    }
    // The flat field over the tissue fails: it isn't blank.
    let state = step(&client, 3).await;
    assert_eq!(state["state"], "failed", "{state}");
    assert!(
        state["error"].as_str().unwrap().contains("isn't blank"),
        "{state}"
    );
    // Retried over clear glass, beside the coverslip, it passes.
    let blank = json!({"x": 10_000.0, "y": 12_500.0, "z": 1_002.0});
    assert_eq!(
        invoke(&client, "stage/move_to", blank).await["status"],
        "completed"
    );
    let retried = invoke(&client, "calibration/wizard_retry", json!({})).await;
    assert_eq!(
        retried["output"]["steps"][3]["state"]["state"], "prompting",
        "{retried}"
    );
    assert_eq!(step(&client, 3).await["state"], "done");
    // The colour target, loaded and centred.
    let loaded = invoke(&client, "simulation/load", json!({"name": "colour target"})).await;
    assert_eq!(loaded["status"], "completed", "{loaded}");
    let target = json!({"x": 47_000.0, "y": 12_500.0, "z": 1_000.05});
    assert_eq!(
        invoke(&client, "stage/move_to", target).await["status"],
        "completed"
    );
    let state = step(&client, 4).await;
    assert_eq!(state["state"], "done", "{state}");
    // The stage, last, back over the tissue: moved there over the target's
    // glass, whose frames are quick to render, then the slide loaded.
    let over = json!({"x": island.x, "y": island.y, "z": 1_002.0});
    let moved = invoke(&client, "stage/move_to", over).await;
    assert_eq!(moved["status"], "completed", "{moved}");
    let loaded = invoke(&client, "simulation/load", json!({"name": "H&E tissue"})).await;
    assert_eq!(loaded["status"], "completed", "{loaded}");
    let state = step(&client, 5).await;
    assert_eq!(state["state"], "done", "{state}");
    let wizard = client.get("/api/v1/calibration/wizard").await.json();
    assert_eq!(
        (wizard["finished"].as_bool(), wizard["cancelled"].as_bool()),
        (Some(true), Some(false))
    );
    // Every set the wizard made is in use.
    let active = client.get("/api/v1/calibration/active").await.json();
    for kind in ["geometry", "stage", "dark", "flat", "colour"] {
        assert!(active[kind].is_object(), "{kind}: {active}");
    }
    // The flat field took the dark away, and found the vignetting.
    let flat = &active["flat"]["values"];
    assert!(
        flat["dark"] == true && flat["uneven"].as_f64().unwrap() > 0.05,
        "{flat}"
    );
    // Its image, to see (W8.2): in blocks, its middle brighter than its corner.
    let id = active["flat"]["id"].as_str().unwrap();
    let seen = client.get(&format!("/api/v1/calibration/image/{id}")).await;
    assert_eq!(seen.status, 200, "{}", seen.text());
    let seen = seen.json();
    let (width, height) = (
        seen["width"].as_u64().unwrap(),
        seen["height"].as_u64().unwrap(),
    );
    let values = seen["values"].as_array().unwrap();
    assert!(
        width.max(height) <= 256 && seen["block"].as_u64() > Some(1),
        "{width} × {height}"
    );
    assert_eq!(
        values.len() as u64,
        width * height * seen["channels"].as_u64().unwrap()
    );
    let middle = values[((height / 2) * width + width / 2) as usize]
        .as_f64()
        .unwrap();
    assert!(
        middle > values[0].as_f64().unwrap() * 1.05,
        "{middle} against {}",
        values[0]
    );
    // Refused: a set without an image, and what isn't a set's ID.
    let geometry = active["geometry"]["id"].as_str().unwrap();
    let none = client
        .get(&format!("/api/v1/calibration/image/{geometry}"))
        .await;
    assert_eq!(none.status, 404, "{}", none.text());
    assert_eq!(
        client.get("/api/v1/calibration/image/a-flat").await.status,
        400
    );
    // The crosstalk showed, and the matrix took it away.
    let colour = &active["colour"]["values"];
    assert!(colour["delta_e_before"].as_f64().unwrap() > 5.0, "{colour}");
    assert!(colour["delta_e_mean"].as_f64().unwrap() < 0.5, "{colour}");
    let confirmed = invoke(&client, "calibration/wizard_confirm", json!({})).await;
    assert!(
        confirmed["error"].to_string().contains("conflict (409)"),
        "{confirmed}"
    );
    stop_streaming(&client).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn the_wizard_s_objective_step_fails_through_a_wrong_magnification() {
    let _turn = TURN.lock().await;
    let folder = tempfile::tempdir().unwrap();
    // Half as much again as the 10×'s: nearer the 20×'s than its own.
    let client = server_scaled(folder.path(), 1.5).await;
    assert_eq!(
        invoke(&client, "stage/home", json!({})).await["status"],
        "completed"
    );
    let island = HeTissue::new(1, Slide::COVERSLIP).islands()[0];
    let over = json!({"x": island.x, "y": island.y, "z": 1_002.0});
    assert_eq!(
        invoke(&client, "stage/move_to", over).await["status"],
        "completed"
    );
    let streaming = invoke(&client, "camera/start_streaming", json!({})).await;
    assert_eq!(streaming["status"], "completed", "{streaming}");
    let steps = json!({"steps": ["objective", "dark"]});
    let started = invoke(&client, "calibration/wizard_start", steps).await;
    assert_eq!(
        started["output"]["steps"].as_array().unwrap().len(),
        2,
        "{started}"
    );
    let state = step(&client, 0).await;
    assert_eq!(state["state"], "failed", "{state}");
    let error = state["error"].as_str().unwrap();
    assert!(error.contains("as through the 20x"), "{error}");
    // Skipped, the next step prompts; cancelled, the wizard is finished.
    let skipped = invoke(&client, "calibration/wizard_skip", json!({})).await;
    let steps = &skipped["output"]["steps"];
    assert_eq!(
        (
            steps[0]["state"]["state"].as_str(),
            steps[1]["state"]["state"].as_str()
        ),
        (Some("skipped"), Some("prompting")),
        "{skipped}"
    );
    let cancelled = invoke(&client, "calibration/wizard_cancel", json!({})).await;
    let wizard = cancelled["output"].clone();
    assert_eq!(
        (wizard["finished"].as_bool(), wizard["cancelled"].as_bool()),
        (Some(true), Some(true)),
        "{cancelled}"
    );
    // Saved as it changed: the server started again finds it as it was.
    stop_streaming(&client).await;
    client.stop().await;
    let again = server_scaled(folder.path(), 1.5).await;
    let resumed = again.get("/api/v1/calibration/wizard").await.json();
    assert_eq!(resumed, wizard);
}
