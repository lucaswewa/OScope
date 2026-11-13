//! The cache of rendered tiles: tiles against the world rendered directly,
//! depth buckets, hits and misses as the stage moves, a slide changed, and
//! the memory budget.

mod common;

use std::sync::{Arc, PoisonError};
use std::time::Duration;

use common::{FOCUS, rig, set, start};
use oscope_core::{FrameBuffer, FrameMeta, Ms, Point2, Rect, Timestamp, Um};
use oscope_hal::{Axis, Camera, ObjectiveCatalogue, Stage, UmPerS};
use oscope_sim::tiles::TILE;
use oscope_sim::{Field, HeTissue, Light, Optics, Rate, SimCameraConfig, Slide, TileCache, World};

fn optics(name: &str) -> Optics {
    let catalogue = ObjectiveCatalogue::mitutoyo();
    Optics::new(&catalogue, catalogue.get(name).unwrap(), Um(2.4))
}

/// The H&E tissue, on a slide at the stage's origin, and an island of it.
fn tissue_world() -> (World, Point2) {
    let tissue = HeTissue::new(1, Slide::COVERSLIP);
    let island = tissue.islands()[0];
    let world = World::with_slide(
        Slide::new(Arc::new(tissue), Point2::new(0.0, 0.0)),
        Um(FOCUS),
    );
    (world, island)
}

/// The world rendered directly over `width` × `height` pixels from
/// `corner`, with the stage at `z`, against the cache's tiles sampled at the
/// same pixel centres: the largest difference.
fn largest_difference(
    cache: &TileCache,
    world: &World,
    o: &Optics,
    corner: Point2,
    z: f64,
    size: u32,
) -> f64 {
    let p = o.pixel_size().0;
    let span = f64::from(size) * p;
    let field = Field {
        centre: Point2::new(corner.x + span / 2.0, corner.y + span / 2.0),
        z: Um(z),
        width: size,
        height: size,
    };
    let direct = world.render(&field, o, Light::Transmitted);
    let tiles = cache.field(
        world,
        o,
        Light::Transmitted,
        Um(z),
        Rect::new(corner.x, corner.y, span, span),
    );
    (0..(size * size) as usize)
        .flat_map(|i| (0..3).map(move |c| (i, c)))
        .map(|(i, c)| {
            let (x, y) = ((i % size as usize) as f64, (i / size as usize) as f64);
            let cached = tiles.sample(corner.x + (x + 0.5) * p, corner.y + (y + 0.5) * p, c);
            (cached - f64::from(direct.data()[i * 3 + c])).abs()
        })
        .fold(0.0, f64::max)
}

#[test]
fn tiles_are_the_world_rendered_directly() {
    // A field across four tiles, on the pixel grid, at a bucket's middle:
    // the tiles hold the same pixels as one rendering of it.
    let (world, island) = tissue_world();
    let o = optics("20x");
    let p = o.pixel_size().0;
    let t = f64::from(TILE) * p;
    let corner = Point2::new(
        ((island.x / t).floor() + 0.5) * t,
        ((island.y / t).floor() + 0.5) * t,
    );
    let (_, depth) = TileCache::bucket_depth(&world, &o, Um(FOCUS + 2.0));
    let cache = TileCache::new(1 << 30);
    let worst = largest_difference(&cache, &world, &o, corner, depth.0, 300);
    assert!(worst < 1e-5, "{worst}");
}

#[test]
fn cached_rendering_matches_uncached_within_a_bucket() {
    // Anywhere in a bucket, a tile renders at its middle: at most a
    // thirty-second of the depth of field away. Through the 50×, where the
    // depth of field is shortest, the difference stays small.
    let (world, island) = tissue_world();
    let o = optics("50x");
    let step = o.depth_of_field().0 / 16.0;
    let cache = TileCache::new(1 << 30);
    for dz in [0.0, 0.3 * step, 0.49 * step, 5.4 * step] {
        let worst = largest_difference(&cache, &world, &o, island, FOCUS + 2.0 + dz, 160);
        assert!(worst < 0.01, "dz {dz}: {worst}");
    }
}

/// A frame's 12-bit counts.
fn counts((buffer, meta): (FrameBuffer, FrameMeta)) -> Vec<u16> {
    buffer.bytes()[..(meta.width * meta.height * 2) as usize]
        .chunks(2)
        .map(|b| u16::from_le_bytes([b[0], b[1]]) >> 4)
        .collect()
}

#[tokio::test]
async fn frames_reuse_tiles_and_a_jog_renders_only_new_ones() {
    // The scaled-down MIChrome 20 over the tissue, at 10×: 0.96 µm pixels,
    // so a tile is 246 µm across. The first frame renders its tiles, the
    // second finds them all. Jogging at 2 mm/s, 276 µm a frame, each frame
    // renders only the one or two columns of tiles that come into view.
    let (world, island) = tissue_world();
    let rig = rig(SimCameraConfig::quarter(), false, island, Rate::Stepped);
    *rig.camera
        .world()
        .write()
        .unwrap_or_else(PoisonError::into_inner) = world;
    set(&rig.camera, |s| s.exposure = Ms(1.0)).await;
    let frames = start(&rig.camera).await;
    let interval = Duration::from_secs_f64(1.0 / 7.24);
    let frame = |k: u32| {
        rig.clock
            .advance_to(Timestamp(0) + interval * k + Duration::from_millis(62));
        frames.recv_timeout(Duration::from_secs(60)).unwrap()
    };
    let _ = frame(0);
    let first = rig.camera.cache_stats();
    assert!(first.misses > 0 && first.hits == 0, "{first:?}");
    let _ = frame(1);
    let second = rig.camera.cache_stats();
    assert_eq!(
        (second.misses, second.hits),
        (first.misses, first.misses),
        "{second:?}"
    );
    rig.stage
        .jog(Axis::X, true, Some(UmPerS(2_000.0)))
        .await
        .unwrap();
    let mut misses = second.misses;
    for k in 2..5 {
        let _ = frame(k);
        let now = rig.camera.cache_stats();
        let new = now.misses - misses;
        assert!(
            new > 0 && new <= first.misses / 2,
            "frame {k}: {new} new tiles; {now:?}"
        );
        misses = now.misses;
    }
    rig.camera.stop().await.unwrap();
}

#[tokio::test]
async fn a_new_slide_is_never_seen_through_old_tiles() {
    // Two frames of the tissue, then the slide unloaded: the next frame
    // shows the empty stage, uniformly bright, not the cached tissue.
    let (world, island) = tissue_world();
    let mut config = SimCameraConfig::small(384, 288);
    config.sensor.noise = false;
    let rig = rig(config, false, island, Rate::Stepped);
    *rig.camera
        .world()
        .write()
        .unwrap_or_else(PoisonError::into_inner) = world;
    rig.camera.set_light(0.3, Light::Transmitted);
    set(&rig.camera, |s| s.exposure = Ms(10.0)).await;
    let frames = start(&rig.camera).await;
    let interval = Duration::from_secs_f64(1.0 / 7.24);
    let frame = |k: u32| {
        rig.clock
            .advance_to(Timestamp(0) + interval * k + Duration::from_millis(71));
        counts(frames.recv_timeout(Duration::from_secs(60)).unwrap())
    };
    let tissue = frame(0);
    assert_eq!(frame(1), tissue);
    rig.camera
        .world()
        .write()
        .unwrap_or_else(PoisonError::into_inner)
        .slide = None;
    let empty = frame(2);
    rig.camera.stop().await.unwrap();
    assert!(empty.iter().all(|c| *c == empty[0]), "not uniform");
    assert!(tissue.iter().any(|c| *c < empty[0] - 100));
}

#[test]
fn a_tile_is_kept_for_its_light_and_objective() {
    // The same place in transmitted light, then reflected; then 20× tiles
    // with the 10× ones' grid positions, at half the stage's coordinates:
    // each from its own tiles, as the world renders it directly.
    let (world, island) = tissue_world();
    let cache = TileCache::new(1 << 30);
    let size = 64;
    let half = Point2::new(island.x / 2.0, island.y / 2.0);
    for (o, light, at) in [
        (optics("10x"), Light::Transmitted, island),
        (optics("10x"), Light::Reflected, island),
        (optics("20x"), Light::Transmitted, half),
    ] {
        let p = o.pixel_size().0;
        let span = f64::from(size) * p;
        // On the pixel grid, so that each pixel centre is a tile's.
        let corner = Point2::new((at.x / p).floor() * p, (at.y / p).floor() * p);
        // In focus: the same depth bucket, 0, through either objective.
        let depth = Um(FOCUS);
        let field = Field {
            centre: Point2::new(corner.x + span / 2.0, corner.y + span / 2.0),
            z: depth,
            width: size,
            height: size,
        };
        let direct = world.render(&field, &o, light);
        let tiles = cache.field(
            &world,
            &o,
            light,
            depth,
            Rect::new(corner.x, corner.y, span, span),
        );
        for (i, v) in direct.data().iter().enumerate() {
            let (x, y, c) = ((i / 3) % size as usize, i / 3 / size as usize, i % 3);
            let cached = tiles.sample(
                corner.x + (x as f64 + 0.5) * p,
                corner.y + (y as f64 + 0.5) * p,
                c,
            );
            assert!(
                (cached - f64::from(*v)).abs() < 1e-5,
                "{light:?} through {:?}",
                o.magnification
            );
        }
    }
}

#[test]
fn the_cache_keeps_to_its_budget() {
    // Room for 20 tiles: fields of 9 tiles each, one after another, never
    // leave more than 20, nor fewer than the field in hand.
    let (world, island) = tissue_world();
    let o = optics("10x");
    let t = f64::from(TILE) * o.pixel_size().0;
    let tile_bytes = (TILE as usize + 1).pow(2) * 3 * 4;
    let cache = TileCache::new(20 * tile_bytes);
    for k in 0..6 {
        let corner = Point2::new(island.x + f64::from(k) * 3.0 * t, island.y);
        let field = cache.field(
            &world,
            &o,
            Light::Transmitted,
            Um(FOCUS),
            Rect::new(corner.x, corner.y, 2.5 * t, 2.5 * t),
        );
        let stats = cache.stats();
        assert!(
            stats.tiles <= 20 && stats.tiles >= field.tiles().len(),
            "{stats:?}"
        );
        assert_eq!(stats.bytes, stats.tiles * tile_bytes);
    }
    // The last field is still there: all hits.
    let before = cache.stats();
    let corner = Point2::new(island.x + 15.0 * t, island.y);
    cache.field(
        &world,
        &o,
        Light::Transmitted,
        Um(FOCUS),
        Rect::new(corner.x, corner.y, 2.5 * t, 2.5 * t),
    );
    assert_eq!(cache.stats().misses, before.misses);
}
