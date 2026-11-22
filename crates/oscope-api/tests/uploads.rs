//! Uploads (W6.3, ADR-0051): a client's file staged in the scratch space at
//! `POST /api/v1/upload/{name}`, imported into a project by the
//! `projects.import` job, then removed; the route's limits, and the
//! scratch space's reserve; a name that tries to leave the staging folder;
//! the lease; a staged file removed once its lifetime is over; and the
//! contract, that an upload cut short leaves nothing staged.

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use oscope_api::auth::{Accounts, Idle, Security};
use oscope_api::upload::{UPLOAD_LIFETIME, UPLOAD_LIMIT};
use oscope_api::{Api, ApiBuilder, Roots};
use oscope_compute::{Image, Shape};
use oscope_imaging::color::encode_tiff16;
use oscope_jobs::{JobService, JobsConfig};
use oscope_store::{Scratch, ScratchPolicy, Store};
use serde_json::{Value, json};
use teta_wot::http::axum::http::Method;
use teta_wot::testing::{TestClient, TestResponse};
use tokio::io::AsyncWriteExt;

const POLICY: ScratchPolicy = ScratchPolicy {
    reserve_bytes: 0,
    low_bytes: 0,
};

/// The server over `folder`'s projects, jobs and scratch space, kept by
/// `policy`, taking uploads of at most `limit` bytes that wait `lifetime`.
fn builder(folder: &Path, policy: ScratchPolicy, limit: u64, lifetime: Duration) -> ApiBuilder {
    let roots = Roots {
        data: folder.join("data"),
        scratch: folder.join("scratch"),
    };
    let store = Store::open(&roots.data, &roots.data.join("index.sqlite")).unwrap();
    let jobs = JobService::open(&folder.join("jobs.sqlite"), JobsConfig::default()).unwrap();
    Api::builder(&roots)
        .projects(store)
        .scratch(Scratch::open(&roots.scratch, policy).unwrap())
        .jobs(jobs)
        .uploads(limit, lifetime)
}

async fn start(builder: ApiBuilder) -> TestClient {
    let server = builder.build().unwrap().into_thing_server();
    TestClient::start(server).await.unwrap()
}

/// A 16-bit TIFF of 40 × 30 pixels.
fn tiff() -> Vec<u8> {
    let image = Image::from_vec(Shape::new(40, 30, 3), vec![0.25; 40 * 30 * 3]).unwrap();
    let mut file = Vec::new();
    encode_tiff16(&mut file, &image).unwrap();
    file
}

/// Posts `bytes` as `name`, with `headers`.
async fn upload(
    client: &TestClient,
    name: &str,
    headers: &[(&str, &str)],
    bytes: Vec<u8>,
) -> TestResponse {
    let path = format!("/api/v1/upload/{name}");
    client
        .request(Method::POST, &path, headers, Some(bytes))
        .await
}

/// What is staged in the scratch space at `scratch`, and on its way there.
fn staged(scratch: &Path) -> Vec<String> {
    let listed = |folder: &str| {
        std::fs::read_dir(scratch.join(folder))
            .into_iter()
            .flatten()
    };
    let entries = listed("uploads").chain(listed("tmp")).flatten();
    entries.map(|e| e.path().display().to_string()).collect()
}

/// Waits until `done`, for at most 30 s.
async fn until(what: &str, done: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(30);
    while !done() {
        assert!(Instant::now() < deadline, "{what}");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

/// Invokes `action` with `input`, and waits for its invocation to end.
async fn invoke(client: &TestClient, action: &str, input: Value) -> Value {
    let started = client
        .post_json(&format!("/api/v1/{action}"), Some(&input))
        .await;
    assert_eq!(started.status, 201, "{action}: {}", started.text());
    let href = started.json()["href"].as_str().unwrap().to_owned();
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        let now = client.get(&href).await.json();
        let ended = |states: &[&str]| states.contains(&now["status"].as_str().unwrap_or(""));
        let job = now["output"]["href"]
            .as_str()
            .filter(|_| ended(&["completed"]));
        match job {
            // A job submitted: followed to its end.
            Some(job) if action == "jobs/submit" => loop {
                let job = client.get(job).await.json();
                if ["completed", "failed", "cancelled"].contains(&job["state"].as_str().unwrap()) {
                    return job;
                }
                assert!(Instant::now() < deadline, "{job}");
                tokio::time::sleep(Duration::from_millis(10)).await;
            },
            _ if !ended(&["pending", "running"]) => return now,
            _ => {}
        }
        assert!(Instant::now() < deadline, "{action}: {now}");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn an_upload_is_staged_imported_into_a_project_then_removed() {
    let folder = tempfile::tempdir().unwrap();
    let scratch = folder.path().join("scratch");
    let client = start(builder(
        folder.path(),
        POLICY,
        UPLOAD_LIMIT,
        UPLOAD_LIFETIME,
    ))
    .await;
    let file = tiff();
    let answer = upload(&client, "slide%201.tif", &[], file.clone()).await;
    assert_eq!(answer.status, 201, "{}", answer.text());
    let upload = answer.json();
    assert_eq!(
        (&upload["name"], &upload["bytes"]),
        (&json!("slide 1.tif"), &json!(file.len()))
    );
    let id = upload["id"].as_str().unwrap();
    let kept = scratch.join("uploads").join(id).join("slide 1.tif");
    assert_eq!(std::fs::read(&kept).unwrap(), file);
    // It goes in a day, unless it is used.
    let expires: chrono::DateTime<chrono::Utc> =
        upload["expires"].as_str().unwrap().parse().unwrap();
    let day = expires - chrono::Utc::now();
    assert!(
        day > chrono::Duration::hours(23) && day <= chrono::Duration::hours(24),
        "{day}"
    );

    let made = invoke(&client, "projects/create", json!({"name": "Uploaded"})).await;
    let project = made["output"]["id"].as_str().unwrap().to_owned();
    let params = json!({"project": project, "upload": id});
    let job = invoke(
        &client,
        "jobs/submit",
        json!({"kind": "projects.import", "params": params}),
    )
    .await;
    assert_eq!(job["state"], "completed", "{job}");
    let contents = client
        .get(&format!("/api/v1/projects/project/{project}"))
        .await
        .json();
    let acquisition = &contents["acquisitions"][0];
    assert_eq!(acquisition["name"], "slide 1", "{contents}");
    assert_eq!(
        (
            &acquisition["image"]["width"],
            &acquisition["image"]["height"]
        ),
        (&json!(40), &json!(30))
    );
    // Used, it is removed.
    assert!(staged(&scratch).is_empty(), "{:?}", staged(&scratch));

    // An upload that isn't there, or none named, or a path besides.
    for params in [
        json!({"project": project, "upload": id}),
        json!({"project": project}),
        json!({"project": project, "upload": id, "path": kept}),
    ] {
        let submit = json!({"kind": "projects.import", "params": params});
        let refused = invoke(&client, "jobs/submit", submit).await;
        assert_eq!(refused["status"], "error", "{params}: {refused}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn an_upload_too_large_with_no_room_or_named_to_leave_its_folder_is_refused() {
    let folder = tempfile::tempdir().unwrap();
    let scratch = folder.path().join("scratch");
    let client = start(builder(folder.path(), POLICY, 1000, UPLOAD_LIFETIME)).await;
    // Over the limit, as it came, or as its length said before it came.
    let answer = upload(&client, "large.tif", &[], vec![0; 1001]).await;
    assert_eq!(answer.status, 413, "{}", answer.text());
    let said = upload(
        &client,
        "large.tif",
        &[("content-length", "1001")],
        vec![0; 10],
    )
    .await;
    assert_eq!(said.status, 413, "{}", said.text());
    assert_eq!(
        upload(&client, "fits.tif", &[], vec![0; 1000]).await.status,
        201
    );
    // A name that isn't one plain part: never staged, and nothing outside.
    for name in [
        "..%2Fout.tif",
        "%2E%2E",
        "a%5Cout.tif",
        ".hidden",
        "c%3Aout.tif",
        "NUL.tif",
        "%00",
    ] {
        let answer = upload(&client, name, &[], vec![1; 10]).await;
        assert_eq!(answer.status, 400, "{name}: {}", answer.text());
    }
    let answer = upload(&client, "../out.tif", &[], vec![1; 10]).await;
    assert_ne!(answer.status.as_u16() / 100, 2, "{}", answer.text());
    assert_eq!(staged(&scratch).len(), 1, "{:?}", staged(&scratch));
    for outside in [
        scratch.join("out.tif"),
        scratch.join("uploads").join("out.tif"),
        folder.path().join("out.tif"),
    ] {
        assert!(!outside.exists(), "{}", outside.display());
    }

    // No room: a reserve larger than the disk.
    let full = tempfile::tempdir().unwrap();
    let policy = ScratchPolicy {
        reserve_bytes: u64::MAX / 2,
        low_bytes: 0,
    };
    let client = start(builder(full.path(), policy, UPLOAD_LIMIT, UPLOAD_LIFETIME)).await;
    let said = upload(&client, "a.tif", &[("content-length", "10")], vec![0; 10]).await;
    assert_eq!(said.status, 507, "{}", said.text());
    assert_eq!(upload(&client, "a.tif", &[], vec![0; 10]).await.status, 507);
    assert!(staged(&full.path().join("scratch")).is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn an_upload_needs_the_lease() {
    let folder = tempfile::tempdir().unwrap();
    let accounts = Accounts::open(&folder.path().join("accounts.json")).unwrap();
    accounts.add("alice", "alice's password").unwrap();
    let security = Security::new(Arc::new(accounts), None, Idle::default());
    let builder = builder(folder.path(), POLICY, UPLOAD_LIMIT, UPLOAD_LIFETIME);
    let client = start(builder.security(security)).await;
    assert_eq!(upload(&client, "a.tif", &[], vec![0; 10]).await.status, 401);
    let body = json!({"name": "alice", "password": "alice's password"});
    let login = client
        .post_json("/api/v1/auth/login", Some(&body))
        .await
        .json();
    let bearer = format!("Bearer {}", login["token"].as_str().unwrap());
    let alice = [("authorization", bearer.as_str())];
    assert_eq!(
        upload(&client, "a.tif", &alice, vec![0; 10]).await.status,
        403
    );
    let acquire = "/api/v1/auth/acquire_control";
    let acquired = client
        .request(Method::POST, acquire, &alice, Some(b"{}".to_vec()))
        .await;
    assert_eq!(acquired.status, 201, "{}", acquired.text());
    // An invocation: the lease is hers once it has completed.
    let href = acquired.json()["href"].as_str().unwrap().to_owned();
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let now = client
            .request(Method::GET, &href, &alice, None)
            .await
            .json();
        if now["status"] == "completed" {
            break;
        }
        assert!(
            Instant::now() < deadline && now["status"] != "error",
            "{now}"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let answer = upload(&client, "a.tif", &alice, vec![0; 10]).await;
    assert_eq!(answer.status, 201, "{}", answer.text());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_staged_file_is_removed_once_its_lifetime_is_over() {
    // A day: the next upload leaves the first.
    let folder = tempfile::tempdir().unwrap();
    let scratch = folder.path().join("scratch");
    let client = start(builder(
        folder.path(),
        POLICY,
        UPLOAD_LIMIT,
        UPLOAD_LIFETIME,
    ))
    .await;
    upload(&client, "first.tif", &[], vec![0; 10]).await;
    upload(&client, "second.tif", &[], vec![0; 10]).await;
    assert_eq!(staged(&scratch).len(), 2, "{:?}", staged(&scratch));

    // None: the next upload, and the next start, sweep what is older.
    let folder = tempfile::tempdir().unwrap();
    let scratch = folder.path().join("scratch");
    let client = start(builder(folder.path(), POLICY, UPLOAD_LIMIT, Duration::ZERO)).await;
    let first = upload(&client, "first.tif", &[], vec![0; 10]).await.json();
    let second = upload(&client, "second.tif", &[], vec![0; 10]).await.json();
    let names: Vec<String> = staged(&scratch);
    assert!(
        names.len() == 1 && names[0].ends_with(second["id"].as_str().unwrap()),
        "{first} {names:?}"
    );
    drop(client);
    let _again = start(builder(folder.path(), POLICY, UPLOAD_LIMIT, Duration::ZERO)).await;
    assert!(staged(&scratch).is_empty(), "{:?}", staged(&scratch));
}

#[tokio::test(flavor = "multi_thread")]
async fn an_upload_cut_short_leaves_nothing_staged() {
    let folder = tempfile::tempdir().unwrap();
    let scratch = folder.path().join("scratch");
    let api = builder(folder.path(), POLICY, UPLOAD_LIMIT, UPLOAD_LIFETIME)
        .build()
        .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    let serving = tokio::spawn(api.serve(listener, async move {
        let _ = stopped.await;
    }));
    // Eight MiB said, one sent, and the connection lost.
    let mut socket = tokio::net::TcpStream::connect(address).await.unwrap();
    let head = format!(
        "POST /api/v1/upload/cut.tif HTTP/1.1\r\nhost: {address}\r\n\
         content-type: application/octet-stream\r\ncontent-length: {}\r\n\r\n",
        8 << 20
    );
    socket.write_all(head.as_bytes()).await.unwrap();
    socket.write_all(&vec![7_u8; 1 << 20]).await.unwrap();
    socket.flush().await.unwrap();
    let tmp = scratch.join("tmp");
    let coming = || {
        let files = std::fs::read_dir(&tmp).into_iter().flatten().flatten();
        // The file's own size: on Windows, a listing's stays 0 while it is written.
        files
            .filter_map(|f| std::fs::metadata(f.path()).ok())
            .any(|m| m.len() > 0)
    };
    until("the part sent arrives", coming).await;
    drop(socket);
    until("nothing is left of it", || staged(&scratch).is_empty()).await;
    stop.send(()).unwrap();
    serving.await.unwrap().unwrap();
}
