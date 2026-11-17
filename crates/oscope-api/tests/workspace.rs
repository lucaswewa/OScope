//! The `workspace` Thing over HTTP (P48): an analysis run as a job, its
//! result listed with its files and provenance, made again with a
//! parameter changed and the two compared, and results deleted;
//! annotations kept, measured and removed (P44); with `OSCOPE_PYTHON=1`, an
//! extension's node in a recipe, named with its version; the refusals, and
//! the Thing's description.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use oscope_api::{Api, Roots};
use oscope_ext::{Extensions, PythonRuntime};
use oscope_jobs::{JobService, JobsConfig};
use oscope_store::{
    ImageInfo, NewProject, NodeVersion, PixelSize, Provenance, SampleType, Samples, Store,
};
use serde_json::{Value, json};
use teta_wot::testing::TestClient;

fn store(data: &Path) -> Store {
    Store::open(data, &data.join("index.sqlite")).unwrap()
}

async fn server(data: &Path) -> TestClient {
    let roots = Roots {
        data: data.to_owned(),
        scratch: data.join("scratch"),
    };
    let jobs = JobService::open(&data.join("jobs.sqlite"), JobsConfig::default()).unwrap();
    let api = Api::builder(&roots)
        .projects(store(data))
        .jobs(jobs)
        .build()
        .unwrap();
    TestClient::start(api.into_thing_server()).await.unwrap()
}

/// Invokes the Workspace's `action` with `input`, and waits for it to end:
/// its final JSON.
async fn invoke(client: &TestClient, action: &str, input: Value) -> Value {
    let path = format!("/api/v1/workspace/{action}");
    let started = client.post_json(&path, Some(&input)).await;
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

/// Waits for the job an action started to end: the result it made, and the
/// job's JSON.
async fn made(client: &TestClient, invocation: &Value) -> (String, Value) {
    let href = invocation["output"]["href"].as_str();
    let href = href.unwrap_or_else(|| panic!("{invocation}"));
    let deadline = Instant::now() + Duration::from_secs(120);
    loop {
        let job = client.get(href).await.json();
        if ["completed", "failed", "cancelled"].contains(&job["state"].as_str().unwrap_or("")) {
            let id = job["results"][0].as_str().unwrap_or("").rsplit('/').next();
            return (id.unwrap_or("").to_owned(), job);
        }
        assert!(Instant::now() < deadline, "{job}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// A recipe: Otsu's level, and its mask.
fn otsu() -> Value {
    json!({
        "format": 1,
        "name": "bright",
        "inputs": [{"name": "image", "type": "image"}],
        "nodes": [{"id": "otsu", "node": "threshold.otsu", "params": {}}],
        "edges": [{"from": "input.image", "to": "otsu.image"}],
        "outputs": [{"name": "mask", "from": "otsu.mask"}, {"name": "level", "from": "otsu.level"}],
    })
}

#[tokio::test(flavor = "multi_thread")]
async fn analyses_are_run_listed_made_again_compared_and_deleted_over_http() {
    let folder = tempfile::tempdir().unwrap();
    // A picture of 64 × 48 grey pixels: a bright square on a dark ground.
    let made_by = store(folder.path());
    let new = NewProject {
        name: "analysed".to_owned(),
        ..NewProject::default()
    };
    let project = made_by.create(new).unwrap().id;
    let provenance = Provenance {
        recipe: Value::Null,
        nodes: vec![NodeVersion {
            node: "a camera".to_owned(),
            version: "1".to_owned(),
            extension: None,
        }],
        inputs: Vec::new(),
        hashes: Default::default(),
        software: "a test".to_owned(),
    };
    let info = ImageInfo::new(64, 48, 1, SampleType::U8);
    let draft = made_by
        .result(project, "a picture", provenance, Some(&info))
        .unwrap();
    let square = |x: u32, y: u32| (16..48).contains(&x) && (12..36).contains(&y);
    let pixels: Vec<u8> = (0..48)
        .flat_map(|y| (0..64).map(move |x| if square(x, y) { 200 } else { 20 }))
        .collect();
    let image = draft.image().unwrap();
    image
        .write(0, (0, 0), (64, 48), &Samples::U8(pixels))
        .unwrap();
    let picture = draft.commit().unwrap().id.to_string();
    drop(made_by);
    let client = server(folder.path()).await;
    // Run, as a job.
    let analysis = json!({"recipe": otsu(), "input": {"kind": "result", "id": picture}});
    let input = json!({"project": project, "analysis": analysis});
    let (first, job) = made(&client, &invoke(&client, "run", input).await).await;
    assert_eq!(job["state"], "completed", "{job}");
    // Listed, with its files and its provenance.
    let listed = client
        .get(&format!("/api/v1/workspace/results/{project}"))
        .await
        .json();
    let analysed = listed.as_array().unwrap().iter().find(|r| r["id"] == first);
    let analysed = analysed.unwrap_or_else(|| panic!("{listed}"));
    assert_eq!(
        (&analysed["name"], &analysed["files"]),
        (
            &json!("bright"),
            &json!(["level.json", "mask.png", "summary.json"])
        )
    );
    let provenance = &analysed["provenance"];
    assert_eq!(
        provenance["nodes"],
        json!([{"node": "threshold.otsu", "version": "1", "extension": null}])
    );
    let hash = provenance["hashes"][&picture].as_str().unwrap_or("");
    assert!(
        hash.starts_with("sha256:") && hash.len() == 71,
        "{provenance}"
    );
    let mask = client
        .get(&format!("/api/v1/data/{project}/results/{first}/mask.png"))
        .await;
    assert_eq!(mask.header("content-type"), Some("image/png"));
    // Made again with fewer bins, and compared.
    let changes = json!({"otsu": {"bins": 16}});
    let again = json!({"project": project, "result": first, "changes": changes});
    let (second, job) = made(&client, &invoke(&client, "rerun", again).await).await;
    assert_eq!(job["state"], "completed", "{job}");
    let changed = client
        .get(&format!(
            "/api/v1/workspace/difference/{project}/{first}/{second}"
        ))
        .await
        .json();
    let path = "/recipe/workspace.run/recipe/nodes/otsu/params/bins";
    assert_eq!(
        changed,
        json!([{"path": path, "before": null, "after": 16}])
    );
    // Refused: a recipe without an image; a result not made here; a result
    // that isn't; and the picture, while the analyses were made from it.
    let mut blind = otsu();
    blind["inputs"][0]["name"] = json!("picture");
    blind["edges"][0]["from"] = json!("input.picture");
    let analysis = json!({"recipe": blind, "input": {"kind": "result", "id": picture}});
    let input = json!({"project": project, "analysis": analysis});
    let refused = invoke(&client, "run", input).await;
    assert!(
        refused["error"].to_string().contains("refused (400)"),
        "{refused}"
    );
    let not_here = json!({"project": project, "result": picture});
    let refused = invoke(&client, "rerun", not_here.clone()).await;
    assert!(
        refused["error"]
            .to_string()
            .contains("wasn't made in the Workspace"),
        "{refused}"
    );
    let unknown = client
        .get(&format!(
            "/api/v1/workspace/difference/{project}/{first}/{}",
            uuid::Uuid::now_v7()
        ))
        .await;
    assert_eq!(unknown.status, 404);
    let refused = invoke(&client, "delete_result", not_here).await;
    assert!(
        refused["error"].to_string().contains("refused (400)"),
        "{refused}"
    );
    // Deleted, the analyses first.
    for id in [&second, &first, &picture] {
        let input = json!({"project": project, "result": id});
        let deleted = invoke(&client, "delete_result", input).await;
        assert_eq!(deleted["status"], "completed", "{deleted}");
    }
    let listed = client
        .get(&format!("/api/v1/workspace/results/{project}"))
        .await
        .json();
    assert_eq!(listed, json!([]));
}

/// Whether to run: `OSCOPE_PYTHON=1`, with uv installed (CLAUDE.md).
fn python() -> bool {
    let on = std::env::var("OSCOPE_PYTHON").is_ok_and(|value| value == "1");
    if !on {
        eprintln!("skipped: set OSCOPE_PYTHON=1, with uv installed, to run it");
    }
    on
}

#[tokio::test(flavor = "multi_thread")]
async fn an_extension_s_node_is_named_with_its_version_in_the_provenance() {
    if !python() {
        return;
    }
    let folder = tempfile::tempdir().unwrap();
    // P46's stub, packed and installed.
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/ext-python-cells");
    let package = folder.path().join("cells.omext");
    oscope_ext::pack(&source, &package).unwrap();
    let root = folder.path().join("extensions");
    let runtime = PythonRuntime::default();
    let installing = Extensions::open_with(&root, Some(runtime.clone())).unwrap();
    installing.install(&package).unwrap();
    let extensions = Extensions::open_with(&root, Some(runtime)).unwrap();
    let data = folder.path().join("data");
    let made_by = store(&data);
    let new = NewProject {
        name: "segmented".to_owned(),
        ..NewProject::default()
    };
    let project = made_by.create(new).unwrap().id;
    let info = ImageInfo::new(32, 24, 1, SampleType::U8);
    let draft = made_by
        .result(
            project,
            "a picture",
            Provenance {
                recipe: Value::Null,
                nodes: Vec::new(),
                inputs: Vec::new(),
                hashes: Default::default(),
                software: "a test".to_owned(),
            },
            Some(&info),
        )
        .unwrap();
    let pixels: Vec<u8> = (0..32 * 24)
        .map(|i| if i % 32 < 16 { 10 } else { 200 })
        .collect();
    draft
        .image()
        .unwrap()
        .write(0, (0, 0), (32, 24), &Samples::U8(pixels))
        .unwrap();
    let picture = draft.commit().unwrap().id.to_string();
    drop(made_by);
    let roots = Roots {
        data: data.clone(),
        scratch: folder.path().join("scratch"),
    };
    let jobs = JobService::open(&data.join("jobs.sqlite"), JobsConfig::default()).unwrap();
    let api = Api::builder(&roots)
        .projects(store(&data))
        .jobs(jobs)
        .extensions(extensions)
        .build()
        .unwrap();
    let client = TestClient::start(api.into_thing_server()).await.unwrap();
    let recipe = json!({
        "format": 1,
        "name": "segmented",
        "inputs": [{"name": "image", "type": "image"}],
        "nodes": [{"id": "segment", "node": "example.cells_segment"}],
        "edges": [{"from": "input.image", "to": "segment.image"}],
        "outputs": [{"name": "labels", "from": "segment.image"}],
    });
    let analysis = json!({"recipe": recipe, "input": {"kind": "result", "id": picture}});
    let input = json!({"project": project, "analysis": analysis});
    let (id, job) = made(&client, &invoke(&client, "run", input).await).await;
    assert_eq!(job["state"], "completed", "{job}");
    let listed = client
        .get(&format!("/api/v1/workspace/results/{project}"))
        .await
        .json();
    let result = listed
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == id)
        .unwrap();
    assert_eq!(
        result["provenance"]["nodes"],
        json!([{"node": "example.cells_segment", "version": "1",
                "extension": "org.oscope.examples.cells-stub@0.1.0"}])
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn the_workspace_thing_describes_itself() {
    let folder = tempfile::tempdir().unwrap();
    let client = server(folder.path()).await;
    let mut td = client.get("/api/v1/workspace/").await.json();
    let id = td.as_object_mut().unwrap().remove("id").unwrap();
    assert!(id.as_str().unwrap().starts_with("urn:uuid:"), "{id}");
    insta::assert_json_snapshot!("workspace", td);
}

#[tokio::test(flavor = "multi_thread")]
async fn annotations_are_kept_measured_and_removed_over_http() {
    let folder = tempfile::tempdir().unwrap();
    // An acquisition of 600 × 400 grey pixels of 0.5 µm, each its column.
    let made = store(folder.path());
    let new = NewProject {
        name: "annotated".to_owned(),
        ..NewProject::default()
    };
    let project = made.create(new).unwrap().id;
    let mut info = ImageInfo::new(600, 400, 1, SampleType::U16);
    info.pixel_size = Some(PixelSize {
        x: 0.5,
        y: 0.5,
        z: 1.0,
    });
    let draft = made.acquisition(project, "a slide", &info).unwrap();
    let columns: Vec<u16> = (0..400).flat_map(|_| 0..600).collect();
    let writer = draft.image().unwrap();
    writer
        .write(0, (0, 0), (600, 400), &Samples::U16(columns))
        .unwrap();
    let id = draft.commit().unwrap().id.to_string();
    drop(made);
    let client = server(folder.path()).await;
    // A square of 20 µm, its columns 100–139: kept with an ID.
    let ring = json!([
        [50.0, 50.0],
        [70.0, 50.0],
        [70.0, 70.0],
        [50.0, 70.0],
        [50.0, 50.0]
    ]);
    let feature = json!({"type": "Feature", "geometry": {"type": "Polygon", "coordinates": [ring]}, "properties": {"name": "a square"}});
    let input = json!({"project": project, "kind": "acquisition", "record": id, "layer": "areas", "feature": feature});
    let kept = invoke(&client, "annotate", input.clone()).await;
    assert_eq!(kept["status"], "completed", "{kept}");
    assert_eq!(kept["output"]["id"], "1");
    // A point, in a layer of its own.
    let mut point = input.clone();
    point["layer"] = json!("points");
    point["feature"] =
        json!({"type": "Feature", "geometry": {"type": "Point", "coordinates": [1.0, 2.0]}});
    let kept = invoke(&client, "annotate", point).await;
    assert_eq!(kept["output"]["id"], "2", "{kept}");
    let base = format!("{project}/acquisitions/{id}");
    let listed = client
        .get(&format!("/api/v1/workspace/annotations/{base}"))
        .await
        .json();
    assert_eq!(listed["layers"]["areas"]["type"], "FeatureCollection");
    assert_eq!(
        listed["layers"]["areas"]["features"][0]["properties"]["name"],
        "a square"
    );
    let measured = client
        .get(&format!("/api/v1/workspace/measurements/{base}"))
        .await;
    assert_eq!(measured.status, 200, "{}", measured.text());
    assert_eq!(measured.json().as_array().map(Vec::len), Some(2));
    let square = &measured.json()[0];
    assert_eq!(
        (square["area"].as_f64(), square["length"].as_f64()),
        (Some(400.0), Some(80.0))
    );
    let grey = &square["channels"][0];
    assert_eq!(
        (grey["pixels"].as_u64(), grey["mean"].as_f64()),
        (Some(1600), Some(119.5))
    );
    let csv = client
        .get(&format!(
            "/api/v1/workspace/measurements/{base}?format=csv&layer=areas"
        ))
        .await;
    assert_eq!(
        csv.headers.get("content-type").map(|v| v.to_str().unwrap()),
        Some("text/csv; charset=utf-8")
    );
    assert!(
        csv.text()
            .starts_with("layer,id,name,geometry,length_um,area_um2,"),
        "{}",
        csv.text()
    );
    // The layer's only: its header and its square.
    let rows: Vec<String> = csv.text().split("\r\n").map(str::to_owned).collect();
    assert_eq!(rows.len(), 3, "{rows:?}");
    assert!(
        rows[1].starts_with("areas,1,a square,Polygon,80,400,60,60,1600,119.5,"),
        "{rows:?}"
    );
    // A plane the image hasn't.
    let plane = client
        .get(&format!("/api/v1/workspace/measurements/{base}?plane=1"))
        .await;
    assert_eq!(plane.status, 400, "{}", plane.text());
    // Refused: an open ring, a report, a record that isn't there.
    let mut open = input.clone();
    open["feature"]["geometry"]["coordinates"][0]
        .as_array_mut()
        .unwrap()
        .pop();
    let refused = invoke(&client, "annotate", open).await;
    assert!(
        refused["error"].to_string().contains("refused (400)"),
        "{refused}"
    );
    let report = client
        .get(&format!(
            "/api/v1/workspace/annotations/{project}/reports/{id}"
        ))
        .await;
    assert_eq!(report.status, 400);
    let missing = uuid::Uuid::now_v7();
    let unknown = client
        .get(&format!(
            "/api/v1/workspace/measurements/{project}/results/{missing}"
        ))
        .await;
    assert_eq!(unknown.status, 404, "{}", unknown.text());
    // The square removed: the point is left.
    let input = json!({"project": project, "kind": "acquisition", "record": id, "layer": "areas", "feature": "1"});
    let removed = invoke(&client, "remove_annotation", input).await;
    assert_eq!(removed["status"], "completed", "{removed}");
    let measured = client
        .get(&format!("/api/v1/workspace/measurements/{base}"))
        .await
        .json();
    let left: Vec<&Value> = measured.as_array().unwrap().iter().collect();
    assert_eq!((left.len(), &left[0]["geometry"]), (1, &json!("Point")));
    let listed = client
        .get(&format!("/api/v1/workspace/annotations/{base}"))
        .await
        .json();
    assert_eq!(listed["layers"]["areas"]["features"], json!([]), "{listed}");
}
