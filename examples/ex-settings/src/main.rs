//! `ex-settings`: P52's settings. A configuration file of version 1 is
//! migrated, its comments kept; then the `settings` Thing reads it, changes
//! it, refuses what the next start would, and exports and imports all the
//! settings. See the README.

use std::net::SocketAddr;
use std::path::Path;
use std::process::ExitCode;
use std::sync::Arc;

use anyhow::ensure;
use ex_common::{Example, get, invoke, put};
use oscope_api::Api;
use oscope_server::{TomlFile, migrate_file};
use serde_json::{Value, json};

/// A configuration of version 1, as P01–P51 wrote them.
const V1: &str = r#"# ex-settings' configuration, as P01–P51 wrote them: version 1's.

[server]
# This computer only.
address = "127.0.0.1"
port = 8300

[storage]
data_root = "data"
scratch_root = "scratch"

[devices.simulator]
# The sample loaded at start.
sample = "H&E tissue"
"#;

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("ex-settings: {error:#}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> anyhow::Result<()> {
    let example = Example::from_args("ex-settings", 8219)?;
    let roots = &example.roots;
    // A file of version 1, new each run, migrated: the old one kept beside.
    let folder = roots.data.join("config");
    let _ = std::fs::remove_dir_all(&folder);
    std::fs::create_dir_all(&folder)?;
    let path = folder.join("oscope.toml");
    std::fs::write(&path, V1)?;
    let changed = migrate_file(&path).map_err(anyhow::Error::msg)?;
    println!("ex-settings: migrated: {}", changed.join("; "));
    let api = Api::builder(roots)
        .settings(roots.data.join("settings"))
        .config_file(Arc::new(TomlFile(path.clone())));
    let early = path.clone();
    example.when_serving(move |address| async move { walk(address, &early, false).await });
    let page = concat!(env!("CARGO_MANIFEST_DIR"), "/page");
    example
        .serve(api, page, |address| async move {
            walk(address, &path, true).await
        })
        .await
}

/// The setting `name`.
async fn setting(address: SocketAddr, name: &str) -> anyhow::Result<Value> {
    get(address, &format!("/api/v1/settings/{name}"))
        .await?
        .json()
}

/// Writes the setting `name`: the answer's status, and why if refused.
async fn write(address: SocketAddr, name: &str, value: Value) -> anyhow::Result<(u16, String)> {
    let answer = put(address, &format!("/api/v1/settings/{name}"), &value).await?;
    let problem = answer.json().unwrap_or_default();
    let why = problem["detail"].as_str().unwrap_or_default().to_owned();
    Ok((answer.status, why))
}

/// The settings read, changed, refused, and imported. With `check`, held
/// to P52's spec.
async fn walk(address: SocketAddr, path: &Path, check: bool) -> anyhow::Result<()> {
    let system = setting(address, "system").await?;
    let simulator = setting(address, "simulator").await?;
    let (port, sample) = (&system["port"], &simulator["sample"]);
    println!("  the file's: port {port}, the sample {sample}");
    // Your display preferences: no lease needed.
    let mine = json!({"theme": "dark", "units": "mm", "scale_bar": true, "colormap": "viridis"});
    let (shown, _) = write(address, "display", mine.clone()).await?;
    let display = setting(address, "display").await?;
    println!("  your display: {display}");
    // The next start's: a sample, and a port.
    let dab = json!({"sample": "H-DAB tissue"});
    let (sampled, _) = write(address, "simulator", dab).await?;
    let mut moved = system.clone();
    moved["port"] = json!(8301);
    let (ported, _) = write(address, "system", moved).await?;
    let restart = setting(address, "restart_needed").await?;
    println!("  the next start's: the sample H-DAB tissue, port 8301; a restart needed: {restart}");
    // Refused, as the next start would refuse it.
    let refused = write(address, "simulator", json!({"rate": 0.0})).await?;
    println!("  a clock rate of 0: {}, {}", refused.0, refused.1);
    // All of them as one document, imported again.
    let document = setting(address, "document").await?;
    let imported = invoke(address, "settings/import", json!({"document": document})).await?;
    println!("  imported: {imported}");
    let text = std::fs::read_to_string(path)?;
    println!("  the file now:");
    for line in text.lines() {
        println!("    {line}");
    }
    println!("  open http://{address}/ to try them");
    if !check {
        return Ok(());
    }
    ensure!(
        (shown, sampled, ported) == (201, 201, 201),
        "{shown} {sampled} {ported}"
    );
    ensure!(display == mine, "{display}");
    ensure!(restart == true, "{restart}");
    ensure!(
        refused.0 == 400 && refused.1.contains("clock rate"),
        "{refused:?}"
    );
    let parts = json!(["system", "simulator", "display_defaults", "accounts"]);
    ensure!(imported == parts, "{imported}");
    for line in ["version = 2", "# This computer only.", "port = 8301"] {
        ensure!(text.contains(line), "{line}: {text}");
    }
    ensure!(text.contains("sample = \"H-DAB tissue\""), "{text}");
    let old = std::fs::read_to_string(path.with_file_name("oscope.toml.v1.bak"))?;
    ensure!(old == V1, "the old file: {old}");
    let index = get(address, "/").await?;
    let served = index.status == 200 && index.body.contains("app.js");
    ensure!(served, "the page: {}", index.status);
    Ok(())
}
