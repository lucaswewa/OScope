//! The `status` Thing over HTTP (P50): the instrument's status as the
//! simulated microscope starts, its stage in alarm until homed; each fault
//! injected reported by its subsystem, and the roll-up following; each
//! taken away again; the event; and the GPU's and the disk's measures.

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use oscope_api::devices::{Devices, DevicesConfig};
use oscope_api::{Api, Roots};
use oscope_jobs::{JobService, JobsConfig};
use oscope_store::{Scratch, ScratchPolicy};
use serde_json::{Value, json};
use teta_wot::testing::TestClient;

async fn server(folder: &Path) -> TestClient {
    let policy = ScratchPolicy {
        reserve_bytes: 0,
        low_bytes: 0,
    };
    server_with(folder, policy).await
}

/// As [`server`], its scratch space kept by `policy`.
async fn server_with(folder: &Path, policy: ScratchPolicy) -> TestClient {
    let roots = Roots {
        data: folder.join("data"),
        scratch: folder.join("scratch"),
    };
    std::fs::create_dir_all(&roots.scratch).unwrap();
    let devices = Devices::from_config(&DevicesConfig::default(), folder).await;
    let jobs = JobService::open(&folder.join("jobs.sqlite"), JobsConfig::default()).unwrap();
    let api = Api::builder(&roots)
        .devices(Arc::new(devices.unwrap()))
        .scratch(Scratch::open(&roots.scratch, policy).unwrap())
        .jobs(jobs)
        .build()
        .unwrap();
    TestClient::start(api.into_thing_server()).await.unwrap()
}

/// Invokes `action` with `input`, and waits for it to end.
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

/// The instrument's status, once `wanted` holds of it.
async fn until(client: &TestClient, what: &str, wanted: impl Fn(&Value) -> bool) -> Value {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let status = client.get("/api/v1/status/instrument").await.json();
        if wanted(&status) {
            return status;
        }
        assert!(Instant::now() < deadline, "{what}: {status}");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// Subsystem `name`'s health in `status`.
fn of<'a>(status: &'a Value, name: &str) -> &'a Value {
    let subsystems = status["subsystems"].as_array().unwrap();
    let found = subsystems.iter().find(|s| s["subsystem"] == name);
    found.unwrap_or_else(|| panic!("{name} in {status}"))
}

/// Injects `fault`, or with `active` false takes it away.
async fn inject(client: &TestClient, fault: &str, active: bool) {
    let input = json!({"fault": fault, "active": active});
    let done = invoke(client, "simulation/inject_fault", input).await;
    assert_eq!(done["status"], "completed", "{done}");
}

#[tokio::test(flavor = "multi_thread")]
async fn each_fault_is_reported_by_its_subsystem_and_rolled_up() {
    let folder = tempfile::tempdir().unwrap();
    let client = server(folder.path()).await;
    // At first the stage wants homing: a fault, said by the stage.
    let start = until(&client, "the stage's alarm", |s| s["state"] == "fault").await;
    assert_eq!(
        start["message"], "stage: in alarm: homing required",
        "{start}"
    );
    let homed = invoke(&client, "stage/home", json!({})).await;
    assert_eq!(homed["status"], "completed", "{homed}");
    let well = until(&client, "all well", |s| s["state"] == "ok").await;
    assert_eq!(well["message"], "all well");
    let names: Vec<&str> = well["subsystems"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|s| s["subsystem"].as_str())
        .collect();
    assert_eq!(names, ["camera", "stage", "lights", "gpu", "disks", "jobs"]);
    // The GPU, if NVML is there, with its measures; the disk's free space.
    let gpu = of(&well, "gpu");
    assert!(
        ["ok", "offline", "degraded"].contains(&gpu["state"].as_str().unwrap()),
        "{gpu}"
    );
    if gpu["state"] == "ok" {
        assert!(
            gpu["details"]["memory_total_mib"].as_u64().unwrap() > 0,
            "{gpu}"
        );
    }
    let disks = of(&well, "disks");
    assert!(
        disks["details"]["available_gb"].as_f64().unwrap() > 0.0,
        "{disks}"
    );
    let mut changes = client.events("/api/v1/status/changed").await;
    // The camera unplugged: it stops streaming, and won't start; offline,
    // and a fault. Plugged in again, well.
    let started = invoke(&client, "camera/start_streaming", json!({})).await;
    assert_eq!(started["status"], "completed", "{started}");
    inject(&client, "camera_unplugged", true).await;
    assert_eq!(client.get("/api/v1/camera/streaming").await.json(), false);
    let refused = invoke(&client, "camera/start_streaming", json!({})).await;
    assert_eq!(refused["status"], "error", "{refused}");
    let unplugged = until(&client, "the camera offline", |s| {
        of(s, "camera")["state"] == "offline"
    })
    .await;
    assert_eq!(
        (&unplugged["state"], &unplugged["message"]),
        (&json!("fault"), &json!("camera: disconnected"))
    );
    let told = changes.next_within(Duration::from_secs(10)).await.unwrap();
    assert_eq!(told["state"], "fault", "{told}");
    inject(&client, "camera_unplugged", false).await;
    until(&client, "the camera back", |s| s["state"] == "ok").await;
    // The stage in alarm, as it moves: the move ends with the alarm; a
    // fault; cleared, well.
    let far = json!({"x": 90_000.0, "y": 70_000.0, "z": 1_000.0});
    let moving = client.post_json("/api/v1/stage/move_to", Some(&far)).await;
    let href = moving.json()["href"].as_str().unwrap().to_owned();
    until(&client, "the stage moving", |s| {
        of(s, "stage")["message"] == "moving"
    })
    .await;
    inject(&client, "stage_alarm", true).await;
    let deadline = Instant::now() + Duration::from_secs(10);
    let ended = loop {
        let invocation = client.get(&href).await.json();
        if invocation["status"] != "running" && invocation["status"] != "pending" {
            break invocation;
        }
        assert!(Instant::now() < deadline, "the move: {invocation}");
        tokio::time::sleep(Duration::from_millis(10)).await;
    };
    assert!(
        ended["error"].to_string().contains("a limit switch"),
        "{ended}"
    );
    // In alarm, it comes to rest: its reported place, which lags by the
    // reports in flight, soon stops changing. Moving on, it wouldn't for
    // seconds more.
    let deadline = Instant::now() + Duration::from_secs(5);
    let place = || async { client.get("/api/v1/stage/position").await.json()["position"].clone() };
    let mut last = place().await;
    loop {
        tokio::time::sleep(Duration::from_millis(250)).await;
        let now = place().await;
        if now == last {
            break;
        }
        assert!(Instant::now() < deadline, "the stage still moves: {now}");
        last = now;
    }
    let alarm = until(&client, "the stage's alarm", |s| s["state"] == "fault").await;
    assert_eq!(
        alarm["message"],
        "stage: in alarm: a limit switch (a fault injected)"
    );
    inject(&client, "stage_alarm", false).await;
    until(&client, "the stage cleared", |s| s["state"] == "ok").await;
    // The scratch disk full: a fault; emptied, well.
    inject(&client, "scratch_full", true).await;
    let full = until(&client, "the disk full", |s| s["state"] == "fault").await;
    assert_eq!(
        full["message"],
        "disks: the scratch disk is full (a fault injected)"
    );
    inject(&client, "scratch_full", false).await;
    until(&client, "the disk back", |s| s["state"] == "ok").await;
    // A job that failed degrades the jobs: auto exposure, the camera
    // unplugged.
    inject(&client, "camera_unplugged", true).await;
    let submitted = invoke(&client, "exposure/auto", json!({})).await;
    assert_eq!(submitted["status"], "completed", "{submitted}");
    inject(&client, "camera_unplugged", false).await;
    let failed = until(&client, "the job failed", |s| {
        of(s, "jobs")["state"] == "degraded"
    })
    .await;
    let said = of(&failed, "jobs")["message"].as_str().unwrap().to_owned();
    assert!(
        said.starts_with("1 failed in the last ten minutes, exposure.auto"),
        "{said}"
    );
    assert_eq!(failed["state"], "degraded");
}

#[tokio::test(flavor = "multi_thread")]
async fn the_scratch_disk_s_own_state_is_reported() {
    // Room below the reserve: full, a fault. Below the low mark: degraded.
    for (reserve_bytes, low_bytes, want, said) in [
        (u64::MAX / 4, 0, "fault", "full: "),
        (0, u64::MAX / 4, "degraded", "low: "),
    ] {
        let folder = tempfile::tempdir().unwrap();
        let policy = ScratchPolicy {
            reserve_bytes,
            low_bytes,
        };
        let client = server_with(folder.path(), policy).await;
        let status = until(&client, "the disk's state", |s| {
            of(s, "disks")["state"] == want
        })
        .await;
        let disks = of(&status, "disks");
        assert!(
            disks["message"].as_str().unwrap().starts_with(said),
            "{disks}"
        );
        assert!(disks["details"]["filling_mb_s"].as_f64().unwrap() >= 0.0);
    }
}
