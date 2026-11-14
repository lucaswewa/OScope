//! `ex-filters`: P23's stock nodes on a field of the simulated H&E slide,
//! compared on a page: the Gaussian, median and bilateral filters, the
//! unsharp mask, CLAHE, the thresholds, morphology, the distance transform
//! and labelling, each with its time on the CPU and, with `cuda`, the GPU;
//! and the nuclei found by a recipe of them. See the README.

use std::net::SocketAddr;
use std::path::Path;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, ensure};
use ex_common::{Example, get};
use oscope_api::Api;
use oscope_compute::{Compute, ComputeConfig, Image};
use oscope_core::{Point2, Um};
use oscope_hal::ObjectiveCatalogue;
use oscope_imaging::color::{Display, encode_png, to_display};
use oscope_pipeline::{Engine, Plan, Recipe, Run, RunOptions, Value, Values, stock};
use oscope_sim::{Field, HeTissue, Light, Optics, Slide, World};
use serde_json::{Value as Json, json};

/// Where the slide's corner sits on the stage, and where its surface is in
/// focus; the section's middle above it.
const SLIDE_AT: Point2 = Point2::new(10_000.0, 20_000.0);
const FOCUS: Um = Um(5_000.0);
const SECTION: f64 = 2.0;

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("ex-filters: {error:#}");
            ExitCode::FAILURE
        }
    }
}

/// A recipe of `nodes` in a chain from the image, each node's first output
/// to the next's input, giving the last's `output`.
fn chain(nodes: &[(&str, Json, &str, &str)], output: &str) -> anyhow::Result<Recipe> {
    let mut from = "input.image".to_owned();
    let (mut steps, mut edges) = (Vec::new(), Vec::new());
    for (k, (node, params, takes, gives)) in nodes.iter().enumerate() {
        let id = format!("s{k}");
        edges.push(json!({ "from": from, "to": format!("{id}.{takes}") }));
        steps.push(json!({ "id": id, "node": node, "params": params }));
        from = format!("{id}.{gives}");
    }
    let mut outputs = vec![json!({ "name": "out", "from": from })];
    if !output.is_empty() {
        outputs.push(json!({ "name": "extra", "from": format!("s{}.{output}", nodes.len() - 1) }));
    }
    let recipe = json!({
        "format": 1,
        "inputs": [{ "name": "image", "type": "image" }],
        "nodes": steps,
        "edges": edges,
        "outputs": outputs,
    });
    Ok(serde_json::from_value(recipe)?)
}

/// Runs `recipe` on `image`, on the CPU, or on the GPU with a second GPU
/// step after it to keep it there: the run, and its time.
fn timed(
    engine: &Engine,
    recipe: &Recipe,
    image: &Image,
    gpu: bool,
) -> anyhow::Result<(Run, Duration)> {
    let mut recipe = recipe.clone();
    if gpu {
        let last = recipe.outputs[0].from.clone();
        recipe.nodes.push(serde_json::from_value(
            json!({ "id": "keep", "node": "resize.bin", "params": { "factor": 1 } }),
        )?);
        recipe.edges.push(serde_json::from_value(
            json!({ "from": last, "to": "keep.image" }),
        )?);
        recipe.outputs[0].from = "keep.image".into();
    }
    let plan = Plan::new(engine.registry(), &recipe, gpu)?;
    let options = RunOptions {
        memoise: false,
        ..RunOptions::default()
    };
    let inputs = Values::from([("image".to_owned(), Value::image(image.clone()))]);
    engine.run(&plan, &inputs, &options)?;
    let run = engine.run(&plan, &inputs, &options)?;
    let time = run.report.time;
    Ok((run, time))
}

/// An image as a PNG: display values through sRGB's curve for linear light,
/// or stretched from its least to its greatest value, or labels each in a
/// colour of its own; a mask black and white.
fn png(value: &Value, how: &str) -> anyhow::Result<Vec<u8>> {
    let (width, height, rgb) = if let Some(mask) = value.as_mask() {
        let rgb = mask.data().iter().flat_map(|&bit| [bit * 255; 3]).collect();
        (mask.width(), mask.height(), rgb)
    } else {
        let image = value.as_image().context("an image")?;
        let grey = image.channels() == 1;
        let rgb: Vec<u8> = match how {
            "linear" if !grey => to_display(image, &Display::default())?,
            "labels" => image
                .data()
                .iter()
                .flat_map(|&label| match label as u32 {
                    0 => [0, 0, 0],
                    n => [
                        (n * 97 % 200 + 55) as u8,
                        (n * 57 % 200 + 55) as u8,
                        (n * 151 % 200 + 55) as u8,
                    ],
                })
                .collect(),
            _ => {
                let (least, greatest) = image
                    .data()
                    .iter()
                    .fold((f32::MAX, f32::MIN), |(l, g), &v| (l.min(v), g.max(v)));
                let scale = 255.0 / (greatest - least).max(1e-6);
                let stretched = image.data().iter().map(|&v| ((v - least) * scale) as u8);
                match grey {
                    true => stretched.flat_map(|v| [v; 3]).collect(),
                    false => stretched.collect(),
                }
            }
        };
        (image.width(), image.height(), rgb)
    };
    let mut file = Vec::new();
    encode_png(&mut file, &rgb, width, height)?;
    Ok(file)
}

async fn run() -> anyhow::Result<()> {
    let example = Example::from_args("ex-filters", 8214)?;
    let (width, height) = if example.smoke {
        (256, 192)
    } else {
        (1024, 768)
    };
    let page = example.roots.data.join("page");
    std::fs::create_dir_all(&page)?;
    for file in ["index.html", "app.js"] {
        std::fs::copy(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("page")
                .join(file),
            page.join(file),
        )?;
    }

    // A field of the H&E slide through the 10× objective, on a nucleus-rich
    // island; and a copy with 2% of its pixels flipped to black or white,
    // for the median.
    let tissue = HeTissue::new(1, Slide::COVERSLIP);
    let island = tissue.islands()[0];
    let world = World::with_slide(Slide::new(Arc::new(tissue), SLIDE_AT), FOCUS);
    let catalogue = ObjectiveCatalogue::mitutoyo();
    let objective = catalogue
        .objectives
        .iter()
        .find(|o| o.name == "10x")
        .context("the 10× objective")?;
    let optics = Optics::new(&catalogue, objective, Um(2.4));
    let field = Field {
        centre: Point2::new(SLIDE_AT.x + island.x, SLIDE_AT.y + island.y),
        z: Um(FOCUS.0 + SECTION),
        width,
        height,
    };
    let image = world.render(&field, &optics, Light::Transmitted);
    let mut noisy = image.clone();
    for (i, pixel) in noisy.data_mut().chunks_mut(3).enumerate() {
        match (i * 2_654_435_761) % 100 {
            0 => pixel.fill(0.0),
            1 => pixel.fill(1.0),
            _ => {}
        }
    }
    std::fs::write(
        page.join("field.png"),
        png(&Value::image(image.clone()), "linear")?,
    )?;
    std::fs::write(
        page.join("noisy.png"),
        png(&Value::image(noisy.clone()), "linear")?,
    )?;

    let compute = Arc::new(Compute::new(&ComputeConfig::default())?);
    let engine = Engine::new(
        stock::registry(example.roots.data.join("references")),
        compute,
        0,
    );
    let gpu = engine.has_gpu();
    let grey = (
        "convert.format",
        json!({ "channels": "grey" }),
        "image",
        "image",
    );
    let invert = (
        "display.lut",
        json!({ "table": [1.0, 0.0] }),
        "image",
        "image",
    );
    let otsu = ("threshold.otsu", json!({}), "image", "mask");
    let open = (
        "morphology.open",
        json!({ "radius": 2, "shape": "disk" }),
        "mask",
        "mask",
    );
    // Each card: its name, its recipe, the input it takes, how its output
    // is shown, and whether it has a CUDA twin to time.
    let cards: Vec<(&str, Recipe, &Image, &str, bool)> = vec![
        (
            "Gaussian, σ 2 px",
            chain(
                &[("filter.gaussian", json!({ "sigma": 2.0 }), "image", "image")],
                "",
            )?,
            &image,
            "linear",
            true,
        ),
        (
            "Median, radius 2, on the noisy copy",
            chain(
                &[("filter.median", json!({ "radius": 2 }), "image", "image")],
                "",
            )?,
            &noisy,
            "linear",
            true,
        ),
        (
            "Bilateral, σ 3 px and 0.08",
            chain(
                &[(
                    "filter.bilateral",
                    json!({ "sigma_space": 3.0, "sigma_range": 0.08 }),
                    "image",
                    "image",
                )],
                "",
            )?,
            &image,
            "linear",
            true,
        ),
        (
            "Unsharp mask, σ 2 px, amount 1.5",
            chain(
                &[(
                    "enhance.unsharp",
                    json!({ "sigma": 2.0, "amount": 1.5 }),
                    "image",
                    "image",
                )],
                "",
            )?,
            &image,
            "linear",
            false,
        ),
        (
            "CLAHE, 8 × 8 tiles, clip 2.5",
            chain(
                &[("enhance.clahe", json!({ "clip": 2.5 }), "image", "image")],
                "",
            )?,
            &image,
            "linear",
            true,
        ),
        (
            "Threshold at 0.55",
            chain(
                &[
                    grey.clone(),
                    (
                        "threshold.manual",
                        json!({ "level": 0.55 }),
                        "image",
                        "mask",
                    ),
                ],
                "",
            )?,
            &image,
            "mask",
            false,
        ),
        (
            "Otsu's threshold, of the inverted grey",
            chain(&[grey.clone(), invert.clone(), otsu.clone()], "level")?,
            &image,
            "mask",
            false,
        ),
        (
            "Opened, a disk of 2",
            chain(
                &[grey.clone(), invert.clone(), otsu.clone(), open.clone()],
                "",
            )?,
            &image,
            "mask",
            false,
        ),
        (
            "The distance transform",
            chain(
                &[
                    grey.clone(),
                    invert.clone(),
                    otsu.clone(),
                    open.clone(),
                    ("morphology.distance", json!({}), "mask", "image"),
                ],
                "",
            )?,
            &image,
            "stretch",
            false,
        ),
        (
            "Nuclei, labelled",
            chain(
                &[
                    grey,
                    invert,
                    otsu,
                    open,
                    ("morphology.label", json!({}), "mask", "labels"),
                ],
                "components",
            )?,
            &image,
            "labels",
            false,
        ),
    ];
    println!(
        "a field of {width} × {height} through the 10× objective; on the CPU{}:",
        if gpu { " and the GPU" } else { "" }
    );
    let mut results = Vec::new();
    for (k, (name, recipe, input, how, twin)) in cards.iter().enumerate() {
        let (run, cpu) = timed(&engine, recipe, input, false)?;
        let gpu_time = match gpu && *twin {
            true => Some(timed(&engine, recipe, input, true)?.1),
            false => None,
        };
        let file = format!("card-{k}.png");
        std::fs::write(page.join(&file), png(&run.outputs["out"], how)?)?;
        let extra = run
            .outputs
            .get("extra")
            .map(|value| match value.as_table() {
                Some(table) => format!("{} found", table.rows.len()),
                None => format!(
                    "level {}",
                    value.as_json().and_then(Json::as_f64).unwrap_or_default()
                ),
            });
        let ms = |time: Duration| time.as_secs_f64() * 1e3;
        let gpu_ms = gpu_time.map(ms);
        let shown_gpu = gpu_ms
            .map(|g| format!(", GPU {g:.1} ms with its copies"))
            .unwrap_or_default();
        println!(
            "  {name:<44} CPU {:>7.1} ms{shown_gpu}{}",
            ms(cpu),
            extra.as_ref().map(|e| format!("; {e}")).unwrap_or_default()
        );
        results.push(json!({ "name": name, "file": file, "cpu_ms": ms(cpu), "gpu_ms": gpu_ms, "input": if std::ptr::eq(*input, &noisy) { "noisy.png" } else { "field.png" }, "note": extra }));
    }
    let summary =
        json!({ "field": [width, height], "objective": "10x", "gpu": gpu, "cards": results });
    std::fs::write(
        page.join("filters.json"),
        serde_json::to_vec_pretty(&summary)?,
    )?;
    let api = Api::builder(&example.roots);
    example.serve(api, &page, smoke).await
}

/// `--smoke`: the page, its images and its results are served, a card for
/// each, and the nuclei were found.
async fn smoke(address: SocketAddr) -> anyhow::Result<()> {
    for (path, kind) in [
        ("/", "text/html"),
        ("/app.js", "text/javascript"),
        ("/filters.json", "application/json"),
        ("/field.png", "image/png"),
        ("/card-9.png", "image/png"),
    ] {
        let file = get(address, path).await?;
        ensure!(
            file.status == 200 && file.content_type.starts_with(kind),
            "{path}"
        );
    }
    let summary = get(address, "/filters.json").await?.json()?;
    let cards = summary["cards"].as_array().context("the cards")?;
    ensure!(cards.len() == 10, "{} cards", cards.len());
    let nuclei = cards[9]["note"].as_str().unwrap_or_default();
    let found: u32 = nuclei
        .split(' ')
        .next()
        .unwrap_or_default()
        .parse()
        .unwrap_or_default();
    ensure!(found > 0, "no nuclei found: {nuclei}");
    println!("served: the page, the field and ten cards; {nuclei}");
    Ok(())
}
