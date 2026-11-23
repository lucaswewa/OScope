//! `ex-pipeline`: a recipe run on an image from the command line (P21). It
//! prints the graph, then runs the recipe once, again from the memo cache,
//! after an edit, tile by tile, and over fields of the slide as a batch
//! job, with each step's time. See the README.

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, bail, ensure};
use clap::Parser;
use oscope_compute::{Compute, ComputeConfig, Image};
use oscope_core::{Point2, Um};
use oscope_hal::ObjectiveCatalogue;
use oscope_imaging::color::{encode_png, read_image};
use oscope_jobs::{JobService, JobState, JobsConfig};
use oscope_pipeline::{
    Batch, Device, Engine, Plan, PortType, Recipe, Run, RunOptions, Value, Values, test_nodes,
};
use oscope_sim::{Field, HeTissue, Light, Optics, Slide, World};
use tokio::sync::broadcast::error::RecvError;

/// Where the slide's corner sits on the stage, and the stage z at which its
/// surface is in focus.
const SLIDE_AT: Point2 = Point2::new(10_000.0, 20_000.0);
const FOCUS: Um = Um(5_000.0);
/// The height of the section's middle above the slide.
const SECTION: f64 = 2.0;

/// The edit made before the third run, with the default recipe.
const EDIT: &str = "mask.level=0.06";

#[derive(Parser)]
#[command(about = "Runs a recipe on an image, and prints the graph and each step's time")]
struct Args {
    /// The recipe. By default, recipes/small-dark-features.json.
    #[arg(long)]
    recipe: Option<PathBuf>,
    /// The image: PNG, JPEG or TIFF. By default, a field of the simulated
    /// H&E slide through the 5× objective.
    #[arg(long)]
    image: Option<PathBuf>,
    /// A parameter to change before the third run, as `node.param=JSON`;
    /// there may be several. With the default recipe, `mask.level=0.06`.
    #[arg(long = "set", value_name = "NODE.PARAM=JSON")]
    set: Vec<String>,
    /// The tiles' size, in pixels, for the tiled run and the batch: 64
    /// with `--smoke`.
    #[arg(long, default_value_t = 256)]
    tile: u32,
    /// The fields of the slide the batch job runs the recipe on.
    #[arg(long, default_value_t = 3)]
    batch: usize,
    /// Runs on small fields, checks what it shows, and exits.
    #[arg(long)]
    smoke: bool,
    /// Require CUDA availability and at least one actual CUDA recipe step.
    #[arg(long)]
    require_cuda: bool,
}

#[tokio::main]
async fn main() -> ExitCode {
    match run(Args::parse()).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("ex-pipeline: {error:#}");
            ExitCode::FAILURE
        }
    }
}

async fn run(mut args: Args) -> anyhow::Result<()> {
    if args.smoke {
        args.tile = 64;
    }
    let temporary = args.smoke.then(tempfile::tempdir).transpose()?;
    let data = match &temporary {
        Some(folder) => folder.path().to_owned(),
        None => PathBuf::from(".oscope/ex-pipeline/data"),
    };
    let out = data.join("out");
    std::fs::create_dir_all(&out)?;
    let compute = Arc::new(Compute::new(&ComputeConfig::default())?);
    let engine = Arc::new(Engine::new(test_nodes::registry(), compute, 1 << 30));
    println!("backend: {}", if engine.has_gpu() { "CUDA" } else { "CPU" });
    ensure!(
        !args.require_cuda || engine.has_gpu(),
        "CUDA required but unavailable"
    );

    let default = Path::new(env!("CARGO_MANIFEST_DIR")).join("recipes/small-dark-features.json");
    let path = args.recipe.clone().unwrap_or(default);
    let text =
        std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    let mut recipe = Recipe::from_json(&text)?;
    let plan = engine.plan(&recipe)?;
    println!("{plan}\n");
    let [input] = plan.inputs() else {
        bail!(
            "ex-pipeline gives a recipe one image, and this one takes {} inputs",
            plan.inputs().len()
        );
    };
    ensure!(
        input.kind == PortType::Image,
        "the recipe's input isn't an image"
    );
    let input = input.name.clone();
    let (width, height) = if args.smoke { (256, 192) } else { (1024, 768) };
    let slide = Arc::new(Sample::new()?);
    let image = match &args.image {
        Some(path) => read_image(path)?
            .into_iter()
            .next()
            .context("the file holds no image")?,
        None => slide.field(0, width, height),
    };
    println!(
        "the image: {} × {} × {}",
        image.width(),
        image.height(),
        image.channels()
    );
    let inputs = Values::from([(input.clone(), Value::image(image))]);

    let options = RunOptions::default();
    let first = engine.run(&plan, &inputs, &options)?;
    ensure!(
        !args.require_cuda
            || first
                .report
                .steps
                .iter()
                .any(|step| step.device == Device::Cuda),
        "CUDA required but the recipe executed no CUDA step"
    );
    show("run 1", &first, true);
    let again = engine.run(&plan, &inputs, &options)?;
    show("run 2, the same again", &again, false);
    let edits = match args.set.is_empty() && args.recipe.is_none() {
        true => vec![EDIT.to_owned()],
        false => args.set.clone(),
    };
    let (mut plan, mut latest, mut edited) = (plan, first, None);
    if !edits.is_empty() {
        for edit in &edits {
            set(&mut recipe, edit)?;
        }
        plan = engine.plan(&recipe)?;
        latest = engine.run(&plan, &inputs, &options)?;
        show(
            &format!("run 3, after {}", edits.join(", ")),
            &latest,
            false,
        );
        edited = Some(
            latest
                .report
                .steps
                .iter()
                .map(|step| step.cached)
                .collect::<Vec<_>>(),
        );
    }
    let files = write(&out, "", &latest.outputs)?;
    println!("written to {}: {}\n", out.display(), files.join(", "));

    let tileable = plan.not_tileable().is_empty();
    let mut same = None;
    if tileable {
        let whole = RunOptions {
            memoise: false,
            ..RunOptions::default()
        };
        let tiled = engine.run_tiled(&plan, &inputs, args.tile, &whole, |_, _| {})?;
        let equal = latest
            .outputs
            .iter()
            .all(|(name, value)| equal(value, &tiled.outputs[name]));
        println!(
            "tile by tile: {} tiles of {} px, each reading {} px around it, in {:.1} ms: {}",
            tiled.report.tiles,
            args.tile,
            plan.halo(),
            ms(tiled.report.time),
            if equal {
                "the same as the whole run, to the bit"
            } else {
                "NOT the same as the whole run"
            }
        );
        same = Some(equal);
    } else {
        println!(
            "tile by tile: not this recipe, as {} can't",
            plan.not_tileable().join(", ")
        );
    }

    let ended = batch(
        &engine,
        plan,
        &args,
        &data,
        slide,
        input,
        (width, height),
        tileable,
    )
    .await?;
    if args.smoke {
        // Steps on the GPU aren't memoised: only the default recipe's are
        // all cached.
        if args.recipe.is_none() {
            let cached = again.report.steps.iter().all(|step| step.cached);
            ensure!(cached, "run 2 wasn't all from the memo cache");
            let expected = [true, true, true, true, false];
            ensure!(
                edited.as_deref() == Some(&expected[..]),
                "after the edit, cached: {edited:?}"
            );
            ensure!(
                same == Some(true),
                "the tiled run differs from the whole one"
            );
        }
        ensure!(
            ended.state == JobState::Completed,
            "the batch {}: {:?}",
            ended.state.name(),
            ended.error
        );
        ensure!(
            ended.results.len() == args.batch,
            "the batch recorded {:?}",
            ended.results
        );
        for file in files {
            ensure!(out.join(&file).is_file(), "{file} wasn't written");
        }
        println!("smoke: ok");
    }
    Ok(())
}

/// Runs `plan` over fields of the slide as a batch job, tile by tile when
/// it can, and follows its progress.
#[allow(clippy::too_many_arguments)]
async fn batch(
    engine: &Arc<Engine>,
    plan: Plan,
    args: &Args,
    data: &Path,
    slide: Arc<Sample>,
    input: String,
    (width, height): (u32, u32),
    tileable: bool,
) -> anyhow::Result<oscope_jobs::JobInfo> {
    let jobs = JobService::open(&data.join("index.sqlite"), JobsConfig::default())?;
    let mut events = jobs.subscribe();
    let out = data.join("out");
    let batch = Batch {
        count: args.batch,
        read: Box::new(move |item| {
            let image = slide.field(item + 1, width, height);
            Ok(Values::from([(input.clone(), Value::image(image))]))
        }),
        write: Box::new(move |item, run| {
            let files = write(&out, &format!("field-{}-", item + 1), &run.outputs);
            Ok(Some(files.map_err(|error| error.to_string())?.join(" ")))
        }),
        tile: tileable.then_some(args.tile),
    };
    let id = jobs.submit(engine.batch_job("ex-pipeline's batch", Arc::new(plan), batch, 0))?;
    println!(
        "\nbatch: a compute job over {} more fields of the slide",
        args.batch
    );
    let mut shown = String::new();
    loop {
        let job = match tokio::time::timeout(Duration::from_secs(600), events.recv()).await {
            Ok(Ok(job)) if job.id == id => job,
            Ok(Err(RecvError::Closed)) => bail!("the job service stopped"),
            Err(_) => bail!("the batch took more than 10 minutes"),
            _ => continue,
        };
        let stage = job
            .progress
            .stage
            .split(':')
            .next()
            .unwrap_or_default()
            .to_owned();
        if stage != shown && !stage.is_empty() {
            let done = 100.0 * job.progress.fraction.unwrap_or_default();
            println!("  {done:>5.1} %  {stage}");
            shown = stage;
        }
        if job.state.is_finished() {
            println!(
                "batch: {}; results: {}",
                job.state.name(),
                job.results.join("; ")
            );
            return Ok(job);
        }
    }
}

/// The simulated H&E slide, through the 5× objective.
struct Sample {
    world: World,
    optics: Optics,
    centre: Point2,
}

impl Sample {
    fn new() -> anyhow::Result<Sample> {
        let tissue = HeTissue::new(1, Slide::COVERSLIP);
        let island = tissue.islands()[0];
        let centre = Point2::new(SLIDE_AT.x + island.x, SLIDE_AT.y + island.y);
        let world = World::with_slide(Slide::new(Arc::new(tissue), SLIDE_AT), FOCUS);
        let catalogue = ObjectiveCatalogue::mitutoyo();
        let objective = catalogue.objectives.iter().find(|o| o.name == "5x");
        let optics = Optics::new(&catalogue, objective.context("the 5× objective")?, Um(2.4));
        Ok(Sample {
            world,
            optics,
            centre,
        })
    }

    /// Field `n`: `n` fields to the right of the tissue's first island.
    fn field(&self, n: usize, width: u32, height: u32) -> Image {
        let across = f64::from(width) * self.optics.pixel_size().0;
        let field = Field {
            centre: Point2::new(self.centre.x + n as f64 * across, self.centre.y),
            z: Um(FOCUS.0 + SECTION),
            width,
            height,
        };
        self.world.render(&field, &self.optics, Light::Transmitted)
    }
}

/// Prints how a run went: each step's device and time, and what it gave.
fn show(title: &str, run: &Run, outputs: bool) {
    println!("{title}: {:.1} ms", ms(run.report.time));
    for step in &run.report.steps {
        let time = match step.cached {
            true => "cached".to_owned(),
            false => format!("{:.1} ms", ms(step.time)),
        };
        let device = if step.device == Device::Cuda {
            "cuda"
        } else {
            "cpu"
        };
        println!("  {:<10} {device:<4} {time:>9}", step.id);
    }
    if outputs {
        for (name, value) in &run.outputs {
            println!("  gives {name}: {}", describe(value));
        }
    }
    println!();
}

fn describe(value: &Value) -> String {
    if let Some(image) = value.as_image() {
        return format!(
            "an image, {} × {} × {}",
            image.width(),
            image.height(),
            image.channels()
        );
    }
    if let Some(mask) = value.as_mask() {
        let set = mask.data().iter().filter(|&&bit| bit == 1).count();
        let share = 100.0 * set as f64 / mask.data().len().max(1) as f64;
        return format!(
            "a mask, {} × {}, {share:.1} % set",
            mask.width(),
            mask.height()
        );
    }
    json(value)
}

fn json(value: &Value) -> String {
    match value.as_table() {
        Some(table) => serde_json::to_string(table).unwrap_or_default(),
        None => value.as_json().map(ToString::to_string).unwrap_or_default(),
    }
}

/// Sets a parameter, from `node.param=JSON`.
fn set(recipe: &mut Recipe, edit: &str) -> anyhow::Result<()> {
    let parts = edit
        .split_once('=')
        .and_then(|(target, value)| Some((target.split_once('.')?, value)));
    let ((node, param), value) =
        parts.with_context(|| format!("`{edit}` isn't node.param=JSON"))?;
    let value = serde_json::from_str(value).with_context(|| format!("`{value}` isn't JSON"))?;
    recipe
        .set_param(node, param, value)
        .map_err(anyhow::Error::msg)
}

/// Whether two outputs are the same, to the bit.
fn equal(a: &Value, b: &Value) -> bool {
    match (a.as_image(), b.as_image(), a.as_mask(), b.as_mask()) {
        (Some(a), Some(b), _, _) => a == b,
        (_, _, Some(a), Some(b)) => a == b,
        _ => json(a) == json(b),
    }
}

/// Writes `outputs` to `folder`, each named `prefix` and its name: images
/// and masks as PNGs, images stretched from their least value to their
/// greatest; the rest as JSON. The files' names.
fn write(folder: &Path, prefix: &str, outputs: &Values) -> anyhow::Result<Vec<String>> {
    let mut files = Vec::new();
    for (name, value) in outputs {
        let (file, bytes) = match (value.as_image(), value.as_mask()) {
            (Some(image), _) => (
                format!("{prefix}{name}.png"),
                png(image.width(), image.height(), &stretch(image))?,
            ),
            (_, Some(mask)) => {
                let grey: Vec<u8> = mask.data().iter().flat_map(|&bit| [bit * 255; 3]).collect();
                (
                    format!("{prefix}{name}.png"),
                    png(mask.width(), mask.height(), &grey)?,
                )
            }
            _ => (format!("{prefix}{name}.json"), json(value).into_bytes()),
        };
        std::fs::write(folder.join(&file), bytes)?;
        files.push(file);
    }
    Ok(files)
}

fn stretch(image: &Image) -> Vec<u8> {
    let bounds = (f32::INFINITY, f32::NEG_INFINITY);
    let (least, greatest) = image
        .data()
        .iter()
        .fold(bounds, |(l, g), &v| (l.min(v), g.max(v)));
    let scale = 255.0 / (greatest - least).max(1e-6);
    let channels = image.channels().max(1);
    let pixels = image.data().chunks(channels);
    pixels
        .flat_map(|pixel| [0, 1, 2].map(|c| ((pixel[c.min(channels - 1)] - least) * scale) as u8))
        .collect()
}

fn png(width: u32, height: u32, rgb8: &[u8]) -> anyhow::Result<Vec<u8>> {
    let mut file = Vec::new();
    encode_png(&mut file, rgb8, width, height)?;
    Ok(file)
}

fn ms(time: Duration) -> f64 {
    time.as_secs_f64() * 1e3
}
