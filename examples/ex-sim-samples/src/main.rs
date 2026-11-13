//! `ex-sim-samples`: the simulator's sample catalogue. It writes an image
//! and a stack to import, and a configuration that lists them, then loads
//! each sample in turn, renders it at 5× and 50× in its own light, and
//! unloads it, leaving the stage empty. See the README.

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::{Arc, PoisonError, RwLock};
use std::time::Instant;

use anyhow::{Context, ensure};
use oscope_compute::Image;
use oscope_core::{Point2, Um};
use oscope_hal::ObjectiveCatalogue;
use oscope_imaging::color::{Display, encode_png, to_display};
use oscope_sim::{
    Excitation, Field, HeTissue, Light, Optics, SampleCatalogue, SampleEntry, SampleKind, Slide,
    SlideLoader, World,
};
use tiff::encoder::{TiffEncoder, colortype};

/// The stage z at which the slide's surface is in focus; the slide's
/// corner is at the stage's origin.
const FOCUS: f64 = 5_000.0;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("ex-sim-samples: {error:#}");
            ExitCode::FAILURE
        }
    }
}

/// The value after `option` on the command line.
fn value(args: &[String], option: &str) -> Option<String> {
    args.iter()
        .position(|a| a == option)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

/// A 400 × 300 image to import, at 0.5 µm a pixel: 10 µm squares of grey,
/// and a dark ring 100 µm across round its middle.
fn write_demo_image(path: &Path) -> anyhow::Result<()> {
    let (w, h) = (400_u32, 300_u32);
    let rgb: Vec<u8> = (0..w * h)
        .flat_map(|i| {
            let (x, y) = (f64::from(i % w), f64::from(i / w));
            let r = (x - 200.0).hypot(y - 150.0);
            let level = if (90.0..100.0).contains(&r) {
                30
            } else if ((i % w) / 20 + (i / w) / 20) % 2 == 0 {
                235
            } else {
                170
            };
            [level, level, level]
        })
        .collect();
    let mut file = Vec::new();
    encode_png(&mut file, &rgb, w, h)?;
    Ok(std::fs::write(path, file)?)
}

/// A stack of three 200 × 200 pages to import as fluorescence: a bright
/// disc 20 µm across, moving 10 µm to the right on each page.
fn write_demo_stack(path: &Path) -> anyhow::Result<()> {
    let mut file = std::fs::File::create(path)?;
    let mut encoder = TiffEncoder::new(&mut file)?;
    for k in 0..3_u32 {
        let page: Vec<u16> = (0..200 * 200_u32)
            .map(|i| {
                let (x, y) = (f64::from(i % 200), f64::from(i / 200));
                let inside = (x - 70.0 - 30.0 * f64::from(k)).hypot(y - 100.0) < 30.0;
                if inside { 50_000 } else { 0 }
            })
            .collect();
        encoder.write_image::<colortype::Gray16>(200, 200, &page)?;
    }
    Ok(())
}

/// The light each sample is seen in, and the height above the slide to
/// focus on.
fn light_for(entry: &SampleEntry) -> (Vec<Light>, f64) {
    let fluorescence = |lines: &[Excitation]| {
        lines
            .iter()
            .map(|line| Light::Fluorescence {
                line: *line,
                confocal: false,
            })
            .collect()
    };
    match entry.kind {
        SampleKind::Tissue => (vec![Light::Transmitted], 2.0),
        SampleKind::TestTarget => (vec![Light::Transmitted], 0.05),
        SampleKind::Surface => (vec![Light::Reflected], 2.5),
        SampleKind::Cells => (
            fluorescence(&[Excitation::Violet, Excitation::Blue, Excitation::Green]),
            2.5,
        ),
        SampleKind::Structures => (fluorescence(&[Excitation::Blue, Excitation::Green]), 0.1),
        SampleKind::Image => (vec![Light::Transmitted], 0.0),
        SampleKind::Stack => (fluorescence(&[Excitation::Blue]), 1.5),
    }
}

/// The world's image of `field` through `optics`, the lights' images added.
fn render(world: &RwLock<World>, field: &Field, optics: &Optics, lights: &[Light]) -> Image {
    let world = world.read().unwrap_or_else(PoisonError::into_inner);
    let mut sum = world.render(field, optics, lights[0]);
    for light in &lights[1..] {
        for (s, v) in sum
            .data_mut()
            .iter_mut()
            .zip(world.render(field, optics, *light).data())
        {
            *s += v;
        }
    }
    sum
}

/// An image as a PNG, scaled so its brightest sample is white, unless
/// that is under 1.
fn png(image: &Image) -> anyhow::Result<Vec<u8>> {
    let top = image.data().iter().copied().fold(0.0_f32, f32::max);
    let display = Display {
        white: if top < 1.0 { top.max(1e-6) } else { 1.0 },
        ..Display::default()
    };
    let mut file = Vec::new();
    encode_png(
        &mut file,
        &to_display(image, &display)?,
        image.width(),
        image.height(),
    )?;
    Ok(file)
}

fn mean(image: &Image) -> f64 {
    image.data().iter().map(|v| f64::from(*v)).sum::<f64>() / image.data().len() as f64
}

fn run() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let smoke = args.iter().any(|a| a == "--smoke");
    let temporary = smoke.then(tempfile::tempdir).transpose()?;
    let folder = match &temporary {
        Some(t) => t.path().to_owned(),
        None => PathBuf::from(".oscope").join("ex-sim-samples"),
    };
    std::fs::create_dir_all(&folder)?;

    // The imports, and the configuration that lists them.
    // Your image by its full path, or the demo image by its path from the
    // configuration's folder.
    let image = match value(&args, "--image") {
        Some(path) => PathBuf::from(path).canonicalize().context("--image")?,
        None => {
            write_demo_image(&folder.join("demo.png"))?;
            PathBuf::from("demo.png")
        }
    };
    let um_per_pixel: f64 = value(&args, "--um-per-pixel").map_or(Ok(0.5), |v| v.parse())?;
    write_demo_stack(&folder.join("demo-stack.tif"))?;
    let configuration = format!(
        "[[samples]]\nname = \"your image\"\nkind = \"image\"\npath = {:?}\num_per_pixel = {um_per_pixel}\n\n\
         [[samples]]\nname = \"a fluorescence stack\"\nkind = \"stack\"\npath = \"demo-stack.tif\"\num_per_pixel = 0.5\nz_step = 1.5\nlight = \"fluorescent\"\ndye = \"green\"\n",
        image.display().to_string()
    );
    std::fs::write(folder.join("samples.toml"), &configuration)?;
    let catalogue = SampleCatalogue::from_toml(&configuration).map_err(anyhow::Error::msg)?;

    let world = Arc::new(RwLock::new(World::empty()));
    world.write().unwrap_or_else(PoisonError::into_inner).focus = Um(FOCUS);
    let loader = SlideLoader::new(world.clone(), catalogue, &folder, Point2::new(0.0, 0.0));
    let cover = Slide::COVERSLIP;
    let middle = Point2::new(
        cover.origin.x + cover.size.width / 2.0,
        cover.origin.y + cover.size.height / 2.0,
    );
    let objectives = ObjectiveCatalogue::mitutoyo();
    let (w, h) = if smoke { (192, 128) } else { (768, 512) };

    println!("the samples in {}:", folder.join("samples.toml").display());
    println!(
        "  {:<28} {:<12} {:<6} {:>9} {:>9} {:>8}",
        "name", "kind", "truth", "mean 5×", "mean 50×", "time"
    );
    let mut means = Vec::new();
    for entry in &loader.catalogue().samples.clone() {
        let started = Instant::now();
        loader.load(&entry.name).map_err(anyhow::Error::msg)?;
        // The tissue lies in islands: look at the first; the rest are at,
        // or cover, the coverslip's middle.
        let at = if entry.kind == SampleKind::Tissue {
            HeTissue::new(entry.seed, cover).islands()[0]
        } else {
            middle
        };
        let (lights, z) = light_for(entry);
        let slug: String = entry
            .name
            .chars()
            .map(|c| {
                if c.is_alphanumeric() {
                    c.to_ascii_lowercase()
                } else {
                    '-'
                }
            })
            .collect();
        let mut row = Vec::new();
        for name in ["5x", "50x"] {
            let optics = Optics::new(&objectives, objectives.get(name)?, Um(2.4));
            let field = Field {
                centre: at,
                z: Um(FOCUS + z),
                width: w,
                height: h,
            };
            let image = render(&world, &field, &optics, &lights);
            std::fs::write(folder.join(format!("{slug}-{name}.png")), png(&image)?)?;
            row.push(mean(&image));
        }
        let ms = started.elapsed().as_secs_f64() * 1e3;
        let truth = if entry.has_ground_truth() {
            "yes"
        } else {
            "no"
        };
        let kind = entry.kind.as_str();
        println!(
            "  {:<28} {:<12} {:<6} {:>9.4} {:>9.4} {:>5.0} ms",
            entry.name, kind, truth, row[0], row[1], ms
        );
        means.push((entry.clone(), row));
    }

    // The imported image's place: its middle at the coverslip's.
    loader.load("your image").map_err(anyhow::Error::msg)?;
    let bounds = world
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .slide
        .as_ref()
        .context("a slide")?
        .sample
        .bounds();
    let centre = Point2::new(
        bounds.origin.x + bounds.size.width / 2.0,
        bounds.origin.y + bounds.size.height / 2.0,
    );
    println!(
        "your image: {:.0} × {:.0} µm, its middle at ({:.0}, {:.0}) µm on the slide",
        bounds.size.width, bounds.size.height, centre.x, centre.y
    );

    let unloaded = loader.unload();
    let optics = Optics::new(&objectives, objectives.get("5x")?, Um(2.4));
    let field = Field {
        centre: middle,
        z: Um(FOCUS),
        width: w,
        height: h,
    };
    let empty = render(&world, &field, &optics, &[Light::Transmitted]);
    std::fs::write(folder.join("empty-stage.png"), png(&empty)?)?;
    let uniform = empty.data().iter().all(|v| *v == 1.0);
    println!(
        "unloaded {:?}: the stage is empty, {} in transmitted light",
        unloaded.unwrap_or_default(),
        if uniform {
            "uniformly 1"
        } else {
            "but not uniform"
        }
    );
    println!("the images are in {}", folder.display());

    if smoke {
        ensure!(means.len() == 7, "{} samples", means.len());
        ensure!(
            means.iter().filter(|(e, _)| e.has_ground_truth()).count() == 5,
            "the built-in samples carry ground truth, the imports don't"
        );
        for (entry, row) in &means {
            ensure!(
                row.iter().all(|m| (m - 1.0).abs() > 1e-3),
                "{}: looks like an empty stage",
                entry.name
            );
        }
        ensure!(uniform, "the empty stage isn't uniformly 1");
        ensure!(
            (bounds.size.width - 200.0).abs() < 1e-9 && centre.distance(middle) < 1e-9,
            "the image landed at {bounds:?}"
        );
        println!("smoke: ok");
    }
    Ok(())
}
