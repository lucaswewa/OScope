//! The simulated camera's exposure on the GPU (P15): tiles of the rendered
//! world held in an atlas, and the kernel in `kernels/sim.cu` that turns
//! them into a frame's counts. `oscope-sim`'s camera holds which tile is in
//! which slot; this crate only moves the numbers and runs the kernel.

use cudarc::driver::{CudaSlice, PushKernelArg};

use super::{Cuda, grid};
use crate::{ComputeError, Lane};

/// Slots for tiles of `tile` + 1 pixels square of R, G and B, in GPU
/// memory.
#[derive(Debug)]
pub struct TileAtlas {
    data: CudaSlice<f32>,
    tile: u32,
    slots: usize,
}

impl TileAtlas {
    /// The tiles' side, without the overlapping pixel.
    pub fn tile(&self) -> u32 {
        self.tile
    }

    /// How many tiles it holds.
    pub fn slots(&self) -> usize {
        self.slots
    }

    fn slot_len(&self) -> usize {
        let side = self.tile as usize + 1;
        side * side * 3
    }
}

/// One frame's exposure of tiles in an atlas: see `kernels/sim.cu`.
#[derive(Clone, Debug)]
pub struct Exposure<'a> {
    /// The frame's width and height.
    pub width: u32,
    /// Its height.
    pub height: u32,
    /// The slot of each tile, `columns` × `rows`, row by row.
    pub grid: &'a [i32],
    /// The grid's columns.
    pub columns: u32,
    /// Its rows.
    pub rows: u32,
    /// The tiles' pixel at the sample, in µm.
    pub pixel: f32,
    /// For each row, `most` places of the stage, (x, y) in µm from the
    /// grid's corner, of which the first `samples[row]` count.
    pub places: &'a [f32],
    /// How many of its places each row averages over: 1 to `most`.
    pub samples: &'a [u32],
    /// The places held for each row.
    pub most: u32,
    /// Pixel (u, v) sees the place plus (a u + b v + c, d u + e v + f).
    pub affine: [f32; 6],
    /// Electrons for light of 1.
    pub scale: f32,
    /// Dark electrons, summed over binned pixels.
    pub dark: f32,
    /// Read noise, in electrons, summed over binned pixels.
    pub read: f32,
    /// The black level, in counts.
    pub black: f32,
    /// Counts an electron, at the gain.
    pub cpe: f32,
    /// Whether to add noise.
    pub noise: bool,
    /// The noise's seed.
    pub seed: u64,
    /// The frame's number, for its noise.
    pub frame: u64,
}

impl Cuda {
    /// An atlas of `slots` tiles of `tile` + 1 pixels square, zeroed.
    pub fn tile_atlas(
        &self,
        lane: Lane,
        tile: u32,
        slots: usize,
    ) -> Result<TileAtlas, ComputeError> {
        if tile == 0 || slots == 0 {
            return Err(ComputeError::Cuda("an atlas of no tiles".to_owned()));
        }
        let side = tile as usize + 1;
        let data = self
            .stream(lane)
            .alloc_zeros::<f32>(slots * side * side * 3)?;
        Ok(TileAtlas { data, tile, slots })
    }

    /// Copies a tile's samples into `slot`.
    pub fn upload_tile(
        &self,
        lane: Lane,
        atlas: &mut TileAtlas,
        slot: usize,
        samples: &[f32],
    ) -> Result<(), ComputeError> {
        let n = atlas.slot_len();
        if slot >= atlas.slots || samples.len() != n {
            return Err(ComputeError::Cuda(format!(
                "a tile of {} samples for slot {slot} of {} tiles of {n}",
                samples.len(),
                atlas.slots
            )));
        }
        let mut view = atlas.data.slice_mut(slot * n..(slot + 1) * n);
        Ok(self.stream(lane).memcpy_htod(samples, &mut view)?)
    }

    /// A frame's counts: the twin of `oscope-sim`'s camera on the CPU.
    pub fn expose(
        &self,
        lane: Lane,
        atlas: &TileAtlas,
        exposure: &Exposure<'_>,
    ) -> Result<Vec<u16>, ComputeError> {
        let e = exposure;
        let (width, height) = (e.width as usize, e.height as usize);
        let wrong = |what: &str| Err(ComputeError::Cuda(format!("an exposure with {what}")));
        if e.columns == 0 || e.rows == 0 || e.grid.len() != (e.columns * e.rows) as usize {
            return wrong("a grid of the wrong size");
        }
        if e.grid
            .iter()
            .any(|slot| *slot < 0 || *slot as usize >= atlas.slots)
        {
            return wrong("a slot outside the atlas");
        }
        if e.samples.len() != height || e.places.len() != height * e.most as usize * 2 {
            return wrong("places of the wrong size");
        }
        let no_pixel = e.pixel.is_nan() || e.pixel <= 0.0;
        if e.samples.iter().any(|n| *n == 0 || *n > e.most) || no_pixel {
            return wrong("a row of no places, or no pixel");
        }
        if width == 0 || height == 0 {
            return Ok(Vec::new());
        }
        let stream = self.stream(lane);
        let grid_slots = self.copy_up(lane, e.grid)?;
        let places = self.copy_up(lane, e.places)?;
        let samples = self.copy_up(lane, e.samples)?;
        let mut out = stream.alloc_zeros::<u16>(width * height)?;
        let [a, b, c, d, f_x, f_y] = e.affine;
        let noise = u32::from(e.noise);
        let mut launch = stream.launch_builder(&self.kernels.expose);
        launch
            .arg(&atlas.data)
            .arg(&grid_slots)
            .arg(&e.columns)
            .arg(&e.rows)
            .arg(&atlas.tile)
            .arg(&e.pixel)
            .arg(&places)
            .arg(&samples)
            .arg(&e.most);
        launch.arg(&a).arg(&b).arg(&c).arg(&d).arg(&f_x).arg(&f_y);
        launch.arg(&e.width).arg(&e.height);
        launch
            .arg(&e.scale)
            .arg(&e.dark)
            .arg(&e.read)
            .arg(&e.black)
            .arg(&e.cpe)
            .arg(&noise)
            .arg(&e.seed)
            .arg(&e.frame)
            .arg(&mut out);
        // SAFETY: the arguments are the kernel's, in kernels/sim.cu. Every
        // slot in `grid` is within the atlas, which holds `slots` tiles of
        // `tile` + 1 pixels square; the kernel clamps each position to the
        // grid's tiles, and reads at most one pixel past a tile's start,
        // which its overlap holds. `places` holds `height` rows of `most`
        // pairs, and each row reads its first `samples[row]` (1 to `most`).
        // `out` holds `width` × `height` counts, each thread writing its own.
        unsafe { launch.launch(grid(e.width, e.height)) }?;
        Ok(stream.clone_dtoh(&out)?)
    }
}
