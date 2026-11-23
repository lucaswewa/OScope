//! Recordings over HTTP: the scratch space's health from the `system`
//! Thing, and a recording ingested into a project by the `projects.ingest`
//! job.

use std::path::Path;
use std::time::{Duration, Instant};

use oscope_api::{Api, Roots};
use oscope_core::{FrameId, FrameMeta, PixelFormat, RecordingId, Timestamp};
use oscope_jobs::{JobService, JobsConfig};
use oscope_store::{RecordingOptions, Scratch, ScratchPolicy, Store};
use serde_json::{Value, json};
use teta_wot::testing::TestClient;

const POLICY: ScratchPolicy = ScratchPolicy {
    reserve_bytes: 0,
    low_bytes: 0,
};

/// A recording of three small frames in the scratch space at `root`.
fn record(root: &Path) -> RecordingId {
    let scratch = Scratch::open(root, POLICY).unwrap();
    // Segments of 64 MiB: the default's 4 GiB, preallocated, would crowd the
    // disk when suites run side by side.
    let options = RecordingOptions {
        name: "three frames".to_owned(),
        segment_bytes: 64 << 20,
        ..RecordingOptions::default()
    };
    let mut writer = scratch.record(options).unwrap();
    for n in 0..3 {
        let meta = FrameMeta::new(FrameId(n), PixelFormat::Mono16, 64, 48, Timestamp(n));
        writer.append(&meta, &vec![n as u8; 64 * 48 * 2]).unwrap();
    }
    writer.finish(None).unwrap().id
}

async fn server(folder: &Path) -> TestClient {
    let roots = Roots {
        data: folder.join("data"),
        scratch: folder.join("scratch"),
    };
    let store = Store::open(&roots.data, &roots.data.join("index.sqlite")).unwrap();
    let jobs = JobService::open(&folder.join("jobs.sqlite"), JobsConfig::default()).unwrap();
    let api = Api::builder(&roots)
        .projects(store)
        .scratch(Scratch::open(&roots.scratch, POLICY).unwrap())
        .jobs(jobs)
        .build()
        .unwrap();
    TestClient::start(api.into_thing_server()).await.unwrap()
}

/// Invokes `action` with `input`, and waits for its invocation to end: its
/// final JSON.
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
        assert!(Instant::now() < deadline, "{action}: {invocation}");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

/// Waits for the job at `href` to end: its JSON.
async fn ended(client: &TestClient, href: &str) -> Value {
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        let job = client.get(href).await.json();
        if ["completed", "failed", "cancelled"].contains(&job["state"].as_str().unwrap_or("")) {
            return job;
        }
        assert!(Instant::now() < deadline, "{job}");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_recording_is_ingested_into_a_project_by_a_job() {
    let folder = tempfile::tempdir().unwrap();
    let recording = record(&folder.path().join("scratch"));
    let client = server(folder.path()).await;

    let health = client.get("/api/v1/system/scratch").await;
    assert_eq!(health.status, 200, "{}", health.text());
    let health = health.json();
    assert_eq!(
        (&health["state"], &health["recordings"]),
        (&json!("ok"), &json!(1))
    );
    assert!(health["recordings_bytes"].as_u64().unwrap() > 3 * 64 * 48 * 2);
    let kinds = client.get("/api/v1/jobs/kinds").await.json();
    assert!(
        kinds
            .as_array()
            .unwrap()
            .contains(&json!("projects.ingest")),
        "{kinds}"
    );

    let made = invoke(&client, "projects/create", json!({"name": "Burst"})).await;
    let project = made["output"]["id"].as_str().unwrap().to_owned();
    let params = json!({"recording": recording, "project": project, "name": "burst"});
    let submitted = invoke(
        &client,
        "jobs/submit",
        json!({"kind": "projects.ingest", "params": params}),
    )
    .await;
    assert_eq!(submitted["status"], "completed", "{submitted}");
    let job = ended(&client, submitted["output"]["href"].as_str().unwrap()).await;
    assert_eq!(job["state"], "completed", "{job}");
    assert_eq!(job["progress"]["fraction"], 1.0, "{job}");

    // The acquisition in the project, the recording gone from the scratch.
    let contents = client
        .get(&format!("/api/v1/projects/project/{project}"))
        .await
        .json();
    let acquisition = &contents["acquisitions"][0];
    assert_eq!(acquisition["name"], "burst", "{contents}");
    assert_eq!(acquisition["image"]["planes"], 3);
    let id = acquisition["id"].as_str().unwrap();
    assert_eq!(
        job["results"],
        json!([format!("projects/{project}/acquisitions/{id}")])
    );
    let health = client.get("/api/v1/system/scratch").await.json();
    assert_eq!(health["recordings"], 0, "{health}");

    // A recording that isn't there.
    let params = json!({"recording": RecordingId::generate(), "project": project});
    let refused = invoke(
        &client,
        "jobs/submit",
        json!({"kind": "projects.ingest", "params": params}),
    )
    .await;
    let error = refused["error"].to_string();
    assert!(
        error.contains("refused (400)") && error.contains("no recording"),
        "{refused}"
    );
}
