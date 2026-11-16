//! `ex-stain`: P45's stain analysis on stitched slides. A snake across the
//! edge of the simulated H&E slide's first island is scanned, stitched, and
//! its stains separated by colour deconvolution, with H&E's vectors and
//! with Macenko's estimate; then the same place of the H-DAB slide, its DAB
//! binned and scored. The page shows each mosaic and stain's image. See the
//! README.

use std::net::SocketAddr;
use std::process::ExitCode;
use std::sync::Arc;

use anyhow::{Context, ensure};
use ex_common::{Example, get, invoke, job, made};
use oscope_api::Api;
use oscope_api::devices::{Devices, DevicesConfig};
use oscope_core::Point2;
use oscope_imaging::stain::{EOSIN, HEMATOXYLIN, Vector};
use oscope_jobs::{JobService, JobsConfig};
use oscope_sim::{HeTissue, Slide};
use oscope_store::{Scratch, ScratchPolicy, Store};
use serde_json::{Value, json};

/// The scan's region across the island's edge, in µm: smaller for
/// `--smoke`.
const REGION: (f64, f64) = (2_000.0, 1_500.0);
const SMOKE_REGION: (f64, f64) = (1_200.0, 900.0);

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("ex-stain: {error:#}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> anyhow::Result<()> {
    let example = Example::from_args_with("ex-stain", 8231, &["--rate"])?;
    let roots = &example.roots;
    for folder in [&roots.data, &roots.scratch] {
        std::fs::create_dir_all(folder)?;
    }
    let mut config = DevicesConfig::default();
    config.simulator.sensor = "quarter".to_owned();
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

/// Where the first island's tissue ends, going right from its middle.
fn edge() -> Point2 {
    let tissue = HeTissue::new(1, Slide::COVERSLIP);
    let middle = tissue.islands()[0];
    let mut at = middle;
    while tissue.is_tissue(at) && at.x < middle.x + 20_000.0 {
        at.x += 25.0;
    }
    at
}

/// Scans and stitches `region` about `at` into project `project`, named
/// `name`: the mosaic's ID.
async fn mosaic(
    address: SocketAddr,
    project: &str,
    name: &str,
    at: Point2,
    (w, h): (f64, f64),
) -> anyhow::Result<String> {
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
    let origin = json!({"x": at.x - w / 2.0, "y": at.y - h / 2.0});
    let rect = json!({"rect": {"origin": origin, "size": {"width": w, "height": h}}});
    let request = json!({"pattern": "snake", "region": rect});
    let scan = json!({"request": request, "project": project, "name": name});
    let acquisition = made(&job(address, "scan/run", scan).await?)?;
    let params = json!({"project": project, "acquisition": acquisition});
    let stitch = json!({"kind": "stitch.run", "params": params});
    made(&job(address, "jobs/submit", stitch).await?)
}

/// Separates mosaic `result`'s stains as `options` say: the summary.
async fn separate(
    address: SocketAddr,
    project: &str,
    result: &str,
    options: Value,
) -> anyhow::Result<Value> {
    let input = json!({"kind": "result", "id": result});
    let params = json!({"project": project, "input": input, "options": options});
    let done = job(
        address,
        "jobs/submit",
        json!({"kind": "stain.separate", "params": params}),
    )
    .await?;
    let amounts = made(&done)?;
    let path = format!("/api/v1/data/{project}/results/{amounts}/summary.json");
    get(address, &path).await?.json()
}

/// The angle between two vectors, in degrees.
fn degrees(a: Vector, b: Vector) -> f64 {
    let n = |v: Vector| v.iter().map(|x| x * x).sum::<f64>().sqrt();
    let cos = (a[0] * b[0] + a[1] * b[1] + a[2] * b[2]) / (n(a) * n(b));
    cos.clamp(-1.0, 1.0).acos().to_degrees()
}

fn vector(v: &Value) -> Vector {
    [0, 1, 2].map(|c| v[c].as_f64().unwrap_or(f64::NAN))
}

/// The two slides' stains, separated and scored. With `check`, held to
/// P45's spec.
async fn walk(address: SocketAddr, region: (f64, f64), check: bool) -> anyhow::Result<()> {
    invoke(address, "stage/home", json!({})).await?;
    invoke(address, "camera/start_streaming", json!({})).await?;
    let created = invoke(address, "projects/create", json!({"name": "ex-stain"})).await?;
    let project = created["id"].as_str().context("a project")?.to_owned();
    let at = edge();
    let he = mosaic(address, &project, "H&E slide", at, region).await?;
    let standard = separate(address, &project, &he, json!({"stains": "he"})).await?;
    let estimated = separate(
        address,
        &project,
        &he,
        json!({"stains": "he", "estimate": true}),
    )
    .await?;
    let (h, e) = (
        vector(&estimated["vectors"][0]),
        vector(&estimated["vectors"][1]),
    );
    let angles = (degrees(h, HEMATOXYLIN), degrees(e, EOSIN));
    let white = &standard["white"];
    println!(
        "ex-stain: H&E, a snake over {:.1} × {:.1} mm across the island's edge, stitched; the white {:.3}, {:.3}, {:.3}",
        region.0 / 1e3,
        region.1 / 1e3,
        white[0].as_f64().unwrap_or(0.0),
        white[1].as_f64().unwrap_or(0.0),
        white[2].as_f64().unwrap_or(0.0)
    );
    let means = &standard["means"];
    println!(
        "  H&E's vectors: hematoxylin {:.3}, eosin {:.3} over the tissue; the residual's RMS {:.4}",
        means[0].as_f64().unwrap_or(0.0),
        means[1].as_f64().unwrap_or(0.0),
        standard["residual_rms"].as_f64().unwrap_or(0.0)
    );
    println!(
        "  Macenko's: hematoxylin {:.1}° and eosin {:.1}° from H&E's",
        angles.0, angles.1
    );
    invoke(address, "simulation/load", json!({"name": "H-DAB tissue"})).await?;
    let hdab = mosaic(address, &project, "H-DAB slide", at, region).await?;
    let scored = separate(address, &project, &hdab, json!({"stains": "hdab"})).await?;
    let dab = &scored["dab"];
    let counts = &dab["counts"];
    println!(
        "ex-stain: H-DAB, the same place: of {} tissue pixels, {} negative, {} weak, {} moderate, {} strong",
        scored["tissue"], counts["negative"], counts["weak"], counts["moderate"], counts["strong"]
    );
    let (positive, score) = (
        dab["positive_percent"].as_f64().unwrap_or(0.0),
        dab["h_score"].as_f64().unwrap_or(0.0),
    );
    println!("  DAB positive {positive:.1} % of the tissue, H-score {score:.0}");
    println!("  open http://{address}/ to see the mosaics and their stains");
    if !check {
        return Ok(());
    }
    // H&E's two stains explain the H&E slide, to its noise and its cells'
    // colours, and better than H-DAB's.
    let residual = standard["residual_rms"].as_f64().unwrap_or(f64::NAN);
    let stained = means[0].as_f64().unwrap_or(0.0) + means[1].as_f64().unwrap_or(0.0);
    ensure!(
        residual < 0.1 * stained,
        "the residual {residual} against stains of {stained}: {standard}"
    );
    let wrong = separate(address, &project, &he, json!({"stains": "hdab"})).await?;
    let worse = wrong["residual_rms"].as_f64().unwrap_or(0.0);
    ensure!(
        worse > 2.0 * residual,
        "H-DAB's residual on H&E, {worse}, against {residual}"
    );
    ensure!(
        angles.0 < 7.0 && angles.1 < 7.0,
        "Macenko's vectors {angles:?}° from H&E's: {estimated}"
    );
    // Some of the nuclei are positive, not all.
    ensure!(
        positive > 1.0 && positive < 40.0,
        "DAB positive {positive} %: {scored}"
    );
    ensure!(
        score >= positive && score <= 3.0 * positive,
        "the H-score {score} of {positive} %"
    );
    ensure!(
        standard["dab"].is_null() && !dab.is_null(),
        "DAB scored on H-DAB alone"
    );
    let page = get(address, "/").await?;
    ensure!(
        page.status == 200 && page.body.contains("app.js"),
        "the page: {}",
        page.status
    );
    let script = get(address, "/app.js").await?;
    ensure!(script.status == 200, "the page's script: {}", script.status);
    Ok(())
}
