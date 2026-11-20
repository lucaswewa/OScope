//! The `power` Thing over HTTP (P51), in a dry run: refused without a
//! confirmation; refused while a hardware job runs, unless forced, but not
//! while a compute job runs; each action's command on this platform, and
//! the event; and nothing done. And (W4.3) the service's restart done while
//! the host's actions stay dry runs.

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use oscope_api::{Api, Restart, Roots};
use oscope_jobs::{Job, JobService, JobSpec, JobsConfig, Resource, Stop};
use serde_json::{Value, json};
use teta_wot::testing::TestClient;

/// A server in a dry run, with two jobs that run until cancelled: a
/// hardware one, `test.hold`, and a compute one, `test.think`; and its
/// restart request.
async fn server(folder: &Path) -> (TestClient, Arc<Restart>) {
    server_with(folder, true).await
}

/// A server as [`server`] makes it, the host's actions dry runs, its
/// restart one or not as `dry_run_restart` says.
async fn server_with(folder: &Path, dry_run_restart: bool) -> (TestClient, Arc<Restart>) {
    let roots = Roots {
        data: folder.join("data"),
        scratch: folder.join("scratch"),
    };
    let jobs = JobService::open(&folder.join("jobs.sqlite"), JobsConfig::default()).unwrap();
    let kinds = [
        ("test.hold", Resource::Hardware),
        ("test.think", Resource::Compute),
    ];
    for (kind, resource) in kinds {
        jobs.register(
            kind,
            Arc::new(move |params: &Value| {
                let spec = JobSpec {
                    kind: kind.to_owned(),
                    title: kind.to_owned(),
                    account: None,
                    params: params.clone(),
                    resource,
                    pausable: false,
                    gpu_mib: 0,
                };
                let run = Box::new(|ctx: oscope_jobs::JobCtx| {
                    Box::pin(async move {
                        ctx.cancelled().await;
                        Err(Stop::Cancelled)
                    }) as _
                });
                Ok(Job { spec, run })
            }),
        );
    }
    let api = Api::builder(&roots).jobs(jobs);
    let api = api.power_with(true, dry_run_restart).build().unwrap();
    let restart = api.restart();
    let client = TestClient::start(api.into_thing_server()).await.unwrap();
    (client, restart)
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

/// Waits for `job`'s state to be as `wanted`.
async fn wait(client: &TestClient, job: &Value, wanted: impl Fn(&Value) -> bool) {
    let href = job["href"].as_str().unwrap();
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let state = client.get(href).await.json()["state"].clone();
        if wanted(&state) {
            return;
        }
        assert!(Instant::now() < deadline, "{job}: {state}");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

/// Submits a job of `kind`, and waits for it to run: the job.
async fn start(client: &TestClient, kind: &str) -> Value {
    let submitted = invoke(client, "jobs/submit", json!({"kind": kind, "params": {}})).await;
    let job = submitted["output"].clone();
    wait(client, &job, |state| *state == "running").await;
    job
}

/// Cancels `job`, at its next checkpoint, and waits for it to end.
async fn end(client: &TestClient, job: &Value) {
    let cancelled = invoke(client, "jobs/cancel", json!({"id": job["id"]})).await;
    assert_eq!(cancelled["status"], "completed", "{cancelled}");
    wait(client, job, |state| *state != "running").await;
}

#[tokio::test(flavor = "multi_thread")]
async fn power_actions_are_safeguarded_and_in_a_dry_run_only_said() {
    let folder = tempfile::tempdir().unwrap();
    let (client, restart) = server(folder.path()).await;
    assert_eq!(client.get("/api/v1/power/dry_run").await.json(), true);
    let dry_run_restart = client.get("/api/v1/power/dry_run_restart").await;
    assert_eq!(dry_run_restart.json(), true);
    let mut powering = client.events("/api/v1/power/powering").await;
    // Not confirmed: refused.
    let refused = invoke(&client, "power/reboot", json!({})).await;
    assert!(
        refused["error"].to_string().contains("refused (400)"),
        "{refused}"
    );
    // Each action, confirmed: what it would run here, and the event. The
    // restart first: had the dry run acted, it stops here, before a reboot.
    let (reboot, shutdown) = match cfg!(windows) {
        true => ("shutdown.exe /r /t 0", "shutdown.exe /s /t 0"),
        false => ("systemctl reboot", "systemctl poweroff"),
    };
    for (action, command) in [
        ("restart", None),
        ("reboot", Some(reboot)),
        ("shutdown", Some(shutdown)),
    ] {
        let done = invoke(
            &client,
            &format!("power/{action}"),
            json!({"confirm": true}),
        )
        .await;
        assert!(!restart.is_requested(), "a dry run acted: {done}");
        let output = &done["output"];
        let said = output["command"].as_array().map(|w| {
            let words: Vec<&str> = w.iter().filter_map(Value::as_str).collect();
            words.join(" ")
        });
        assert_eq!(
            (&output["action"], said.as_deref(), &output["dry_run"]),
            (&json!(action), command, &json!(true)),
            "{done}"
        );
        let told = powering.next_within(Duration::from_secs(10)).await.unwrap();
        assert_eq!(&told, output);
    }
    // Nothing was done: the server still serves.
    assert_eq!(client.get("/health").await.status, 200);
    // A compute job running doesn't refuse.
    let thinking = start(&client, "test.think").await;
    let confirmed = json!({"confirm": true});
    let goes = invoke(&client, "power/shutdown", confirmed.clone()).await;
    assert_eq!(goes["output"]["dry_run"], true, "{goes}");
    // A hardware job running: refused, naming it, unless forced.
    let held = start(&client, "test.hold").await;
    let busy = invoke(&client, "power/shutdown", confirmed.clone()).await;
    let error = busy["error"].to_string();
    assert!(
        error.contains("conflict (409)") && error.contains("test.hold"),
        "{busy}"
    );
    let forced = invoke(
        &client,
        "power/shutdown",
        json!({"confirm": true, "force": true}),
    )
    .await;
    assert_eq!(forced["output"]["dry_run"], true, "{forced}");
    // The hardware job ended: it no longer refuses.
    end(&client, &held).await;
    let after = invoke(&client, "power/shutdown", confirmed).await;
    assert_eq!(after["output"]["dry_run"], true, "{after}");
    end(&client, &thinking).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn the_restart_is_done_while_the_hosts_actions_stay_dry_runs() {
    let folder = tempfile::tempdir().unwrap();
    let (client, restart) = server_with(folder.path(), false).await;
    assert_eq!(client.get("/api/v1/power/dry_run").await.json(), true);
    let dry_run_restart = client.get("/api/v1/power/dry_run_restart").await;
    assert_eq!(dry_run_restart.json(), false);
    // The host's actions only say what they would run.
    let confirmed = json!({"confirm": true});
    for action in ["reboot", "shutdown"] {
        let said = invoke(&client, &format!("power/{action}"), confirmed.clone()).await;
        assert_eq!(said["output"]["dry_run"], true, "{said}");
    }
    assert!(!restart.is_requested(), "a dry run acted");
    // The restart is done: asked of the server, which stops and exits with 75.
    let restarted = invoke(&client, "power/restart", confirmed).await;
    assert_eq!(restarted["output"]["dry_run"], false, "{restarted}");
    assert!(restart.is_requested(), "{restarted}");
}
