//! The GPU's tiles of the H&E section against the CPU's (ADR-0062), with
//! `OSCOPE_CUDA=1` and the `cuda` feature:
//!
//! - within half precision's bounds, at each objective, binned, out of
//!   focus, and stained for DAB, from the tissue's middle out past its edge
//!   to the glass;
//! - the same to the bit, whichever tiles were made before;
//! - made from the cached layers when only the focus changes, which a new
//!   slide empties.

#![cfg(feature = "cuda")]

use std::sync::Arc;

use oscope_compute::cuda::Cuda;
use oscope_core::{Point2, Rect, Um};
use oscope_hal::ObjectiveCatalogue;
use oscope_sim::gpu::GpuTiles;
use oscope_sim::{HeTissue, Light, Optics, Slide, TileCache, World};

/// The stage's height at which the slide is in focus.
const FOCUS: f64 = 1_000.0;

/// The GPU, when `OSCOPE_CUDA=1` asks for the tests that need one.
fn gpu() -> Option<Cuda> {
    if std::env::var("OSCOPE_CUDA").as_deref() != Ok("1") {
        eprintln!("skipped: set OSCOPE_CUDA=1 to run the tests that need a GPU");
        return None;
    }
    Some(Cuda::open(0).expect("OSCOPE_CUDA=1, but no GPU"))
}

/// `objective` onto the sensor's pixels, binned `binning` × `binning`.
fn optics(objective: &str, binning: f64) -> Optics {
    let catalogue = ObjectiveCatalogue::mitutoyo();
    Optics::new(
        &catalogue,
        catalogue.get(objective).unwrap(),
        Um(2.4 * binning),
    )
}

fn world(tissue: HeTissue) -> World {
    World::with_slide(
        Slide::new(Arc::new(tissue), Point2::new(0.0, 0.0)),
        Um(FOCUS),
    )
}

/// The CPU's tile at column `i` and row `j`, from the region
/// `GpuTiles::tile` looks at.
fn cpu_tile(
    cpu: &TileCache,
    world: &World,
    optics: &Optics,
    z: f64,
    (i, j): (i64, i64),
) -> Vec<f32> {
    let (p, t) = (optics.pixel_size().0, 256.0);
    let region = Rect::new(
        (i as f64 * t + 10.0) * p,
        (j as f64 * t + 10.0) * p,
        100.0 * p,
        100.0 * p,
    );
    let field = cpu.field(world, optics, Light::Transmitted, Um(z), region);
    assert_eq!(field.tiles().len(), 1);
    field.tiles()[0].data.clone()
}

/// Ten tiles 900 µm apart, from the island's middle out along x: tissue,
/// its edge, and the glass beyond. Islands' radii are 2.9 to 8.4 mm.
fn places(island: Point2, optics: &Optics) -> Vec<(i64, i64)> {
    let side = optics.pixel_size().0 * 256.0;
    let step = (900.0 / side).round() as i64;
    let (i, j) = (
        (island.x / side).floor() as i64,
        (island.y / side).floor() as i64,
    );
    (0..10).map(|k| (i + k * step, j)).collect()
}

/// The largest and the mean difference between two tiles' samples.
fn differences(a: &[f32], b: &[f32]) -> (f32, f64) {
    assert_eq!(a.len(), b.len());
    let worst = a
        .iter()
        .zip(b)
        .map(|(x, y)| (x - y).abs())
        .fold(0.0, f32::max);
    let mean = a
        .iter()
        .zip(b)
        .map(|(x, y)| f64::from((x - y).abs()))
        .sum::<f64>()
        / a.len() as f64;
    (worst, mean)
}

#[test]
fn the_gpu_makes_the_cpu_s_tiles_within_half_precision() {
    let Some(cuda) = gpu() else { return };
    // One cache of each, and one slide, for every case but the last: each
    // case finds the others' layers and tiles, and must take only those of
    // its own pixel and focus. 5× and 10× binned 2 × 2 share a pixel, and so
    // their layers.
    let mut gpu = GpuTiles::new(cuda, 512, oscope_sim::camera::IDEAL).unwrap();
    let cpu = TileCache::new(1 << 30);
    let he = HeTissue::new(1, Slide::COVERSLIP);
    let island = he.islands()[0];
    let (he, dab) = (world(he), world(HeTissue::ihc(1, Slide::COVERSLIP)));
    // σ in focus is 1.7–4.4 pixels: a kernel. 10 µm out of focus, 6.1
    // pixels at 10×: a wider kernel; 40 µm out, 24: box filters.
    let cases = [
        ("10×, in focus", "10x", 1.0, 1.0, &he),
        ("5×", "5x", 1.0, 1.0, &he),
        ("20×", "20x", 1.0, 1.0, &he),
        ("50×", "50x", 1.0, 1.0, &he),
        ("10×, binned 2 × 2", "10x", 2.0, 1.0, &he),
        ("10×, 10 µm out of focus", "10x", 1.0, 11.0, &he),
        ("10×, 40 µm out of focus", "10x", 1.0, 41.0, &he),
        ("10×, H-DAB", "10x", 1.0, 1.0, &dab),
    ];
    // Half precision is within 2^-12 of each sample; the GPU's floats, and
    // the half-precision layers under the blur, add a little: within 2^-8
    // everywhere, and 2^-12 on average.
    let mut failed = Vec::new();
    for (name, objective, binning, defocus, world) in cases {
        let optics = optics(objective, binning);
        let z = FOCUS + defocus;
        let (mut worst, mut mean) = (0.0_f32, 0.0_f64);
        let places = places(island, &optics);
        for place in &places {
            let on_gpu = gpu
                .tile(world, &optics, Light::Transmitted, Um(z), *place, &cpu)
                .unwrap();
            let on_cpu = cpu_tile(&cpu, world, &optics, z, *place);
            let (w, m) = differences(&on_cpu, &on_gpu);
            worst = worst.max(w);
            mean += m / places.len() as f64;
        }
        let line = format!("{name}: up to {worst:.6}, {mean:.7} on average");
        println!("{line}");
        if worst > 2f32.powi(-8) || mean > 2f64.powi(-12) {
            failed.push(line);
        }
    }
    assert!(failed.is_empty(), "{failed:#?}");
    let stats = gpu.stats();
    assert!(stats.made >= 80 && stats.from_cpu == 0, "{stats:?}");
}

#[test]
fn a_colour_response_is_applied_as_the_cpu_applies_it() {
    let Some(cuda) = gpu() else { return };
    let response = [[0.9, 0.1, 0.0], [0.05, 0.75, 0.05], [0.0, 0.1, 0.5]];
    let mut gpu = GpuTiles::new(cuda, 64, response).unwrap();
    let cpu = TileCache::with_response(1 << 28, response);
    let tissue = HeTissue::new(1, Slide::COVERSLIP);
    let island = tissue.islands()[0];
    let (world, optics) = (world(tissue), optics("10x", 1.0));
    let place = places(island, &optics)[0];
    let on_gpu = gpu
        .tile(
            &world,
            &optics,
            Light::Transmitted,
            Um(FOCUS + 1.0),
            place,
            &cpu,
        )
        .unwrap();
    let on_cpu = cpu_tile(&cpu, &world, &optics, FOCUS + 1.0, place);
    let (worst, mean) = differences(&on_cpu, &on_gpu);
    assert!(
        worst <= 2f32.powi(-8) && mean <= 2f64.powi(-12),
        "up to {worst}, {mean} on average"
    );
}

#[test]
fn a_tile_is_the_same_to_the_bit_whichever_were_made_before() {
    let Some(_) = gpu() else { return };
    let cpu = TileCache::new(1 << 28);
    let tissue = HeTissue::new(1, Slide::COVERSLIP);
    let island = tissue.islands()[0];
    let (world, optics) = (world(tissue), optics("10x", 1.0));
    let (i, j) = places(island, &optics)[0];
    let z = Um(FOCUS + 1.0);
    // Alone; then after its neighbours, whose layers it shares.
    let mut alone = GpuTiles::new(gpu().unwrap(), 64, oscope_sim::camera::IDEAL).unwrap();
    let first = alone
        .tile(&world, &optics, Light::Transmitted, z, (i, j), &cpu)
        .unwrap();
    let mut after = GpuTiles::new(gpu().unwrap(), 64, oscope_sim::camera::IDEAL).unwrap();
    for place in [(i - 1, j), (i + 1, j + 1), (i, j)] {
        let tile = after
            .tile(&world, &optics, Light::Transmitted, z, place, &cpu)
            .unwrap();
        if place == (i, j) {
            assert!(
                first == tile,
                "the tile changed with what was made before it"
            );
        }
    }
}

#[test]
fn a_focus_change_only_blurs_again_and_a_new_slide_empties_the_layers() {
    let Some(_) = gpu() else { return };
    let cpu = TileCache::new(1 << 28);
    let tissue = HeTissue::new(1, Slide::COVERSLIP);
    let island = tissue.islands()[0];
    let (world, optics) = (world(tissue), optics("10x", 1.0));
    let place = places(island, &optics)[0];
    let tile = |gpu: &mut GpuTiles, world: &World, z: f64| {
        gpu.tile(world, &optics, Light::Transmitted, Um(z), place, &cpu)
            .unwrap()
    };
    let mut gpu_tiles = GpuTiles::new(gpu().unwrap(), 64, oscope_sim::camera::IDEAL).unwrap();
    tile(&mut gpu_tiles, &world, FOCUS + 1.0);
    let before = gpu_tiles.stats();
    // 3 µm higher: several depth buckets of 0.49 µm away.
    let refocused = tile(&mut gpu_tiles, &world, FOCUS + 4.0);
    let after = gpu_tiles.stats();
    assert_eq!(after.layers, before.layers, "the layers were made again");
    assert_eq!(after.made, before.made + 1);
    let mut fresh = GpuTiles::new(gpu().unwrap(), 64, oscope_sim::camera::IDEAL).unwrap();
    assert!(
        refocused == tile(&mut fresh, &world, FOCUS + 4.0),
        "a tile from the cached layers isn't one made from scratch"
    );
    // The same section on a new slide: its layers are made again.
    let again = self::world(HeTissue::new(1, Slide::COVERSLIP));
    tile(&mut gpu_tiles, &again, FOCUS + 4.0);
    assert_eq!(gpu_tiles.stats().layers, after.layers + 9);
}

#[test]
fn each_pixel_s_layers_are_its_own() {
    let Some(cuda) = gpu() else { return };
    // The same column and row through 10× and then 5× are different places
    // on the slide, a tile's side twice as far: the second must make its
    // own layers, not take the first's.
    let mut gpu = GpuTiles::new(cuda, 64, oscope_sim::camera::IDEAL).unwrap();
    let cpu = TileCache::new(1 << 28);
    let tissue = HeTissue::new(1, Slide::COVERSLIP);
    let island = tissue.islands()[0];
    let world = world(tissue);
    let place = places(island, &optics("10x", 1.0))[0];
    for objective in ["10x", "5x"] {
        let optics = optics(objective, 1.0);
        let z = FOCUS + 1.0;
        let on_gpu = gpu
            .tile(&world, &optics, Light::Transmitted, Um(z), place, &cpu)
            .unwrap();
        let on_cpu = cpu_tile(&cpu, &world, &optics, z, place);
        let (worst, mean) = differences(&on_cpu, &on_gpu);
        assert!(
            worst <= 2f32.powi(-8) && mean <= 2f64.powi(-12),
            "{objective}: up to {worst}, {mean} on average"
        );
    }
    assert_eq!(gpu.stats().layers, 18, "each pixel's layers made once");
}
