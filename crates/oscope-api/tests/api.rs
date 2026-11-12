//! OScope's server: `/health`, the `system` Thing, the media plane behind
//! the authenticator, pages, and shutting down with a media WebSocket open.

use std::path::Path;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use oscope_api::{Api, GIT_COMMIT, Roots, VERSION};
use serde_json::{Value, json};
use teta_wot::BoxFuture;
use teta_wot::Principal;
use teta_wot::http::axum::http::request::Parts;
use teta_wot::server::{Authenticator, Rejection};
use teta_wot::testing::TestClient;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;

/// Roots in `folder`, created.
fn roots(folder: &Path) -> Roots {
    let roots = Roots {
        data: folder.join("data"),
        scratch: folder.join("scratch"),
    };
    std::fs::create_dir_all(&roots.data).unwrap();
    std::fs::create_dir_all(&roots.scratch).unwrap();
    roots
}

async fn client(api: Api) -> TestClient {
    TestClient::start(api.into_thing_server()).await.unwrap()
}

#[tokio::test]
async fn health_answers_ok() {
    let folder = tempfile::tempdir().unwrap();
    let client = client(Api::builder(&roots(folder.path())).build().unwrap()).await;
    let answer = client.get("/health").await;
    assert_eq!(answer.status, 200);
    assert_eq!(answer.json(), json!({"status": "ok"}));
}

#[tokio::test]
async fn the_system_thing_reports_the_build_the_computer_and_its_folders() {
    let folder = tempfile::tempdir().unwrap();
    let roots = roots(folder.path());
    let client = client(Api::builder(&roots).build().unwrap()).await;
    let read = async |name: &str| {
        let answer = client.get(&format!("/api/v1/system/{name}")).await;
        assert_eq!(answer.status, 200, "{name}: {}", answer.text());
        answer.json()
    };

    assert_eq!(read("version").await, VERSION);
    assert_eq!(read("git_commit").await, GIT_COMMIT);
    assert!(
        GIT_COMMIT == "unknown"
            || GIT_COMMIT.len() == 12 && GIT_COMMIT.chars().all(|c| c.is_ascii_hexdigit()),
        "{GIT_COMMIT}"
    );
    let os = read("os").await;
    assert!(!os["name"].as_str().unwrap().is_empty(), "{os}");
    assert_eq!(os["arch"], std::env::consts::ARCH);
    let cpu = read("cpu").await;
    let (cores, threads) = (
        cpu["cores"].as_u64().unwrap(),
        cpu["threads"].as_u64().unwrap(),
    );
    assert!(cores >= 1 && threads >= cores, "{cpu}");
    let memory = read("memory").await;
    let available = memory["available_bytes"].as_u64().unwrap();
    assert!(0 < available && available <= memory["total_bytes"].as_u64().unwrap());

    for (name, path) in [("data_root", &roots.data), ("scratch_root", &roots.scratch)] {
        let root = read(name).await;
        assert_eq!(
            root["path"],
            std::path::absolute(path).unwrap().display().to_string()
        );
        let available = root["available_bytes"].as_u64().unwrap();
        assert!(
            0 < available && available <= root["total_bytes"].as_u64().unwrap(),
            "{root}"
        );
    }

    let started = read("uptime").await.as_f64().unwrap();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    while read("uptime").await.as_f64().unwrap() <= started {
        assert!(
            tokio::time::Instant::now() < deadline,
            "the uptime doesn't grow"
        );
        tokio::task::yield_now().await;
    }

    let td = client.get("/api/v1/system/").await.json();
    let properties = td["properties"].as_object().unwrap();
    assert_eq!(properties.len(), 8);
    assert!(properties.values().all(|p| p["readOnly"] == true), "{td}");
    assert_eq!(properties["uptime"]["unit"], "s");
}

#[tokio::test]
async fn a_root_that_is_missing_is_an_error_not_a_panic() {
    let folder = tempfile::tempdir().unwrap();
    let roots = Roots {
        data: folder.path().join("data"),
        scratch: folder.path().join("never-made"),
    };
    std::fs::create_dir_all(&roots.data).unwrap();
    let client = client(Api::builder(&roots).build().unwrap()).await;
    assert_eq!(client.get("/api/v1/system/data_root").await.status, 200);
    let answer = client.get("/api/v1/system/scratch_root").await;
    assert!(answer.status.is_server_error(), "{}", answer.status);
    assert!(answer.text().contains("never-made"), "{}", answer.text());
}

/// Lets in only `Authorization: Bearer let-me-in`.
struct TestAuthenticator;

impl Authenticator for TestAuthenticator {
    fn authenticate<'a>(
        &'a self,
        request: &'a Parts,
    ) -> BoxFuture<'a, Result<Principal, Rejection>> {
        Box::pin(async move {
            match request
                .headers
                .get("authorization")
                .and_then(|v| v.to_str().ok())
            {
                Some("Bearer let-me-in") => Ok(Principal::new("tester")),
                _ => Err(Rejection::unauthorized(Some("Bearer".into()))),
            }
        })
    }
}

#[tokio::test]
async fn the_media_plane_is_behind_the_authenticator_with_the_things() {
    let folder = tempfile::tempdir().unwrap();
    let api = Api::builder(&roots(folder.path()))
        .authenticator(TestAuthenticator)
        .build()
        .unwrap();
    let anonymous = client(api).await;
    let protected = ["/api/v1/media/ping", "/api/v1/system/version", "/health"];
    for path in protected {
        assert_eq!(anonymous.get(path).await.status, 401, "{path}");
    }
    // Descriptions stay public, so that clients can find out what to send.
    for path in ["/api/v1/system/", "/openapi.json"] {
        assert_eq!(anonymous.get(path).await.status, 200, "{path}");
    }

    let tester = anonymous.with_header("authorization", "Bearer let-me-in");
    let ping = tester.get("/api/v1/media/ping").await;
    assert_eq!(
        (ping.status.as_u16(), ping.text()),
        (200, "pong".to_owned())
    );
    assert_eq!(tester.get("/api/v1/system/version").await.json(), VERSION);
}

#[tokio::test]
async fn shutting_down_closes_an_open_media_websocket_at_once() {
    let folder = tempfile::tempdir().unwrap();
    let api = Api::builder(&roots(folder.path())).build().unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    let serving = tokio::spawn(api.serve(listener, async {
        let _ = stopped.await;
    }));

    let url = format!("ws://{address}/api/v1/media/ping");
    let (mut socket, _) = tokio_tungstenite::connect_async(url).await.unwrap();
    for message in [Message::text("hello"), Message::binary(vec![1, 2, 3])] {
        socket.send(message.clone()).await.unwrap();
        assert_eq!(socket.next().await.unwrap().unwrap(), message);
    }

    stop.send(()).unwrap();
    // Well inside the 5 s grace period: the socket doesn't hold it up.
    let bound = Duration::from_secs(3);
    let closed = tokio::time::timeout(bound, socket.next())
        .await
        .expect("closed in time");
    match closed {
        Some(Ok(Message::Close(Some(frame)))) => assert_eq!(frame.code, CloseCode::Away),
        other => panic!("expected a close frame, got {other:?}"),
    }
    let served = tokio::time::timeout(bound, serving)
        .await
        .expect("stopped in time");
    served.unwrap().unwrap();
}

#[tokio::test]
async fn pages_are_served_beside_the_api_and_read_when_asked_for() {
    let folder = tempfile::tempdir().unwrap();
    let pages = folder.path().join("page");
    std::fs::create_dir_all(pages.join("js")).unwrap();
    std::fs::write(pages.join("index.html"), "<p>hello</p>").unwrap();
    std::fs::write(pages.join("js/app.js"), "let a = 1;").unwrap();
    let api = Api::builder(&roots(folder.path()))
        .pages(&pages)
        .build()
        .unwrap();
    let client = client(api).await;

    for (path, kind, body) in [
        ("/", "text/html", "<p>hello</p>"),
        ("/index.html", "text/html", "<p>hello</p>"),
        ("/js/app.js", "text/javascript", "let a = 1;"),
    ] {
        let answer = client.get(path).await;
        assert_eq!(answer.status, 200, "{path}");
        assert!(
            answer.header("content-type").unwrap().starts_with(kind),
            "{path}"
        );
        assert_eq!(answer.text(), body, "{path}");
    }
    std::fs::write(pages.join("js/app.js"), "let a = 2;").unwrap();
    assert_eq!(client.get("/js/app.js").await.text(), "let a = 2;");
    assert_eq!(
        client.get("/api/v1/system/version").await.json(),
        Value::from(VERSION)
    );
    assert_eq!(client.get("/missing.js").await.status, 404);
}

#[test]
fn pages_that_would_hide_the_api_are_refused() {
    let folder = tempfile::tempdir().unwrap();
    let pages = folder.path().join("page");
    std::fs::create_dir_all(pages.join("docs")).unwrap();
    std::fs::write(pages.join("docs/index.html"), "").unwrap();
    let refused = Api::builder(&roots(folder.path())).pages(&pages).build();
    let message = refused.unwrap_err().to_string();
    assert!(message.contains("docs/index.html"), "{message}");
}
