//! `ex-register`: P31's registration on fields of the simulated H&E slide
//! at 10×. A pair shifted by a known sub-pixel amount, and a large shift
//! found in a search window around the one expected, registered on the CPU
//! and, built with `cuda`, on the GPU; with `--bench`, a 2048² pair timed
//! against §5.20's 15 ms. See the README.

use std::process::ExitCode;
use std::time::{Duration, Instant};

use anyhow::ensure;
use oscope_compute::{Image, Shape};
use oscope_core::{Point2, Um};
use oscope_hal::ObjectiveCatalogue;
use oscope_imaging::registration::{self, RegisterOptions, Registration};
use oscope_sim::{Field, HeTissue, Light, Optics, Slide, World};

/// Where the slide's surface is in focus; the section's middle is 2 µm up.
const FOCUS: Um = Um(1_000.0);

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("ex-register: {error:#}");
            ExitCode::FAILURE
        }
    }
}

/// The simulated slide through the 10× objective, looking at the middle of
/// the tissue's first island.
struct Scene {
    world: World,
    optics: Optics,
    centre: Point2,
}

impl Scene {
    fn new() -> anyhow::Result<Self> {
        let tissue = HeTissue::new(1, Slide::COVERSLIP);
        let centre = tissue.islands()[0];
        let catalogue = ObjectiveCatalogue::mitutoyo();
        let optics = Optics::new(&catalogue, catalogue.get("10x")?, Um(2.4));
        let slide = Slide::new(std::sync::Arc::new(tissue), Point2::new(0.0, 0.0));
        let world = World::with_slide(slide, FOCUS);
        Ok(Self {
            world,
            optics,
            centre,
        })
    }

    /// A grey field of `size`, its centre `moved` pixels from the island's
    /// middle: the mean of the rendering's red, green and blue.
    fn grey(&self, moved: (f64, f64), size: u32) -> anyhow::Result<Image> {
        let p = self.optics.pixel_size().0;
        let field = Field {
            centre: Point2::new(self.centre.x + moved.0 * p, self.centre.y + moved.1 * p),
            z: Um(FOCUS.0 + 2.0),
            width: size,
            height: size,
        };
        let rgb = self.world.render(&field, &self.optics, Light::Transmitted);
        let grey = rgb.data().chunks(3).map(|p| (p[0] + p[1] + p[2]) / 3.0);
        Ok(Image::from_vec(Shape::new(size, size, 1), grey.collect())?)
    }
}

/// The GPU's registration, when built with `cuda` and a GPU is there.
#[cfg(feature = "cuda")]
struct Gpu(oscope_compute::Compute);

#[cfg(feature = "cuda")]
impl Gpu {
    fn open() -> Option<Self> {
        let compute = oscope_compute::Compute::new(&Default::default()).ok()?;
        compute.cuda().is_some().then_some(Self(compute))
    }

    fn register(
        &self,
        a: &Image,
        b: &Image,
        options: &RegisterOptions,
    ) -> anyhow::Result<Registration> {
        let cuda = anyhow::Context::context(self.0.cuda(), "the GPU")?;
        let lane = oscope_compute::Lane::Live;
        Ok(registration::gpu::register(cuda, lane, a, b, options)?)
    }
}

/// Without `cuda`, no GPU.
#[cfg(not(feature = "cuda"))]
struct Gpu;

#[cfg(not(feature = "cuda"))]
impl Gpu {
    fn open() -> Option<Self> {
        None
    }

    fn register(&self, _: &Image, _: &Image, _: &RegisterOptions) -> anyhow::Result<Registration> {
        anyhow::bail!("built without `cuda`")
    }
}

/// The median of `runs` timings of `register`, after a first run that
/// warms it up, and what it found.
fn timed(
    runs: usize,
    mut register: impl FnMut() -> anyhow::Result<Registration>,
) -> anyhow::Result<(Registration, Duration)> {
    let found = register()?;
    let mut times = Vec::with_capacity(runs);
    for _ in 0..runs {
        let start = Instant::now();
        register()?;
        times.push(start.elapsed());
    }
    times.sort();
    Ok((found, times[runs / 2]))
}

fn ms(time: Duration) -> String {
    format!("{:.1} ms", time.as_secs_f64() * 1e3)
}

fn pair(x: f64, y: f64) -> String {
    format!("({x:8.3}, {y:8.3})")
}

/// Registers B, its field moved by `moved` pixels from A's, so its content
/// the other way: prints a row, and checks the shift within 0.1 px and the
/// confidence above 0.5.
fn case(
    scene: &Scene,
    gpu: Option<&Gpu>,
    name: &str,
    size: u32,
    moved: (f64, f64),
    options: &RegisterOptions,
) -> anyhow::Result<()> {
    let (a, b) = (scene.grey((0.0, 0.0), size)?, scene.grey(moved, size)?);
    let expected = (-moved.0, -moved.1);
    // The GPU first, while the CPU's threads are idle; the median of 11
    // runs on the GPU, and of 3 on the CPU.
    let gpu = match gpu {
        Some(gpu) => Some(timed(11, || gpu.register(&a, &b, options))?),
        None => None,
    };
    let (found, cpu) = timed(3, || Ok(registration::register(&a, &b, options)?))?;
    println!(
        "{name:<34} {}  {}  {:>10.3}  {:>5.3}  {:>9}  {:>9}",
        pair(expected.0, expected.1),
        pair(found.dx, found.dy),
        found.confidence,
        found.peak,
        ms(cpu),
        gpu.map_or("".to_owned(), |(_, time)| ms(time)),
    );
    // What `cargo xtask perf` reads (P53): the 2048² pair.
    if size == 2048 {
        println!("perf: register.cpu_ms {:.2}", cpu.as_secs_f64() * 1e3);
        if let Some((_, time)) = gpu {
            println!("perf: register.gpu_ms {:.2}", time.as_secs_f64() * 1e3);
        }
    }
    let error = (found.dx - expected.0)
        .abs()
        .max((found.dy - expected.1).abs());
    ensure!(
        error <= 0.1,
        "{name}: {found:?}, where {expected:?} was expected"
    );
    ensure!(
        found.confidence > 0.5,
        "{name}: a confidence of {}",
        found.confidence
    );
    if let Some((on_gpu, _)) = gpu {
        let apart = (on_gpu.dx - found.dx)
            .abs()
            .max((on_gpu.dy - found.dy).abs());
        ensure!(
            apart <= 0.01,
            "{name}: the GPU found {on_gpu:?}, the CPU {found:?}"
        );
    }
    Ok(())
}

fn run() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (smoke, bench) = (
        args.iter().any(|a| a == "--smoke"),
        args.iter().any(|a| a == "--bench"),
    );
    if let Some(other) = args.iter().find(|a| *a != "--smoke" && *a != "--bench") {
        anyhow::bail!("{other:?}: the options are --smoke and --bench");
    }
    let scene = Scene::new()?;
    let gpu = Gpu::open();
    let size = if smoke { 256 } else { 512 };
    println!(
        "fields of the simulated H&E slide at 10× ({} µm pixels), {size} × {size}; the GPU: {}",
        scene.optics.pixel_size().0,
        if gpu.is_some() { "yes" } else { "no" },
    );
    println!(
        "{:<34} {:<20}  {:<20}  {:>10}  {:>5}  {:>9}  {:>9}",
        "", "expected", "found", "confidence", "peak", "CPU", "GPU"
    );
    let every = RegisterOptions::default();
    case(
        &scene,
        gpu.as_ref(),
        "a sub-pixel shift",
        size,
        (3.3, -1.7),
        &every,
    )?;
    case(
        &scene,
        gpu.as_ref(),
        "a whole one",
        size,
        (-12.0, 5.0),
        &every,
    )?;
    // A scan's next field: the stage says where it should be, to a few
    // pixels, and only the shifts around it are considered.
    let far = (size as f64 * 0.35 + 0.4, -(size as f64) * 0.2 - 0.3);
    let near = RegisterOptions {
        expected: Some((-far.0.round() + 3.0, -far.1.round() - 2.0)),
        radius: 16,
        ..every
    };
    case(
        &scene,
        gpu.as_ref(),
        "a large shift, everywhere",
        size,
        far,
        &every,
    )?;
    case(
        &scene,
        gpu.as_ref(),
        "the same, in a window of ±16 px",
        size,
        far,
        &near,
    )?;
    if bench {
        println!("\na 2048² pair, §5.20's budget on the GPU 15 ms:");
        case(
            &scene,
            gpu.as_ref(),
            "a sub-pixel shift, 2048²",
            2048,
            (37.25, -18.5),
            &every,
        )?;
    }
    if smoke {
        println!("smoke: ok");
    }
    Ok(())
}
