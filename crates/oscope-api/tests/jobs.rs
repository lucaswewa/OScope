//! The `jobs` Thing over HTTP: submitting and following a job, listing and
//! filtering, cancelling, pausing and reordering, the events, the device
//! Things refusing manual moves while a hardware job runs, and the Thing's
//! description.

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use oscope_api::devices::{Devices, DevicesConfig};
use oscope_api::{Api, Roots};
use oscope_jobs::{Job, JobService, JobSpec, JobsConfig, Resource, Stop};
use serde_json::{Value, json};
use teta_wot::testing::TestClient;

/// A job that waits, at checkpoints, until it is cancelled.
fn hold(resource: Resource, pausable: bool) -> Job {
    Job {
        spec: JobSpec {
            kind: String::new(),
            title: "holds".to_owned(),
            account: None,
            params: Value::Null,
            resource,
            pausable,
            gpu_mib: 0,
        },
        run: Box::new(|ctx| {
            Box::pin(async move {
                loop {
                    ctx.checkpoint().await?;
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
            })
        }),
    }
}

/// The fakes and a job service with two kinds: `test.hold`, a hardware job
/// that holds until cancelled, and `test.count`, a compute job that counts
/// to its `to`.
async fn server(folder: &Path) -> TestClient {
    let config = DevicesConfig {
        camera: "fake".to_owned(),
        stage: "fake".to_owned(),
        illumination: "fake".to_owned(),
        objective: "fake".to_owned(),
        ..DevicesConfig::default()
    };
    let devices = Arc::new(Devices::from_config(&config, folder).await.unwrap());
    let jobs = JobService::open(&folder.join("index.sqlite"), JobsConfig::default()).unwrap();
    jobs.register(
        "test.hold",
        Arc::new(|_| Ok(hold(Resource::Hardware, false))),
    );
    jobs.register(
        "test.count",
        Arc::new(|params| {
            let to = params["to"].as_u64().ok_or("`to` is a number")?;
            let mut job = hold(Resource::Compute, false);
            job.run = Box::new(move |ctx| {
                Box::pin(async move {
                    for i in 1..=to {
                        ctx.checkpoint().await?;
                        ctx.count("counted", i);
                    }
                    ctx.add_result(format!("counted to {to}"));
                    Ok::<(), Stop>(())
                })
            });
            Ok(job)
        }),
    );
    let roots = Roots {
        data: folder.join("data"),
        scratch: folder.join("scratch"),
    };
    std::fs::create_dir_all(&roots.data).unwrap();
    let api = Api::builder(&roots)
        .devices(devices)
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
        assert!(
            Instant::now() < deadline,
            "{action} didn't end: {invocation}"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

/// Waits up to 30 s for the job at `href` to be in `state`: its JSON.
async fn wait_for(client: &TestClient, href: &str, state: &str) -> Value {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let job = client.get(href).await.json();
        if job["state"] == state {
            return job;
        }
        assert!(Instant::now() < deadline, "not {state}: {job}");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_job_is_submitted_followed_and_listed() {
    let folder = tempfile::tempdir().unwrap();
    let client = server(folder.path()).await;
    assert_eq!(
        client.get("/api/v1/jobs/kinds").await.json(),
        json!([
            "autofocus.run",
            "calibration.check_objective",
            "calibration.colour",
            "calibration.dark",
            "calibration.flat",
            "calibration.geometry",
            "calibration.stage",
            "exposure.auto",
            "exposure.white_balance",
            "pipeline.batch",
            "test.count",
            "test.hold"
        ])
    );
    let mut changes = client.events("/api/v1/jobs/changed").await;

    // The action answers at once with where to follow the job.
    let submitted = invoke(
        &client,
        "jobs/submit",
        json!({"kind": "test.count", "params": {"to": 3}}),
    )
    .await;
    assert_eq!(submitted["status"], "completed", "{submitted}");
    let href = submitted["output"]["href"].as_str().unwrap().to_owned();
    assert_eq!(
        href,
        format!(
            "/api/v1/jobs/job/{}",
            submitted["output"]["id"].as_str().unwrap()
        )
    );
    let job = wait_for(&client, &href, "completed").await;
    assert_eq!(job["kind"], "test.count");
    assert_eq!(job["params"], json!({"to": 3}));
    assert_eq!(job["progress"]["counters"]["counted"], 3);
    assert_eq!(job["results"], json!(["counted to 3"]));

    // Its changes, as events: queued, running, …, completed. Read only now,
    // after the job has ended: its progress was coalesced, so its end
    // wasn't pushed out of the stream's 5-event buffer.
    let mut states = Vec::new();
    while states.last() != Some(&"completed".to_owned()) {
        let event = tokio::time::timeout(Duration::from_secs(10), changes.next())
            .await
            .unwrap()
            .unwrap();
        if event["id"] == job["id"] {
            states.push(event["state"].as_str().unwrap().to_owned());
        }
    }
    assert_eq!(
        states.first().map(String::as_str),
        Some("queued"),
        "{states:?}"
    );
    assert!(states.contains(&"running".to_owned()), "{states:?}");

    // Listed, filtered, and refused.
    let listed = client
        .get("/api/v1/jobs/list?state=completed&limit=5")
        .await;
    assert_eq!(listed.status, 200);
    assert_eq!(listed.json()[0]["id"], job["id"]);
    assert_eq!(
        client.get("/api/v1/jobs/list?kind=test.hold").await.json(),
        json!([])
    );
    assert_eq!(client.get("/api/v1/jobs/job/not-an-id").await.status, 400);
    let unknown = client
        .get("/api/v1/jobs/job/01900000-0000-7000-8000-000000000000")
        .await;
    assert_eq!(
        (unknown.status.as_u16(), unknown.json()["title"].clone()),
        (404, json!("unknown"))
    );
    let refused = invoke(&client, "jobs/submit", json!({"kind": "test.nothing"})).await;
    assert_eq!(refused["status"], "error");
    assert!(
        refused["error"].to_string().contains("refused (400)"),
        "{refused}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn manual_moves_are_refused_while_a_hardware_job_runs() {
    let folder = tempfile::tempdir().unwrap();
    let client = server(folder.path()).await;
    let home = invoke(&client, "stage/home", json!({})).await;
    assert_eq!(home["status"], "completed", "{home}");

    let held = invoke(&client, "jobs/submit", json!({"kind": "test.hold"})).await;
    let (id, href) = (
        held["output"]["id"].clone(),
        held["output"]["href"].as_str().unwrap().to_owned(),
    );
    wait_for(&client, &href, "running").await;
    assert_eq!(client.get("/api/v1/jobs/hardware").await.json(), id);

    // Moves, jogs and homing are refused as busy; halting isn't.
    let target = json!({"x": 100.0, "y": 100.0, "z": 0.0});
    for (action, input) in [
        ("stage/move_to", target.clone()),
        ("stage/move_by", json!({"dx": 10.0, "dy": 0.0, "dz": 0.0})),
        ("stage/jog", json!({"axis": "x", "towards_max": true})),
        ("stage/home", json!({})),
    ] {
        let refused = invoke(&client, action, input).await;
        assert_eq!(refused["status"], "error", "{action}: {refused}");
        let error = refused["error"].to_string();
        assert!(
            error.contains("busy (409)") && error.contains("has the hardware"),
            "{action}: {error}"
        );
    }
    assert_eq!(
        invoke(&client, "stage/halt", json!({})).await["status"],
        "completed"
    );

    // A second hardware job queues behind it, and can move; a job that
    // can't pause refuses to.
    let next = invoke(&client, "jobs/submit", json!({"kind": "test.hold"})).await["output"].clone();
    let last = invoke(&client, "jobs/submit", json!({"kind": "test.hold"})).await["output"].clone();
    let moved = invoke(
        &client,
        "jobs/reorder",
        json!({"id": last["id"], "position": 0}),
    )
    .await;
    assert_eq!(moved["status"], "completed", "{moved}");
    let queued = client.get(last["href"].as_str().unwrap()).await.json();
    assert_eq!(queued["position"], 0);
    let paused = invoke(&client, "jobs/pause", json!({"id": id})).await;
    assert!(
        paused["error"].to_string().contains("conflict (409)"),
        "{paused}"
    );

    // Cancelled, they end, and the stage moves again.
    for job in [&next, &last] {
        invoke(&client, "jobs/cancel", json!({"id": job["id"]})).await;
    }
    invoke(&client, "jobs/cancel", json!({"id": id})).await;
    wait_for(&client, &href, "cancelled").await;
    assert_eq!(
        client.get("/api/v1/jobs/hardware").await.json(),
        Value::Null
    );
    let moved = invoke(&client, "stage/move_to", target).await;
    assert_eq!(moved["status"], "completed", "{moved}");
}

#[tokio::test(flavor = "multi_thread")]
async fn the_jobs_thing_describes_itself() {
    let folder = tempfile::tempdir().unwrap();
    let client = server(folder.path()).await;
    let mut td = client.get("/api/v1/jobs/").await.json();
    // The ID differs from computer to computer: its form, then the rest.
    let id = td.as_object_mut().unwrap().remove("id").unwrap();
    assert!(id.as_str().unwrap().starts_with("urn:uuid:"), "{id}");
    insta::assert_json_snapshot!("jobs", td);
}
