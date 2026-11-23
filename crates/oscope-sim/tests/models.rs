//! What the simulated camera's models cost: the full sensor over the H&E
//! section at 10×, as `ex-live-view` shows it, with one model changed at a
//! time (`docs/performance/live-view-baseline.md`). A benchmark, so ignored
//! unless asked for:
//!
//! ```sh
//! cargo test --release -p oscope-sim --test models -- --ignored --nocapture --test-threads=1
//! ```
//!
//! - **Frames,** in mode `full`, 16 bits, 15 ms: one cold, whose tiles are
//!   all rendered, then five warm, with each of the camera's steps timed.
//! - **One tile, on one thread:** the section's two layers, their blur, and
//!   the rest of the world's rendering, for each objective and focus.
//! - **One pixel, on one thread:** what the exposure does for each of the
//!   sensor's 20 million pixels.

mod common;

use std::hint::black_box;
use std::sync::Arc;
use std::time::{Duration, Instant};

use common::{FOCUS, rig_over, start};
use oscope_core::timing::{self, StepStats};
use oscope_core::{Point2, Rect, Timestamp, Um};
use oscope_hal::{Axis, Camera, ObjectiveCatalogue, Stage, UmPerS};
use oscope_sim::blur::gaussian;
use oscope_sim::camera::vignette;
use oscope_sim::sensor::Random;
use oscope_sim::{
    Field, HeTissue, Light, Mounting, Optics, Rate, Sample, SensorModel, SimCamera,
    SimCameraConfig, Slide, TileCache, World,
};

/// The live view's models, with one changed.
#[derive(Clone, Copy)]
struct Variant {
    name: &'static str,
    slide: bool,
    objective: &'static str,
    /// The stage's height above the section's best focus, in µm: about 1
    /// in the live view.
    defocus: f64,
    noise: bool,
    intensity: f64,
    vignetting: f64,
    /// The stage's speed along x during the warm frames, in µm/s.
    jog: f64,
}

const LIVE: Variant = Variant {
    name: "as the live view",
    slide: true,
    objective: "10x",
    defocus: 1.0,
    noise: true,
    intensity: 0.6,
    vignetting: 0.0,
    jog: 0.0,
};

/// The section, with its best focus `defocus` below the stage's, and where
/// its first island is.
fn tissue(defocus: f64) -> (World, Point2) {
    let tissue = HeTissue::new(1, Slide::COVERSLIP);
    let island = tissue.islands()[0];
    let slide = Slide::new(Arc::new(tissue), Point2::new(0.0, 0.0));
    (World::with_slide(slide, Um(FOCUS - defocus)), island)
}

fn optics(objective: &str) -> Optics {
    let catalogue = ObjectiveCatalogue::mitutoyo();
    Optics::new(&catalogue, catalogue.get(objective).unwrap(), Um(2.4))
}

fn p50(steps: &[StepStats], step: &str) -> f64 {
    steps
        .iter()
        .find(|s| s.step == step)
        .map_or(0.0, |s| s.p50_ms)
}

/// What a variant's frames took, in ms: a cold frame, and its tiles; then
/// the warm frames' medians: the frame, its tiles, the motion and the
/// exposure; the tiles made between frames, a frame's mean; and the new
/// tiles a frame.
#[derive(Debug, Default)]
struct Measured {
    cold: f64,
    tiles: f64,
    frame: f64,
    field: f64,
    motion: f64,
    expose: f64,
    between: f64,
    more: f64,
}

/// The tiles `camera` has rendered or made, on the CPU and on the GPU.
fn made(camera: &SimCamera) -> f64 {
    #[cfg(feature = "cuda")]
    let gpu = camera.gpu_stats().map_or(0, |s| s.made + s.from_cpu);
    #[cfg(not(feature = "cuda"))]
    let gpu = 0;
    (camera.cache_stats().misses + gpu) as f64
}

/// `v`'s frames, the camera set up by `prepare`: a cold frame, then five
/// warm ones.
async fn frames(v: Variant, prepare: impl FnOnce(&SimCamera)) -> Measured {
    let (world, at) = match v.slide {
        true => tissue(v.defocus),
        false => (World::empty(), Point2::new(40_000.0, 12_000.0)),
    };
    let mut config = SimCameraConfig::default();
    config.sensor.noise = v.noise;
    config.vignetting = v.vignetting;
    let rig = rig_over(config, world, at, Rate::Stepped);
    rig.camera.set_optics(optics(v.objective));
    rig.camera.set_light(v.intensity, Light::Transmitted);
    prepare(&rig.camera);
    let frames = start(&rig.camera).await;
    // Mode `full` in 16 bits: 7.24 frames a second.
    let interval = Duration::from_secs_f64(1.0 / 7.24);
    let mut k = 0;
    let mut next = || {
        k += 1;
        rig.clock
            .advance_to(Timestamp(0) + interval * k - Duration::from_millis(1));
        frames.recv_timeout(Duration::from_secs(600)).unwrap();
    };
    timing::reset();
    next();
    let cold = timing::snapshot();
    let tiles = made(&rig.camera);
    if v.jog > 0.0 {
        rig.stage
            .jog(Axis::X, true, Some(UmPerS(v.jog)))
            .await
            .unwrap();
    }
    timing::reset();
    for _ in 0..5 {
        next();
    }
    let warm = timing::snapshot();
    let more = made(&rig.camera) - tiles;
    rig.camera.stop().await.unwrap();
    let total = |step: &str| {
        warm.iter()
            .find(|s| s.step == step)
            .map_or(0.0, |s| s.mean_ms * s.count as f64)
    };
    Measured {
        cold: p50(&cold, "camera.render"),
        tiles,
        frame: p50(&warm, "camera.render"),
        field: p50(&warm, "camera.field"),
        motion: p50(&warm, "camera.motion"),
        expose: p50(&warm, "camera.expose").max(p50(&warm, "camera.expose_gpu")),
        between: total("camera.prefetch") / 5.0,
        more: more / 5.0,
    }
}

/// Prints a table of `variants`' frames.
async fn table(variants: &[Variant], prepare: impl Fn(&SimCamera)) {
    println!(
        "frames, in ms: cold, its tiles; warm medians: frame, tiles, motion, exposure; between frames; new tiles a frame"
    );
    for v in variants {
        let m = frames(*v, &prepare).await;
        println!(
            "  {:28} {:7.0} {:4.0}   {:6.1} {:6.1} {:5.1} {:6.1}   {:6.1}   {:4.1}",
            v.name, m.cold, m.tiles, m.frame, m.field, m.motion, m.expose, m.between, m.more
        );
    }
}

/// The median of `runs` timings of `work`, in ms.
fn median(runs: usize, mut work: impl FnMut()) -> f64 {
    let mut ms: Vec<f64> = (0..runs)
        .map(|_| {
            let started = Instant::now();
            work();
            started.elapsed().as_secs_f64() * 1e3
        })
        .collect();
    ms.sort_by(f64::total_cmp);
    ms[runs / 2]
}

/// One tile of the section through `objective`, `defocus` µm from focus,
/// on one thread: its two layers, their blur, and the whole tile, in ms.
fn one_tile(objective: &str, defocus: f64) -> [f64; 3] {
    let one = rayon::ThreadPoolBuilder::new()
        .num_threads(1)
        .build()
        .unwrap();
    let (world, island) = tissue(defocus);
    let tissue = HeTissue::new(1, Slide::COVERSLIP);
    let optics = optics(objective);
    let p = optics.pixel_size().0;
    // As the world renders a tile: 257 pixels, and the blur's margin of
    // 3σ + 1 for the layer furthest from focus; the layers sit 1 and 3 µm
    // above the slide.
    let sigmas = [1.0, 3.0].map(|z| optics.blur(Um(defocus - z)).0 / p);
    let margin = (sigmas[0].max(sigmas[1]).min(64.0) * 3.0 + 1.0).ceil() as u32;
    let side = 257 + 2 * margin;
    let corner = Point2::new(island.x - f64::from(side) * p / 2.0, island.y);
    one.install(|| {
        let layers = median(5, || {
            for layer in 0..2 {
                black_box(tissue.render(layer, corner, Um(p), side, side));
            }
        });
        let mut image = tissue.render(0, corner, Um(p), side, side);
        let blur = 2.0
            * median(5, || {
                gaussian(&mut image, sigmas[0]);
            });
        let field = Field {
            centre: island,
            z: Um(FOCUS),
            width: 257,
            height: 257,
        };
        let tile = median(5, || {
            black_box(world.render(&field, &optics, Light::Transmitted));
        });
        [layers, blur, tile]
    })
}

/// What the exposure does for one pixel, on one thread, in ns: the
/// mounting, sampling the field, the vignetting, and the sensor's counts
/// with and without noise.
fn one_pixel() -> [f64; 5] {
    const N: u32 = 2_000_000;
    let (world, island) = tissue(1.0);
    let optics = optics("10x");
    let region = Rect::new(island.x, island.y, 500.0, 500.0);
    let field =
        TileCache::new(1 << 30).field(&world, &optics, Light::Transmitted, Um(FOCUS), region);
    let mounting = Mounting::default();
    let ns = |ms: f64| ms * 1e6 / f64::from(N);
    let to_stage = median(3, || {
        for i in 0..N {
            black_box(mounting.to_stage(f64::from(i % 5472), f64::from(i / 5472)));
        }
    });
    let sample = median(3, || {
        for i in 0..N {
            let (x, y) = (f64::from(i % 2000) * 0.24, f64::from(i / 2000) * 0.24);
            black_box(field.sample(island.x + x, island.y + y, (i % 3) as usize));
        }
    });
    let vignetting = median(3, || {
        for i in 0..N {
            black_box(vignette(0.3, f64::from(i % 5472), f64::from(i / 5472), 1e7));
        }
    });
    let counts = |noise: bool| {
        let sensor = SensorModel {
            noise,
            ..SensorModel::default()
        };
        let mut random = Random::new(1);
        median(3, || {
            for i in 0..N {
                let signal = 6000.0 + f64::from(i % 100);
                black_box(sensor.count(signal, 0.015, 1, 0.0, &mut random));
            }
        })
    };
    [
        ns(to_stage),
        ns(sample),
        ns(vignetting),
        ns(counts(true)),
        ns(counts(false)),
    ]
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "a benchmark: run it in release with --ignored"]
async fn what_each_model_costs() {
    timing::enable(true);
    let variants = [
        LIVE,
        Variant {
            name: "the sensor without noise",
            noise: false,
            ..LIVE
        },
        Variant {
            name: "no slide",
            slide: false,
            ..LIVE
        },
        Variant {
            name: "5×",
            objective: "5x",
            ..LIVE
        },
        Variant {
            name: "20×",
            objective: "20x",
            ..LIVE
        },
        Variant {
            name: "50×",
            objective: "50x",
            ..LIVE
        },
        Variant {
            name: "10 µm out of focus",
            defocus: 11.0,
            ..LIVE
        },
        Variant {
            name: "50 µm out of focus",
            defocus: 51.0,
            ..LIVE
        },
        Variant {
            name: "the light at full intensity",
            intensity: 1.0,
            ..LIVE
        },
        Variant {
            name: "vignetting of 0.3",
            vignetting: 0.3,
            ..LIVE
        },
        Variant {
            name: "jogging at 200 µm/s",
            jog: 200.0,
            ..LIVE
        },
        Variant {
            name: "jogging at 500 µm/s",
            jog: 500.0,
            ..LIVE
        },
    ];
    table(&variants, |_| {}).await;
    println!("one tile on one thread, in ms: the two layers, their blur, the whole tile");
    for (objective, defocus) in [
        ("5x", 1.0),
        ("10x", 1.0),
        ("20x", 1.0),
        ("50x", 1.0),
        ("10x", 11.0),
        ("10x", 51.0),
    ] {
        let [layers, blur, tile] = one_tile(objective, defocus);
        println!(
            "  {objective:4} {:3.0} µm from focus   {layers:6.1} {blur:6.1} {tile:6.1}",
            defocus - 1.0
        );
    }
    let [to_stage, sample, vignetting, noisy, plain] = one_pixel();
    println!(
        "one pixel on one thread, in ns: the mounting {to_stage:.1}, a sample of the field {sample:.1}, \
         the vignetting {vignetting:.1}, the counts with noise {noisy:.1} and without {plain:.1}"
    );
}

/// The same on the GPU (ADR-0062), with `OSCOPE_CUDA=1`: the cold frame's
/// tiles made in full, with no budget, to time them.
#[cfg(feature = "cuda")]
#[tokio::test(flavor = "multi_thread")]
#[ignore = "a benchmark: run it in release with --ignored, and OSCOPE_CUDA=1"]
async fn what_each_model_costs_on_the_gpu() {
    if std::env::var("OSCOPE_CUDA").as_deref() != Ok("1") {
        eprintln!("skipped: set OSCOPE_CUDA=1 to run the tests that need a GPU");
        return;
    }
    timing::enable(true);
    let variants = [
        LIVE,
        Variant {
            name: "the sensor without noise",
            noise: false,
            ..LIVE
        },
        Variant {
            name: "50×",
            objective: "50x",
            ..LIVE
        },
        Variant {
            name: "50 µm out of focus",
            defocus: 51.0,
            ..LIVE
        },
        Variant {
            name: "jogging at 200 µm/s",
            jog: 200.0,
            ..LIVE
        },
        Variant {
            name: "jogging at 500 µm/s",
            jog: 500.0,
            ..LIVE
        },
    ];
    table(&variants, |camera| {
        let cuda = oscope_compute::cuda::Cuda::open(0).unwrap();
        camera
            .use_gpu(cuda, oscope_sim::devices::GPU_SLOTS)
            .unwrap();
        camera.set_tile_budget(usize::MAX);
    })
    .await;
    let steps = timing::snapshot();
    for step in ["camera.gpu_layers", "camera.gpu_tiles", "camera.expose_gpu"] {
        if let Some(s) = steps.iter().find(|s| s.step == step) {
            println!(
                "  {step}: {} batches, {:.1} ms on average",
                s.count, s.mean_ms
            );
        }
    }
}
