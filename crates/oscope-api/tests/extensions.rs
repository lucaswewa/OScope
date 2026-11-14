//! The `extensions` Thing over HTTP: installing through the upload route,
//! listing, one extension's details, enabling, disabling and uninstalling,
//! each for the next start; the events, the refusals, and the Thing's
//! description.

use std::path::Path;
use std::time::{Duration, Instant};

use oscope_api::{Api, Roots};
use oscope_ext::Extensions;
use serde_json::{Value, json};
use teta_wot::http::axum::http::Method;
use teta_wot::testing::{TestClient, TestResponse};

const ID: &str = "org.example.hello";

/// A server with the extensions in `data`, as a start opens them.
async fn server(data: &Path) -> TestClient {
    let roots = Roots {
        data: data.to_owned(),
        scratch: data.join("scratch"),
    };
    let extensions = Extensions::open(data.join("extensions")).unwrap();
    let api = Api::builder(&roots).extensions(extensions).build().unwrap();
    TestClient::start(api.into_thing_server()).await.unwrap()
}

/// A package of a Python extension, packed from a folder in `folder`; its
/// manifest's `oscope-api` range is `api`.
fn package(folder: &Path, version: &str, api: &str) -> Vec<u8> {
    let source = folder.join("hello");
    std::fs::create_dir_all(source.join("hello")).unwrap();
    let manifest = format!(
        "id = \"{ID}\"\nname = \"Hello\"\nversion = \"{version}\"\nlicense = \"MIT\"\n\
         publisher = \"Example Lab\"\noscope-api = \"{api}\"\nkind = \"python\"\n\n\
         [provides]\nnodes = [\"hello.greet\"]\n\n\
         [python]\nrequires-python = \">=3.12\"\nentry = \"hello\"\n"
    );
    std::fs::write(source.join("manifest.toml"), manifest).unwrap();
    std::fs::write(
        source.join("hello/__init__.py"),
        format!("VERSION = '{version}'\n"),
    )
    .unwrap();
    let out = folder.join("hello.omext");
    oscope_ext::pack(&source, &out).unwrap();
    std::fs::read(out).unwrap()
}

/// Posts `body` to the upload route.
async fn upload(client: &TestClient, body: Vec<u8>) -> TestResponse {
    let headers = [("content-type", "application/octet-stream")];
    let path = "/api/v1/extensions/install";
    client
        .request(Method::POST, path, &headers, Some(body))
        .await
}

/// Invokes `action` with `input`, and waits for its invocation to end: its
/// final JSON.
async fn invoke(client: &TestClient, action: &str, input: Value) -> Value {
    let path = format!("/api/v1/extensions/{action}");
    let started = client.post_json(&path, Some(&input)).await;
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

#[tokio::test(flavor = "multi_thread")]
async fn a_package_uploaded_loads_at_the_next_start() {
    let folder = tempfile::tempdir().unwrap();
    let data = folder.path().join("data");
    let client = server(&data).await;
    let mut changes = client.events("/api/v1/extensions/changed").await;
    let host = client.get("/api/v1/extensions/host").await.json();
    assert_eq!(host["api"], "1.0.0", "{host}");

    // Installed for the next start, and said so.
    let bytes = package(folder.path(), "0.1.0", "^1.0");
    let installed = upload(&client, bytes.clone()).await;
    assert_eq!(installed.status, 201, "{}", installed.text());
    let extension = installed.json();
    assert_eq!(extension["manifest"]["id"], ID);
    assert_eq!(
        (&extension["loaded"], &extension["next"]),
        (&Value::Null, &json!("0.1.0"))
    );
    assert_eq!(extension["sha256"].as_str().map(str::len), Some(64));
    let said = changes.next_within(Duration::from_secs(10)).await;
    assert_eq!(said, Some(json!(ID)));
    assert_eq!(
        client.get("/api/v1/extensions/restart_needed").await.json(),
        true
    );

    // Again: the same, unchanged.
    let again = upload(&client, bytes).await;
    assert_eq!(again.status, 200, "{}", again.text());
    assert_eq!(again.json(), extension);

    // Listed, and on its own.
    let listed = client.get("/api/v1/extensions/installed").await.json();
    assert_eq!(listed, json!([extension]));
    let one = client
        .get(&format!("/api/v1/extensions/extension/{ID}"))
        .await;
    assert_eq!((one.status.as_u16(), one.json()), (200, extension.clone()));
    let none = client
        .get("/api/v1/extensions/extension/org.example.none")
        .await;
    assert_eq!(none.status, 404);
    assert!(
        none.json()["detail"]
            .as_str()
            .unwrap()
            .contains("no extension"),
        "{}",
        none.text()
    );
    client.stop().await;

    // The next start loads it, with what it provides.
    let client = server(&data).await;
    let listed = client.get("/api/v1/extensions/installed").await.json();
    assert_eq!(listed[0]["loaded"], "0.1.0", "{listed}");
    let provided = client.get("/api/v1/extensions/provided").await.json();
    assert_eq!(provided, json!({"nodes": {"hello.greet": ID}}));
    assert_eq!(
        client.get("/api/v1/extensions/restart_needed").await.json(),
        false
    );

    // Disabled, enabled, uninstalled: each for the next start.
    let mut changes = client.events("/api/v1/extensions/changed").await;
    let disabled = invoke(&client, "disable", json!({"id": ID})).await;
    assert_eq!(disabled["status"], "completed", "{disabled}");
    assert_eq!(disabled["output"]["next"], Value::Null, "{disabled}");
    assert_eq!(
        changes.next_within(Duration::from_secs(10)).await,
        Some(json!(ID))
    );
    assert_eq!(
        client.get("/api/v1/extensions/restart_needed").await.json(),
        true
    );
    let enabled = invoke(&client, "enable", json!({"id": ID})).await;
    assert_eq!(enabled["output"]["next"], "0.1.0", "{enabled}");
    let uninstalled = invoke(&client, "uninstall", json!({"id": ID})).await;
    assert_eq!(uninstalled["output"]["uninstalled"], true, "{uninstalled}");
    client.stop().await;
    let client = server(&data).await;
    let listed = client.get("/api/v1/extensions/installed").await.json();
    assert_eq!(listed, json!([]));
}

#[tokio::test(flavor = "multi_thread")]
async fn what_cannot_be_installed_is_refused_with_its_reason() {
    let folder = tempfile::tempdir().unwrap();
    let client = server(&folder.path().join("data")).await;
    let cases = [
        (Vec::new(), 400, "the request has no package in its body"),
        (
            b"not a zip file at all".to_vec(),
            400,
            "the package: not a zip file",
        ),
        (
            package(folder.path(), "0.1.0", "^2.0"),
            400,
            "it needs the extension API ^2.0, and this OScope offers 1.0.0",
        ),
    ];
    for (body, status, detail) in cases {
        let answer = upload(&client, body).await;
        assert_eq!(answer.status, status, "{detail}: {}", answer.text());
        let said = answer.json()["detail"].as_str().unwrap_or("").to_owned();
        assert!(said.contains(detail), "{detail}: {said}");
    }
    // The same version from another package: a conflict.
    assert_eq!(
        upload(&client, package(folder.path(), "0.1.0", "^1.0"))
            .await
            .status,
        201
    );
    let other = upload(&client, package(folder.path(), "0.1.0", ">=1.0")).await;
    assert_eq!(other.status, 409, "{}", other.text());
    // Actions on an extension that isn't there.
    let refused = invoke(&client, "enable", json!({"id": "org.example.none"})).await;
    assert_eq!(refused["status"], "error", "{refused}");
    assert!(
        refused["error"].to_string().contains("unknown (404)"),
        "{refused}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn the_extensions_thing_describes_itself() {
    let folder = tempfile::tempdir().unwrap();
    let client = server(folder.path()).await;
    let mut td = client.get("/api/v1/extensions/").await.json();
    // The ID differs from computer to computer: its form, then the rest.
    let id = td.as_object_mut().unwrap().remove("id").unwrap();
    assert!(id.as_str().unwrap().starts_with("urn:uuid:"), "{id}");
    insta::assert_json_snapshot!("extensions", td);
}
