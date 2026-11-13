//! The camera's exposure on the GPU (P15): which cached tiles are in which
//! slots of `oscope-compute`'s atlas, and a frame's numbers for its kernel,
//! the twin of the camera's CPU path.

use std::collections::HashMap;
use std::sync::Arc;

use oscope_compute::cuda::{Cuda, Exposure, TileAtlas};
use oscope_compute::{ComputeError, Lane};
use oscope_core::Point2;

use crate::sensor::SensorModel;
use crate::tiles::{TILE, Tile, TiledField};

/// One frame, as the camera's CPU path would expose it.
pub(crate) struct GpuFrame<'a> {
    pub field: &'a TiledField,
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
}

/// The GPU, its atlas, and which tile each slot holds. A tile is known by
/// its `Arc`, which a slot keeps, so that no other tile can take its address
/// while it is there.
pub(crate) struct GpuTiles {
    cuda: Cuda,
    atlas: TileAtlas,
    held: Vec<Option<(Arc<Tile>, u64)>>,
    slot_of: HashMap<usize, usize>,
    clock: u64,
}

impl GpuTiles {
    pub fn new(cuda: Cuda, slots: usize) -> Result<Self, ComputeError> {
        let atlas = cuda.tile_atlas(Lane::Live, TILE, slots)?;
        Ok(Self {
            cuda,
            atlas,
            held: vec![None; slots],
            slot_of: HashMap::new(),
            clock: 0,
        })
    }

    /// The slot of each of `field`'s tiles, uploading those not yet there
    /// into free slots, or those least recently used by other frames.
    fn slots(&mut self, field: &TiledField) -> Result<Vec<i32>, ComputeError> {
        self.clock += 1;
        let now = self.clock;
        let mut grid = Vec::with_capacity(field.tiles().len());
        for tile in field.tiles() {
            let key = Arc::as_ptr(tile) as usize;
            let slot = match self.slot_of.get(&key) {
                Some(slot) => *slot,
                None => {
                    let slot = (0..self.held.len())
                        .min_by_key(|s| self.held[*s].as_ref().map_or(0, |(_, used)| *used))
                        .filter(|s| self.held[*s].as_ref().is_none_or(|(_, used)| *used < now))
                        .ok_or_else(|| {
                            ComputeError::Cuda("more tiles than the atlas holds".into())
                        })?;
                    if let Some((old, _)) = self.held[slot].take() {
                        self.slot_of.remove(&(Arc::as_ptr(&old) as usize));
                    }
                    self.cuda
                        .upload_tile(Lane::Live, &mut self.atlas, slot, &tile.data)?;
                    self.slot_of.insert(key, slot);
                    slot
                }
            };
            self.held[slot] = Some((Arc::clone(tile), now));
            grid.push(slot as i32);
        }
        Ok(grid)
    }

    /// The frame's counts, from the GPU.
    pub fn expose(&mut self, job: &GpuFrame<'_>) -> Result<Vec<u16>, ComputeError> {
        let grid = self.slots(job.field)?;
        let (i0, j0, columns, rows) = job.field.grid();
        let p = job.field.pixel();
        let t = f64::from(TILE);
        let corner = Point2::new(i0 as f64 * t * p, j0 as f64 * t * p);
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
            grid: &grid,
            columns: columns as u32,
            rows: rows as u32,
            pixel: p as f32,
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
        };
        self.cuda.expose(Lane::Live, &self.atlas, &exposure)
    }
}
