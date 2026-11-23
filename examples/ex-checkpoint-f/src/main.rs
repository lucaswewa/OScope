//! `ex-checkpoint-f`: Checkpoint F, a demo script over HTTP. A snake across
//! the edge of the simulated H-DAB slide's first island is scanned at 20×
//! and stitched. Its nuclei are counted, each classed by its DAB (P46),
//! and its DAB quantified across the tissue (P45). Then a report on all of
//! it is made as PDF and HTML (P47). See the README.

use std::net::SocketAddr;
use std::process::ExitCode;
use std::sync::Arc;

use anyhow::{Context, ensure};
use ex_common::{Example, fetch, get, invoke, job, made};
use oscope_api::Api;
use oscope_api::devices::{Devices, DevicesConfig};
use oscope_core::Point2;
use oscope_jobs::{JobService, JobsConfig};
use oscope_sim::{HeTissue, Slide};
use oscope_store::{Scratch, ScratchPolicy, Store};
use serde_json::{Value, json};

/// The scan's region across the island's edge, in µm: smaller for
/// `--smoke`.
const REGION: (f64, f64) = (1_000.0, 750.0);
const SMOKE_REGION: (f64, f64) = (600.0, 450.0);

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("ex-checkpoint-f: {error:#}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> anyhow::Result<()> {
    let example = Example::from_args_with("ex-checkpoint-f", 8217, &["--rate"])?;
    let roots = &example.roots;
    for folder in [&roots.data, &roots.scratch] {
        std::fs::create_dir_all(folder)?;
    }
    let mut config = DevicesConfig::default();
    config.simulator.sensor = "quarter".to_owned();
    config.simulator.objective = "20x".to_owned();
    let rate = example.number("--rate");
    config.simulator.rate = rate.unwrap_or(if example.smoke { 10 } else { 1 }) as f64;
    let devices = Devices::from_config(&config, &roots.data).await;
    let devices = Arc::new(devices.map_err(anyhow::Error::msg)?);
    let jobs = JobService::open(&roots.data.join("jobs.sqlite"), JobsConfig::default())?;
    let store = Store::open(&roots.data, &roots.data.join("index.sqlite"))?;
    let scratch = Scratch::open(&roots.scratch, ScratchPolicy::default())?;
    let api = Api::builder(roots)
        .devices(devices)
        .projects(store)
        .scratch(scratch)
        .jobs(jobs);
    let region = if example.smoke { SMOKE_REGION } else { REGION };
    example.when_serving(move |address| async move { walk(address, region, false).await });
    let page = concat!(env!("CARGO_MANIFEST_DIR"), "/page");
    example
        .serve(api, page, move |address| async move {
            walk(address, region, true).await
        })
        .await
}

/// A point inside the first island, near where its tissue ends.
fn edge() -> Point2 {
    let tissue = HeTissue::new(1, Slide::COVERSLIP);
    let middle = tissue.islands()[0];
    let mut at = middle;
    while tissue.is_tissue(at) && at.x < middle.x + 20_000.0 {
        at.x += 25.0;
    }
    Point2::new(at.x - 150.0, at.y)
}

/// Submits job `kind` with `params`: the first result it made.
async fn submit(address: SocketAddr, kind: &str, params: Value) -> anyhow::Result<String> {
    made(
        &job(
            address,
            "jobs/submit",
            json!({"kind": kind, "params": params}),
        )
        .await?,
    )
}

/// A result's `summary.json`.
async fn summary(address: SocketAddr, project: &str, id: &str) -> anyhow::Result<Value> {
    let path = format!("/api/v1/data/{project}/results/{id}/summary.json");
    get(address, &path).await?.json()
}

/// The script. With `check`, held to Checkpoint F.
async fn walk(address: SocketAddr, (w, h): (f64, f64), check: bool) -> anyhow::Result<()> {
    invoke(address, "stage/home", json!({})).await?;
    invoke(address, "camera/start_streaming", json!({})).await?;
    invoke(address, "simulation/load", json!({"name": "H-DAB tissue"})).await?;
    let at = edge();
    let focus = invoke(
        address,
        "simulation/in_focus",
        json!({"x": at.x - 200.0, "y": at.y}),
    )
    .await?;
    invoke(
        address,
        "stage/move_to",
        json!({"x": at.x, "y": at.y, "z": focus}),
    )
    .await?;
    job(address, "exposure/auto", json!({})).await?;
    let sample = "the simulated H-DAB slide";
    let created = invoke(
        address,
        "projects/create",
        json!({"name": "Checkpoint F", "sample": sample}),
    )
    .await?;
    let project = created["id"].as_str().context("a project")?.to_owned();
    // The slide, scanned and stitched.
    let origin = json!({"x": at.x - w / 2.0, "y": at.y - h / 2.0});
    let rect = json!({"rect": {"origin": origin, "size": {"width": w, "height": h}}});
    let request = json!({"pattern": "snake", "region": rect});
    let scan = json!({"request": request, "project": project, "name": "an H-DAB slide"});
    let acquisition = made(&job(address, "scan/run", scan).await?)?;
    let params = json!({"project": project, "acquisition": acquisition});
    let mosaic = submit(address, "stitch.run", params).await?;
    let input = json!({"kind": "result", "id": mosaic});
    // Its nuclei counted and classed by their DAB; its DAB across the
    // tissue.
    let params = json!({"project": project, "input": input, "options": {"dab": true}});
    let counted = summary(
        address,
        &project,
        &submit(address, "cells.count", params).await?,
    )
    .await?;
    let params = json!({"project": project, "input": input, "options": {"stains": "hdab"}});
    let separated = submit(address, "stain.separate", params).await?;
    let stained = summary(address, &project, &separated).await?;
    // The report.
    let params = json!({"project": project, "options": {}});
    let report = submit(address, "report.make", params).await?;
    let folder = format!("/api/v1/data/{project}/reports/{report}");
    let pdf = fetch(address, &format!("{folder}/report.pdf")).await?;
    let page = get(address, &format!("{folder}/report.html")).await?;
    let model = get(address, &format!("{folder}/model.json"))
        .await?
        .json()?;
    let classes = &counted["dab"];
    let class = |name: &str| classes[name].as_u64().unwrap_or(0);
    let nuclei = counted["count"].as_u64().unwrap_or(0);
    let positive = class("weak") + class("moderate") + class("strong");
    let dab = &stained["dab"];
    let sections = model["sections"].as_array().map_or(0, Vec::len);
    println!(
        "ex-checkpoint-f: an H-DAB slide, a snake over {:.2} × {:.2} mm at 20×, stitched; {nuclei} nuclei, {:.0} per mm² of tissue",
        w / 1e3,
        h / 1e3,
        counted["per_mm2"].as_f64().unwrap_or(0.0)
    );
    println!(
        "  by their DAB: {} negative, {} weak, {} moderate, {} strong: {:.1} % positive",
        class("negative"),
        class("weak"),
        class("moderate"),
        class("strong"),
        100.0 * positive as f64 / nuclei.max(1) as f64
    );
    println!(
        "  across the tissue: DAB positive {:.1} %, H-score {:.0}",
        dab["positive_percent"].as_f64().unwrap_or(0.0),
        dab["h_score"].as_f64().unwrap_or(0.0)
    );
    println!(
        "  a report of {sections} sections: report.pdf of {} kB and report.html of {} kB",
        pdf.body.len() / 1024,
        page.body.len() / 1024
    );
    println!("  open http://{address}/ to read it");
    if !check {
        return Ok(());
    }
    ensure!(nuclei > 50, "{nuclei} nuclei");
    let share = positive as f64 / nuclei as f64;
    ensure!(
        (0.2..=0.6).contains(&share),
        "{share:.2} of the nuclei positive"
    );
    let tissue = dab["positive_percent"].as_f64().unwrap_or(0.0);
    ensure!(
        tissue > 0.0 && dab["h_score"].as_f64().unwrap_or(0.0) > 0.0,
        "DAB: {dab}"
    );
    ensure!(
        pdf.status == 200 && pdf.body.starts_with(b"%PDF-"),
        "the PDF: {}",
        pdf.status
    );
    for want in [
        "an H-DAB slide stitched: cells",
        "positive_percent",
        "h_score",
        "strong",
        "<svg",
    ] {
        ensure!(page.body.contains(want), "{want:?} in the HTML report");
    }
    ensure!(sections >= 3, "{sections} sections");
    let index = get(address, "/").await?;
    ensure!(
        index.status == 200 && index.body.contains("app.js"),
        "the page: {}",
        index.status
    );
    Ok(())
}
