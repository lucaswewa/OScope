//! The `calibration` Thing over HTTP (P32): its jobs on the simulator, with
//! a turned camera, objectives magnifying 2 % less than the catalogue says,
//! and 3 µm of backlash; the sets listed, active, exported, imported and
//! activated; the objective check; and what is refused.

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use oscope_api::devices::{Devices, DevicesConfig};
use oscope_api::{Api, Roots};
use oscope_jobs::{JobService, JobsConfig};
use oscope_sim::{HeTissue, Slide};
use serde_json::{Value, json};
use teta_wot::testing::TestClient;

/// The simulator ten times as fast as real time, on the quarter sensor
/// (9.6 µm pixels), with the truth calibration recovers.
async fn server(folder: &Path) -> TestClient {
    let mut config = DevicesConfig::default();
    let sim = &mut config.simulator;
    (sim.sensor, sim.focus, sim.rate) = ("quarter".to_owned(), 1_000.0, 10.0);
    (sim.rotation, sim.scale, sim.backlash) = (-20.0, 0.98, 3.0);
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

/// Invokes `action` with `input`, and waits for its invocation to end.
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
    assert_eq!(active, json!({"geometry": null, "stage": null}));
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
}
