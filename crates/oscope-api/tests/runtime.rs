//! R06 lifecycle, authentication and advertised capability boundaries.

use std::sync::Arc;

use oscope_api::auth::{Accounts, Idle, Security};
use oscope_api::devices::{Devices, DevicesConfig};
use oscope_api::{Api, Roots};
use serde_json::json;
use teta_wot::testing::TestClient;

#[tokio::test]
async fn readiness_is_authenticated_and_turns_unavailable_on_restart() {
    let folder = tempfile::tempdir().unwrap();
    let roots = Roots {
        data: folder.path().join("data"),
        scratch: folder.path().join("scratch"),
    };
    let accounts = Arc::new(Accounts::open(&folder.path().join("accounts.json")).unwrap());
    let api = Api::builder(&roots)
        .security(Security::new(
            accounts,
            Some("runtime-test-token".into()),
            Idle::default(),
        ))
        .build()
        .unwrap();
    let restart = api.restart();
    let client = TestClient::start(api.into_thing_server()).await.unwrap();
    assert_eq!(client.get("/ready").await.status, 401);
    let client = client.with_header("authorization", "Bearer runtime-test-token");
    assert_eq!(client.get("/ready").await.json(), json!({"status":"ready"}));
    restart.request();
    let answer = client.get("/ready").await;
    assert_eq!(answer.status, 503);
    assert_eq!(answer.json(), json!({"status":"draining"}));
    assert_eq!(client.get("/health").await.status, 200);
}

#[tokio::test]
async fn microscope_workflows_are_advertised_without_host_power() {
    let folder = tempfile::tempdir().unwrap();
    let roots = Roots {
        data: folder.path().join("data"),
        scratch: folder.path().join("scratch"),
    };
    let config = DevicesConfig {
        camera: "fake".into(),
        stage: "fake".into(),
        illumination: "fake".into(),
        objective: "fake".into(),
        ..DevicesConfig::default()
    };
    let devices = Arc::new(Devices::from_config(&config, folder.path()).await.unwrap());
    let store = oscope_store::Store::open(&roots.data, &roots.data.join("index.sqlite")).unwrap();
    let api = Api::builder(&roots)
        .projects(store)
        .devices(devices)
        .power(false)
        .build()
        .unwrap();
    let descriptions = api.thing_descriptions("http://localhost:8210/").unwrap();
    for name in [
        "preview",
        "capture",
        "autofocus",
        "exposure",
        "scan",
        "calibration",
        "live_mosaic",
        "workspace",
    ] {
        assert!(descriptions.contains_key(name), "{name}");
    }
    let actions = descriptions["power"]["actions"].as_object().unwrap();
    assert!(actions.contains_key("restart"));
    assert!(!actions.contains_key("reboot") && !actions.contains_key("shutdown"));
    let client = TestClient::start(api.into_thing_server()).await.unwrap();
    for action in ["reboot", "shutdown"] {
        let answer = client
            .post_json(
                &format!("/api/v1/power/{action}"),
                Some(&json!({"confirm":true,"force":true})),
            )
            .await;
        assert_eq!(answer.status, 404, "{action}: {}", answer.text());
    }
}
