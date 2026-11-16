//! `ex-count-cells`: P46's cell counting on a stitched slide. A snake
//! across the edge of the simulated H&E slide's first island is scanned at
//! 20× and stitched; its nuclei are counted by the job `cells.count`, on
//! the hematoxylin channel, and matched to the simulator's own. The page
//! draws their outlines over the mosaic. See the README.

use std::net::SocketAddr;
use std::process::ExitCode;
use std::sync::Arc;

use anyhow::{Context, ensure};
use ex_common::{Example, get, invoke, job, made};
use oscope_api::Api;
use oscope_api::devices::{Devices, DevicesConfig};
use oscope_core::{Point2, Rect};
use oscope_jobs::{JobService, JobsConfig};
use oscope_sim::{HeTissue, Slide};
use oscope_store::{Scratch, ScratchPolicy, Store};
use serde_json::json;

/// The scan's region across the island's edge, in µm: smaller for
/// `--smoke`.
const REGION: (f64, f64) = (1_000.0, 750.0);
const SMOKE_REGION: (f64, f64) = (600.0, 450.0);

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("ex-count-cells: {error:#}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> anyhow::Result<()> {
    let example = Example::from_args_with("ex-count-cells", 8232, &["--rate"])?;
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

/// A point between the first island's middle and where its tissue ends,
/// going right: so that the scan holds glass as well as tissue.
fn edge() -> Point2 {
    let tissue = HeTissue::new(1, Slide::COVERSLIP);
    let middle = tissue.islands()[0];
    let mut at = middle;
    while tissue.is_tissue(at) && at.x < middle.x + 20_000.0 {
        at.x += 25.0;
    }
    Point2::new(at.x - 150.0, at.y)
}

/// The nuclei found and the simulator's, matched: the mosaic's offset from
/// where its first frame says, as the median of each found nucleus's to its
/// nearest within 10 µm; then pairs, nearest first, within 3 µm. The
/// offset, and the pairs' distances.
fn matched(found: &[[f64; 2]], truth: &[[f64; 2]]) -> ([f64; 2], Vec<(usize, usize, f64)>) {
    let nearest = |p: [f64; 2]| {
        truth
            .iter()
            .enumerate()
            .map(|(j, t)| (j, (t[0] - p[0]).hypot(t[1] - p[1])))
            .min_by(|a, b| a.1.total_cmp(&b.1))
    };
    let mut offsets: [Vec<f64>; 2] = [Vec::new(), Vec::new()];
    for &f in found {
        if let Some((j, _)) = nearest(f).filter(|n| n.1 <= 10.0) {
            offsets[0].push(truth[j][0] - f[0]);
            offsets[1].push(truth[j][1] - f[1]);
        }
    }
    let median = |v: &mut Vec<f64>| {
        v.sort_by(f64::total_cmp);
        v.get(v.len() / 2).copied().unwrap_or(0.0)
    };
    let shift = [median(&mut offsets[0]), median(&mut offsets[1])];
    let mut pairs: Vec<(usize, usize, f64)> = found
        .iter()
        .enumerate()
        .flat_map(|(i, f)| {
            let f = [f[0] + shift[0], f[1] + shift[1]];
            truth.iter().enumerate().filter_map(move |(j, t)| {
                let d = (f[0] - t[0]).hypot(f[1] - t[1]);
                (d <= 3.0).then_some((i, j, d))
            })
        })
        .collect();
    pairs.sort_by(|a, b| a.2.total_cmp(&b.2));
    let (mut used_f, mut used_t) = (vec![false; found.len()], vec![false; truth.len()]);
    pairs.retain(|&(i, j, _)| {
        let free = !used_f[i] && !used_t[j];
        if free {
            (used_f[i], used_t[j]) = (true, true);
        }
        free
    });
    (shift, pairs)
}

/// The scan, its stitch, its nuclei counted and matched. With `check`,
/// held to P46's spec.
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
        json!({"name": "ex-count-cells"}),
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
    let counted = made(
        &job(
            address,
            "jobs/submit",
            json!({"kind": "cells.count", "params": params}),
        )
        .await?,
    )?;
    let data = format!("/api/v1/data/{project}/results/{counted}");
    let summary = get(address, &format!("{data}/summary.json"))
        .await?
        .json()?;
    let csv = get(address, &format!("{data}/cells.csv")).await?.body;
    // Each nucleus's place on the stage, from the mosaic's top-left corner
    // through the first frame's field, and the simulator's in the region.
    let contents = get(address, &format!("/api/v1/projects/project/{project}"))
        .await?
        .json()?;
    let first = &contents["acquisitions"][0];
    let corner = |k: usize, axis: &str| {
        first["plan"]["field"]["corners_um"][k][axis]
            .as_f64()
            .unwrap_or(0.0)
    };
    let (tl, tr, bl) = (
        [corner(0, "x"), corner(0, "y")],
        [corner(1, "x"), corner(1, "y")],
        [corner(3, "x"), corner(3, "y")],
    );
    let place = &first["frames"][0]["position"];
    let centre = [
        place["x"].as_f64().unwrap_or(0.0),
        place["y"].as_f64().unwrap_or(0.0),
    ];
    let pixel = first["image"]["pixel_size"]["x"]
        .as_f64()
        .context("a pixel size")?;
    let frame = [
        first["image"]["width"].as_f64().unwrap_or(1.0) * pixel,
        first["image"]["height"].as_f64().unwrap_or(1.0) * pixel,
    ];
    let found: Vec<[f64; 2]> = csv
        .split("\r\n")
        .skip(1)
        .filter(|l| !l.is_empty())
        .filter_map(|l| {
            let f: Vec<f64> = l
                .split(',')
                .take(3)
                .filter_map(|v| v.parse().ok())
                .collect();
            let (u, v) = (*f.get(1)? / frame[0], *f.get(2)? / frame[1]);
            Some([0, 1].map(|k| centre[k] + tl[k] + u * (tr[k] - tl[k]) + v * (bl[k] - tl[k])))
        })
        .collect();
    let tissue = HeTissue::new(1, Slide::COVERSLIP);
    let area = Rect::new(at.x - w / 2.0, at.y - h / 2.0, w, h);
    let truth: Vec<[f64; 2]> = tissue
        .nuclei(area)
        .iter()
        .map(|n| [n.centre.x, n.centre.y])
        .collect();
    let (shift, pairs) = matched(&found, &truth);
    // Recall and precision away from the region's edges, where the mosaic
    // and the region don't quite meet.
    let inner = |p: [f64; 2]| {
        (area.origin.x + 30.0..area.origin.x + w - 30.0).contains(&p[0])
            && (area.origin.y + 30.0..area.origin.y + h - 30.0).contains(&p[1])
    };
    let truth_in = truth.iter().filter(|&&t| inner(t)).count() as f64;
    let found_in = found
        .iter()
        .filter(|f| inner([f[0] + shift[0], f[1] + shift[1]]))
        .count() as f64;
    let paired_truth = pairs.iter().filter(|p| inner(truth[p.1])).count() as f64;
    let paired_found = pairs
        .iter()
        .filter(|p| inner([found[p.0][0] + shift[0], found[p.0][1] + shift[1]]))
        .count() as f64;
    let (recall, precision) = (
        paired_truth / truth_in.max(1.0),
        paired_found / found_in.max(1.0),
    );
    let mut misses: Vec<f64> = pairs.iter().map(|p| p.2).collect();
    misses.sort_by(f64::total_cmp);
    let median = misses.get(misses.len() / 2).copied().unwrap_or(f64::NAN);
    println!(
        "ex-count-cells: a snake over {:.2} × {:.2} mm at 20× across the island's edge, stitched; {} nuclei counted, {:.0} per mm² of tissue",
        w / 1e3,
        h / 1e3,
        summary["count"],
        summary["per_mm2"].as_f64().unwrap_or(0.0)
    );
    println!(
        "  against the simulator's {} nuclei, the mosaic {:.1}, {:.1} µm from where its first frame says: recall {recall:.3}, precision {precision:.3}, the median miss {median:.2} µm",
        truth.len(),
        shift[0],
        shift[1]
    );
    println!("  open http://{address}/ to see their outlines");
    if !check {
        return Ok(());
    }
    ensure!(
        summary["count"].as_u64().unwrap_or(0) > 50,
        "too few nuclei: {summary}"
    );
    ensure!(
        recall >= 0.85 && precision >= 0.85,
        "recall {recall}, precision {precision}"
    );
    ensure!(
        shift[0].abs() < 10.0 && shift[1].abs() < 10.0,
        "an offset of {shift:?}"
    );
    let layer = get(
        address,
        &format!("/api/v1/projects/annotations/{project}/results/{mosaic}"),
    )
    .await?
    .json()?;
    let outlines = layer["layers"]["cells"]["features"]
        .as_array()
        .map_or(0, Vec::len);
    ensure!(
        Some(outlines as u64) == summary["count"].as_u64(),
        "{outlines} outlines"
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
