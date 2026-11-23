//! The camera's image on the GPU (P15, ADR-0062).
//!
//! - **Two atlases in VRAM,** in half precision. Each slot is known by what
//!   it holds, and the one least recently used goes first:
//!   - **tiles,** keyed as the CPU's tile cache keys them: the objective,
//!     the light, the depth bucket and the place;
//!   - **the H&E section's layers before their blur,** keyed by the pixel
//!     and the place, so that a change of focus only blurs again.
//! - **The H&E section's tiles are made here** in transmitted light, while
//!   every layer's blur is within [`FINEST_BLUR`]: layers, then each tile
//!   from its 3 × 3 neighbourhood's layers. Every other tile comes from the
//!   CPU's tile cache, converted to half precision.
//! - **A frame makes at most a budget of new tiles.** One that would need
//!   more is exposed from a level four times coarser, and its fine tiles
//!   wait for the time between frames, with the tiles ahead of the stage.

use std::collections::{BTreeSet, HashMap};
use std::hash::Hash;

use oscope_compute::cuda::{
    Blur, Cuda, Exposure, HeModel, LayerAtlas, LayerJob, PinnedAllocator, TileAtlas, TileBatch,
    TileGrid, TileJob,
};
use oscope_compute::{ComputeError, Lane};
use oscope_core::{Point2, Rect, Um, timing};

use crate::blur::{BOXES, BOXES_FROM, box_radii, kernel};
use crate::optics::Optics;
use crate::sample::{GpuModel, LayerKind};
use crate::sensor::SensorModel;
use crate::tiles::{Level, Plan, TILE, TileCache, WorldMark};
use crate::world::{FINEST_BLUR, Light, World, glass};

/// New tiles a frame makes at most, by default, before it falls back to the
/// coarse level.
pub const BUDGET: usize = 128;

/// How much coarser the coarse level's pixel is.
const COARSER: f64 = 4.0;

/// Tiles made in one batch: their layers' neighbourhoods must fit the
/// layers' atlas together.
const CHUNK: usize = 64;

/// Which key each slot of an atlas holds, and when it was last used.
struct Slots<K> {
    held: Vec<Option<(K, u64)>>,
    of: HashMap<K, usize>,
    free: Vec<usize>,
}

impl<K: Copy + Eq + Hash> Slots<K> {
    fn new(n: usize) -> Self {
        Self {
            held: vec![None; n],
            of: HashMap::new(),
            free: (0..n).rev().collect(),
        }
    }

    /// The slot holding `key`, now used at `now`.
    fn get(&mut self, key: &K, now: u64) -> Option<usize> {
        let slot = *self.of.get(key)?;
        if let Some((_, used)) = self.held[slot].as_mut() {
            *used = now;
        }
        Some(slot)
    }

    /// The slot of `key` if it is held, used at `now`, without claiming one.
    fn peek(&self, key: &K) -> Option<usize> {
        self.of.get(key).copied()
    }

    /// A slot for `key`, used at `now`: a free one, or the one least
    /// recently used before `keep`. None if every slot was used since.
    fn claim(&mut self, key: K, now: u64, keep: u64) -> Option<usize> {
        let slot = match self.free.pop() {
            Some(slot) => slot,
            None => {
                let oldest = (0..self.held.len())
                    .min_by_key(|s| self.held[*s].as_ref().map_or(0, |(_, used)| *used))?;
                if self.held[oldest]
                    .as_ref()
                    .is_some_and(|(_, used)| *used >= keep)
                {
                    return None;
                }
                oldest
            }
        };
        if let Some((old, _)) = self.held[slot].take() {
            self.of.remove(&old);
        }
        self.held[slot] = Some((key, now));
        self.of.insert(key, slot);
        Some(slot)
    }

    fn clear(&mut self) {
        self.held.iter_mut().for_each(|slot| *slot = None);
        self.of.clear();
        self.free = (0..self.held.len()).rev().collect();
    }

    fn len(&self) -> usize {
        self.of.len()
    }
}

/// A region of the stage to have tiles for.
#[derive(Clone)]
pub(crate) struct Request {
    pub world: World,
    pub optics: Optics,
    pub light: Light,
    pub z: Um,
    pub region: Rect,
}

/// A field of tiles in the atlas, for the exposure.
pub(crate) struct GpuField {
    grid: Vec<i32>,
    columns: u32,
    rows: u32,
    corner: Point2,
    pixel: f64,
}

/// One frame, as the camera's CPU path would expose it.
pub(crate) struct GpuFrame<'a> {
    /// Each row's places of the stage during its exposure.
    pub rows: &'a [Vec<Point2>],
    /// Pixel (u, v) sees a place plus (a u + b v + c, d u + e v + f).
    pub affine: [f64; 6],
    pub size: (u32, u32),
    /// Electrons for light of 1.
    pub scale: f64,
    pub exposure_s: f64,
    /// Pixels binned into each.
    pub pixels: u32,
    pub gain: f64,
    pub sensor: SensorModel,
    pub seed: u64,
    pub frame: u64,
    /// The vignetting: x0, y0, the step, the half diagonal's square, s.
    pub vignette: [f64; 5],
    /// Bytes a sample: 2 or 1.
    pub bytes: u32,
}

/// How the GPU's tiles have done since they were set up.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GpuStats {
    /// Tiles made from the H&E section's layers.
    pub made: u64,
    /// Layer tiles made.
    pub layers: u64,
    /// Tiles taken from the CPU's cache.
    pub from_cpu: u64,
    /// Frames exposed from the coarse level.
    pub coarse_frames: u64,
    /// The most new tiles one frame has made.
    pub most_in_a_frame: usize,
    /// The new tiles the latest frame made: none when the time between
    /// frames made them ahead.
    pub last_frame: usize,
    /// Tiles held now.
    pub tiles: usize,
    /// Layer tiles held now.
    pub layer_tiles: usize,
}

/// The tiles and layers in the GPU's memory, and the work waiting for the
/// time between frames.
pub struct GpuTiles {
    cuda: Cuda,
    tiles: TileAtlas,
    tile_slots: Slots<(Level, i64, i64)>,
    layers: LayerAtlas,
    layer_slots: Slots<(u64, i64, i64)>,
    response: Option<[f32; 9]>,
    /// What the tiles were made from, and the layers.
    world: Option<WorldMark>,
    sample: Option<WorldMark>,
    budget: usize,
    clock: u64,
    /// The fine tiles a coarse frame left.
    unfinished: Option<Request>,
    /// The tiles where the next frame will look.
    ahead: Option<Request>,
    stats: GpuStats,
}

/// `optics`, its pixel `COARSER` times larger.
fn coarser(optics: &Optics) -> Optics {
    Optics {
        sensor_pixel: optics.sensor_pixel * COARSER,
        ..*optics
    }
}

/// A layer's blur of `sigma` pixels, as `blur::gaussian` blurs it.
fn blur(sigma: f64) -> Blur {
    if sigma < 0.2 {
        Blur::None
    } else if sigma < BOXES_FROM {
        Blur::Weights(kernel(sigma))
    } else {
        Blur::Boxes(
            box_radii(sigma, BOXES)
                .into_iter()
                .map(|r| r as u32)
                .collect(),
        )
    }
}

/// Tiles made together: their margin and their two blurs' bits, and their
/// places.
type Group = ((u32, u64, u64), Vec<(i64, i64)>);

/// The H&E model, if `world`'s sample has one, in two absorbing layers, and
/// `light` is transmitted.
fn he_model(world: &World, light: Light) -> Option<GpuModel> {
    let slide = world.slide.as_ref()?;
    let layers = slide.sample.layers();
    let absorbing = layers.len() == 2 && layers.iter().all(|l| l.kind == LayerKind::Absorbing);
    let model = slide.sample.gpu_model()?;
    let he = matches!(model, GpuModel::HeTissue { .. });
    (he && absorbing && light == Light::Transmitted).then_some(model)
}

impl GpuTiles {
    /// Tiles on `cuda`'s GPU: `slots` tiles, and `slots` layer tiles, for a
    /// sensor of colour `response` (P33).
    pub fn new(cuda: Cuda, slots: usize, response: [[f64; 3]; 3]) -> Result<Self, ComputeError> {
        let tiles = cuda.tile_atlas(Lane::Live, TILE, slots)?;
        let layers = cuda.layer_atlas(Lane::Live, TILE, slots)?;
        let ideal = response == crate::camera::IDEAL;
        let flat: Vec<f32> = response.iter().flatten().map(|v| *v as f32).collect();
        let response = (!ideal).then(|| flat.try_into().expect("nine numbers"));
        Ok(Self {
            cuda,
            tiles,
            tile_slots: Slots::new(slots),
            layers,
            layer_slots: Slots::new(slots),
            response,
            world: None,
            sample: None,
            budget: BUDGET,
            clock: 0,
            unfinished: None,
            ahead: None,
            stats: GpuStats::default(),
        })
    }

    /// Pinned host memory on its GPU, for the frames it copies back.
    pub fn pinned_allocator(&self) -> PinnedAllocator {
        self.cuda.pinned_allocator()
    }

    /// The most new tiles a frame makes: [`BUDGET`] by default.
    pub fn set_budget(&mut self, tiles: usize) {
        self.budget = tiles;
    }

    /// How its tiles have done.
    pub fn stats(&self) -> GpuStats {
        GpuStats {
            tiles: self.tile_slots.len(),
            layer_tiles: self.layer_slots.len(),
            ..self.stats
        }
    }

    /// The tile at column `i` and row `j` of `world` through `optics` in
    /// `light`, with the stage at `z`, as a frame would have it: made if
    /// missing, with `cpu` for the tiles the GPU doesn't make. For tests.
    #[allow(clippy::too_many_arguments)]
    pub fn tile(
        &mut self,
        world: &World,
        optics: &Optics,
        light: Light,
        z: Um,
        (i, j): (i64, i64),
        cpu: &TileCache,
    ) -> Result<Vec<f32>, ComputeError> {
        let p = optics.pixel_size().0;
        let t = f64::from(TILE);
        let region = Rect::new(
            (i as f64 * t + 10.0) * p,
            (j as f64 * t + 10.0) * p,
            100.0 * p,
            100.0 * p,
        );
        let request = Request {
            world: world.clone(),
            optics: *optics,
            light,
            z,
            region,
        };
        self.clock += 1;
        let now = self.clock;
        self.mark(world);
        let plan = Plan::new(world, optics, light, z, region);
        let missing = self.missing(&plan, now);
        self.make(&request, &plan, &missing, cpu, now)?;
        let slot = self
            .tile_slots
            .get(&(plan.level, i, j), now)
            .ok_or_else(|| ComputeError::Cuda("the tile wasn't made".to_owned()))?;
        self.cuda.download_tile(Lane::Live, &self.tiles, slot)
    }

    /// Forgets the tiles made from another world, and the layers made from
    /// another sample or slide.
    fn mark(&mut self, world: &World) {
        let mark = WorldMark::of(world);
        if !self.world.as_ref().is_some_and(|w| w.same(&mark)) {
            self.tile_slots.clear();
            self.world = Some(mark.clone());
            self.unfinished = None;
            self.ahead = None;
        }
        if !self.sample.as_ref().is_some_and(|w| w.same_sample(&mark)) {
            self.layer_slots.clear();
            self.sample = Some(mark);
        }
    }

    /// `plan`'s tiles not in the atlas; those that are, used at `now`.
    fn missing(&mut self, plan: &Plan, now: u64) -> Vec<(i64, i64)> {
        let places = plan.places();
        places
            .into_iter()
            .filter(|&(i, j)| self.tile_slots.get(&(plan.level, i, j), now).is_none())
            .collect()
    }

    /// The tiles of `request`'s region in the atlas, `missing` made: at most
    /// the budget of them, or else the coarse level's.
    pub(crate) fn field(
        &mut self,
        request: &Request,
        cpu: &TileCache,
    ) -> Result<GpuField, ComputeError> {
        self.clock += 1;
        let now = self.clock;
        self.mark(&request.world);
        let Request {
            world,
            optics,
            light,
            z,
            region,
        } = request;
        let mut plan = Plan::new(world, optics, *light, *z, *region);
        let mut missing = self.missing(&plan, now);
        if missing.len() > self.budget {
            // The fine tiles wait for the time between frames.
            self.unfinished = Some(request.clone());
            self.stats.coarse_frames += 1;
            let coarse = Request {
                optics: coarser(optics),
                ..request.clone()
            };
            plan = Plan::new(world, &coarse.optics, *light, *z, *region);
            missing = self.missing(&plan, now);
            timing::time("camera.coarse", || {
                self.make(&coarse, &plan, &missing, cpu, now)
            })?;
        } else {
            self.make(request, &plan, &missing, cpu, now)?;
        }
        self.stats.most_in_a_frame = self.stats.most_in_a_frame.max(missing.len());
        self.stats.last_frame = missing.len();
        let grid: Option<Vec<i32>> = plan
            .places()
            .iter()
            .map(|&(i, j)| self.tile_slots.get(&(plan.level, i, j), now))
            .map(|slot| slot.map(|s| s as i32))
            .collect();
        let grid =
            grid.ok_or_else(|| ComputeError::Cuda("a frame's tile wasn't made".to_owned()))?;
        let t = TILE as i64;
        let p = plan.pixel;
        Ok(GpuField {
            grid,
            columns: plan.columns() as u32,
            rows: plan.rows() as u32,
            corner: Point2::new((plan.i0 * t) as f64 * p, (plan.j0 * t) as f64 * p),
            pixel: p,
        })
    }

    /// Where the next frame will look: its tiles are made between frames.
    pub(crate) fn ahead(&mut self, request: Option<Request>) {
        self.ahead = request;
    }

    /// Makes up to a batch of the tiles that wait: the fine tiles a coarse
    /// frame left, then those ahead of the stage. Whether any wait still.
    pub(crate) fn work(&mut self, cpu: &TileCache) -> Result<bool, ComputeError> {
        for ahead in [false, true] {
            let waiting = match ahead {
                false => self.unfinished.take(),
                true => self.ahead.take(),
            };
            let Some(request) = waiting else { continue };
            // A request from a world that has since changed is dropped.
            let current = WorldMark::of(&request.world);
            if !self.world.as_ref().is_some_and(|w| w.same(&current)) {
                continue;
            }
            self.clock += 1;
            let now = self.clock;
            let Request {
                world,
                optics,
                light,
                z,
                region,
            } = &request;
            let plan = Plan::new(world, optics, *light, *z, *region);
            let mut missing = self.missing(&plan, now);
            if missing.is_empty() {
                continue;
            }
            missing.truncate(CHUNK);
            self.make(&request, &plan, &missing, cpu, now)?;
            match ahead {
                false => self.unfinished = Some(request),
                true => self.ahead = Some(request),
            }
            return Ok(true);
        }
        Ok(false)
    }

    /// Makes `missing` tiles of `plan`: on the GPU from the H&E section's
    /// layers when it can, else from the CPU's cache.
    fn make(
        &mut self,
        request: &Request,
        plan: &Plan,
        missing: &[(i64, i64)],
        cpu: &TileCache,
        now: u64,
    ) -> Result<(), ComputeError> {
        if missing.is_empty() {
            return Ok(());
        }
        let Request {
            world,
            optics,
            light,
            ..
        } = request;
        // Every layer's blur within what the GPU's tiles render.
        let blurs: Vec<_> = missing
            .iter()
            .map(|&(i, j)| world.blurs(&plan.field(i, j), optics, *light))
            .collect();
        let fine = blurs
            .iter()
            .all(|b| b.sigmas.iter().all(|s| *s <= FINEST_BLUR));
        match he_model(world, *light) {
            Some(model) if fine => {
                let mut groups: Vec<Group> = Vec::new();
                for (place, b) in missing.iter().zip(&blurs) {
                    let key = (b.margin, b.sigmas[0].to_bits(), b.sigmas[1].to_bits());
                    match groups.iter_mut().find(|(k, _)| *k == key) {
                        Some((_, places)) => places.push(*place),
                        None => groups.push((key, vec![*place])),
                    }
                }
                for ((margin, low, high), places) in groups {
                    let sigmas = [f64::from_bits(low), f64::from_bits(high)];
                    for chunk in places.chunks(CHUNK) {
                        self.he_tiles(request, plan, &model, margin, sigmas, chunk, now)?;
                    }
                }
                Ok(())
            }
            _ => self.cpu_tiles(request, plan, missing, cpu, now),
        }
    }

    /// Makes `places` on the GPU, from their neighbourhoods' layers, all
    /// with the same margin and blurs.
    #[allow(clippy::too_many_arguments)]
    fn he_tiles(
        &mut self,
        request: &Request,
        plan: &Plan,
        model: &GpuModel,
        margin: u32,
        sigmas: [f64; 2],
        places: &[(i64, i64)],
        now: u64,
    ) -> Result<(), ComputeError> {
        let slide = request
            .world
            .slide
            .as_ref()
            .ok_or_else(|| ComputeError::Cuda("H&E tiles without a slide".to_owned()))?;
        let grid = TileGrid {
            pixel: plan.pixel,
            slide: [slide.origin.x, slide.origin.y],
        };
        let t = TILE as i64;
        let pixel = plan.pixel.to_bits();
        // This batch's layers stay while it uses them; earlier batches' may
        // go to make room.
        self.clock += 1;
        let tick = self.clock;
        let needed: BTreeSet<(i64, i64)> = places
            .iter()
            .flat_map(|&(i, j)| {
                (-1..=1).flat_map(move |dj| (-1..=1).map(move |di| (i + di, j + dj)))
            })
            .collect();
        let mut jobs = Vec::new();
        for &(i, j) in &needed {
            let key = (pixel, i, j);
            if self.layer_slots.get(&key, tick).is_some() {
                continue;
            }
            let slot = self.layer_slots.claim(key, tick, tick).ok_or_else(|| {
                ComputeError::Cuda("more layers than their atlas holds".to_owned())
            })?;
            jobs.push(LayerJob {
                origin: [i * t, j * t],
                slot,
            });
        }
        let GpuModel::HeTissue {
            numbers,
            salts,
            islands,
            vectors,
        } = model;
        let model = HeModel {
            numbers,
            salts,
            islands: *islands,
        };
        timing::time("camera.gpu_layers", || {
            self.cuda
                .he_layers(Lane::Live, &mut self.layers, &model, grid, &jobs)
                .and_then(|()| self.synced())
        })?;
        self.stats.layers += jobs.len() as u64;
        let mut tiles = Vec::with_capacity(places.len());
        for &(i, j) in places {
            let mut neighbours = [-1_i32; 9];
            for (k, (di, dj)) in (-1..=1)
                .flat_map(|dj| (-1..=1).map(move |di| (di, dj)))
                .enumerate()
            {
                let slot = self.layer_slots.peek(&(pixel, i + di, j + dj));
                neighbours[k] = slot.map_or(-1, |s| s as i32);
            }
            let slot = self
                .tile_slots
                .claim((plan.level, i, j), tick, now)
                .ok_or_else(|| ComputeError::Cuda("more tiles than the atlas holds".to_owned()))?;
            tiles.push(TileJob {
                origin: [i * t, j * t],
                slot,
                neighbours,
            });
        }
        let batch = TileBatch {
            jobs: &tiles,
            margin,
            blurs: sigmas.map(blur),
            vectors: *vectors,
            glass: glass(slide),
            response: self.response,
        };
        timing::time("camera.gpu_tiles", || {
            self.cuda
                .he_tiles(Lane::Live, &self.layers, &mut self.tiles, grid, &batch)
                .and_then(|()| self.synced())
        })?;
        self.stats.made += places.len() as u64;
        Ok(())
    }

    /// With timing on, waits for the GPU to finish what it was given, so
    /// that each step is timed on its own; its launches otherwise return at
    /// once.
    fn synced(&self) -> Result<(), ComputeError> {
        match timing::enabled() {
            true => self.cuda.synchronize(Lane::Live),
            false => Ok(()),
        }
    }

    /// Takes `missing` tiles from the CPU's cache, which renders those it
    /// lacks, into the atlas.
    fn cpu_tiles(
        &mut self,
        request: &Request,
        plan: &Plan,
        missing: &[(i64, i64)],
        cpu: &TileCache,
        now: u64,
    ) -> Result<(), ComputeError> {
        let Request {
            world,
            optics,
            light,
            z,
            region,
        } = request;
        let field = timing::time("camera.cpu_tiles", || {
            cpu.field(world, optics, *light, *z, *region)
        });
        let columns = plan.columns();
        for &(i, j) in missing {
            let index = (j - plan.j0) as usize * columns + (i - plan.i0) as usize;
            let tile = &field.tiles()[index];
            let slot = self
                .tile_slots
                .claim((plan.level, i, j), now, now)
                .ok_or_else(|| ComputeError::Cuda("more tiles than the atlas holds".to_owned()))?;
            self.cuda
                .upload_tile(Lane::Live, &mut self.tiles, slot, &tile.data)?;
        }
        self.stats.from_cpu += missing.len() as u64;
        Ok(())
    }

    /// The frame's samples, packed, into `out`.
    pub(crate) fn expose(
        &mut self,
        field: &GpuField,
        job: &GpuFrame<'_>,
        out: &mut [u8],
    ) -> Result<(), ComputeError> {
        let corner = field.corner;
        let most = job.rows.iter().map(Vec::len).max().unwrap_or(1).max(1);
        let mut places = vec![0.0_f32; job.rows.len() * most * 2];
        for (v, row) in job.rows.iter().enumerate() {
            for (j, place) in row.iter().enumerate() {
                let at = (v * most + j) * 2;
                places[at] = (place.x - corner.x) as f32;
                places[at + 1] = (place.y - corner.y) as f32;
            }
        }
        let samples: Vec<u32> = job.rows.iter().map(|r| r.len() as u32).collect();
        let s = job.sensor;
        let n = f64::from(job.pixels.max(1));
        let exposure = Exposure {
            width: job.size.0,
            height: job.size.1,
            grid: &field.grid,
            columns: field.columns,
            rows: field.rows,
            pixel: field.pixel as f32,
            places: &places,
            samples: &samples,
            most: most as u32,
            affine: job.affine.map(|v| v as f32),
            scale: job.scale as f32,
            dark: (s.dark_current * job.exposure_s * n) as f32,
            read: (s.read_noise * n.sqrt()) as f32,
            black: f32::from(s.black),
            cpe: s.counts_per_electron(job.gain) as f32,
            noise: s.noise,
            seed: job.seed,
            frame: job.frame,
            vignette: job.vignette.map(|v| v as f32),
            bytes: job.bytes,
        };
        self.cuda.expose(Lane::Live, &self.tiles, &exposure, out)
    }
}
