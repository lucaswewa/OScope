//! `ex-python-node`: the sample Python extension, `ext-python-node`, as
//! OScope runs it (P26, ADR-0020): installed, with its environment made by
//! uv; loaded at a start; and run in a recipe on a noisy field of the
//! simulated slide, its worker started on the first run and kept warm, with
//! each run's time. See the README.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Instant;

use anyhow::{Context, bail, ensure};
use oscope_compute::{Compute, ComputeConfig, Image};
use oscope_core::{Point2, Um};
use oscope_ext::{Extensions, PythonRuntime};
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

/// The noise added to the field: its standard deviation.
const NOISE: f32 = 0.08;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("ex-python-node: {error:#}");
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
        None => PathBuf::from(".oscope").join("ex-python-node"),
    };
    fs::create_dir_all(&data)?;

    // Packed, then installed: uv makes its environment.
    let sample = Path::new(env!("CARGO_MANIFEST_DIR")).join("../ext-python-node");
    let package = data.join("org.oscope.examples.tv-denoise-0.1.0.omext");
    let manifest = oscope_ext::pack(&sample, &package)?;
    println!(
        "packed {} {} into {}",
        manifest.id,
        manifest.version,
        package.display()
    );
    let uv = match oscope_ext::find_uv(None) {
        Ok(uv) => uv,
        // As CI on Windows, where uv isn't installed (the owner's decision).
        Err(error) if smoke => {
            println!("smoke: skipped: {error}");
            return Ok(());
        }
        Err(error) => return Err(error.into()),
    };
    println!("uv: {}", uv.display());
    let python = PythonRuntime::default();
    let root = data.join("extensions");
    let started = Instant::now();
    let (installed, unpacked) =
        Extensions::open_with(&root, Some(python.clone()))?.install(&package)?;
    let took = started.elapsed().as_secs_f64();
    match unpacked {
        true => println!(
            "installed in {took:.1} s, with its environment: Python, scikit-image and OScope's SDK"
        ),
        false => println!(
            "installed already: the next start loads {}",
            installed.manifest.version
        ),
    }

    // The next start loads it; its worker starts on the first run.
    let extensions = Extensions::open_with(&root, Some(python))?;
    ensure!(
        extensions.nodes().len() == 1,
        "the start loaded {} nodes",
        extensions.nodes().len()
    );
    for node in extensions.nodes() {
        let descriptor = node.descriptor();
        println!("  {}: {}", descriptor.id, descriptor.description);
        println!("    parameters: {}", descriptor.params["properties"]);
    }
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
        "nodes": [{ "id": "denoise", "node": "example.denoise_tv", "params": { "weight": 0.1 } }],
        "edges": [{ "from": "input.image", "to": "denoise.image" }],
        "outputs": [{ "name": "image", "from": "denoise.image" }],
    }))?;
    let plan = Plan::new(engine.registry(), &recipe, false)?;

    let (width, height) = if smoke { (128, 96) } else { (768, 576) };
    let clean = field(width, height)?;
    let noisy = noisy(&clean);
    println!("a field of the simulated H&E slide, {width} × {height}, with noise of σ {NOISE}");
    let inputs = Values::from([("image".to_owned(), Value::image(noisy.clone()))]);
    let options = RunOptions {
        memoise: false,
        ..RunOptions::default()
    };
    let mut denoised = None;
    for (n, what) in [(1, "starting its worker"), (2, "warm"), (3, "warm")] {
        let run = engine.run(&plan, &inputs, &options)?;
        let ms = run.report.time.as_secs_f64() * 1e3;
        let pid = extensions.workers().first().and_then(|worker| worker.pid);
        println!(
            "run {n}, {what}: {ms:.0} ms, in worker {}",
            pid.map_or("?".to_owned(), |p| p.to_string())
        );
        denoised = Some(run.outputs["image"].as_image().context("an image")?.clone());
    }
    let denoised = denoised.context("a run")?;

    // What a call costs: an 8 × 8 image, one iteration, 20 times.
    let mut recipe = recipe;
    recipe.nodes[0].params = json!({ "iterations": 1 });
    let plan = Plan::new(engine.registry(), &recipe, false)?;
    let tiny = Values::from([("image".to_owned(), Value::image(field(8, 8)?))]);
    let started = Instant::now();
    for _ in 0..20 {
        engine.run(&plan, &tiny, &options)?;
    }
    let each = started.elapsed().as_secs_f64() * 1e3 / 20.0;
    println!("a call on an 8 × 8 image, one iteration: {each:.2} ms, in all");
    let (before, after) = (psnr(&noisy, &clean), psnr(&denoised, &clean));
    println!("against the clean field: {before:.1} dB noisy, {after:.1} dB denoised");
    ensure!(
        after > before + 3.0,
        "denoising should gain more than 3 dB: {before:.1} to {after:.1}"
    );

    let out = data.join("out");
    fs::create_dir_all(&out)?;
    for (name, image) in [("noisy.png", &noisy), ("denoised.png", &denoised)] {
        let mut file = Vec::new();
        encode_png(&mut file, &rgb8(image), image.width(), image.height())?;
        fs::write(out.join(name), file)?;
    }
    println!("wrote noisy.png and denoised.png in {}", out.display());
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

/// `image` with Gaussian noise of σ [`NOISE`] added, the same each time:
/// sums of 12 uniform numbers from a xorshift generator.
fn noisy(image: &Image) -> Image {
    let mut state: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut uniform = || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        (state >> 40) as f32 / (1u64 << 24) as f32
    };
    let mut noisy = image.clone();
    for sample in noisy.data_mut() {
        let gaussian: f32 = (0..12).map(|_| uniform()).sum::<f32>() - 6.0;
        *sample += NOISE * gaussian;
    }
    noisy
}

/// The peak signal-to-noise ratio of `image` against `truth`, for samples
/// from 0 to 1, in decibels.
fn psnr(image: &Image, truth: &Image) -> f64 {
    let pairs = image.data().iter().zip(truth.data());
    let squares: f64 = pairs.map(|(a, b)| f64::from(a - b).powi(2)).sum();
    let mean = squares / image.data().len() as f64;
    10.0 * (1.0 / mean).log10()
}

/// The image as 8-bit RGB, from 0 to 1.
fn rgb8(image: &Image) -> Vec<u8> {
    let channels = image.channels();
    let byte = |sample: f32| (sample.clamp(0.0, 1.0) * 255.0).round() as u8;
    let pixels = image.data().chunks(channels);
    pixels
        .flat_map(|pixel| [0, 1, 2].map(|c| byte(pixel[c.min(channels - 1)])))
        .collect()
}
