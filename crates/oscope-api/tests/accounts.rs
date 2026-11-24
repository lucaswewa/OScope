//! The `accounts` Thing over HTTP (W8.6, ADR-0055): on a server without
//! security, the first account added and the restart it waits for; on a
//! secured one, an account added and removed with the lease, its sessions
//! ended, the sessions open and the idle times; one's own password changed
//! with the old one and without the lease, and refused without it; and the
//! last account kept while the next start listens on the network; and its
//! description.

use std::net::{IpAddr, Ipv4Addr};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use oscope_api::auth::{Accounts, Idle, Security};
use oscope_api::settings::{ConfigFile, Saved, System};
use oscope_api::{Api, Roots};
use oscope_sim::SimulatorConfig;
use serde_json::{Value, json};
use teta_wot::http::axum::http::Method;
use teta_wot::testing::{TestClient, TestResponse};

/// A configuration file in memory, its next start listening on `address`.
struct Memory(Mutex<Saved>);

impl Memory {
    fn on(address: IpAddr) -> Arc<Self> {
        let system = System {
            data_root: "data".into(),
            scratch_root: "scratch".into(),
            address,
            port: 8210,
        };
        let simulator = SimulatorConfig::default();
        Arc::new(Self(Mutex::new(Saved { system, simulator })))
    }
}

impl ConfigFile for Memory {
    fn read(&self) -> Result<Saved, String> {
        Ok(self.0.lock().unwrap().clone())
    }

    fn write(&self, saved: &Saved) -> Result<(), String> {
        *self.0.lock().unwrap() = saved.clone();
        Ok(())
    }
}

/// A server on `data`, with `file`, secured if the accounts' file there
/// has accounts, as `oscope-server` is.
async fn server(data: &Path, file: Arc<Memory>) -> TestClient {
    let roots = Roots {
        data: data.to_owned(),
        scratch: data.join("scratch"),
    };
    let mut api = Api::builder(&roots).config_file(file);
    let accounts = Accounts::open(&data.join("accounts.json")).unwrap();
    if !accounts.is_empty() {
        let security = Security::new(Arc::new(accounts), None, Idle::default());
        api = api.security(security);
    }
    TestClient::start(api.build().unwrap().into_thing_server())
        .await
        .unwrap()
}

/// A caller: their token, if any, sent with each request.
struct Caller<'a> {
    client: &'a TestClient,
    bearer: Option<String>,
}

impl Caller<'_> {
    async fn send(&self, method: Method, path: &str, body: Option<&Value>) -> TestResponse {
        let mut headers = vec![("content-type", "application/json")];
        if let Some(bearer) = &self.bearer {
            headers.push(("authorization", bearer.as_str()));
        }
        let bytes = body.map(|b| b.to_string().into_bytes());
        let path = match path.starts_with("http") {
            true => path.to_owned(),
            false => format!("/api/v1/{path}"),
        };
        self.client.request(method, &path, &headers, bytes).await
    }

    async fn get(&self, path: &str) -> Value {
        let answer = self.send(Method::GET, path, None).await;
        assert_eq!(answer.status, 200, "{path}: {}", answer.text());
        answer.json()
    }

    /// Invokes `action` with `input`: its status, and its invocation as it ended.
    async fn invoke(&self, action: &str, input: Value) -> (u16, Value) {
        let started = self.send(Method::POST, action, Some(&input)).await;
        if started.status != 201 {
            return (started.status.as_u16(), started.json());
        }
        let href = started.json()["href"].as_str().unwrap().to_owned();
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            let answer = self.send(Method::GET, &href, None).await.json();
            if !["pending", "running"].contains(&answer["status"].as_str().unwrap_or("")) {
                return (201, answer);
            }
            assert!(Instant::now() < deadline, "{action}: {answer}");
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }
}

/// `name`, logged in with `password`.
async fn login<'a>(client: &'a TestClient, name: &str, password: &str) -> Caller<'a> {
    let body = json!({"name": name, "password": password});
    let answer = client.post_json("/api/v1/auth/login", Some(&body)).await;
    assert_eq!(answer.status, 200, "{name}: {}", answer.text());
    let token = answer.json()["token"].as_str().unwrap().to_owned();
    let bearer = Some(format!("Bearer {token}"));
    Caller { client, bearer }
}

const HERE: IpAddr = IpAddr::V4(Ipv4Addr::LOCALHOST);

#[tokio::test(flavor = "multi_thread")]
async fn the_first_account_turns_security_on_at_the_next_start() {
    let folder = tempfile::tempdir().unwrap();
    let client = server(folder.path(), Memory::on(HERE)).await;
    let anyone = Caller {
        client: &client,
        bearer: None,
    };
    assert_eq!(anyone.get("accounts/names").await, json!([]));
    assert_eq!(anyone.get("accounts/secure").await, false);
    assert_eq!(anyone.get("accounts/restart_needed").await, false);
    assert_eq!(anyone.get("accounts/sessions").await, json!([]));
    assert_eq!(anyone.get("accounts/idle").await, Value::Null);
    // Refused: a password too short, a name an account can't have.
    for wrong in [
        json!({"name": "ana", "password": "short"}),
        json!({"name": "Ana Smith", "password": "a long password"}),
        json!({"name": "local", "password": "a long password"}),
    ] {
        let (_, refused) = anyone
            .invoke("accounts/add", json!({"account": wrong}))
            .await;
        assert!(
            refused["error"].to_string().contains("refused (400)"),
            "{wrong}: {refused}"
        );
    }
    let first = json!({"account": {"name": "ana", "password": "ana's password"}});
    let (_, added) = anyone.invoke("accounts/add", first.clone()).await;
    assert_eq!(added["output"], json!(["ana"]), "{added}");
    // Kept in the file the server reads at its start; security waits for it.
    assert_eq!(
        Accounts::open(&folder.path().join("accounts.json"))
            .unwrap()
            .names(),
        ["ana"]
    );
    assert_eq!(anyone.get("accounts/restart_needed").await, true);
    assert_eq!(anyone.get("accounts/secure").await, false);
    let (_, twice) = anyone.invoke("accounts/add", first).await;
    assert!(
        twice["error"]
            .to_string()
            .contains("there is an account ana"),
        "{twice}"
    );
    // Without a login, no password is anyone's own to change.
    let change = json!({"old": "ana's password", "password": "a new password"});
    let (_, refused) = anyone.invoke("accounts/change_password", change).await;
    assert!(
        refused["error"].to_string().contains("refused (401)"),
        "{refused}"
    );
    // Removed again, as it was: nothing waits for a restart.
    let (_, removed) = anyone
        .invoke("accounts/remove", json!({"name": "ana"}))
        .await;
    assert_eq!(removed["output"], json!([]), "{removed}");
    assert_eq!(anyone.get("accounts/restart_needed").await, false);
}

#[tokio::test(flavor = "multi_thread")]
async fn with_security_accounts_change_with_the_lease_and_a_password_with_the_old_one() {
    let folder = tempfile::tempdir().unwrap();
    let accounts = Accounts::open(&folder.path().join("accounts.json")).unwrap();
    accounts.add("ana", "ana's password").unwrap();
    let client = server(folder.path(), Memory::on(HERE)).await;
    let ana = login(&client, "ana", "ana's password").await;
    assert_eq!(ana.get("accounts/secure").await, true);
    assert_eq!(ana.get("accounts/restart_needed").await, false);
    // Without the lease: refused.
    let ben = json!({"account": {"name": "ben", "password": "ben's password"}});
    let (status, _) = ana.invoke("accounts/add", ben.clone()).await;
    assert_eq!(status, 403);
    let (_, lease) = ana.invoke("auth/acquire_control", json!({})).await;
    assert_eq!(lease["status"], "completed", "{lease}");
    let (_, added) = ana.invoke("accounts/add", ben).await;
    assert_eq!(added["output"], json!(["ana", "ben"]), "{added}");
    // Added, it logs in at once: security has the same accounts.
    let ben = login(&client, "ben", "ben's password").await;
    let sessions = ana.get("accounts/sessions").await;
    let mut whose: Vec<&str> = (sessions.as_array().unwrap().iter())
        .map(|s| s["account"].as_str().unwrap())
        .collect();
    whose.sort_unstable();
    assert_eq!(whose, ["ana", "ben"]);
    assert!(sessions[0]["idle_s"].as_f64().unwrap() >= 0.0, "{sessions}");
    let idle = ana.get("accounts/idle").await;
    assert_eq!(
        (idle["session_s"].as_f64(), idle["lease_s"].as_f64()),
        (Some(8.0 * 3600.0), Some(600.0))
    );
    assert!(idle["lease_idle_s"].as_f64().unwrap() < 60.0, "{idle}");
    // One's own password: the old one needed; no lease needed.
    let wrong = json!({"old": "not it", "password": "ben's new password"});
    let (_, refused) = ben.invoke("accounts/change_password", wrong).await;
    assert!(
        refused["error"]
            .to_string()
            .contains("the old password isn't right"),
        "{refused}"
    );
    let right = json!({"old": "ben's password", "password": "ben's new password"});
    let (_, changed) = ben.invoke("accounts/change_password", right).await;
    assert_eq!(changed["status"], "completed", "{changed}");
    let again = json!({"name": "ben", "password": "ben's password"});
    assert_eq!(
        client
            .post_json("/api/v1/auth/login", Some(&again))
            .await
            .status,
        401
    );
    login(&client, "ben", "ben's new password").await;
    // Removed: its sessions end with it.
    let (_, removed) = ana.invoke("accounts/remove", json!({"name": "ben"})).await;
    assert_eq!(removed["output"], json!(["ana"]), "{removed}");
    assert_eq!(
        ben.send(Method::GET, "accounts/names", None).await.status,
        401
    );
    let (_, gone) = ana.invoke("accounts/remove", json!({"name": "ben"})).await;
    assert!(
        gone["error"].to_string().contains("no account ben"),
        "{gone}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn the_last_account_stays_while_the_next_start_listens_on_the_network() {
    let folder = tempfile::tempdir().unwrap();
    let accounts = Accounts::open(&folder.path().join("accounts.json")).unwrap();
    accounts.add("ana", "ana's password").unwrap();
    accounts.add("ben", "ben's password").unwrap();
    let file = Memory::on(IpAddr::V4(Ipv4Addr::UNSPECIFIED));
    let client = server(folder.path(), Arc::clone(&file)).await;
    let ana = login(&client, "ana", "ana's password").await;
    ana.invoke("auth/acquire_control", json!({})).await;
    // One of two goes; the last stays, saying why.
    let (_, removed) = ana.invoke("accounts/remove", json!({"name": "ben"})).await;
    assert_eq!(removed["output"], json!(["ana"]), "{removed}");
    let (_, kept) = ana.invoke("accounts/remove", json!({"name": "ana"})).await;
    assert!(
        kept["error"].to_string().contains("conflict (409)")
            && kept["error"].to_string().contains("listens on the network"),
        "{kept}"
    );
    // The next start on this computer alone: the last may go, and security
    // with it. It is the caller's own, whose session ends with it.
    file.0.lock().unwrap().system.address = HERE;
    let (status, _) = ana.invoke("accounts/remove", json!({"name": "ana"})).await;
    assert_eq!(status, 201);
    let path = folder.path().join("accounts.json");
    let deadline = Instant::now() + Duration::from_secs(30);
    while !Accounts::open(&path).unwrap().is_empty() {
        assert!(Instant::now() < deadline, "ana wasn't removed");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(
        ana.send(Method::GET, "accounts/names", None).await.status,
        401
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn the_accounts_thing_describes_itself() {
    let folder = tempfile::tempdir().unwrap();
    let client = server(folder.path(), Memory::on(HERE)).await;
    let mut td = client.get("/api/v1/accounts/").await.json();
    // The ID differs from computer to computer: its form, then the rest.
    let id = td.as_object_mut().unwrap().remove("id").unwrap();
    assert!(id.as_str().unwrap().starts_with("urn:uuid:"), "{id}");
    insta::assert_json_snapshot!("accounts", td);
}
