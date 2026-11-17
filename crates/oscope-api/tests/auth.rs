//! Security over HTTP (P49, ADR-0031): accounts kept hashed; logging in and
//! out, and sessions that expire; the control lease acquired, asked for,
//! handed over, expiring, and taken by the desktop app; a test made from
//! the Thing Descriptions, that every affordance that changes state refuses
//! a caller without the lease; and TLS.

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use oscope_api::auth::{Accounts, Idle, LOCAL, Security};
use oscope_api::devices::{Devices, DevicesConfig};
use oscope_api::{Api, Roots};
use oscope_jobs::{JobService, JobsConfig};
use oscope_store::{Scratch, ScratchPolicy, Store};
use serde_json::{Value, json};
use teta_wot::http::axum::http::Method;
use teta_wot::testing::{TestClient, TestResponse};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// The desktop app's token, in these tests.
const DESKTOP: &str = "a local token for the desktop app";

/// Two accounts, `alice` and `bob`, in `data`.
fn accounts(data: &Path) -> Arc<Accounts> {
    let accounts = Accounts::open(&data.join("accounts.json")).unwrap();
    accounts.add("alice", "alice's password").unwrap();
    accounts.add("bob", "bob's password").unwrap();
    Arc::new(accounts)
}

/// The projects and jobs in `data`, behind security idle for `idle`; and
/// with `devices`, the simulator's.
async fn server(data: &Path, idle: Idle, devices: bool) -> TestClient {
    let roots = Roots {
        data: data.to_owned(),
        scratch: data.join("scratch"),
    };
    std::fs::create_dir_all(&roots.scratch).unwrap();
    let store = Store::open(data, &data.join("index.sqlite")).unwrap();
    let jobs = JobService::open(&data.join("jobs.sqlite"), JobsConfig::default()).unwrap();
    let security = Security::new(accounts(data), Some(DESKTOP.to_owned()), idle);
    let mut api = Api::builder(&roots)
        .projects(store)
        .jobs(jobs)
        .security(security);
    if devices {
        let made = Devices::from_config(&DevicesConfig::default(), data).await;
        let policy = ScratchPolicy {
            reserve_bytes: 0,
            low_bytes: 0,
        };
        api = api
            .devices(Arc::new(made.unwrap()))
            .scratch(Scratch::open(&roots.scratch, policy).unwrap());
    }
    TestClient::start(api.build().unwrap().into_thing_server())
        .await
        .unwrap()
}

/// A caller: their token, sent with each request.
struct Caller<'a> {
    client: &'a TestClient,
    bearer: String,
}

impl Caller<'_> {
    async fn send(&self, method: Method, path: &str, body: Option<&Value>) -> TestResponse {
        let auth = ("authorization", self.bearer.as_str());
        let json = ("content-type", "application/json");
        let bytes = body.map(|b| b.to_string().into_bytes());
        self.client
            .request(method, path, &[auth, json], bytes)
            .await
    }

    async fn get(&self, path: &str) -> TestResponse {
        self.send(Method::GET, path, None).await
    }

    /// Invokes `action`, `thing/action`, with `input`: the refusal's
    /// status, or 200 and the invocation as it ended.
    async fn invoke(&self, action: &str, input: Value) -> (u16, Value) {
        let path = format!("/api/v1/{action}");
        let started = self.send(Method::POST, &path, Some(&input)).await;
        if started.status != 201 {
            return (started.status.as_u16(), started.json());
        }
        let href = started.json()["href"].as_str().unwrap().to_owned();
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            let invocation = self.get(&href).await.json();
            if !["pending", "running"].contains(&invocation["status"].as_str().unwrap_or("")) {
                return (200, invocation);
            }
            assert!(Instant::now() < deadline, "{action}: {invocation}");
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }
}

/// Logs `name` in: a caller with their session's token.
async fn login<'a>(client: &'a TestClient, name: &str, password: &str) -> Caller<'a> {
    let body = json!({"name": name, "password": password});
    let answer = client.post_json("/api/v1/auth/login", Some(&body)).await;
    assert_eq!(answer.status, 200, "{name}: {}", answer.text());
    let token = answer.json()["token"].as_str().unwrap().to_owned();
    Caller {
        client,
        bearer: format!("Bearer {token}"),
    }
}

#[test]
fn accounts_are_kept_hashed_and_checked() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("accounts.json");
    let accounts = Accounts::open(&path).unwrap();
    assert!(accounts.is_empty());
    accounts.add("alice", "alice's password").unwrap();
    assert!(accounts.verify("alice", "alice's password"));
    assert!(!accounts.verify("alice", "bob's password") && !accounts.verify("bob", "x"));
    // An unknown name is checked against a decoy, which never lets one in.
    assert!(!accounts.verify("nobody", "a decoy password"));
    // Kept as argon2id hashes, never the password, and read again.
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("$argon2id$") && !text.contains("alice's password"));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }
    let again = Accounts::open(&path).unwrap();
    assert_eq!(again.names(), ["alice"]);
    again.passwd("alice", "a new password").unwrap();
    assert!(again.verify("alice", "a new password") && !again.verify("alice", "alice's password"));
    // Refused: names, short passwords, twice, and accounts that aren't.
    for name in [
        "",
        "Alice",
        "aLice",
        "9lives",
        "a b",
        LOCAL,
        &"a".repeat(65),
    ] {
        assert!(again.add(name, "a long password").is_err(), "{name:?}");
    }
    assert!(again.add("bob", "short").is_err());
    assert!(again.add("alice", "a long password").is_err());
    assert!(again.passwd("bob", "a long password").is_err());
    assert!(again.remove("bob").is_err());
    again.remove("alice").unwrap();
    assert!(Accounts::open(&path).unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn sessions_log_in_and_out_and_expire_when_idle() {
    let folder = tempfile::tempdir().unwrap();
    let idle = Idle {
        session: Duration::from_secs(4),
        ..Idle::default()
    };
    let client = server(folder.path(), idle, false).await;
    // Nothing without a token but logging in; a wrong password is a 401.
    assert_eq!(client.get("/api/v1/auth/me").await.status, 401);
    let unauthorised = client.get("/api/v1/projects/list").await;
    assert_eq!(unauthorised.status, 401);
    assert_eq!(
        unauthorised.header("www-authenticate"),
        Some("Bearer realm=\"oscope\"")
    );
    let wrong = json!({"name": "alice", "password": "bob's password"});
    let refused = client.post_json("/api/v1/auth/login", Some(&wrong)).await;
    assert_eq!(refused.status, 401);
    let alice = login(&client, "alice", "alice's password").await;
    let me = alice.get("/api/v1/auth/me").await.json();
    assert_eq!(me, json!({"name": "alice", "control": false}));
    assert_eq!(alice.get("/api/v1/projects/list").await.status, 200);
    // Logged out, the token is no more.
    let out = alice.send(Method::POST, "/api/v1/auth/logout", None).await;
    assert_eq!(out.status, 204, "{}", out.text());
    assert_eq!(alice.get("/api/v1/auth/me").await.status, 401);
    // Unused for longer than the idle time, a session ends.
    // Used, a session lasts past its idle time; unused for longer, it ends.
    let bob = login(&client, "bob", "bob's password").await;
    for _ in 0..12 {
        assert_eq!(bob.get("/api/v1/auth/me").await.status, 200);
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    tokio::time::sleep(Duration::from_secs(9)).await;
    assert_eq!(bob.get("/api/v1/auth/me").await.status, 401);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_lease_is_asked_for_handed_over_taken_and_lost_when_idle() {
    let folder = tempfile::tempdir().unwrap();
    let idle = Idle {
        lease: Duration::from_secs(2),
        ..Idle::default()
    };
    let client = server(folder.path(), idle, false).await;
    let alice = login(&client, "alice", "alice's password").await;
    let bob = login(&client, "bob", "bob's password").await;
    let create = |name: &str| json!({"name": name});
    // Reading needs no lease; changing does.
    assert_eq!(bob.get("/api/v1/projects/list").await.status, 200);
    assert_eq!(bob.invoke("projects/create", create("bob's")).await.0, 403);
    let (status, lease) = alice.invoke("auth/acquire_control", json!({})).await;
    assert_eq!((status, &lease["output"]["holder"]), (200, &json!("alice")));
    let (_, made) = alice.invoke("projects/create", create("alice's")).await;
    assert_eq!(made["status"], "completed", "{made}");
    // Bob can't take it, but can ask; Alice hands it over.
    let (_, taken) = bob.invoke("auth/acquire_control", json!({})).await;
    assert!(taken["error"].to_string().contains("409"), "{taken}");
    let (_, asked) = bob.invoke("auth/request_control", json!({})).await;
    assert_eq!(asked["output"]["requests"], json!(["bob"]));
    // Only its holder releases it or hands it over, to an account.
    let (_, released) = bob.invoke("auth/release_control", json!({})).await;
    assert!(released["error"].to_string().contains("409"), "{released}");
    let to_alice = json!({"to": "alice"});
    let (_, kept) = bob.invoke("auth/hand_over_control", to_alice).await;
    assert!(kept["error"].to_string().contains("409"), "{kept}");
    let to_carol = json!({"to": "carol"});
    let (_, nobody) = alice.invoke("auth/hand_over_control", to_carol).await;
    assert!(nobody["error"].to_string().contains("400"), "{nobody}");
    let to_bob = json!({"to": "bob"});
    let (_, handed) = alice.invoke("auth/hand_over_control", to_bob).await;
    assert_eq!(
        (&handed["output"]["holder"], &handed["output"]["requests"]),
        (&json!("bob"), &json!([])),
        "{handed}"
    );
    assert_eq!(bob.invoke("projects/create", create("bob's")).await.0, 200);
    assert_eq!(
        alice.invoke("projects/create", create("again")).await.0,
        403
    );
    let me = bob.get("/api/v1/auth/me").await.json();
    assert_eq!(me["control"], true);
    // The desktop app takes it from whoever has it; no one else may.
    let desktop = Caller {
        client: &client,
        bearer: format!("Bearer {DESKTOP}"),
    };
    let (_, refused) = alice.invoke("auth/take_control", json!({})).await;
    assert!(refused["error"].to_string().contains("403"), "{refused}");
    let (_, took) = desktop.invoke("auth/take_control", json!({})).await;
    assert_eq!(took["output"]["holder"], LOCAL, "{took}");
    assert_eq!(bob.invoke("projects/create", create("late")).await.0, 403);
    // Changing things, its holder keeps it past its idle time; idle for
    // longer, they lose it.
    let (_, handed) = desktop
        .invoke("auth/hand_over_control", json!({"to": "bob"}))
        .await;
    assert_eq!(handed["output"]["holder"], "bob", "{handed}");
    for k in 0..6 {
        let name = format!("busy {k}");
        assert_eq!(bob.invoke("projects/create", create(&name)).await.0, 200);
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    tokio::time::sleep(Duration::from_secs(5)).await;
    let lease = alice.get("/api/v1/auth/lease").await.json();
    assert_eq!(lease["holder"], Value::Null, "{lease}");
    let (_, mine) = alice.invoke("auth/acquire_control", json!({})).await;
    assert_eq!(mine["output"]["holder"], "alice", "{mine}");
}

#[tokio::test(flavor = "multi_thread")]
async fn every_affordance_that_changes_state_refuses_a_caller_without_the_lease() {
    let folder = tempfile::tempdir().unwrap();
    let client = server(folder.path(), Idle::default(), true).await;
    let alice = login(&client, "alice", "alice's password").await;
    // Every Thing's description, from the directory.
    let directory = alice.get("/api/v1/directory/things").await.json();
    let things = directory.as_array().cloned().unwrap_or_default();
    assert!(things.len() > 10, "{directory}");
    let mut checked = Vec::new();
    // Each affordance's path: its first form's, the invocation's for an
    // action, the write's for a property.
    let path = |form: &Value| form["href"].as_str().unwrap_or("").to_owned();
    for td in &things {
        let actions = td["actions"].as_object().cloned().unwrap_or_default();
        for (_, action) in actions {
            let path = path(&action["forms"][0]);
            if path.starts_with("/api/v1/auth/") {
                continue;
            }
            let answer = alice.send(Method::POST, &path, Some(&json!({}))).await;
            assert_eq!(answer.status, 403, "{path}: {}", answer.text());
            checked.push(path);
        }
        let properties = td["properties"].as_object().cloned().unwrap_or_default();
        for (_, property) in properties {
            let forms = property["forms"].as_array().cloned().unwrap_or_default();
            let writes = forms
                .iter()
                .find(|f| f["op"].to_string().contains("writeproperty"));
            let Some(form) = writes.filter(|_| property["readOnly"] != true) else {
                continue;
            };
            let path = path(form);
            let answer = alice.send(Method::PUT, &path, Some(&Value::Null)).await;
            assert_eq!(answer.status, 403, "{path}: {}", answer.text());
            checked.push(path);
        }
    }
    assert!(
        checked.len() >= 70,
        "{} affordances: {checked:?}",
        checked.len()
    );
    // With the lease, they are hers to use.
    alice.invoke("auth/acquire_control", json!({})).await;
    let (_, homed) = alice.invoke("stage/home", json!({})).await;
    assert_eq!(homed["status"], "completed", "{homed}");
}

#[tokio::test(flavor = "multi_thread")]
async fn tls_serves_https_and_refuses_plain_http() {
    let folder = tempfile::tempdir().unwrap();
    let certified = rcgen::generate_simple_self_signed(vec!["localhost".to_owned()]).unwrap();
    let (certificate, key) = (certified.cert.pem(), certified.signing_key.serialize_pem());
    let roots = Roots {
        data: folder.path().to_owned(),
        scratch: folder.path().join("scratch"),
    };
    let api = Api::builder(&roots).build().unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    let (pem, private) = (certificate.clone().into_bytes(), key.into_bytes());
    let serving = tokio::spawn(async move {
        let shutdown = async move {
            let _ = stopped.await;
        };
        api.serve_tls(listener, &pem, &private, shutdown).await
    });
    let request = b"GET /health HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n";
    // Over TLS, trusting the certificate.
    let provider = Arc::new(tokio_rustls::rustls::crypto::ring::default_provider());
    let mut trusted = tokio_rustls::rustls::RootCertStore::empty();
    let der = certified.cert.der().clone();
    trusted.add(der).unwrap();
    let config = tokio_rustls::rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_root_certificates(trusted)
        .with_no_client_auth();
    let connector = tokio_rustls::TlsConnector::from(Arc::new(config));
    let tcp = tokio::net::TcpStream::connect(address).await.unwrap();
    let name = tokio_rustls::rustls::pki_types::ServerName::try_from("localhost").unwrap();
    let mut tls = connector.connect(name, tcp).await.unwrap();
    tls.write_all(request).await.unwrap();
    let mut answer = String::new();
    tls.read_to_string(&mut answer).await.unwrap_or_default();
    assert!(answer.starts_with("HTTP/1.1 200"), "{answer}");
    // Plain HTTP gets no answer.
    let mut plain = tokio::net::TcpStream::connect(address).await.unwrap();
    plain.write_all(request).await.unwrap();
    let mut bytes = Vec::new();
    let _ = plain.read_to_end(&mut bytes).await;
    assert!(!bytes.starts_with(b"HTTP/1.1 200"), "{bytes:?}");
    let _ = stop.send(());
    serving.await.unwrap().unwrap();
}
