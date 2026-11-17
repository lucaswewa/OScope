//! `ex-workspace`: P48's Workspace. A snake across the edge of the simulated
//! H&E slide's first island is scanned at 20× and stitched, and P46's count
//! gives the glass's white. Then, over HTTP, the cell-counting recipe runs
//! on a region of the mosaic as an analysis. It is made again, the same;
//! made again with a larger least area; and the two compared. The page
//! lists the results with their provenance. See the README.

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

/// The region analysed, in µm: its size, and how far its left edge is left
/// of the mosaic's middle, in the tissue whose edge is 150 µm right of it.
const ROI: (f64, f64, f64) = (200.0, 150.0, 250.0);

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("ex-workspace: {error:#}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> anyhow::Result<()> {
    let example = Example::from_args_with("ex-workspace", 8216, &["--rate"])?;
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

/// The cell-counting recipe, for pixels of `cell` µm and the glass's
/// `white`: each stain's amount, the hematoxylin's thresholded at Otsu's
/// level, the nuclei split and measured.
fn counting(cell: f64, white: &Value) -> Value {
    json!({
        "format": 1,
        "name": "nuclei",
        "inputs": [{"name": "image", "type": "image"}],
        "nodes": [
            {"id": "stains", "node": "stain.deconvolve", "params": {"stains": "he", "white": white}},
            {"id": "hematoxylin", "node": "convert.format", "params": {"channels": "red"}},
            {"id": "nuclei", "node": "threshold.otsu", "params": {}},
            {"id": "split", "node": "cells.split", "params": {}},
            {"id": "measure", "node": "cells.measure", "params": {"pixel_um": [cell, cell], "min_area": 10}},
        ],
        "edges": [
            {"from": "input.image", "to": "stains.image"},
            {"from": "stains.amounts", "to": "hematoxylin.image"},
            {"from": "hematoxylin.image", "to": "nuclei.image"},
            {"from": "nuclei.mask", "to": "split.mask"},
            {"from": "split.labels", "to": "measure.labels"},
        ],
        "outputs": [
            {"name": "hematoxylin", "from": "hematoxylin.image"},
            {"name": "objects", "from": "measure.objects"},
            {"name": "outlines", "from": "measure.outlines"},
            {"name": "nuclei", "from": "nuclei.mask"},
        ],
    })
}

/// Waits for the job an action of the Workspace started: the result it
/// made.
async fn analysed(address: SocketAddr, action: &str, input: Value) -> anyhow::Result<String> {
    made(&job(address, &format!("workspace/{action}"), input).await?)
}

/// A result's table of nuclei, as its bytes, and its rows.
async fn objects(address: SocketAddr, project: &str, id: &str) -> anyhow::Result<(Vec<u8>, usize)> {
    let path = format!("/api/v1/data/{project}/results/{id}/objects.csv");
    let csv = fetch(address, &path).await?;
    ensure!(csv.status == 200, "{path}: {}", csv.status);
    let rows = csv
        .body
        .split(|&b| b == b'\n')
        .filter(|l| l.len() > 1)
        .count();
    Ok((csv.body, rows.saturating_sub(1)))
}

/// The slide, its count, and the analyses. With `check`, held to P48's
/// spec.
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
    let sample = "the simulated H&E slide";
    let created = invoke(
        address,
        "projects/create",
        json!({"name": "ex-workspace", "sample": sample}),
    )
    .await?;
    let project = created["id"].as_str().context("a project")?.to_owned();
    let origin = json!({"x": at.x - w / 2.0, "y": at.y - h / 2.0});
    let rect = json!({"rect": {"origin": origin, "size": {"width": w, "height": h}}});
    let request = json!({"pattern": "snake", "region": rect});
    let scan = json!({"request": request, "project": project, "name": "a slide"});
    let acquisition = made(&job(address, "scan/run", scan).await?)?;
    let params = json!({"project": project, "acquisition": acquisition});
    let stitch = json!({"kind": "stitch.run", "params": params});
    let mosaic = made(&job(address, "jobs/submit", stitch).await?)?;
    // P46's count of the whole mosaic, for the glass's white.
    let params = json!({"project": project, "input": {"kind": "result", "id": mosaic}});
    let count = json!({"kind": "cells.count", "params": params});
    let counted = made(&job(address, "jobs/submit", count).await?)?;
    let path = format!("/api/v1/data/{project}/results/{counted}/summary.json");
    let summary = get(address, &path).await?.json()?;
    let contents = get(address, &format!("/api/v1/projects/project/{project}"))
        .await?
        .json()?;
    let results = contents["results"].as_array().context("results")?;
    let record = results.iter().find(|r| r["id"] == mosaic.as_str());
    let image = &record.context("the mosaic's record")?["image"];
    let pixel = image["pixel_size"]["x"].as_f64();
    let pixel = pixel.context("the mosaic's pixel size")?;
    // A raw mosaic's 2 × 2 cells, as the recipe sees them.
    let cell = 2.0 * pixel;
    // The recipe, on a region of tissue left of the mosaic's middle: the
    // same tissue, however large the scan.
    let middle = |side: &str| image[side].as_f64().unwrap_or(0.0) * pixel / 2.0;
    let (width, height, left) = ROI;
    let (x, y) = (middle("width") - left, middle("height") - height / 2.0);
    let analysis = json!({
        "recipe": counting(cell, &summary["white"]),
        "input": {"kind": "result", "id": mosaic},
        "roi": {"x": x, "y": y, "width": width, "height": height},
        "name": "nuclei in a region",
    });
    let input = json!({"project": project, "analysis": analysis});
    let first = analysed(address, "run", input).await?;
    let (table, nuclei) = objects(address, &project, &first).await?;
    // Made again, as it was, and with nuclei of 25 µm² at least.
    let again = json!({"project": project, "result": first});
    let second = analysed(address, "rerun", again).await?;
    let (same, _) = objects(address, &project, &second).await?;
    let larger = json!({"measure": {"min_area": 25}});
    let changed = json!({"project": project, "result": first, "changes": larger});
    let third = analysed(address, "rerun", changed).await?;
    let (_, fewer) = objects(address, &project, &third).await?;
    let path = format!("/api/v1/workspace/difference/{project}/{first}/{third}");
    let difference = get(address, &path).await?.json()?;
    let area_mm2 = width * height / 1e6;
    println!(
        "ex-workspace: a snake over {:.2} × {:.2} mm at 20×, stitched; the counting recipe on {width:.0} × {height:.0} µm of it: {nuclei} nuclei, {:.0} per mm² (P46's count of the slide: {:.0})",
        w / 1e3,
        h / 1e3,
        nuclei as f64 / area_mm2,
        summary["per_mm2"].as_f64().unwrap_or(0.0)
    );
    println!(
        "  made again: {}; with nuclei of 25 µm² at least: {fewer}, the difference {difference}",
        if same == table {
            "the same table"
        } else {
            "a different table"
        }
    );
    println!("  open http://{address}/ to see the results and their provenance");
    if !check {
        return Ok(());
    }
    ensure!(nuclei > 50, "{nuclei} nuclei in the region");
    let density = nuclei as f64 / area_mm2;
    let slide = summary["per_mm2"].as_f64().unwrap_or(0.0);
    ensure!(
        (density / slide - 1.0).abs() < 0.25,
        "{density:.0} per mm² against the count's {slide:.0}"
    );
    ensure!(same == table, "made again, a different table");
    ensure!(fewer < nuclei, "{fewer} of 25 µm², against {nuclei}");
    let path = "/recipe/workspace.run/recipe/nodes/measure/params/min_area";
    ensure!(
        difference == json!([{"path": path, "before": 10, "after": 25}]),
        "the difference: {difference}"
    );
    let listed = get(address, &format!("/api/v1/workspace/results/{project}"))
        .await?
        .json()?;
    let ours = listed.as_array().context("results")?.iter();
    let hashed = ours
        .filter(|r| r["provenance"]["hashes"][&mosaic].is_string())
        .count();
    ensure!(hashed == 3, "{hashed} results with the mosaic's hash");
    let index = get(address, "/").await?;
    ensure!(
        index.status == 200 && index.body.contains("app.js"),
        "the page: {}",
        index.status
    );
    Ok(())
}
