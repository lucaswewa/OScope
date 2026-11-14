//! `ex-rust-ext`: the sample Rust extension, `ext-rust-node`, as OScope
//! sees it (P25, ADR-0019): packed for this platform, installed, loaded at
//! a start with its ABI checked, and run in a recipe on a field of the
//! simulated slide, whole and tile by tile, against the sample's own code.
//! See the README.

use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Instant;

use anyhow::{Context, bail, ensure};
use oscope_compute::{Compute, ComputeConfig, Image};
use oscope_core::{Point2, Um};
use oscope_ext::Extensions;
use oscope_hal::ObjectiveCatalogue;
use oscope_imaging::color::encode_png;
use oscope_pipeline::{Engine, Plan, Recipe, RunOptions, Value, Values, stock};
use oscope_sim::{Field, HeTissue, Light, Optics, Slide, World};
use serde_json::json;

/// Where the slide's corner sits on the stage, the stage z at which its
/// surface is in focus, and the height of the section's middle above it.
const SLIDE_AT: Point2 = Point2::new(10_000.0, 20_000.0);
const FOCUS: Um = Um(5_000.0);
const SECTION: f64 = 2.0;

/// The relief's strength in the recipe.
const STRENGTH: f32 = 3.0;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("ex-rust-ext: {error:#}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> anyhow::Result<()> {
    let smoke = match std::env::args().nth(1).as_deref() {
        None => false,
        Some("--smoke") => true,
        Some(other) => bail!("`{other}`: the only option is --smoke"),
    };
    let temporary = smoke.then(tempfile::tempdir).transpose()?;
    let data = match &temporary {
        Some(folder) => folder.path().to_owned(),
        None => PathBuf::from(".oscope").join("ex-rust-ext"),
    };
    fs::create_dir_all(&data)?;

    // The library, as Cargo built it; packed for this platform.
    let library = ex_rust_ext::library()?;
    let size = fs::metadata(&library)?.len() / 1024;
    println!("the sample's library: {} ({size} KiB)", library.display());
    let package = data.join("org.oscope.examples.emboss-0.1.0.omext");
    let manifest = ex_rust_ext::pack(&library, &data, &package)?;
    let sha256 = oscope_ext::sha256(&package)?;
    println!(
        "packed {} {} into {}",
        manifest.id,
        manifest.version,
        package.display()
    );
    println!("  SHA-256 {sha256}");

    // Installed for the next start, which loads it.
    let root = data.join("extensions");
    let (installed, unpacked) = Extensions::open(&root)?.install(&package)?;
    let how = if unpacked {
        "installed"
    } else {
        "installed already"
    };
    println!("{how}: the next start loads {}", installed.manifest.version);
    let started = Instant::now();
    let extensions = Extensions::open(&root)?;
    let took = started.elapsed().as_secs_f64() * 1e3;
    for loaded in extensions.loaded() {
        let (id, version) = (&loaded.manifest.id, &loaded.manifest.version);
        println!("the next start loaded {id} {version}, its ABI checked, in {took:.1} ms");
    }
    ensure!(
        extensions.nodes().len() == 1,
        "the start loaded {} nodes",
        extensions.nodes().len()
    );
    for node in extensions.nodes() {
        let (descriptor, can) = (node.descriptor(), node.descriptor().capabilities);
        println!("  {}: {}", descriptor.id, descriptor.description);
        println!(
            "    live: {}, tile by tile with a halo of {} px",
            can.live, can.halo
        );
    }

    // A recipe with the stock nodes and the extension's.
    let mut registry = stock::registry(data.join("references"));
    for node in extensions.nodes() {
        registry
            .register(Arc::clone(node))
            .map_err(anyhow::Error::msg)?;
    }
    let engine = Engine::new(
        registry,
        Arc::new(Compute::new(&ComputeConfig::default())?),
        0,
    );
    let recipe: Recipe = serde_json::from_value(json!({
        "format": 1,
        "inputs": [{ "name": "image", "type": "image" }],
        "nodes": [{ "id": "emboss", "node": "example.emboss", "params": { "strength": STRENGTH } }],
        "edges": [{ "from": "input.image", "to": "emboss.image" }],
        "outputs": [{ "name": "image", "from": "emboss.image" }],
    }))?;
    let plan = Plan::new(engine.registry(), &recipe, false)?;
    let ((width, height), tile) = if smoke {
        ((256, 192), 64)
    } else {
        ((1024, 768), 256)
    };
    let field = field(width, height)?;
    println!("a field of the simulated H&E slide, {width} × {height}, through the 5× objective");
    let inputs = Values::from([("image".to_owned(), Value::image(field.clone()))]);
    let options = RunOptions {
        memoise: false,
        ..RunOptions::default()
    };
    let whole = engine.run(&plan, &inputs, &options)?;
    let ms = |run: &oscope_pipeline::Run| run.report.time.as_secs_f64() * 1e3;
    println!("run whole: {:.1} ms", ms(&whole));
    let tiled = engine.run_tiled(&plan, &inputs, tile, &options, |_, _| {})?;
    println!("run tile by tile, {tile} px square: {:.1} ms", ms(&tiled));

    // The same as the sample's own code, run directly.
    let view = oscope_ext_sdk::Image::new(
        field.width(),
        field.height(),
        field.channels(),
        field.data(),
    );
    let (view, mut direct) = (
        view.map_err(anyhow::Error::msg)?,
        vec![0.0; field.data().len()],
    );
    let started = Instant::now();
    ext_rust_node::emboss(&view, STRENGTH, &mut direct);
    let took = started.elapsed().as_secs_f64() * 1e3;
    println!("the sample's own code, run directly: {took:.1} ms");
    for (how, run) in [("whole", &whole), ("tile by tile", &tiled)] {
        let image = run.outputs["image"].as_image().context("an image")?;
        ensure!(
            image.data() == &direct[..],
            "{how}, the node gave other samples than its code"
        );
    }
    println!("both the same as the sample's own code run directly, to the bit");

    let out = data.join("out");
    fs::create_dir_all(&out)?;
    let embossed = whole.outputs["image"].as_image().context("an image")?;
    for (name, image) in [("field.png", &field), ("embossed.png", embossed)] {
        let mut file = Vec::new();
        encode_png(&mut file, &rgb8(image), image.width(), image.height())?;
        fs::write(out.join(name), file)?;
    }
    println!("wrote field.png and embossed.png in {}", out.display());
    if smoke {
        println!("smoke: ok");
    }
    Ok(())
}

/// A field of the simulated H&E slide through the 5× objective, over the
/// first island of tissue.
fn field(width: u32, height: u32) -> anyhow::Result<Image> {
    let tissue = HeTissue::new(1, Slide::COVERSLIP);
    let island = tissue.islands()[0];
    let world = World::with_slide(Slide::new(Arc::new(tissue), SLIDE_AT), FOCUS);
    let catalogue = ObjectiveCatalogue::mitutoyo();
    let objective = catalogue.objectives.iter().find(|o| o.name == "5x");
    let optics = Optics::new(&catalogue, objective.context("the 5× objective")?, Um(2.4));
    let field = Field {
        centre: Point2::new(SLIDE_AT.x + island.x, SLIDE_AT.y + island.y),
        z: Um(FOCUS.0 + SECTION),
        width,
        height,
    };
    Ok(world.render(&field, &optics, Light::Transmitted))
}

/// The image as 8-bit RGB, from 0 to 1.
fn rgb8(image: &Image) -> Vec<u8> {
    let channels = image.channels();
    let pixels = image.data().chunks(channels);
    let byte = |sample: f32| (sample.clamp(0.0, 1.0) * 255.0).round() as u8;
    pixels
        .flat_map(|pixel| [0, 1, 2].map(|c| byte(pixel[c.min(channels - 1)])))
        .collect()
}
