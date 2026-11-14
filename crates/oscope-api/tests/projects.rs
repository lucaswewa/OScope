//! The `projects` Thing over HTTP: made, listed, searched, paged, changed
//! and deleted; one project's contents; thumbnails; the events, the
//! refusals, and the Thing's description.

use std::path::Path;
use std::time::{Duration, Instant};

use oscope_api::{Api, Roots};
use oscope_core::ProjectId;
use oscope_store::{ImageInfo, SampleType, Samples, Store};
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
    let api = Api::builder(&roots).projects(store(data)).build().unwrap();
    TestClient::start(api.into_thing_server()).await.unwrap()
}

/// Invokes `action` with `input`, and waits for it to end: its final JSON.
async fn invoke(client: &TestClient, action: &str, input: Value) -> Value {
    let path = format!("/api/v1/projects/{action}");
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

async fn names(client: &TestClient, query: &str) -> (u64, Vec<String>) {
    let page = client
        .get(&format!("/api/v1/projects/list{query}"))
        .await
        .json();
    let projects = page["projects"].as_array().unwrap();
    let names = projects
        .iter()
        .map(|p| p["name"].as_str().unwrap().to_owned())
        .collect();
    (page["total"].as_u64().unwrap(), names)
}

#[tokio::test(flavor = "multi_thread")]
async fn projects_are_made_listed_changed_and_deleted_over_http() {
    let folder = tempfile::tempdir().unwrap();
    let data = folder.path().join("data");
    let client = server(&data).await;
    let mut changes = client.events("/api/v1/projects/changed").await;

    let made = invoke(
        &client,
        "create",
        json!({"name": "Kidney", "sample": "Mouse kidney", "tags": ["he"]}),
    )
    .await;
    let kidney = made["output"].clone();
    assert_eq!(
        (&kidney["name"], &kidney["tags"]),
        (&json!("Kidney"), &json!(["he"])),
        "{made}"
    );
    let id = kidney["id"].as_str().unwrap().to_owned();
    assert_eq!(
        changes.next_within(Duration::from_secs(10)).await,
        Some(json!(id))
    );
    invoke(&client, "create", json!({"name": "Tonsil"})).await;

    assert_eq!(
        names(&client, "").await,
        (2, vec!["Tonsil".into(), "Kidney".into()])
    );
    assert_eq!(names(&client, "?text=MOUSE").await.1, ["Kidney"]);
    assert_eq!(names(&client, "?tag=he").await.1, ["Kidney"]);
    assert_eq!(
        names(&client, "?offset=1&limit=1").await,
        (2, vec!["Kidney".into()])
    );

    // Renamed, described, tagged and given metadata.
    invoke(&client, "rename", json!({"id": id, "name": "Kidney 2"})).await;
    invoke(
        &client,
        "set_sample",
        json!({"id": id, "sample": "Rat kidney"}),
    )
    .await;
    invoke(&client, "tag", json!({"id": id, "tags": ["rat", "he"]})).await;
    invoke(
        &client,
        "set_metadata",
        json!({"id": id, "metadata": {"stain": "H&E", "slide": 3}}),
    )
    .await;
    let removed = invoke(
        &client,
        "set_metadata",
        json!({"id": id, "metadata": {"slide": null}}),
    )
    .await;
    assert_eq!(
        removed["output"]["metadata"],
        json!({"stain": "H&E"}),
        "{removed}"
    );
    let contents = client
        .get(&format!("/api/v1/projects/project/{id}"))
        .await
        .json();
    let project = &contents["project"];
    assert_eq!(
        (&project["name"], &project["sample"]),
        (&json!("Kidney 2"), &json!("Rat kidney"))
    );
    assert_eq!(project["tags"], json!(["he", "rat"]));

    // An acquisition, from another store on the same folders: its
    // thumbnail is the project's.
    let other = store(&data);
    let project_id: ProjectId = id.parse().unwrap();
    let info = ImageInfo::new(300, 200, 1, SampleType::U16);
    let draft = other.acquisition(project_id, "field", &info).unwrap();
    let ramp = (0..200_u16)
        .flat_map(|y| (0..300_u16).map(move |x| x * 200 + y))
        .collect();
    draft
        .image()
        .unwrap()
        .write(0, (0, 0), (300, 200), &Samples::U16(ramp))
        .unwrap();
    let acquisition = draft.commit().unwrap();
    let contents = client
        .get(&format!("/api/v1/projects/project/{id}"))
        .await
        .json();
    assert_eq!(
        contents["acquisitions"][0]["id"],
        json!(acquisition.id),
        "{contents}"
    );
    assert_eq!(
        names(&client, "").await.1[0],
        "Kidney 2",
        "the most recently changed"
    );
    for path in [
        format!("thumbnail/{id}"),
        format!("thumbnail/{id}/{}", acquisition.id),
    ] {
        let thumbnail = client.get(&format!("/api/v1/projects/{path}")).await;
        assert_eq!(thumbnail.status, 200, "{path}");
        assert_eq!(thumbnail.header("content-type"), Some("image/jpeg"));
        assert!(thumbnail.body.starts_with(&[0xFF, 0xD8]), "{path}");
    }

    // Refused, and not found.
    let refused = invoke(&client, "create", json!({"name": " "})).await;
    assert!(
        refused["error"]
            .to_string()
            .contains("refused (400): a name has"),
        "{refused}"
    );
    let nowhere = ProjectId::generate();
    let unknown = invoke(&client, "rename", json!({"id": nowhere, "name": "x"})).await;
    assert!(
        unknown["error"].to_string().contains("unknown (404)"),
        "{unknown}"
    );
    let tonsil = names(&client, "?text=tonsil").await;
    assert_eq!(tonsil.0, 1);
    for (path, status) in [
        ("project/not-an-id".to_owned(), 400),
        (format!("thumbnail/{id}/..%2F..%2F..%2Fx"), 400),
        (format!("project/{nowhere}"), 404),
        (format!("thumbnail/{nowhere}"), 404),
    ] {
        let answer = client.get(&format!("/api/v1/projects/{path}")).await;
        assert_eq!(answer.status, status, "{path}: {}", answer.text());
    }

    // Deleted, and the index rebuilt from what is left.
    let deleted = invoke(&client, "delete", json!({"id": id})).await;
    assert_eq!(deleted["status"], "completed", "{deleted}");
    let gone = client.get(&format!("/api/v1/projects/project/{id}")).await;
    assert_eq!(gone.status, 404);
    let rebuilt = invoke(&client, "rebuild_index", json!({})).await;
    assert_eq!(
        rebuilt["output"],
        json!({"projects": 1, "skipped": []}),
        "{rebuilt}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn the_projects_thing_describes_itself() {
    let folder = tempfile::tempdir().unwrap();
    let client = server(folder.path()).await;
    let mut td = client.get("/api/v1/projects/").await.json();
    let id = td.as_object_mut().unwrap().remove("id").unwrap();
    assert!(id.as_str().unwrap().starts_with("urn:uuid:"), "{id}");
    insta::assert_json_snapshot!("projects", td);
}
