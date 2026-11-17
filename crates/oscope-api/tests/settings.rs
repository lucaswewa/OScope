//! The `settings` Thing over HTTP (P52): each account's display
//! preferences, kept across a restart and written without the lease; the
//! configuration file's system and simulator settings, checked before
//! they are written; and one document of them all, exported and imported.

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

/// A configuration file in memory.
struct Memory(Mutex<Saved>);

impl Memory {
    fn new() -> Arc<Self> {
        let system = System {
            data_root: "data".into(),
            scratch_root: "scratch".into(),
            address: IpAddr::V4(Ipv4Addr::LOCALHOST),
            port: 8210,
        };
        let simulator = SimulatorConfig::default();
        Arc::new(Self(Mutex::new(Saved { system, simulator })))
    }

    fn get(&self) -> Saved {
        self.0.lock().unwrap().clone()
    }
}

impl ConfigFile for Memory {
    fn read(&self) -> Result<Saved, String> {
        Ok(self.get())
    }

    fn write(&self, saved: &Saved) -> Result<(), String> {
        *self.0.lock().unwrap() = saved.clone();
        Ok(())
    }
}

/// A server on `data`, its settings kept there, with `file` if any, and
/// with the accounts `alice` and `bob` if `secure`.
async fn server(data: &Path, file: Option<Arc<Memory>>, secure: bool) -> TestClient {
    let roots = Roots {
        data: data.to_owned(),
        scratch: data.join("scratch"),
    };
    let mut api = Api::builder(&roots).settings(data.join("settings"));
    if let Some(file) = file {
        api = api.config_file(file);
    }
    if secure {
        let accounts = Accounts::open(&data.join("accounts.json")).unwrap();
        if accounts.is_empty() {
            accounts.add("alice", "alice's password").unwrap();
            accounts.add("bob", "bob's password").unwrap();
        }
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
    /// Sends a request for `path`, under the API's prefix unless it is a
    /// whole href.
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

    /// Writes `value` to the property `path`: the answer's status.
    async fn put(&self, path: &str, value: Value) -> (u16, String) {
        let answer = self.send(Method::PUT, path, Some(&value)).await;
        (answer.status.as_u16(), answer.text())
    }

    /// Invokes `action` with `input`: its invocation as it ended.
    async fn invoke(&self, action: &str, input: Value) -> Value {
        let started = self.send(Method::POST, action, Some(&input)).await;
        assert_eq!(started.status, 201, "{action}: {}", started.text());
        let href = started.json()["href"].as_str().unwrap().to_owned();
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            let answer = self.send(Method::GET, &href, None).await.json();
            if !["pending", "running"].contains(&answer["status"].as_str().unwrap_or("")) {
                return answer;
            }
            assert!(Instant::now() < deadline, "{action}: {answer}");
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }
}

/// `name`, logged in.
async fn login<'a>(client: &'a TestClient, name: &str) -> Caller<'a> {
    let body = json!({"name": name, "password": format!("{name}'s password")});
    let answer = client.post_json("/api/v1/auth/login", Some(&body)).await;
    assert_eq!(answer.status, 200, "{name}: {}", answer.text());
    let token = answer.json()["token"].as_str().unwrap().to_owned();
    let bearer = Some(format!("Bearer {token}"));
    Caller { client, bearer }
}

fn display(theme: &str, units: &str, scale_bar: bool, colormap: &str) -> Value {
    json!({"theme": theme, "units": units, "scale_bar": scale_bar, "colormap": colormap})
}

#[tokio::test(flavor = "multi_thread")]
async fn display_preferences_are_each_account_s_and_need_no_lease() {
    let folder = tempfile::tempdir().unwrap();
    let client = server(folder.path(), None, true).await;
    let (alice, bob) = (login(&client, "alice").await, login(&client, "bob").await);
    let defaults = display("system", "um", true, "grey");
    assert_eq!(alice.get("settings/display").await, defaults);
    // Her own, without the lease; Bob's are still the defaults.
    let hers = display("dark", "mm", false, "viridis");
    assert_eq!(alice.put("settings/display", hers.clone()).await.0, 201);
    assert_eq!(alice.get("settings/display").await, hers);
    assert_eq!(bob.get("settings/display").await, defaults);
    // The defaults are everyone's: they need the lease.
    let light = display("light", "um", true, "magma");
    let refused = bob.put("settings/display_defaults", light.clone()).await;
    assert_eq!(refused.0, 403, "{}", refused.1);
    let lease = bob.invoke("auth/acquire_control", json!({})).await;
    assert_eq!(lease["output"]["holder"], "bob", "{lease}");
    let set = bob.put("settings/display_defaults", light.clone()).await;
    assert_eq!(set.0, 201, "{}", set.1);
    assert_eq!(bob.get("settings/display").await, light);
    assert_eq!(alice.get("settings/display").await, hers);
    // Not a valid preference: refused by its schema, and hers kept.
    let wrong = display("dark", "inches", false, "grey");
    assert_eq!(alice.put("settings/display", wrong).await.0, 422);
    let accounts = alice.get("settings/accounts").await;
    assert_eq!(accounts, json!({"alice": hers}));
    // Kept across a restart.
    drop((alice, bob));
    drop(client);
    let client = server(folder.path(), None, true).await;
    let alice = login(&client, "alice").await;
    assert_eq!(alice.get("settings/display").await, hers);
    assert_eq!(alice.get("settings/display_defaults").await, light);
    // Reset, she has the defaults again.
    let reset = alice
        .send(Method::POST, "settings/display/reset", None)
        .await;
    assert!(reset.status.is_success(), "{}", reset.text());
    assert_eq!(alice.get("settings/display").await, light);
    assert_eq!(alice.get("settings/accounts").await, json!({}));
}

#[tokio::test(flavor = "multi_thread")]
async fn the_file_s_settings_are_checked_before_they_are_written() {
    let folder = tempfile::tempdir().unwrap();
    let file = Memory::new();
    let client = server(folder.path(), Some(Arc::clone(&file)), false).await;
    let local = Caller {
        client: &client,
        bearer: None,
    };
    let system = local.get("settings/system").await;
    assert_eq!(system["port"], 8210, "{system}");
    let simulator = local.get("settings/simulator").await;
    assert_eq!(simulator["sample"], "H&E tissue", "{simulator}");
    assert_eq!(local.get("settings/restart_needed").await, false);
    // Each refused as the next start would refuse it, and nothing written.
    let mut wrong = system.clone();
    wrong["scratch_root"] = wrong["data_root"].clone();
    let refused = local.put("settings/system", wrong).await;
    assert!(
        refused.0 == 400 && refused.1.contains("different"),
        "{refused:?}"
    );
    let mut none = system.clone();
    none["data_root"] = json!("");
    let refused = local.put("settings/system", none).await;
    assert!(
        refused.0 == 400 && refused.1.contains("needed"),
        "{refused:?}"
    );
    let mut open = system.clone();
    open["address"] = json!("0.0.0.0");
    let refused = local.put("settings/system", open).await;
    assert!(
        refused.0 == 400 && refused.1.contains("accounts"),
        "{refused:?}"
    );
    for (wrong, why) in [
        (json!({"sample": "no such sample"}), "no such sample"),
        (json!({"rate": 0.0}), "clock rate"),
        (json!({"colour": true}), "colour"),
    ] {
        let refused = local.put("settings/simulator", wrong).await;
        assert!(refused.0 == 400 && refused.1.contains(why), "{refused:?}");
    }
    assert_eq!(file.get().system.port, 8210);
    assert_eq!(local.get("settings/restart_needed").await, false);
    // Good ones are written, and taken up at the next start.
    let mut moved = system.clone();
    moved["port"] = json!(8300);
    assert_eq!(local.put("settings/system", moved).await.0, 201);
    let faster = json!({"sample": "H-DAB tissue", "rate": 2.0});
    assert_eq!(local.put("settings/simulator", faster).await.0, 201);
    let saved = file.get();
    assert_eq!(saved.system.port, 8300);
    let (sample, rate) = (saved.simulator.sample.as_str(), saved.simulator.rate);
    assert_eq!((sample, rate), ("H-DAB tissue", 2.0));
    assert_eq!(saved.simulator.focus, SimulatorConfig::default().focus);
    assert_eq!(local.get("settings/restart_needed").await, true);
    // Without a file, there are none to read or write.
    let other = tempfile::tempdir().unwrap();
    let bare = server(other.path(), None, false).await;
    let bare = Caller {
        client: &bare,
        bearer: None,
    };
    assert_eq!(bare.get("settings/system").await, Value::Null);
    let refused = bare.put("settings/system", system).await;
    assert!(
        refused.0 == 409 && refused.1.contains("--config"),
        "{refused:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_settings_document_is_exported_and_imported_whole_or_not_at_all() {
    let folder = tempfile::tempdir().unwrap();
    let file = Memory::new();
    let client = server(folder.path(), Some(Arc::clone(&file)), false).await;
    let here = Caller {
        client: &client,
        bearer: None,
    };
    let mine = display("dark", "mm", true, "inferno");
    assert_eq!(here.put("settings/display", mine.clone()).await.0, 201);
    let defaults = display("light", "um", false, "grey");
    assert_eq!(here.put("settings/display_defaults", defaults).await.0, 201);
    let quarter = json!({"sensor": "quarter"});
    assert_eq!(here.put("settings/simulator", quarter).await.0, 201);
    let document = here.get("settings/document").await;
    assert_eq!(document["format"], "oscope.settings");
    assert_eq!(document["accounts"], json!({"local": mine}));
    // Into another server: every part.
    let other = tempfile::tempdir().unwrap();
    let elsewhere = Memory::new();
    let client = server(other.path(), Some(Arc::clone(&elsewhere)), false).await;
    let there = Caller {
        client: &client,
        bearer: None,
    };
    let imported = there
        .invoke("settings/import", json!({"document": document}))
        .await;
    let parts = json!(["system", "simulator", "display_defaults", "accounts"]);
    assert_eq!(imported["output"], parts, "{imported}");
    assert_eq!(elsewhere.get(), file.get());
    assert_eq!(there.get("settings/document").await, document);
    assert_eq!(there.get("settings/restart_needed").await, true);
    // A part alone: accounts join those there.
    let hers = display("light", "mm", false, "magma");
    let part = json!({"format": "oscope.settings", "version": 1, "accounts": {"alice": hers}});
    let imported = there
        .invoke("settings/import", json!({"document": part}))
        .await;
    assert_eq!(imported["output"], json!(["accounts"]), "{imported}");
    let both = json!({"alice": hers, "local": mine});
    assert_eq!(there.get("settings/accounts").await, both);
    let mut document = document;
    document["accounts"] = both;
    // A part that is wrong: nothing changes.
    let mut wrong = document.clone();
    wrong["simulator"]["rate"] = json!(-1.0);
    wrong["display_defaults"] = display("dark", "um", true, "magma");
    let refused = there
        .invoke("settings/import", json!({"document": wrong}))
        .await;
    let error = refused["error"].to_string();
    assert!(
        error.contains("refused (400)") && error.contains("rate"),
        "{refused}"
    );
    assert_eq!(there.get("settings/document").await, document);
    // Not a settings document, or a newer one's: refused.
    for (key, value) in [("format", json!("something else")), ("version", json!(2))] {
        let mut other = document.clone();
        other[key] = value;
        let refused = there
            .invoke("settings/import", json!({"document": other}))
            .await;
        assert!(refused["error"].to_string().contains("400"), "{refused}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn the_settings_thing_describes_itself() {
    let folder = tempfile::tempdir().unwrap();
    let client = server(folder.path(), None, false).await;
    let mut td = client.get("/api/v1/settings/").await.json();
    // The ID differs from computer to computer: its form, then the rest.
    let id = td.as_object_mut().unwrap().remove("id").unwrap();
    assert!(id.as_str().unwrap().starts_with("urn:uuid:"), "{id}");
    insta::assert_json_snapshot!("settings", td);
}
