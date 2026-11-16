//! Reports by an extension over HTTP (P47), with `OSCOPE_PYTHON=1`: the
//! sample Python generator, `example.fields`, packed and installed; its
//! report made by the job `report.make` and served as PDF and HTML; and
//! what it refuses failing the job, with its reason.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use oscope_api::{Api, Roots};
use oscope_ext::{Extensions, PythonRuntime};
use oscope_jobs::{JobService, JobsConfig};
use oscope_store::Store;
use serde_json::{Value, json};
use teta_wot::testing::TestClient;

/// Whether to run: `OSCOPE_PYTHON=1`, with uv installed (CLAUDE.md).
fn python() -> bool {
    let on = std::env::var("OSCOPE_PYTHON").is_ok_and(|value| value == "1");
    if !on {
        eprintln!("skipped: set OSCOPE_PYTHON=1, with uv installed, to run it");
    }
    on
}

/// Invokes `action` with `input`, and waits for it to end: its final JSON.
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

/// Makes a report on `project` as `options` say, and waits for its job to
/// end: the job's JSON.
async fn report(client: &TestClient, project: &str, options: Value) -> Value {
    let params = json!({"project": project, "options": options});
    let input = json!({"kind": "report.make", "params": params});
    let submitted = invoke(client, "jobs/submit", input).await;
    let href = submitted["output"]["href"].as_str().unwrap().to_owned();
    // The first call starts the extension's Python, which uv may set up.
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

#[tokio::test(flavor = "multi_thread")]
async fn a_python_generator_s_report_is_made_and_served() {
    if !python() {
        return;
    }
    let folder = tempfile::tempdir().unwrap();
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/ext-python-report");
    let package = folder.path().join("fields.omext");
    oscope_ext::pack(&source, &package).unwrap();
    let root = folder.path().join("extensions");
    let runtime = PythonRuntime::default();
    let installing = Extensions::open_with(&root, Some(runtime.clone())).unwrap();
    installing.install(&package).unwrap();
    let extensions = Extensions::open_with(&root, Some(runtime)).unwrap();
    let roots = Roots {
        data: folder.path().join("data"),
        scratch: folder.path().join("scratch"),
    };
    let store = Store::open(&roots.data, &roots.data.join("index.sqlite")).unwrap();
    let jobs = JobService::open(&folder.path().join("jobs.sqlite"), JobsConfig::default());
    let api = Api::builder(&roots)
        .projects(store)
        .jobs(jobs.unwrap())
        .extensions(extensions)
        .build()
        .unwrap();
    let client = TestClient::start(api.into_thing_server()).await.unwrap();
    let created = invoke(&client, "projects/create", json!({"name": "a bench"})).await;
    let project = created["output"]["id"].as_str().unwrap().to_owned();
    // The MIChrome 20's sensor under three objectives.
    let sensor = json!({"width": 5472, "height": 3648, "pitch_um": 2.4});
    let params = json!({"sensor": sensor, "magnifications": [5, 10, 20]});
    let options = json!({"generator": "example.fields", "params": params});
    let made = report(&client, &project, options).await;
    assert_eq!(made["state"], "completed", "{made}");
    let folder = made["results"][0]
        .as_str()
        .unwrap()
        .replacen("projects/", "", 1);
    let html = client
        .get(&format!("/api/v1/data/{folder}/report.html"))
        .await
        .text();
    for want in [
        "Fields of view",
        "0.657 × 0.438",
        "data:image/png;base64,iVBOR",
        "<svg",
    ] {
        assert!(html.contains(want), "{want} in {html}");
    }
    let pdf = client
        .get(&format!("/api/v1/data/{folder}/report.pdf"))
        .await;
    assert!(pdf.body.starts_with(b"%PDF-"));
    // Refused by the generator: the job fails, saying why.
    let wrong = json!({"generator": "example.fields", "params": {"magnifications": [20]}});
    let failed = report(&client, &project, wrong).await;
    let error = failed["error"].as_str().unwrap_or("");
    assert!(
        failed["state"] == "failed" && error.contains("fields need a sensor"),
        "{failed}"
    );
}
