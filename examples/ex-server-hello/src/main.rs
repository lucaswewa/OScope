//! `ex-server-hello`: OScope's server with the `system` Thing, and a page
//! beside it that shows the Thing's properties and its Thing Description
//! (P04). See the README.

use std::net::SocketAddr;
use std::process::ExitCode;

use anyhow::{Context, ensure};
use ex_common::{Example, get};
use oscope_api::Api;
use oscope_store::{Scratch, ScratchPolicy};

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("ex-server-hello: {error:#}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> anyhow::Result<()> {
    let example = Example::from_args("ex-server-hello", 8211)?;
    // A scratch space, as the server has: the `system` Thing gives its health.
    let scratch = Scratch::open(&example.roots.scratch, ScratchPolicy::default())?;
    let api = Api::builder(&example.roots).scratch(scratch);
    let page = concat!(env!("CARGO_MANIFEST_DIR"), "/page");
    example.serve(api, page, smoke).await
}

/// `--smoke`: everything the page reads is served, at the page's origin.
async fn smoke(address: SocketAddr) -> anyhow::Result<()> {
    for (path, kind) in [("/", "text/html"), ("/app.js", "text/javascript")] {
        let file = get(address, path).await?;
        ensure!(file.status == 200, "{path}: {}", file.status);
        ensure!(
            file.content_type.starts_with(kind),
            "{path}: {}",
            file.content_type
        );
    }
    let td = get(address, "/api/v1/system/").await?.json()?;
    let properties = td["properties"]
        .as_object()
        .context("the TD's properties")?;
    println!("the system Thing's {} properties:", properties.len());
    for (name, property) in properties {
        let href = property["forms"][0]["href"].as_str().context("a form")?;
        let value = get(address, href).await?;
        ensure!(value.status == 200, "{name}: {}", value.status);
        println!("  {name}: {}", value.body);
    }
    ensure!(
        properties.len() == 10,
        "10 properties, not {}",
        properties.len()
    );
    let ping = get(address, "/api/v1/media/ping").await?;
    ensure!(ping.body == "pong", "the media plane's ping: {}", ping.body);
    Ok(())
}
