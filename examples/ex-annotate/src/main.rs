//! `ex-annotate`: P44's annotations and measurements on a stitched slide. A
//! snake over the simulated H&E slide is scanned and stitched, and shapes
//! drawn on the mosaic in µm are kept in layers and measured: lengths,
//! areas, and their pixels by the colours of the camera's filters. The
//! page draws them in OpenSeadragon. See the README.

use std::net::SocketAddr;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, ensure};
use ex_common::{Example, get, invoke, job, made};
use oscope_api::Api;
use oscope_api::devices::{Devices, DevicesConfig};
use oscope_jobs::{JobService, JobsConfig};
use oscope_sim::{HeTissue, Slide};
use oscope_store::{Scratch, ScratchPolicy, Store};
use serde_json::{Value, json};

/// The scan's region about the first island, in µm: smaller for `--smoke`.
const REGION: (f64, f64) = (2_000.0, 1_500.0);
const SMOKE_REGION: (f64, f64) = (1_200.0, 900.0);

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("ex-annotate: {error:#}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> anyhow::Result<()> {
    let example = Example::from_args_with("ex-annotate", 8230, &["--rate"])?;
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

/// A rectangle in µm, as a GeoJSON polygon named `name`.
fn rect(name: &str, (x, y): (f64, f64), (w, h): (f64, f64)) -> Value {
    let ring = json!([[x, y], [x + w, y], [x + w, y + h], [x, y + h], [x, y]]);
    let geometry = json!({"type": "Polygon", "coordinates": [ring]});
    json!({"type": "Feature", "geometry": geometry, "properties": {"name": name}})
}

/// Scans and stitches a mosaic: its project and result.
async fn mosaic(address: SocketAddr, (w, h): (f64, f64)) -> anyhow::Result<(String, String)> {
    invoke(address, "stage/home", json!({})).await?;
    let island = HeTissue::new(1, Slide::COVERSLIP).islands()[0];
    let at = json!({"x": island.x, "y": island.y});
    let z = invoke(address, "simulation/in_focus", at).await?;
    let to = json!({"x": island.x, "y": island.y, "z": z});
    invoke(address, "stage/move_to", to).await?;
    invoke(address, "camera/start_streaming", json!({})).await?;
    job(address, "exposure/auto", json!({})).await?;
    let created = invoke(address, "projects/create", json!({"name": "ex-annotate"})).await?;
    let project = created["id"].as_str().context("a project")?.to_owned();
    let origin = json!({"x": island.x - w / 2.0, "y": island.y - h / 2.0});
    let rect = json!({"rect": {"origin": origin, "size": {"width": w, "height": h}}});
    let request = json!({"pattern": "snake", "region": rect});
    let scan = json!({"request": request, "project": project, "name": "a slide"});
    let acquisition = made(&job(address, "scan/run", scan).await?)?;
    let params = json!({"project": project, "acquisition": acquisition});
    let stitch = json!({"kind": "stitch.run", "params": params});
    let result = made(&job(address, "jobs/submit", stitch).await?)?;
    Ok((project, result))
}

/// The scan, its stitch, and annotations measured on it. With `check`,
/// held to P44's spec.
async fn walk(address: SocketAddr, region: (f64, f64), check: bool) -> anyhow::Result<()> {
    let (project, result) = mosaic(address, region).await?;
    let contents = get(address, &format!("/api/v1/projects/project/{project}")).await?;
    let image = contents.json()?["results"][0]["image"].clone();
    let (px, py) = (
        image["pixel_size"]["x"].as_f64(),
        image["pixel_size"]["y"].as_f64(),
    );
    let (px, py) = (px.context("a pixel size")?, py.context("a pixel size")?);
    let (width, height) = (
        image["width"].as_f64().unwrap_or(0.0),
        image["height"].as_f64().unwrap_or(0.0),
    );
    // Two squares of 300 µm side by side about the mosaic's middle, and the
    // rectangle of both; a line; a point.
    let (cx, cy) = (width * px / 2.0, height * py / 2.0);
    let shapes = [
        (
            "regions",
            rect("left", (cx - 300.0, cy - 150.0), (300.0, 300.0)),
        ),
        ("regions", rect("right", (cx, cy - 150.0), (300.0, 300.0))),
        (
            "regions",
            rect("both", (cx - 300.0, cy - 150.0), (600.0, 300.0)),
        ),
        (
            "lines",
            json!({"type": "Feature", "geometry": {"type": "LineString", "coordinates": [[cx, cy], [cx + 300.0, cy + 400.0]]}, "properties": {"name": "3-4-5"}}),
        ),
        (
            "points",
            json!({"type": "Feature", "geometry": {"type": "Point", "coordinates": [cx, cy]}, "properties": {}}),
        ),
    ];
    for (layer, feature) in shapes {
        let input = json!({"project": project, "kind": "result", "record": result, "layer": layer, "feature": feature});
        invoke(address, "projects/annotate", input).await?;
    }
    let base = format!("/api/v1/projects/measurements/{project}/results/{result}");
    let started = Instant::now();
    let measured = get(address, &base).await?.json()?;
    let took = started.elapsed();
    let find = |name: &str| {
        measured
            .as_array()
            .into_iter()
            .flatten()
            .find(|m| m["name"] == name)
            .cloned()
    };
    let both = find("both").context("the rectangle's measurement")?;
    let means: Vec<String> = both["channels"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|c| {
            format!(
                "{} {:.0}",
                c["name"].as_str().unwrap_or("?"),
                c["mean"].as_f64().unwrap_or(0.0)
            )
        })
        .collect();
    println!(
        "ex-annotate: a mosaic of {width} × {height} pixels of {px:.3} × {py:.3} µm, with 5 annotations in 3 layers"
    );
    println!(
        "  the rectangle of 600 × 300 µm: {} µm², {} µm round, its pixels' means {}, measured in {} ms",
        both["area"],
        both["length"],
        means.join(", "),
        took.as_millis()
    );
    println!("  open http://{address}/ to draw polygons, lines and points, and measure them");
    if check {
        held(address, &measured, (&project, &result), (px, py)).await?;
    }
    Ok(())
}

/// Holds the measurements to P44's spec: areas and lengths as drawn; the
/// two squares' pixels, sharing an edge, the rectangle's; the CSV; one
/// removed; the page.
async fn held(
    address: SocketAddr,
    measured: &Value,
    (project, result): (&str, &str),
    (px, py): (f64, f64),
) -> anyhow::Result<()> {
    let base = format!("/api/v1/projects/measurements/{project}/results/{result}");
    let find = |name: &str| {
        let found = measured
            .as_array()
            .into_iter()
            .flatten()
            .find(|m| m["name"] == name);
        found
            .cloned()
            .with_context(|| format!("{name}'s measurement"))
    };
    let (left, right, both, line) = (find("left")?, find("right")?, find("both")?, find("3-4-5")?);
    // As drawn, to a float's rounding.
    let near = |m: &Value, k: &str, want: f64| {
        (m[k].as_f64().unwrap_or(f64::NAN) - want).abs() < 1e-9 * want
    };
    ensure!(
        near(&both, "area", 180_000.0) && near(&both, "length", 1_800.0),
        "the rectangle: {both}"
    );
    ensure!(
        near(&left, "area", 90_000.0) && near(&line, "length", 500.0),
        "a square and the line: {left} {line}"
    );
    let names: Vec<&str> = both["channels"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|c| c["name"].as_str())
        .collect();
    ensure!(
        names == ["red", "green", "blue"],
        "a raw mosaic's colours: {names:?}"
    );
    let mut pixels = 0.0;
    for c in 0..3 {
        let stat = |m: &Value, k: &str| m["channels"][c][k].as_f64().unwrap_or(f64::NAN);
        let (nl, nr, nb) = (
            stat(&left, "pixels"),
            stat(&right, "pixels"),
            stat(&both, "pixels"),
        );
        ensure!(
            nl + nr == nb,
            "the squares' pixels, {nl} + {nr}, aren't the rectangle's {nb}"
        );
        let mean = (nl * stat(&left, "mean") + nr * stat(&right, "mean")) / nb;
        ensure!(
            (mean - stat(&both, "mean")).abs() < 1e-9 * mean,
            "the squares' mean {mean}: {both}"
        );
        pixels += nb;
    }
    let expected = 180_000.0 / (px * py);
    ensure!(
        (pixels - expected).abs() < 0.01 * expected,
        "{pixels} pixels in 180,000 µm², of {expected}"
    );
    let csv = get(address, &format!("{base}?format=csv")).await?;
    let rows = csv.body.split("\r\n").filter(|l| !l.is_empty()).count();
    ensure!(
        csv.content_type.starts_with("text/csv") && rows == 6,
        "the CSV: {}",
        csv.body
    );
    // The point removed: four left.
    let point = find_id(measured, "points")?;
    let input = json!({"project": project, "kind": "result", "record": result, "layer": "points", "feature": point});
    invoke(address, "projects/remove_annotation", input).await?;
    let left_now = get(address, &base).await?.json()?;
    ensure!(
        left_now.as_array().map(Vec::len) == Some(4),
        "after a removal: {left_now}"
    );
    let page = get(address, "/").await?;
    ensure!(
        page.status == 200 && page.body.contains("app.js"),
        "the page: {}",
        page.status
    );
    let script = get(address, "/app.js").await?;
    ensure!(script.status == 200, "the page's script: {}", script.status);
    let viewer = get(address, "/vendor/openseadragon-6.1.1/openseadragon.min.js").await?;
    ensure!(viewer.status == 200, "OpenSeadragon: {}", viewer.status);
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        // The deep zoom the page reads, warmed as the stitch ended.
        let status = get(
            address,
            &format!("/api/v1/zoom/{project}/results/{result}/status"),
        )
        .await?
        .json()?;
        if status["coarse"] == true {
            return Ok(());
        }
        ensure!(Instant::now() < deadline, "deep zoom didn't warm: {status}");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// The ID of the first annotation of `layer` in `measured`.
fn find_id(measured: &Value, layer: &str) -> anyhow::Result<String> {
    let found = measured
        .as_array()
        .into_iter()
        .flatten()
        .find(|m| m["layer"] == layer);
    let id = found.and_then(|m| m["id"].as_str());
    id.map(str::to_owned)
        .with_context(|| format!("an annotation in {layer}"))
}
