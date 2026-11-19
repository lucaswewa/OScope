//! The origins a browser may call the server from (W1.3, ADR-0045): a
//! client without one, the server's own and those listed are let in; a
//! page anywhere else is refused, 403, before it is served, a WebSocket's
//! upgrade among them.

use oscope_api::origins::Origins;
use oscope_api::{Api, Roots};
use teta_wot::testing::TestClient;

/// The dev server's origin, listed.
const LISTED: &[&str] = &["http://127.0.0.1:8280"];

/// A client of a server that lets in `listed`, calling as a page at
/// `origin` would, if any, at the server's own `127.0.0.1:8210`.
async fn client(listed: &[&str], origin: Option<&str>) -> (TestClient, tempfile::TempDir) {
    let folder = tempfile::tempdir().unwrap();
    let roots = Roots {
        data: folder.path().to_owned(),
        scratch: folder.path().join("scratch"),
    };
    let origins = Origins::new(listed.iter().map(|origin| (*origin).to_owned()));
    let api = Api::builder(&roots).origins(origins).build().unwrap();
    let mut client = TestClient::start(api.into_thing_server())
        .await
        .unwrap()
        .with_host("127.0.0.1:8210");
    if let Some(origin) = origin {
        client = client.with_header("origin", origin);
    }
    (client, folder)
}

/// A WebSocket's upgrade request, from this client.
fn upgrading(client: TestClient) -> TestClient {
    client
        .with_header("connection", "upgrade")
        .with_header("upgrade", "websocket")
        .with_header("sec-websocket-version", "13")
        .with_header("sec-websocket-key", "dGhlIHNhbXBsZSBub25jZQ==")
}

#[tokio::test]
async fn a_client_without_an_origin_the_server_s_own_and_a_listed_one_are_let_in() {
    for origin in [
        None,
        Some("http://127.0.0.1:8210"),
        Some("http://127.0.0.1:8280"),
    ] {
        let (client, _folder) = client(LISTED, origin).await;
        assert_eq!(
            client.get("/api/v1/system/version").await.status,
            200,
            "{origin:?}"
        );
        assert_eq!(client.get("/health").await.status, 200, "{origin:?}");
    }
}

#[tokio::test]
async fn a_page_anywhere_else_is_refused_before_it_is_served() {
    for origin in ["http://127.0.0.1:8281", "https://elsewhere.example", "null"] {
        let (client, _folder) = client(LISTED, Some(origin)).await;
        for path in ["/api/v1/system/version", "/health"] {
            let refused = client.get(path).await;
            assert_eq!(refused.status, 403, "{origin} {path}");
            assert_eq!(refused.json()["title"], "origin not allowed");
            assert_eq!(
                refused.header("content-type"),
                Some("application/problem+json")
            );
        }
    }
}

#[tokio::test]
async fn a_websocket_from_anywhere_else_is_refused_before_its_upgrade() {
    let (elsewhere, _a) = client(LISTED, Some("https://elsewhere.example")).await;
    assert_eq!(
        upgrading(elsewhere).get("/api/v1/system/ws").await.status,
        403
    );
    // Let in, its upgrade goes on to the Thing's socket, which this client
    // can't complete without a connection: anything but the refusal.
    let (listed, _b) = client(LISTED, Some("http://127.0.0.1:8280")).await;
    assert_ne!(upgrading(listed).get("/api/v1/system/ws").await.status, 403);
}

#[tokio::test]
async fn with_none_listed_only_the_server_s_own_origin_is_let_in() {
    let (dev, _a) = client(&[], Some("http://127.0.0.1:8280")).await;
    assert_eq!(dev.get("/api/v1/system/version").await.status, 403);
    let (own, _b) = client(&[], Some("http://127.0.0.1:8210")).await;
    assert_eq!(own.get("/api/v1/system/version").await.status, 200);
}
