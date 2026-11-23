//! The `pipeline` Thing over HTTP (W7.1, ADR-0052): the catalogue; `check`,
//! every fault at once at its node and port; the library, kept, listed,
//! read and deleted, with its changes told, and names that would leave its
//! folder refused; and `run`, the library's recipe and one given, on a
//! project's records as jobs, refused for a recipe with faults.

use std::path::Path;
use std::time::{Duration, Instant};

use oscope_api::{Api, Roots};
use oscope_jobs::{JobService, JobsConfig};
use oscope_store::{ImageInfo, NewProject, Provenance, SampleType, Samples, Store};
use serde_json::{Value, json};
use teta_wot::http::axum::http::Method;
use teta_wot::testing::TestClient;

async fn server(data: &Path) -> TestClient {
    let roots = Roots {
        data: data.to_owned(),
        scratch: data.join("scratch"),
    };
    let store = Store::open(data, &data.join("index.sqlite")).unwrap();
    let jobs = JobService::open(&data.join("jobs.sqlite"), JobsConfig::default()).unwrap();
    let api = Api::builder(&roots)
        .projects(store)
        .jobs(jobs)
        .build()
        .unwrap();
    TestClient::start(api.into_thing_server()).await.unwrap()
}

/// Invokes `action`, `thing/action`, with `input`, and waits for it to end:
/// its final JSON.
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
    let deadline = Instant::now() + Duration::from_secs(120);
    loop {
        let job = client.get(href).await.json();
        if ["completed", "failed", "cancelled"].contains(&job["state"].as_str().unwrap_or("")) {
            return job;
        }
        assert!(Instant::now() < deadline, "{job}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// A recipe: a Gaussian blur of `sigma`.
fn blur(sigma: f64) -> Value {
    json!({
        "format": 1,
        "name": "blurred",
        "inputs": [{"name": "image", "type": "image"}],
        "nodes": [{"id": "blur", "node": "filter.gaussian", "params": {"sigma": sigma}}],
        "edges": [{"from": "input.image", "to": "blur.image"}],
        "outputs": [{"name": "image", "from": "blur.image"}],
    })
}

#[tokio::test(flavor = "multi_thread")]
async fn the_catalogue_lists_each_node_with_its_ports_and_parameters() {
    let folder = tempfile::tempdir().unwrap();
    let client = server(folder.path()).await;
    let nodes = client.get("/api/v1/pipeline/nodes").await;
    assert_eq!(nodes.status, 200, "{}", nodes.text());
    let nodes = nodes.json();
    let gaussian = nodes
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["id"] == "filter.gaussian");
    let gaussian = gaussian.unwrap_or_else(|| panic!("{nodes}"));
    assert_eq!(gaussian["inputs"][0]["name"], "image", "{gaussian}");
    assert_eq!(gaussian["outputs"][0]["name"], "image", "{gaussian}");
    assert!(
        gaussian["params"]["properties"]["sigma"].is_object(),
        "{gaussian}"
    );
    assert!(nodes.as_array().unwrap().len() >= 30, "{nodes}");
}

#[tokio::test(flavor = "multi_thread")]
async fn check_gives_every_fault_at_its_node_and_port() {
    let folder = tempfile::tempdir().unwrap();
    let client = server(folder.path()).await;
    let fine = invoke(&client, "pipeline/check", json!({"recipe": blur(2.0)})).await;
    assert_eq!(fine["output"], json!({"faults": []}), "{fine}");
    // A missing input and a type mismatch, both at once.
    let mut broken = blur(2.0);
    broken["nodes"] = json!([
        {"id": "blur", "node": "filter.gaussian", "params": {"sigma": 2.0}},
        {"id": "level", "node": "threshold.otsu"},
        {"id": "open", "node": "morphology.open", "params": {"radius": 1}}
    ]);
    broken["edges"] = json!([
        {"from": "input.image", "to": "blur.image"},
        {"from": "blur.image", "to": "open.mask"}
    ]);
    let checked = invoke(&client, "pipeline/check", json!({"recipe": broken})).await;
    let faults = checked["output"]["faults"].as_array().unwrap().clone();
    let places: Vec<(&str, &str)> = faults
        .iter()
        .map(|f| {
            (
                f["node"].as_str().unwrap_or(""),
                f["port"].as_str().unwrap_or(""),
            )
        })
        .collect();
    assert!(places.contains(&("level", "image")), "{checked}");
    assert!(places.contains(&("open", "mask")), "{checked}");
    // Not a recipe: one fault, at the recipe.
    let wrong = invoke(&client, "pipeline/check", json!({"recipe": {"nodes": 3}})).await;
    assert_eq!(wrong["output"]["faults"][0]["at"], "recipe", "{wrong}");
}

#[tokio::test(flavor = "multi_thread")]
async fn the_library_keeps_lists_reads_and_deletes_recipes_and_tells_its_changes() {
    let folder = tempfile::tempdir().unwrap();
    let client = server(folder.path()).await;
    let mut changes = client.events("/api/v1/pipeline/changed").await;
    let listed = client.get("/api/v1/pipeline/recipes").await.json();
    let stock: Vec<&str> = listed
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["stock"] == true)
        .map(|r| r["name"].as_str().unwrap())
        .collect();
    assert!(
        stock.contains(&"count-nuclei") && stock.contains(&"levels"),
        "{listed}"
    );

    let saved = invoke(
        &client,
        "pipeline/save_recipe",
        json!({"name": "mine", "recipe": blur(3.0)}),
    )
    .await;
    assert_eq!(saved["status"], "completed", "{saved}");
    let told = changes.next_within(Duration::from_secs(10)).await;
    assert_eq!(told, Some(json!("mine")));
    let kept = client.get("/api/v1/pipeline/recipe/mine").await;
    assert_eq!(kept.status, 200, "{}", kept.text());
    let kept = kept.json();
    assert_eq!(
        (&kept["name"], &kept["stock"]),
        (&json!("mine"), &json!(false))
    );
    assert_eq!(kept["recipe"]["nodes"][0]["params"]["sigma"], 3.0);
    let listed = client.get("/api/v1/pipeline/recipes").await.json();
    let last = listed.as_array().unwrap().last().unwrap().clone();
    assert_eq!(
        last,
        json!({"name": "mine", "title": "blurred", "stock": false})
    );
    // A stock recipe reads, and can't be changed.
    let levels = client.get("/api/v1/pipeline/recipe/levels").await.json();
    assert_eq!(levels["stock"], true, "{levels}");
    let refused = invoke(&client, "pipeline/delete_recipe", json!({"name": "levels"})).await;
    assert!(
        refused["error"].to_string().contains("refused (400)"),
        "{refused}"
    );

    let deleted = invoke(&client, "pipeline/delete_recipe", json!({"name": "mine"})).await;
    assert_eq!(deleted["status"], "completed", "{deleted}");
    assert_eq!(
        changes.next_within(Duration::from_secs(10)).await,
        Some(json!("mine"))
    );
    let gone = client.get("/api/v1/pipeline/recipe/mine").await;
    assert_eq!(gone.status, 404, "{}", gone.text());
    assert!(!folder.path().join("recipes").join("mine.json").exists());
}

#[tokio::test(flavor = "multi_thread")]
async fn names_that_would_leave_the_library_are_refused() {
    let folder = tempfile::tempdir().unwrap();
    let client = server(folder.path()).await;
    for name in ["../out", "a/b", ".hidden", "Capital", ""] {
        let saved = invoke(
            &client,
            "pipeline/save_recipe",
            json!({"name": name, "recipe": blur(1.0)}),
        )
        .await;
        assert!(
            saved["error"].to_string().contains("refused (400)"),
            "{name}: {saved}"
        );
    }
    for path in ["..%2Fout", "%2E%2E", "Capital"] {
        let read = client
            .request(
                Method::GET,
                &format!("/api/v1/pipeline/recipe/{path}"),
                &[],
                None,
            )
            .await;
        assert_eq!(read.status, 400, "{path}: {}", read.text());
    }
    assert!(!folder.path().join("out.json").exists());
    assert!(!folder.path().join("recipes").exists());
}

#[tokio::test(flavor = "multi_thread")]
async fn run_makes_a_result_for_each_record_and_refuses_a_recipe_with_faults() {
    let folder = tempfile::tempdir().unwrap();
    // Two pictures of 64 × 48 grey pixels in a project.
    let store = Store::open(folder.path(), &folder.path().join("index.sqlite")).unwrap();
    let new = NewProject {
        name: "run on".to_owned(),
        ..NewProject::default()
    };
    let project = store.create(new).unwrap().id;
    let mut pictures = Vec::new();
    for shade in [40_u8, 200] {
        let provenance = Provenance {
            recipe: Value::Null,
            nodes: Vec::new(),
            inputs: Vec::new(),
            hashes: Default::default(),
            software: "a test".to_owned(),
        };
        let info = ImageInfo::new(64, 48, 1, SampleType::U8);
        let draft = store
            .result(project, "a picture", provenance, Some(&info))
            .unwrap();
        let image = draft.image().unwrap();
        image
            .write(0, (0, 0), (64, 48), &Samples::U8(vec![shade; 64 * 48]))
            .unwrap();
        pictures.push(draft.commit().unwrap().id.to_string());
    }
    drop(store);
    let client = server(folder.path()).await;
    let records: Vec<Value> = pictures
        .iter()
        .map(|id| json!({"kind": "result", "id": id}))
        .collect();

    // The library's, on both.
    let input = json!({"project": project, "recipe": {"named": "smooth"}, "records": records});
    let started = invoke(&client, "pipeline/run", input).await;
    let jobs = started["output"]
        .as_array()
        .unwrap_or_else(|| panic!("{started}"))
        .clone();
    assert_eq!(jobs.len(), 2, "{started}");
    for job in &jobs {
        let ended = ended(&client, job["href"].as_str().unwrap()).await;
        assert_eq!(ended["state"], "completed", "{ended}");
        assert_eq!(ended["kind"], "workspace.run", "{ended}");
    }
    let results = client
        .get(&format!("/api/v1/workspace/results/{project}"))
        .await
        .json();
    let smoothed: Vec<&Value> = results
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["name"] == "Smoothed")
        .collect();
    assert_eq!(smoothed.len(), 2, "{results}");
    assert_eq!(
        smoothed[0]["provenance"]["nodes"][0]["node"],
        "filter.gaussian"
    );

    // One given, on one.
    let given =
        json!({"project": project, "recipe": {"given": blur(1.5)}, "records": [records[0]]});
    let started = invoke(&client, "pipeline/run", given).await;
    assert_eq!(
        started["output"].as_array().map(Vec::len),
        Some(1),
        "{started}"
    );

    // One with faults, or none to run on: refused, no job made.
    let broken = json!({"project": project, "recipe": {"given": blur(99.0)}, "records": records});
    let refused = invoke(&client, "pipeline/run", broken).await;
    assert!(
        refused["error"].to_string().contains("can't run"),
        "{refused}"
    );
    let none = json!({"project": project, "recipe": {"named": "smooth"}, "records": []});
    let refused = invoke(&client, "pipeline/run", none).await;
    assert!(
        refused["error"].to_string().contains("no records"),
        "{refused}"
    );
    let unknown =
        json!({"project": project, "recipe": {"named": "nothing-here"}, "records": records});
    let refused = invoke(&client, "pipeline/run", unknown).await;
    assert!(
        refused["error"].to_string().contains("not found (404)"),
        "{refused}"
    );
    let listed = client.get("/api/v1/jobs/list").await.json();
    let runs = listed.as_array().map_or(0, |all| {
        all.iter().filter(|j| j["kind"] == "workspace.run").count()
    });
    assert_eq!(runs, 3, "{listed}");
}
