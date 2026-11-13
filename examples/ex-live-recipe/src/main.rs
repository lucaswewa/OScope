//! `ex-live-recipe`: live recipes (P22). It serves the simulated microscope,
//! homed and over the H&E slide's tissue, lit and streaming, with a page
//! that shows the live view, a choice of live recipes, and buttons that
//! capture a flat field and a dark frame. See the README.

use std::net::SocketAddr;
use std::path::Path;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, bail, ensure};
use ex_common::{Example, get, post, put};
use futures_util::{SinkExt, StreamExt};
use oscope_acquire::preview::wire::Header;
use oscope_api::Api;
use oscope_api::devices::{Devices, DevicesConfig};
use oscope_core::{LightState, Um, Xyz};
use oscope_sim::Rate;
use serde_json::{Value, json};
use tokio_tungstenite::tungstenite::Message;

/// The live recipes' budget for `--smoke`, in ms: ten times the default,
/// as the smoke runs are built without optimising.
const SMOKE_BUDGET_MS: f64 = 200.0;

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("ex-live-recipe: {error:#}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> anyhow::Result<()> {
    let example = Example::from_args("ex-live-recipe", 8218)?;
    // The simulated microscope on the scaled-down sensor, homed and placed
    // over the tissue at 50 times real time, then in real time.
    let mut config = DevicesConfig::default();
    config.simulator.sensor = "quarter".to_owned();
    config.simulator.rate = 50.0;
    let devices = Devices::from_config(&config, Path::new("."))
        .await
        .map_err(anyhow::Error::msg)?;
    let simulator = devices.simulator.clone().context("the simulator")?;
    devices.stage.home().await?;
    let over_tissue = Xyz::new(Um(40_000.0), Um(12_000.0), Um(5_000.0));
    devices.stage.move_to(over_tissue, None).await?;
    simulator.clock.set_rate(Rate::RealTime);
    let light = LightState {
        channel: "transmitted".to_owned(),
        on: true,
        intensity: 0.6,
    };
    devices.lights.set(light).await?;
    let page = Path::new(env!("CARGO_MANIFEST_DIR")).join("page");
    let api = Api::builder(&example.roots)
        .settings(example.roots.data.join("settings"))
        .devices(Arc::new(devices));
    example.serve(api, page, smoke).await
}

/// Invokes `action` with `input`, and waits for it to end: its output, or
/// its error.
async fn invoke(
    address: SocketAddr,
    action: &str,
    input: Value,
) -> anyhow::Result<Result<Value, String>> {
    let started = post(address, &format!("/api/v1/{action}"), &input).await?;
    ensure!(
        started.status == 201,
        "{action}: {} {}",
        started.status,
        started.body
    );
    let started = started.json()?;
    let href = started["href"].as_str().context("an invocation's href")?;
    let path = href
        .split_once("/api/")
        .map_or(href.to_owned(), |(_, path)| format!("/api/{path}"));
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        let invocation = get(address, &path).await?.json()?;
        match invocation["status"].as_str() {
            Some("completed") => return Ok(Ok(invocation["output"].clone())),
            Some("error") => return Ok(Err(invocation["error"]["detail"].to_string())),
            _ if Instant::now() > deadline => bail!("{action} didn't end"),
            _ => tokio::time::sleep(Duration::from_millis(20)).await,
        }
    }
}

/// The next preview's header. Panics after 60 s.
async fn preview<S>(socket: &mut S) -> anyhow::Result<Header>
where
    S: StreamExt<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin,
{
    loop {
        let message = tokio::time::timeout(Duration::from_secs(60), socket.next())
            .await
            .context("no preview within 60 s")?
            .context("the stream closed")??;
        if let Message::Binary(bytes) = message {
            return Ok(Header::read(&bytes)?.header);
        }
    }
}

/// `--smoke`: the page and its recipes; a flat field and a dark frame
/// captured; each recipe checked, set, and run on the previews; one over
/// its budget refused; and none again.
async fn smoke(address: SocketAddr) -> anyhow::Result<()> {
    let page = get(address, "/").await?;
    ensure!(
        page.status == 200 && page.content_type.starts_with("text/html"),
        "the page"
    );
    let budget = put(
        address,
        "/api/v1/preview/recipe_budget_ms",
        &json!(SMOKE_BUDGET_MS),
    )
    .await?;
    ensure!(budget.status < 300, "the budget: {}", budget.status);
    // Recipes are timed at the default viewport's size: the stream's here,
    // which a slow machine can time quickly.
    let defaults = json!({ "width": 320, "height": 240, "quality": 80, "max_fps": 30.0 });
    let defaults = put(address, "/api/v1/preview/defaults", &defaults).await?;
    ensure!(defaults.status < 300, "the defaults: {}", defaults.status);
    let index = get(address, "/recipes/index.json").await?.json()?;
    let recipes = index.as_array().context("the recipes' index")?;
    ensure!(
        post(address, "/api/v1/camera/start_streaming", &json!({}))
            .await?
            .status
            == 201,
        "streaming"
    );
    let url = format!("ws://{address}/api/v1/preview/stream");
    let (mut socket, _) = tokio_tungstenite::connect_async(url).await?;
    let mut params = json!({ "width": 320, "height": 240, "ack": 0 });
    socket.send(Message::text(params.to_string())).await?;

    // The flat field with the slide taken out; the dark frame with the light
    // off.
    let sample = invoke(address, "simulation/unload", json!({}))
        .await?
        .map_err(anyhow::Error::msg)?;
    let flat = invoke(
        address,
        "preview/capture_reference",
        json!({ "name": "flat", "frames": 2 }),
    )
    .await?;
    let sample = sample.as_str().context("the sample unloaded")?;
    invoke(address, "simulation/load", json!({ "name": sample }))
        .await?
        .map_err(anyhow::Error::msg)?;
    let off = json!({ "channel": "transmitted", "on": false });
    invoke(address, "illumination/set_light", off)
        .await?
        .map_err(anyhow::Error::msg)?;
    let dark = invoke(
        address,
        "preview/capture_reference",
        json!({ "name": "dark", "frames": 2 }),
    )
    .await?;
    let on = json!({ "channel": "transmitted", "on": true, "intensity": 0.6 });
    invoke(address, "illumination/set_light", on)
        .await?
        .map_err(anyhow::Error::msg)?;
    ensure!(
        flat.is_ok() && dark.is_ok(),
        "the references: {flat:?} {dark:?}"
    );
    let references = get(address, "/api/v1/preview/references").await?.json()?;
    ensure!(
        references == json!(["dark", "flat"]),
        "references: {references}"
    );

    let mut times = Vec::new();
    for entry in recipes {
        let file = entry["file"].as_str().context("a recipe's file")?;
        let recipe = get(address, &format!("/recipes/{file}")).await?.json()?;
        let check = invoke(
            address,
            "preview/set_live_recipe",
            json!({ "recipe": recipe }),
        )
        .await?;
        let check = check.map_err(|error| anyhow::anyhow!("{file}: {error}"))?;
        times.push(format!(
            "{file} {:.1} ms",
            check["time_ms"].as_f64().unwrap_or_default()
        ));
        // Previews come, made with it.
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            let header = preview(&mut socket).await?;
            params["ack"] = json!(header.sequence + 1);
            socket.send(Message::text(params.to_string())).await?;
            let sessions = get(address, "/api/v1/preview/sessions").await?.json()?;
            if sessions[0]["recipe_ms"].is_number() {
                break;
            }
            ensure!(Instant::now() < deadline, "{file}: no preview made with it");
        }
    }

    // Over its budget, refused; and none, the look again.
    put(
        address,
        "/api/v1/preview/recipe_budget_ms",
        &json!(0.000_001),
    )
    .await?;
    let recipe = get(address, "/recipes/levels.json").await?.json()?;
    let refused = invoke(
        address,
        "preview/set_live_recipe",
        json!({ "recipe": recipe }),
    )
    .await?;
    ensure!(
        refused
            .as_ref()
            .is_err_and(|e| e.contains("over its budget")),
        "{refused:?}"
    );
    put(
        address,
        "/api/v1/preview/recipe_budget_ms",
        &json!(SMOKE_BUDGET_MS),
    )
    .await?;
    let cleared = invoke(
        address,
        "preview/set_live_recipe",
        json!({ "recipe": null }),
    )
    .await?;
    ensure!(cleared == Ok(Value::Null), "{cleared:?}");
    socket.close(None).await?;
    println!(
        "served: the page; a flat field and a dark frame captured; {}; one over budget refused",
        times.join(", ")
    );
    Ok(())
}
