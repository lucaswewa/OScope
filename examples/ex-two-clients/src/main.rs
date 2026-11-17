//! `ex-two-clients`: P49's security. Two accounts, `alice` and `bob`, share
//! the simulated microscope behind logins. One holds control and moves the
//! stage. The other watches, can't move it, asks for control, and is
//! handed it. The page logs in as either: open it twice. See the README.

use std::net::SocketAddr;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, ensure};
use ex_common::{Example, call_as, get, post};
use oscope_api::Api;
use oscope_api::auth::{Accounts, Idle, Security};
use oscope_api::devices::{Devices, DevicesConfig};
use oscope_jobs::{JobService, JobsConfig};
use serde_json::{Value, json};

/// The two accounts, and their passwords.
const ACCOUNTS: [(&str, &str); 2] = [("alice", "alice's password"), ("bob", "bob's password")];

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("ex-two-clients: {error:#}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> anyhow::Result<()> {
    let example = Example::from_args("ex-two-clients", 8213)?;
    let roots = &example.roots;
    std::fs::create_dir_all(&roots.data)?;
    let accounts = Accounts::open(&roots.data.join("accounts.json"))?;
    for (name, password) in ACCOUNTS {
        if !accounts.names().iter().any(|n| n == name) {
            accounts.add(name, password)?;
        }
    }
    let devices = Devices::from_config(&DevicesConfig::default(), &roots.data).await;
    let devices = Arc::new(devices.map_err(anyhow::Error::msg)?);
    let jobs = JobService::open(&roots.data.join("jobs.sqlite"), JobsConfig::default())?;
    let security = Security::new(Arc::new(accounts), None, Idle::default());
    let api = Api::builder(roots)
        .devices(devices)
        .jobs(jobs)
        .security(security);
    example.when_serving(|address| async move { walk(address, false).await });
    let page = concat!(env!("CARGO_MANIFEST_DIR"), "/page");
    example
        .serve(
            api,
            page,
            |address| async move { walk(address, true).await },
        )
        .await
}

/// Logs `name` in: their session's token.
async fn login(address: SocketAddr, name: &str, password: &str) -> anyhow::Result<String> {
    let body = json!({"name": name, "password": password});
    let answer = post(address, "/api/v1/auth/login", &body).await?;
    ensure!(answer.status == 200, "{name}: {}", answer.status);
    let token = answer.json()?["token"].as_str().map(str::to_owned);
    token.context("a token")
}

/// Invokes `action` as `token`'s account: the refusal's status, or 200 and
/// the action's output.
async fn act(
    address: SocketAddr,
    token: &str,
    action: &str,
    input: Value,
) -> anyhow::Result<(u16, Value)> {
    let path = format!("/api/v1/{action}");
    let started = call_as(address, token, "POST", &path, Some(&input)).await?;
    if started.status != 201 {
        return Ok((started.status, Value::Null));
    }
    let href = started.json()?["href"]
        .as_str()
        .context("an href")?
        .to_owned();
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        let invocation = call_as(address, token, "GET", &href, None).await?.json()?;
        match invocation["status"].as_str() {
            Some("completed") => return Ok((200, invocation["output"].clone())),
            Some("pending" | "running") => {}
            _ => anyhow::bail!("{action}: {invocation}"),
        }
        ensure!(Instant::now() < deadline, "{action} didn't end");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// Alice and Bob, scripted. With `check`, held to P49's spec.
async fn walk(address: SocketAddr, check: bool) -> anyhow::Result<()> {
    let [(a, a_password), (b, b_password)] = ACCOUNTS;
    let alice = login(address, a, a_password).await?;
    let bob = login(address, b, b_password).await?;
    let to = |x: f64| json!({"x": x, "y": 10_000.0, "z": 1_000.0});
    let anonymous = get(address, "/api/v1/stage/position").await?.status;
    let (took, _) = act(address, &alice, "auth/acquire_control", json!({})).await?;
    act(address, &alice, "stage/home", json!({})).await?;
    let (moved, _) = act(address, &alice, "stage/move_to", to(10_000.0)).await?;
    let (refused, _) = act(address, &bob, "stage/move_to", to(10_100.0)).await?;
    let (_, asked) = act(address, &bob, "auth/request_control", json!({})).await?;
    let to_bob = json!({"to": b});
    let (_, handed) = act(address, &alice, "auth/hand_over_control", to_bob).await?;
    let (bob_moved, _) = act(address, &bob, "stage/move_to", to(10_100.0)).await?;
    let (alice_refused, _) = act(address, &alice, "stage/move_to", to(10_200.0)).await?;
    println!(
        "ex-two-clients: alice took control ({took}), homed the stage and moved it ({moved}); bob was refused ({refused})"
    );
    println!(
        "  the requests {}; alice handed it over to {}; bob moved ({bob_moved}), alice was refused ({alice_refused})",
        asked["requests"], handed["holder"]
    );
    println!("  open http://{address}/ twice, and log in as alice in one and bob in the other");
    if !check {
        return Ok(());
    }
    ensure!(anonymous == 401, "without a token: {anonymous}");
    ensure!(
        (took, moved, refused) == (200, 200, 403),
        "{took} {moved} {refused}"
    );
    ensure!(asked["requests"] == json!([b]), "the requests: {asked}");
    ensure!(handed["holder"] == b, "the holder: {handed}");
    ensure!(
        (bob_moved, alice_refused) == (200, 403),
        "{bob_moved} {alice_refused}"
    );
    let index = get(address, "/").await?;
    ensure!(
        index.status == 200 && index.body.contains("app.js"),
        "the page: {}",
        index.status
    );
    Ok(())
}
