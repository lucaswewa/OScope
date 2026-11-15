//! The `autofocus` and `exposure` Things over HTTP (P34): autofocus as a job
//! on the simulator, within a quarter of the depth of field of the truth the
//! simulation Thing gives; its report, measures and defaults; an empty field
//! refused; auto exposure and white balance as jobs, the gains given to the
//! preview by the client; options that can't be, refused.

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use oscope_api::devices::{Devices, DevicesConfig};
use oscope_api::{Api, Roots};
use oscope_jobs::{JobService, JobsConfig};
use oscope_sim::{HeTissue, Slide};
use serde_json::{Value, json};
use teta_wot::testing::TestClient;

/// The simulator ten times as fast as real time, on the quarter sensor.
async fn server(folder: &Path) -> TestClient {
    let mut config = DevicesConfig::default();
    let sim = &mut config.simulator;
    (sim.sensor, sim.focus, sim.rate) = ("quarter".to_owned(), 1_000.0, 10.0);
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

/// Submits the job `action` starts, and waits up to 300 s for it to end.
async fn job(client: &TestClient, action: &str, input: Value) -> Value {
    let submitted = invoke(client, action, input).await;
    assert_eq!(submitted["status"], "completed", "{submitted}");
    let href = submitted["output"]["href"].as_str().unwrap().to_owned();
    let deadline = Instant::now() + Duration::from_secs(300);
    loop {
        let job = client.get(&href).await.json();
        if ["completed", "failed", "cancelled"].contains(&job["state"].as_str().unwrap_or("")) {
            return job;
        }
        assert!(Instant::now() < deadline, "{job}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

async fn stage_z(client: &TestClient) -> f64 {
    let position = client.get("/api/v1/stage/position").await.json();
    position["position"]["z"].as_f64().unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn autofocus_exposure_and_white_balance_run_as_jobs() {
    let folder = tempfile::tempdir().unwrap();
    let client = server(folder.path()).await;
    // A 256 × 192 region of the sensor's middle, quick to render.
    let roi = json!({"x": 556, "y": 360, "width": 256, "height": 192});
    let set = client.put_json("/api/v1/camera/roi", &roi).await;
    assert!(set.status.is_success(), "{}", set.text());
    assert_eq!(
        invoke(&client, "stage/home", json!({})).await["status"],
        "completed"
    );
    // Over the tissue, 12 µm above where it is sharpest.
    let island = HeTissue::new(1, Slide::COVERSLIP).islands()[0];
    let at = json!({"x": island.x, "y": island.y});
    let truth = invoke(&client, "simulation/in_focus", at).await["output"]
        .as_f64()
        .unwrap();
    let over = json!({"x": island.x, "y": island.y, "z": truth + 12.0});
    assert_eq!(
        invoke(&client, "stage/move_to", over).await["status"],
        "completed"
    );
    let streaming = invoke(&client, "camera/start_streaming", json!({})).await;
    assert_eq!(streaming["status"], "completed", "{streaming}");

    let defaults = client.get("/api/v1/autofocus/defaults").await.json();
    assert_eq!(defaults["measure"], "brenner", "{defaults}");
    assert_eq!(defaults["light"], "transmitted", "{defaults}");
    let measures = client.get("/api/v1/autofocus/measures").await.json();
    assert!(measures["brenner"].as_f64().unwrap() > 0.0, "{measures}");
    assert_eq!(
        client.get("/api/v1/autofocus/last").await.json(),
        Value::Null
    );

    let focused = job(&client, "autofocus/run", json!({})).await;
    assert_eq!(focused["state"], "completed", "{focused}");
    let last = client.get("/api/v1/autofocus/last").await.json();
    let quarter = last["depth_of_field_um"].as_f64().unwrap() / 4.0;
    let error = stage_z(&client).await - truth;
    assert!(error.abs() < quarter, "{error:+.3} µm: {last}");
    assert_eq!(
        (last["found"].as_bool(), last["measure"].as_str()),
        (Some(true), Some("brenner"))
    );
    // Sharper now than it was.
    let sharp = client.get("/api/v1/autofocus/measures").await.json();
    assert!(
        sharp["brenner"].as_f64() > measures["brenner"].as_f64(),
        "{sharp} {measures}"
    );

    // Auto exposure, from too dark.
    assert!(
        client
            .put_json("/api/v1/camera/exposure", &json!(0.1))
            .await
            .status
            .is_success()
    );
    let exposed = job(&client, "exposure/auto", json!({})).await;
    assert_eq!(exposed["state"], "completed", "{exposed}");
    let report = client.get("/api/v1/exposure/last").await.json();
    assert_eq!(report["converged"], true, "{report}");
    let exposure = client.get("/api/v1/camera/exposure").await.json();
    assert_eq!(exposure, report["exposure_ms"], "{report}");

    // White balance, its gains given to the preview by the client.
    let balanced = job(&client, "exposure/white_balance", json!({})).await;
    assert_eq!(balanced["state"], "completed", "{balanced}");
    let white = client
        .get("/api/v1/exposure/last_white_balance")
        .await
        .json();
    let gains = white["gains"].clone();
    assert_eq!(gains[1], 1.0, "{white}");
    let mut look = client.get("/api/v1/preview/look").await.json();
    look["gains"] = gains.clone();
    assert!(
        client
            .put_json("/api/v1/preview/look", &look)
            .await
            .status
            .is_success()
    );
    // The preview keeps them as f32s.
    let kept = client.get("/api/v1/preview/look").await.json()["gains"].clone();
    for i in 0..3 {
        let (a, b) = (kept[i].as_f64().unwrap(), gains[i].as_f64().unwrap());
        assert!((a - b).abs() < 1e-6, "{kept} {gains}");
    }

    // An empty field, beside the coverslip: refused, the stage back.
    let glass = json!({"x": 10_000.0, "y": 12_500.0, "z": truth});
    assert_eq!(
        invoke(&client, "stage/move_to", glass).await["status"],
        "completed"
    );
    let start = stage_z(&client).await;
    let refused = job(&client, "autofocus/run", json!({"sweep": "continuous"})).await;
    assert_eq!(refused["state"], "failed", "{refused}");
    assert!(
        refused["error"].as_str().unwrap().contains("empty"),
        "{refused}"
    );
    assert!((stage_z(&client).await - start).abs() < 0.5);
    let last = client.get("/api/v1/autofocus/last").await.json();
    assert_eq!(
        (last["found"].as_bool(), last["sweep"].as_str()),
        (Some(false), Some("continuous"))
    );

    // Options that can't be are refused as the job is submitted.
    for (action, wrong) in [
        ("autofocus/run", json!({"range_um": -5.0})),
        ("exposure/auto", json!({"target": 1.5})),
    ] {
        let refused = invoke(&client, action, wrong).await;
        assert!(
            refused["error"].to_string().contains("refused (400)"),
            "{action}: {refused}"
        );
    }
    let stopped = invoke(&client, "camera/stop_streaming", json!({})).await;
    assert_eq!(stopped["status"], "completed", "{stopped}");
}
