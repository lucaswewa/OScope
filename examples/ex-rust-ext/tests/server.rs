//! The sample, packed and installed, then loaded by a server's start: the
//! `extensions` Thing lists it loaded, and the `preview` Thing offers its
//! node to live recipes, and takes one with it; and its PGM importer and
//! exporter bring a file in and take it out again, as jobs.

use std::sync::Arc;
use std::time::{Duration, Instant};

use oscope_api::devices::{Devices, DevicesConfig};
use oscope_api::{Api, Roots};
use oscope_ext::Extensions;
use oscope_jobs::{JobService, JobsConfig};
use oscope_store::Store;
use serde_json::{Value, json};
use teta_wot::testing::TestClient;

/// Invokes `action` with `input`, and waits for its invocation to end: its
/// final JSON.
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
        assert!(
            Instant::now() < deadline,
            "{action} didn't end: {invocation}"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_start_loads_the_sample_and_live_recipes_can_use_its_node() {
    let folder = tempfile::tempdir().unwrap();
    let (data, package) = (
        folder.path().join("data"),
        folder.path().join("emboss.omext"),
    );
    let library = ex_rust_ext::library().unwrap();
    ex_rust_ext::pack(&library, folder.path(), &package).unwrap();
    let extensions = data.join("extensions");
    Extensions::open(&extensions)
        .unwrap()
        .install(&package)
        .unwrap();

    // A start, with the fakes for devices.
    let fake = || "fake".to_owned();
    let config = DevicesConfig {
        camera: fake(),
        stage: fake(),
        illumination: fake(),
        objective: fake(),
        ..DevicesConfig::default()
    };
    let devices = Arc::new(Devices::from_config(&config, folder.path()).await.unwrap());
    let roots = Roots {
        data: data.clone(),
        scratch: folder.path().join("scratch"),
    };
    let store = Store::open(&data, &data.join("index.sqlite")).unwrap();
    let jobs = JobService::open(&folder.path().join("jobs.sqlite"), JobsConfig::default()).unwrap();
    let api = Api::builder(&roots)
        .devices(devices)
        .extensions(Extensions::open(&extensions).unwrap())
        .projects(store)
        .jobs(jobs)
        .build()
        .unwrap();
    let client = TestClient::start(api.into_thing_server()).await.unwrap();
    let installed = client.get("/api/v1/extensions/installed").await.json();
    assert_eq!(installed[0]["loaded"], "0.1.0", "{installed}");
    let provided = client.get("/api/v1/extensions/provided").await.json();
    let id = "org.oscope.examples.emboss";
    let expected = json!({
        "nodes": { "example.emboss": id },
        "panels": { "example.emboss": id },
        "importers": { "example.pgm.read": id },
        "exporters": { "example.pgm.write": id },
    });
    assert_eq!(provided, expected);
    let nodes = client.get("/api/v1/preview/nodes").await.json();
    let emboss = nodes
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["id"] == "example.emboss");
    assert_eq!(emboss.unwrap()["capabilities"]["live"], true);

    // A live recipe with it, checked against a budget for unoptimised
    // builds, on a small viewport.
    let budget = client
        .put_json("/api/v1/preview/recipe_budget_ms", &json!(2000.0))
        .await;
    assert!(budget.status.is_success(), "{}", budget.text());
    let defaults = json!({ "width": 160, "height": 120, "quality": 80, "max_fps": 30.0 });
    let set = client.put_json("/api/v1/preview/defaults", &defaults).await;
    assert!(set.status.is_success(), "{}", set.text());
    let recipe = json!({
        "format": 1,
        "inputs": [{ "name": "image", "type": "image" }, { "name": "region", "type": "json" }],
        "nodes": [{ "id": "emboss", "node": "example.emboss", "params": { "strength": 2 } }],
        "edges": [{ "from": "input.image", "to": "emboss.image" }],
        "outputs": [{ "name": "image", "from": "emboss.image" }],
    });
    let set = invoke(
        &client,
        "preview/set_live_recipe",
        json!({ "recipe": recipe }),
    )
    .await;
    assert_eq!(set["status"], "completed", "{set}");
    assert_eq!(set["output"]["steps"], json!(["emboss"]), "{set}");

    // Its importer and exporter, after OScope's own.
    let formats = client.get("/api/v1/projects/formats").await.json();
    let pgm = |list: &str, id: &str| {
        let found = formats[list]
            .as_array()
            .unwrap()
            .iter()
            .find(|f| f["id"] == id)
            .cloned();
        found.map(|f| (f["name"].clone(), f["extensions"].clone()))
    };
    let named = Some((json!("PGM (Netpbm)"), json!(["pgm"])));
    assert_eq!(pgm("importers", "example.pgm.read"), named, "{formats}");
    assert_eq!(pgm("exporters", "example.pgm.write"), named, "{formats}");

    // A 16-bit PGM in, as an acquisition, and out again.
    let file = folder.path().join("ramp.pgm");
    let mut ramp = b"P5\n# a ramp\n4 3\n1000\n".to_vec();
    let samples: Vec<u8> = (0..12_u16).flat_map(|i| (i * 80).to_be_bytes()).collect();
    ramp.extend(&samples);
    std::fs::write(&file, &ramp).unwrap();
    let made = invoke(&client, "projects/create", json!({ "name": "PGM" })).await;
    let project = made["output"]["id"].as_str().unwrap().to_owned();
    let params = json!({ "project": project, "path": file });
    let imported = job(&client, "projects.import", params).await;
    let acquisition = imported["results"][0]
        .as_str()
        .unwrap()
        .rsplit('/')
        .next()
        .unwrap()
        .to_owned();
    let params =
        json!({ "project": project, "record": acquisition, "format": "example.pgm.write" });
    let exported = job(&client, "projects.export", params).await;
    let path = exported["results"][0]
        .as_str()
        .unwrap()
        .split_once(&project)
        .unwrap()
        .1
        .to_owned();
    let written = std::fs::read(
        data.join("projects")
            .join(&project)
            .join(path.trim_start_matches('/')),
    )
    .unwrap();
    let header = b"P5\n4 3\n65535\n";
    assert_eq!(&written[..header.len()], header);
    assert_eq!(&written[header.len()..], samples.as_slice());
}

/// Submits a job of `kind` with `params`, and waits for it to complete: its
/// JSON.
async fn job(client: &TestClient, kind: &str, params: Value) -> Value {
    let submitted = invoke(
        client,
        "jobs/submit",
        json!({ "kind": kind, "params": params }),
    )
    .await;
    assert_eq!(submitted["status"], "completed", "{submitted}");
    let href = submitted["output"]["href"].as_str().unwrap().to_owned();
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        let job = client.get(&href).await.json();
        if ["completed", "failed", "cancelled"].contains(&job["state"].as_str().unwrap_or("")) {
            assert_eq!(job["state"], "completed", "{job}");
            return job;
        }
        assert!(Instant::now() < deadline, "{job}");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

#[test]
fn a_library_whose_nodes_are_not_its_manifest_s_does_not_load() {
    let folder = tempfile::tempdir().unwrap();
    let package = folder.path().join("emboss.omext");
    ex_rust_ext::pack(&ex_rust_ext::library().unwrap(), folder.path(), &package).unwrap();
    // The same library, its manifest providing another node.
    let source = folder.path().join("ext-rust-node");
    let manifest = std::fs::read_to_string(source.join("manifest.toml")).unwrap();
    // Its panel, which submits to the node the library has, left out too.
    let other = (manifest.replace(
        "nodes = [\"example.emboss\"]",
        "nodes = [\"example.relief\"]",
    ))
    .replace(
        "panels = [\"example.emboss\"]
",
        "",
    )
    .replace(
        "assets = [\"panels/example.emboss.json\"]
",
        "",
    );
    std::fs::write(source.join("manifest.toml"), other).unwrap();
    oscope_ext::pack(&source, &package).unwrap();
    let root = folder.path().join("extensions");
    Extensions::open(&root).unwrap().install(&package).unwrap();
    let extensions = Extensions::open(&root).unwrap();
    assert!(extensions.loaded().is_empty() && extensions.nodes().is_empty());
    let extension = extensions.get("org.oscope.examples.emboss").unwrap();
    let said =
        "its library has the nodes `example.emboss`, and its manifest provides `example.relief`";
    assert_eq!(extension.problem.as_deref(), Some(said));

    // Its manifest providing no importer.
    let other = manifest.replace("importers = [\"example.pgm.read\"]\n", "");
    std::fs::write(source.join("manifest.toml"), other).unwrap();
    oscope_ext::pack(&source, &package).unwrap();
    let root = folder.path().join("again");
    Extensions::open(&root).unwrap().install(&package).unwrap();
    let extensions = Extensions::open(&root).unwrap();
    let extension = extensions.get("org.oscope.examples.emboss").unwrap();
    let problem = extension.problem.as_deref().unwrap_or("");
    assert!(
        problem.contains("the importers `example.pgm.read`"),
        "{problem}"
    );
}
