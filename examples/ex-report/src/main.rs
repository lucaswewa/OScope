//! `ex-report`: P47's reports, on P46's results. A snake across the edge of
//! the simulated H&E slide's first island is scanned at 20×, stitched and
//! its nuclei counted; then the job `report.make` writes a report on the
//! project's results as PDF and HTML, through OScope's template and
//! through one of the example's own. The page lists the reports. See the
//! README.

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
            eprintln!("ex-report: {error:#}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> anyhow::Result<()> {
    let example = Example::from_args_with("ex-report", 8233, &["--rate"])?;
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

/// Makes a report on `project` as `options` say: its files' folder in the
/// data route.
async fn report(address: SocketAddr, project: &str, options: Value) -> anyhow::Result<String> {
    let params = json!({"project": project, "options": options});
    let done = job(
        address,
        "jobs/submit",
        json!({"kind": "report.make", "params": params}),
    )
    .await?;
    Ok(format!("/api/v1/data/{project}/reports/{}", made(&done)?))
}

/// The scan, its stitch and its count, and reports on them. With `check`,
/// held to P47's spec.
async fn walk(address: SocketAddr, (w, h): (f64, f64), check: bool) -> anyhow::Result<()> {
    invoke(address, "stage/home", json!({})).await?;
    invoke(address, "camera/start_streaming", json!({})).await?;
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
    let created = invoke(
        address,
        "projects/create",
        json!({"name": "ex-report", "sample": "the simulated H&E slide"}),
    )
    .await?;
    let project = created["id"].as_str().context("a project")?.to_owned();
    let origin = json!({"x": at.x - w / 2.0, "y": at.y - h / 2.0});
    let rect = json!({"rect": {"origin": origin, "size": {"width": w, "height": h}}});
    let scan = json!({"request": {"pattern": "snake", "region": rect}, "project": project, "name": "a slide"});
    let acquisition = made(&job(address, "scan/run", scan).await?)?;
    let stitch =
        json!({"kind": "stitch.run", "params": {"project": project, "acquisition": acquisition}});
    let mosaic = made(&job(address, "jobs/submit", stitch).await?)?;
    let params = json!({"project": project, "input": {"kind": "result", "id": mosaic}});
    job(
        address,
        "jobs/submit",
        json!({"kind": "cells.count", "params": params}),
    )
    .await?;
    // OScope's template, then one of the example's: the standard one with
    // its title in blue.
    let standard = report(address, &project, json!({})).await?;
    let template = get(address, "/api/v1/projects/template/standard")
        .await?
        .json()?;
    let typst = template["typst"].as_str().context("a Typst template")?;
    let blue = typst.replace(
        "#text(18pt, weight: \"bold\", report.title)",
        "#text(18pt, weight: \"bold\", fill: blue, report.title)",
    );
    ensure!(blue != typst, "the title's line in the standard template");
    let html = template["html"]
        .as_str()
        .context("an HTML template")?
        .replace("h1 { margin: 0;", "h1 { color: #1d4ed8; margin: 0;");
    invoke(
        address,
        "projects/save_template",
        json!({"name": "blue", "typst": blue, "html": html}),
    )
    .await?;
    let own = report(
        address,
        &project,
        json!({"template": "blue", "formats": ["pdf"]}),
    )
    .await?;
    let pdf = fetch(address, &format!("{standard}/report.pdf")).await?;
    let page = get(address, &format!("{standard}/report.html")).await?;
    let model = get(address, &format!("{standard}/model.json"))
        .await?
        .json()?;
    let headings: Vec<&str> = model["sections"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|s| s["heading"].as_str())
        .collect();
    println!(
        "ex-report: a snake over {:.2} × {:.2} mm at 20×, stitched and its nuclei counted; a report on its {} results: {}",
        w / 1e3,
        h / 1e3,
        headings.len(),
        headings.join("; ")
    );
    println!(
        "  report.pdf of {} kB and report.html of {} kB, through OScope's template; another through the example's own, \"blue\"",
        pdf.body.len() / 1024,
        page.body.len() / 1024
    );
    println!("  open http://{address}/ to read them");
    if !check {
        return Ok(());
    }
    ensure!(
        pdf.status == 200 && pdf.body.starts_with(b"%PDF-"),
        "the PDF: {}",
        pdf.status
    );
    ensure!(
        pdf.header("content-type") == Some("application/pdf"),
        "the PDF's type: {:?}",
        pdf.header("content-type")
    );
    ensure!(
        headings == ["a slide stitched", "a slide stitched: cells"],
        "the sections: {headings:?}"
    );
    for want in [
        "a slide stitched: cells",
        "<svg",
        "data:image/jpeg;base64,",
        "per_mm2",
        "The areas of",
    ] {
        ensure!(page.body.contains(want), "{want:?} in the HTML report");
    }
    let mine = fetch(address, &format!("{own}/report.pdf")).await?;
    ensure!(
        mine.status == 200 && mine.body.starts_with(b"%PDF-") && mine.body != pdf.body,
        "the example's template's PDF"
    );
    let contents = get(address, &format!("/api/v1/projects/project/{project}"))
        .await?
        .json()?;
    let named: Vec<&str> = contents["reports"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|r| r["name"].as_str())
        .collect();
    ensure!(
        named == ["ex-report: results", "ex-report: results (blue)"],
        "the reports: {named:?}"
    );
    let templates = get(address, "/api/v1/projects/templates").await?.json()?;
    ensure!(
        templates == json!(["standard", "blue"]),
        "the templates: {templates}"
    );
    let index = get(address, "/").await?;
    ensure!(
        index.status == 200 && index.body.contains("app.js"),
        "the page: {}",
        index.status
    );
    Ok(())
}
