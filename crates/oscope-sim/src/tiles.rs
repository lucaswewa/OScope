//! A cache of rendered tiles of the world, so that the camera renders each
//! piece of a slide once for each objective, light and depth, not once a
//! frame (plan §5.10, §5.20; P15).

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};

use oscope_core::{Point2, Rect, Um};
use rayon::prelude::*;

use crate::optics::Optics;
use crate::sample::Sample;
use crate::world::{Field, Light, World};

/// A tile's side, in pixels.
pub const TILE: u32 = 256;

/// Each z bucket is this share of the depth of field: a tile renders at
/// its bucket's middle, at most a thirty-second of the depth of field from
/// where the stage is.
const BUCKETS_PER_DEPTH: f64 = 16.0;

/// Everything a tile's rendering depends on but its place: the optics, the
/// light, and the depth, as exact bits.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct Level {
    optics: [u64; 5],
    light: Light,
    bucket: i64,
}

/// One tile: the world's linear RGB over `TILE` + 1 pixels square, from
/// global pixel (`i` · `TILE`, `j` · `TILE`). The extra column and row
/// overlap the next tiles, so that interpolating never needs two tiles.
#[derive(Debug)]
pub struct Tile {
    /// Its column and row in the grid of tiles.
    pub i: i64,
    /// Its row.
    pub j: i64,
    /// Its samples, `TILE` + 1 rows of `TILE` + 1 pixels of R, G and B.
    pub data: Vec<f32>,
}

/// The tiles covering a region, ready to sample: what one frame sees.
#[derive(Clone, Debug)]
pub struct TiledField {
    pixel: f64,
    i0: i64,
    j0: i64,
    columns: usize,
    rows: usize,
    tiles: Vec<Arc<Tile>>,
}

impl TiledField {
    /// The pixel's side at the sample.
    pub fn pixel(&self) -> f64 {
        self.pixel
    }

    /// The grid position of the first tile, and how many columns and rows
    /// of tiles there are.
    pub fn grid(&self) -> (i64, i64, usize, usize) {
        (self.i0, self.j0, self.columns, self.rows)
    }

    /// The tiles, row by row.
    pub fn tiles(&self) -> &[Arc<Tile>] {
        &self.tiles
    }

    /// Channel `c` at stage point `(x, y)`, interpolated between the four
    /// nearest pixel centres. Global pixel (gx, gy) has its centre at
    /// ((gx + ½) p, (gy + ½) p). Outside the tiles, the nearest edge.
    pub fn sample(&self, x: f64, y: f64, c: usize) -> f64 {
        let t = i64::from(TILE);
        let side = TILE as usize + 1;
        let (fx, fy) = (x / self.pixel - 0.5, y / self.pixel - 0.5);
        let span_x = (self.i0 * t) as f64..=((self.i0 + self.columns as i64) * t - 1) as f64;
        let span_y = (self.j0 * t) as f64..=((self.j0 + self.rows as i64) * t - 1) as f64;
        let fx = fx.clamp(*span_x.start(), *span_x.end());
        let fy = fy.clamp(*span_y.start(), *span_y.end());
        let (gx, gy) = (fx.floor(), fy.floor());
        let (ax, ay) = (fx - gx, fy - gy);
        let (gx, gy) = (gx as i64, gy as i64);
        let (i, j) = (gx.div_euclid(t), gy.div_euclid(t));
        let tile = &self.tiles[(j - self.j0) as usize * self.columns + (i - self.i0) as usize];
        let (lx, ly) = ((gx - i * t) as usize, (gy - j * t) as usize);
        let at = |x: usize, y: usize| f64::from(tile.data[(y * side + x) * 3 + c]);
        let top = at(lx, ly) + (at(lx + 1, ly) - at(lx, ly)) * ax;
        let bottom = at(lx, ly + 1) + (at(lx + 1, ly + 1) - at(lx, ly + 1)) * ax;
        top + (bottom - top) * ay
    }
}

/// How the cache has done since it was made.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CacheStats {
    /// Tiles found in the cache.
    pub hits: u64,
    /// Tiles rendered.
    pub misses: u64,
    /// Tiles held now.
    pub tiles: usize,
    /// The bytes they hold.
    pub bytes: usize,
}

/// What the cached tiles were rendered from. When the world changes (a
/// slide loaded or unloaded, its place, the focus), every tile goes. The
/// sample is kept, so that a new one can't take its place at its address.
#[derive(Clone)]
struct WorldMark {
    sample: Option<Arc<dyn Sample>>,
    numbers: [u64; 7],
}

impl WorldMark {
    fn of(world: &World) -> Self {
        let slide = world.slide.as_ref();
        let c = slide.map(|s| s.coverslip);
        let origin = slide.map_or(Point2::new(0.0, 0.0), |s| s.origin);
        Self {
            sample: slide.map(|s| Arc::clone(&s.sample)),
            numbers: [
                origin.x.to_bits(),
                origin.y.to_bits(),
                c.map_or(0, |c| c.origin.x.to_bits()),
                c.map_or(0, |c| c.origin.y.to_bits()),
                c.map_or(0, |c| c.size.width.to_bits()),
                c.map_or(0, |c| c.size.height.to_bits()),
                world.focus.0.to_bits(),
            ],
        }
    }

    fn same(&self, other: &Self) -> bool {
        let samples = match (&self.sample, &other.sample) {
            (Some(a), Some(b)) => Arc::ptr_eq(a, b),
            (None, None) => true,
            _ => false,
        };
        samples && self.numbers == other.numbers
    }
}

struct Entry {
    tile: Arc<Tile>,
    used: u64,
}

struct State {
    world: Option<WorldMark>,
    tiles: HashMap<(Level, i64, i64), Entry>,
    clock: u64,
    stats: CacheStats,
}

/// Rendered tiles of the world, for each level: objective, light and depth
/// bucket. The least recently used go once they hold more than the budget.
pub struct TileCache {
    budget: usize,
    state: Mutex<State>,
}

/// The bytes a tile holds.
const TILE_BYTES: usize = (TILE as usize + 1) * (TILE as usize + 1) * 3 * 4;

impl TileCache {
    /// A cache holding up to `budget` bytes of tiles, and always at least
    /// the tiles of the field in hand.
    pub fn new(budget: usize) -> Self {
        Self {
            budget,
            state: Mutex::new(State {
                world: None,
                tiles: HashMap::new(),
                clock: 0,
                stats: CacheStats::default(),
            }),
        }
    }

    /// How it has done.
    pub fn stats(&self) -> CacheStats {
        self.state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .stats
    }

    /// The depth a stage at `z` renders at: the middle of its bucket, the
    /// world's focus being one bucket's middle.
    pub fn bucket_depth(world: &World, optics: &Optics, z: Um) -> (i64, Um) {
        let step = optics.depth_of_field().0 / BUCKETS_PER_DEPTH;
        let bucket = ((z.0 - world.focus.0) / step).round();
        (bucket as i64, Um(world.focus.0 + bucket * step))
    }

    /// The tiles of `world` covering `region` of the stage, through
    /// `optics` in `light`, with the stage at `z`: from the cache, and any
    /// missing rendered, in parallel.
    pub fn field(
        &self,
        world: &World,
        optics: &Optics,
        light: Light,
        z: Um,
        region: Rect,
    ) -> TiledField {
        let p = optics.pixel_size().0;
        let (bucket, depth) = Self::bucket_depth(world, optics, z);
        let level = Level {
            optics: [
                optics.na.to_bits(),
                optics.magnification.to_bits(),
                optics.sensor_pixel.0.to_bits(),
                optics.wavelength.0.to_bits(),
                optics.medium.to_bits(),
            ],
            light,
            bucket,
        };
        // The global pixels whose centres the region's interpolation needs,
        // and the tiles that hold them.
        let t = i64::from(TILE);
        let pixel = |v: f64| (v / p - 0.5).floor() as i64;
        let (gx0, gy0) = (pixel(region.origin.x), pixel(region.origin.y));
        let (gx1, gy1) = (
            pixel(region.origin.x + region.size.width) + 1,
            pixel(region.origin.y + region.size.height) + 1,
        );
        let (i0, j0) = (gx0.div_euclid(t), gy0.div_euclid(t));
        let (i1, j1) = ((gx1 - 1).div_euclid(t), (gy1 - 1).div_euclid(t));
        let places: Vec<(i64, i64)> = (j0..=j1)
            .flat_map(|j| (i0..=i1).map(move |i| (i, j)))
            .collect();

        let mark = WorldMark::of(world);
        let missing: Vec<(i64, i64)> = {
            let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
            if !state.world.as_ref().is_some_and(|w| w.same(&mark)) {
                state.tiles.clear();
                state.stats.tiles = 0;
                state.stats.bytes = 0;
                state.world = Some(mark);
            }
            state.clock += 1;
            let now = state.clock;
            let mut missing = Vec::new();
            for place in &places {
                match state.tiles.get_mut(&(level, place.0, place.1)) {
                    Some(entry) => entry.used = now,
                    None => missing.push(*place),
                }
            }
            state.stats.hits += (places.len() - missing.len()) as u64;
            state.stats.misses += missing.len() as u64;
            missing
        };

        // Each missing tile, rendered alone: a field of its pixels, which
        // the world renders as it would any other.
        let side = TILE + 1;
        let rendered: Vec<Arc<Tile>> = missing
            .par_iter()
            .map(|&(i, j)| {
                let corner = Point2::new((i * t) as f64 * p, (j * t) as f64 * p);
                let half = f64::from(side) * p / 2.0;
                let field = Field {
                    centre: Point2::new(corner.x + half, corner.y + half),
                    z: depth,
                    width: side,
                    height: side,
                };
                let image = world.render(&field, optics, light);
                Arc::new(Tile {
                    i,
                    j,
                    data: image.data().to_vec(),
                })
            })
            .collect();

        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        let now = state.clock;
        for tile in rendered {
            let key = (level, tile.i, tile.j);
            if state.tiles.insert(key, Entry { tile, used: now }).is_none() {
                state.stats.bytes += TILE_BYTES;
            }
        }
        let tiles: Vec<Arc<Tile>> = places
            .iter()
            .map(|&(i, j)| Arc::clone(&state.tiles[&(level, i, j)].tile))
            .collect();
        // Over the budget: the least recently used go, but never this
        // field's.
        if state.stats.bytes > self.budget {
            let mut old: Vec<((Level, i64, i64), u64)> = state
                .tiles
                .iter()
                .filter(|(_, e)| e.used < now)
                .map(|(k, e)| (*k, e.used))
                .collect();
            old.sort_by_key(|(_, used)| *used);
            for (key, _) in old {
                if state.stats.bytes <= self.budget {
                    break;
                }
                state.tiles.remove(&key);
                state.stats.bytes -= TILE_BYTES;
            }
        }
        state.stats.tiles = state.tiles.len();
        TiledField {
            pixel: p,
            i0,
            j0,
            columns: (i1 - i0 + 1) as usize,
            rows: (j1 - j0 + 1) as usize,
            tiles,
        }
    }
}
