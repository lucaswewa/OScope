//! Captures, downloads, exports and imports over HTTP: a single capture
//! with its rendition, a burst, the formats, an export downloaded whole and
//! by ranges and imported back, and what the data route refuses.

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use oscope_api::devices::{Devices, DevicesConfig};
use oscope_api::{Api, Roots};
use oscope_jobs::{JobService, JobsConfig};
use oscope_store::{Scratch, ScratchPolicy, Store};
use serde_json::{Value, json};
use teta_wot::http::axum::http::Method;
use teta_wot::testing::TestClient;

async fn server(folder: &Path) -> TestClient {
    let config = DevicesConfig {
        camera: "fake".to_owned(),
        stage: "fake".to_owned(),
        illumination: "fake".to_owned(),
        objective: "fake".to_owned(),
        ..DevicesConfig::default()
    };
    let devices = Arc::new(Devices::from_config(&config, folder).await.unwrap());
    let roots = Roots {
        data: folder.join("data"),
        scratch: folder.join("scratch"),
    };
    let store = Store::open(&roots.data, &roots.data.join("index.sqlite")).unwrap();
    let policy = ScratchPolicy {
        reserve_bytes: 0,
        low_bytes: 0,
    };
    let jobs = JobService::open(&folder.join("jobs.sqlite"), JobsConfig::default()).unwrap();
    let api = Api::builder(&roots)
        .devices(devices)
        .projects(store)
        .scratch(Scratch::open(&roots.scratch, policy).unwrap())
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
        assert!(Instant::now() < deadline, "{action}: {invocation}");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

/// Starts the job `action` asks for, and waits for it to end: its JSON.
async fn job(client: &TestClient, action: &str, input: Value) -> Value {
    let submitted = invoke(client, action, input).await;
    assert_eq!(submitted["status"], "completed", "{submitted}");
    let href = submitted["output"]["href"].as_str().unwrap().to_owned();
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        let job = client.get(&href).await.json();
        if ["completed", "failed", "cancelled"].contains(&job["state"].as_str().unwrap_or("")) {
            return job;
        }
        assert!(Instant::now() < deadline, "{job}");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

async fn download(client: &TestClient, path: &str, range: Option<&str>) -> (u16, Vec<u8>, Value) {
    let headers: Vec<(&str, &str)> = range.map(|r| ("range", r)).into_iter().collect();
    let answer = client.request(Method::GET, path, &headers, None).await;
    let head = json!({
        "type": answer.header("content-type"),
        "length": answer.header("content-length"),
        "range": answer.header("content-range"),
        "ranges": answer.header("accept-ranges"),
    });
    (answer.status.as_u16(), answer.body.to_vec(), head)
}

#[tokio::test(flavor = "multi_thread")]
async fn captures_exports_downloads_and_imports() {
    let folder = tempfile::tempdir().unwrap();
    let client = server(folder.path()).await;
    let started = invoke(&client, "camera/start_streaming", json!({})).await;
    assert_eq!(started["status"], "completed", "{started}");
    let made = invoke(&client, "projects/create", json!({"name": "Captures"})).await;
    let project = made["output"]["id"].as_str().unwrap().to_owned();

    // A single capture, with its rendition.
    let single = job(
        &client,
        "capture/single",
        json!({"project": project, "name": "one", "rendition": "png"}),
    )
    .await;
    assert_eq!(single["state"], "completed", "{single}");
    assert_eq!(single["kind"], "capture.single");
    let results = single["results"].as_array().unwrap();
    let rendition = results[1].as_str().unwrap();
    assert!(rendition.ends_with("/rendition.png"), "{single}");
    let (status, body, head) = download(&client, &format!("/api/v1/{rendition}"), None).await;
    assert_eq!(
        (status, &head["type"]),
        (200, &json!("image/png")),
        "{head}"
    );
    assert!(body.starts_with(b"\x89PNG"));

    // A burst of five frames.
    let burst = job(
        &client,
        "capture/burst",
        json!({"project": project, "frames": 5}),
    )
    .await;
    assert_eq!(burst["state"], "completed", "{burst}");
    let contents = client
        .get(&format!("/api/v1/projects/project/{project}"))
        .await
        .json();
    let acquisitions = contents["acquisitions"].as_array().unwrap();
    let planes: Vec<(&str, u64)> = acquisitions
        .iter()
        .map(|a| {
            (
                a["name"].as_str().unwrap(),
                a["image"]["planes"].as_u64().unwrap(),
            )
        })
        .collect();
    assert!(
        planes.contains(&("one", 1)) && planes.contains(&("burst of 5", 5)),
        "{planes:?}"
    );
    let first = &acquisitions.iter().find(|a| a["name"] == "one").unwrap();
    assert_eq!(
        first["camera"]["format"],
        json!({"kind": "bayer8", "pattern": "RGGB"})
    );

    // The formats, OScope's own.
    let formats = client.get("/api/v1/projects/formats").await.json();
    let ids = |key: &str| -> Vec<String> {
        let list = formats[key].as_array().unwrap();
        list.iter()
            .map(|f| f["id"].as_str().unwrap().to_owned())
            .collect()
    };
    assert_eq!(ids("importers"), ["tiff", "png", "jpeg"]);
    assert_eq!(ids("exporters"), ["ome-tiff", "tiff16", "png"]);

    // The capture exported as OME-TIFF, downloaded whole and by ranges.
    let record = first["id"].as_str().unwrap();
    let params = json!({"project": project, "record": record, "format": "ome-tiff"});
    let exported = job(
        &client,
        "jobs/submit",
        json!({"kind": "projects.export", "params": params}),
    )
    .await;
    assert_eq!(exported["state"], "completed", "{exported}");
    let path = format!("/api/v1/{}", exported["results"][0].as_str().unwrap());
    assert!(
        path.ends_with(&format!("/acquisitions/{record}/exports/one.ome.tif")),
        "{path}"
    );
    let (status, whole, head) = download(&client, &path, None).await;
    assert_eq!(
        (status, &head["type"], &head["ranges"]),
        (200, &json!("image/tiff"), &json!("bytes"))
    );
    assert!(whole.starts_with(b"II*\0"));
    let length = whole.len();
    assert_eq!(head["length"], json!(length.to_string()));
    for (range, from, to) in [
        ("bytes=0-7", 0, 8),
        ("bytes=100-", 100, length),
        ("bytes=-16", length - 16, length),
    ] {
        let (status, part, head) = download(&client, &path, Some(range)).await;
        assert_eq!(status, 206, "{range}");
        assert_eq!(part, whole[from..to], "{range}");
        let expected = format!("bytes {from}-{}/{length}", to - 1);
        assert_eq!(head["range"], json!(expected), "{range}");
    }
    let (status, _, head) = download(&client, &path, Some(&format!("bytes={length}-"))).await;
    assert_eq!(
        (status, &head["range"]),
        (416, &json!(format!("bytes */{length}")))
    );

    // Imported back: the same frame.
    let file = folder
        .path()
        .join("data")
        .join("projects")
        .join(&project)
        .join(path.split_once(&format!("{project}/")).unwrap().1);
    let params = json!({"project": project, "path": file, "name": "back"});
    let imported = job(
        &client,
        "jobs/submit",
        json!({"kind": "projects.import", "params": params}),
    )
    .await;
    assert_eq!(imported["state"], "completed", "{imported}");
    let contents = client
        .get(&format!("/api/v1/projects/project/{project}"))
        .await
        .json();
    let back = contents["acquisitions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["name"] == "back")
        .unwrap()
        .clone();
    assert_eq!(back["image"]["width"], first["image"]["width"]);
    assert_eq!(back["image"]["sample"], "u8");
    assert_eq!(back["instrument"]["imported"]["format"], "tiff");

    // The captures' recordings are gone from the scratch space.
    let scratch = client.get("/api/v1/system/scratch").await.json();
    assert_eq!(scratch["recordings"], 0, "{scratch}");

    // An importer named that can't read the file: the job fails, saying so.
    let params = json!({"project": project, "path": file, "format": "jpeg"});
    let wrong = job(
        &client,
        "jobs/submit",
        json!({"kind": "projects.import", "params": params}),
    )
    .await;
    assert_eq!(wrong["state"], "failed", "{wrong}");

    // What the data route refuses.
    for (bad, status) in [
        (format!("/api/v1/data/not-a-project/{record}"), 400),
        (
            format!("/api/v1/data/{project}/acquisitions/..%2F..%2Fx"),
            400,
        ),
        (
            format!("/api/v1/data/{project}/acquisitions/{record}/nothing.png"),
            404,
        ),
        (
            format!("/api/v1/data/{project}/acquisitions/{record}/exports"),
            404,
        ),
    ] {
        let (got, _, _) = download(&client, &bad, None).await;
        assert_eq!(got, status, "{bad}");
    }

    // A burst of no frames, and one of too many.
    for frames in [0, 10_001] {
        let refused = invoke(
            &client,
            "capture/burst",
            json!({"project": project, "frames": frames}),
        )
        .await;
        let error = refused["error"].to_string();
        assert!(
            error.contains("refused (400)") && error.contains("frames"),
            "{refused}"
        );
    }
}
