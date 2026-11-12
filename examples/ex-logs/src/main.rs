//! `ex-logs`: OScope's logs on a page, with a live tail and its filters,
//! buttons that write a record at each level, and the daily files (P05).
//! See the README.

use std::net::SocketAddr;
use std::process::ExitCode;
use std::time::Duration;

use anyhow::{Context, ensure};
use ex_common::{Example, get, post};
use futures_util::{SinkExt, StreamExt};
use oscope_api::Api;
use oscope_api::logs::EMIT_TARGET;
use serde_json::{Value, json};
use tokio_tungstenite::tungstenite::Message;

/// The levels, lowest first.
const LEVELS: [&str; 6] = ["trace", "debug", "info", "warn", "error", "critical"];

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("ex-logs: {error:#}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> anyhow::Result<()> {
    let example = Example::from_args("ex-logs", 8213)?;
    tokio::spawn(tick());
    let api = Api::builder(&example.roots);
    let page = concat!(env!("CARGO_MANIFEST_DIR"), "/page");
    example.serve(api, page, smoke).await
}

/// Something to watch: a record every 2 s, and every fifth in a job.
async fn tick() {
    let mut interval = tokio::time::interval(Duration::from_secs(2));
    for n in 1_u64..=u64::MAX {
        interval.tick().await;
        if n % 5 == 0 {
            let job = format!("demo-{}", n / 5);
            tracing::info_span!("job", job_id = %job).in_scope(|| {
                tracing::warn!(target: "ex_logs", n, "a slow step in a job");
            });
        } else {
            tracing::info!(target: "ex_logs", n, "tick");
        }
    }
}

/// The next JSON message from the tail.
async fn next<S>(socket: &mut S) -> anyhow::Result<Value>
where
    S: StreamExt<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin,
{
    let message = tokio::time::timeout(Duration::from_secs(10), socket.next()).await;
    let message = message
        .context("the tail is silent")?
        .context("the tail ended")??;
    Ok(serde_json::from_str(message.to_text()?)?)
}

/// `--smoke`: the page, the buttons' records through the tail and the
/// query, and today's file.
async fn smoke(address: SocketAddr) -> anyhow::Result<()> {
    for (path, kind) in [("/", "text/html"), ("/app.js", "text/javascript")] {
        let file = get(address, path).await?;
        ensure!(
            file.status == 200 && file.content_type.starts_with(kind),
            "{path}"
        );
    }

    let url = format!("ws://{address}/api/v1/media/logs");
    let (mut socket, _) = tokio_tungstenite::connect_async(url).await?;
    let query = json!({"target": EMIT_TARGET, "text": "smoke"});
    socket.send(Message::text(query.to_string())).await?;
    // What it sent on opening, then what follows the query.
    while next(&mut socket).await?["query"]["text"] != "smoke" {}
    while next(&mut socket).await?.get("caught_up").is_none() {}
    for level in LEVELS {
        let note = json!({"level": level, "message": format!("smoke {level}")});
        let answer = post(address, "/api/v1/logs/emit", &note).await?;
        ensure!(answer.status == 201, "emit {level}: {}", answer.status);
        let record = next(&mut socket).await?["record"].clone();
        ensure!(record["level"] == level, "the tail sent {record}");
        println!("tail: {} {}", record["level"], record["message"]);
    }

    let query = format!("/api/v1/logs/records?target={EMIT_TARGET}&text=smoke");
    let page = get(address, &query).await?.json()?;
    let levels: Vec<&str> = page["records"]
        .as_array()
        .context("records")?
        .iter()
        .filter_map(|record| record["level"].as_str())
        .collect();
    ensure!(
        levels.iter().rev().eq(LEVELS.iter()),
        "the query found {levels:?}"
    );
    println!("query: {} records, newest first", levels.len());

    let files = get(address, "/api/v1/logs/files").await?.json()?;
    let name = files[0]["name"].as_str().context("a daily file")?;
    let file = get(address, &format!("/api/v1/logs/files/{name}")).await?;
    ensure!(
        file.status == 200 && file.body.contains("smoke critical"),
        "{name}"
    );
    println!("file: {name}, {} lines", file.body.lines().count());
    Ok(())
}
