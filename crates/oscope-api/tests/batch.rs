//! The `pipeline.batch` job (Checkpoint B): a recipe over the camera's next
//! frames, as live recipes see them, its images saved as PNG files; and
//! what it refuses.

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use oscope_api::devices::{Devices, DevicesConfig};
use oscope_api::{Api, Roots};
use oscope_jobs::{JobService, JobsConfig};
use serde_json::{Value, json};
use teta_wot::testing::TestClient;

async fn server(folder: &Path) -> TestClient {
    let fake = || "fake".to_owned();
    let config = DevicesConfig {
        camera: fake(),
        stage: fake(),
        illumination: fake(),
        objective: fake(),
        ..DevicesConfig::default()
    };
    let devices = Arc::new(Devices::from_config(&config, folder).await.unwrap());
    let jobs = JobService::open(&folder.join("index.sqlite"), JobsConfig::default()).unwrap();
    let roots = Roots {
        data: folder.join("data"),
        scratch: folder.join("scratch"),
    };
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
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let invocation = client.get(&href).await.json();
        if !["pending", "running"].contains(&invocation["status"].as_str().unwrap_or("")) {
            return invocation;
        }
        assert!(
            Instant::now() < deadline,
            "{action} didn't end: {invocation}"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

fn recipe(node: &str) -> Value {
    json!({
        "format": 1,
        "name": "Levels",
        "inputs": [{ "name": "image", "type": "image" }, { "name": "region", "type": "json" }],
        "nodes": [{ "id": "levels", "node": node }],
        "edges": [{ "from": "input.image", "to": "levels.image" }],
        "outputs": [{ "name": "image", "from": "levels.image" }],
    })
}

#[tokio::test(flavor = "multi_thread")]
async fn a_recipe_runs_over_the_camera_s_next_frames_as_a_job() {
    let folder = tempfile::tempdir().unwrap();
    let client = server(folder.path()).await;
    let kinds = client.get("/api/v1/jobs/kinds").await.json();
    assert!(
        kinds.as_array().unwrap().contains(&json!("pipeline.batch")),
        "{kinds}"
    );
    let started = invoke(&client, "camera/start_streaming", json!({})).await;
    assert_eq!(started["status"], "completed", "{started}");

    let params = json!({ "recipe": recipe("display.levels"), "frames": 2 });
    let submitted = invoke(
        &client,
        "jobs/submit",
        json!({ "kind": "pipeline.batch", "params": params }),
    )
    .await;
    assert_eq!(submitted["status"], "completed", "{submitted}");
    let href = submitted["output"]["href"].as_str().unwrap().to_owned();
    let deadline = Instant::now() + Duration::from_secs(60);
    let job = loop {
        let job = client.get(&href).await.json();
        if job["state"] == "completed" {
            break job;
        }
        assert!(
            !["failed", "cancelled"].contains(&job["state"].as_str().unwrap()),
            "{job}"
        );
        assert!(Instant::now() < deadline, "{job}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    };
    let results: Vec<String> = serde_json::from_value(job["results"].clone()).unwrap();
    assert_eq!(results.len(), 2, "{job}");
    for (n, result) in results.iter().enumerate() {
        assert!(
            result.ends_with(&format!("/frame-{}-image.png", n + 1)),
            "{result}"
        );
        let file = std::fs::read(folder.path().join("data").join(result)).unwrap();
        assert!(file.starts_with(b"\x89PNG"), "{result}");
    }

    // Refused: frames out of range, and a node there is none of.
    for (params, said) in [
        (
            json!({ "recipe": recipe("display.levels"), "frames": 0 }),
            "frames: 0, where 1 to 100 may be",
        ),
        (json!({ "recipe": recipe("no.such_node") }), "no.such_node"),
    ] {
        let refused = invoke(
            &client,
            "jobs/submit",
            json!({ "kind": "pipeline.batch", "params": params }),
        )
        .await;
        assert_eq!(refused["status"], "error", "{refused}");
        assert!(
            refused["error"].to_string().contains(said),
            "{said}: {refused}"
        );
    }
}
