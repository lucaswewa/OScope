//! The simulated camera on the GPU (P15, ADR-0062): atlases of tiles in half
//! precision; the H&E section's kernels, in `kernels/tissue.cu`, that make
//! them; and the exposure, in `kernels/sim.cu`, that turns them into a
//! frame. `oscope-sim` holds which tile is in which slot, and what each
//! tile is; this crate only moves the numbers and runs the kernels.

use cudarc::driver::{CudaSlice, LaunchConfig, PushKernelArg};

use super::{BLOCK, Cuda, grid};
use crate::{ComputeError, Lane};

/// The most scratch memory a batch of tiles takes, in bytes.
const SCRATCH: usize = 256 << 20;

/// Slots for tiles of `tile` + 1 pixels square of R, G and B, in half
/// precision, in GPU memory.
#[derive(Debug)]
pub struct TileAtlas {
    data: CudaSlice<u16>,
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

/// Slots for layer tiles of `tile` pixels square: for each pixel, the
/// lower and then the upper layer's two stains, in half precision
/// (`kernels/tissue.cu`).
#[derive(Debug)]
pub struct LayerAtlas {
    data: CudaSlice<u16>,
    tile: u32,
    slots: usize,
}

impl LayerAtlas {
    /// How many layer tiles it holds.
    pub fn slots(&self) -> usize {
        self.slots
    }
}

/// The H&E section as `kernels/tissue.cu` reads it: its numbers and salted
/// seeds, as `oscope-sim`'s `HeTissue::gpu_model` lays them out.
#[derive(Clone, Copy, Debug)]
pub struct HeModel<'a> {
    /// The numbers.
    pub numbers: &'a [f64],
    /// The salted seeds.
    pub salts: &'a [u64],
    /// How many islands the numbers hold.
    pub islands: u32,
}

/// Where tiles lie: the pixel's side at the sample, in µm, and the slide's
/// corner on the stage.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TileGrid {
    /// The pixel's side.
    pub pixel: f64,
    /// The slide's corner, x and y, in µm.
    pub slide: [f64; 2],
}

/// A layer tile to make: its first global pixel, x and y, and its slot.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayerJob {
    /// The global pixel at its corner.
    pub origin: [i64; 2],
    /// Its slot in the layers' atlas.
    pub slot: usize,
}

/// A tile to make from the layers of its 3 × 3 neighbourhood.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TileJob {
    /// The global pixel at its corner.
    pub origin: [i64; 2],
    /// Its slot in the tiles' atlas.
    pub slot: usize,
    /// The slots of its neighbourhood's layer tiles, row by row from the
    /// one above and to the left; −1 for none, which reads as no stain.
    pub neighbours: [i32; 9],
}

/// How a layer is blurred, as `oscope-sim`'s `blur::gaussian` blurs it.
#[derive(Clone, Debug, PartialEq)]
pub enum Blur {
    /// Not at all.
    None,
    /// By a kernel of these weights, along each axis in turn.
    Weights(Vec<f32>),
    /// By box filters of these radii, along each axis in turn.
    Boxes(Vec<u32>),
}

/// Tiles to make together, with the same margin and blurs.
#[derive(Clone, Debug)]
pub struct TileBatch<'a> {
    /// The tiles.
    pub jobs: &'a [TileJob],
    /// The pixels around each tile that its blur reads: at most the layer
    /// tiles' side.
    pub margin: u32,
    /// The lower layer's blur, then the upper's.
    pub blurs: [Blur; 2],
    /// The stains' vectors: hematoxylin's R, G and B, then the second
    /// stain's.
    pub vectors: [f32; 6],
    /// The glass: the slide's rectangle (x, y, width, height) on the slide,
    /// the coverslip's, a surface's transmittance, an edge's, and the
    /// width of an edge's band.
    pub glass: [f64; 11],
    /// The sensor's colour response, by rows, if it isn't the identity.
    pub response: Option<[f32; 9]>,
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
    /// The vignetting (P33): pixel (u, v) lies (x0 + k u, y0 + k v) sensor
    /// pixels from the sensor's centre, over the half diagonal's square
    /// `r2`, with s its strength: `[x0, y0, k, r2, s]`.
    pub vignette: [f32; 5],
    /// Bytes a sample: 2 for 16 bits aligned high, 1 for 8 bits.
    pub bytes: u32,
}

/// A launch over `count` images of `width` × `height` pixels, a thread for
/// each pixel, in blocks of 16 × 16.
fn pixels(width: u32, height: u32, count: usize) -> LaunchConfig {
    LaunchConfig {
        grid_dim: (width.div_ceil(16), height.div_ceil(16), count as u32),
        block_dim: (16, 16, 1),
        shared_mem_bytes: 0,
    }
}

/// The most images a launch takes: its grid's depth.
const MOST_IMAGES: usize = 65_535;

/// A half-precision number's value.
pub fn half_to_f32(bits: u16) -> f32 {
    let sign = if bits & 0x8000 == 0 { 1.0 } else { -1.0 };
    let exponent = i32::from((bits >> 10) & 0x1f);
    let fraction = f32::from(bits & 0x3ff);
    sign * match exponent {
        0 => fraction * 2f32.powi(-24),
        31 if fraction == 0.0 => f32::INFINITY,
        31 => f32::NAN,
        _ => (1.0 + fraction / 1024.0) * 2f32.powi(exponent - 15),
    }
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
            .alloc_zeros::<u16>(slots * side * side * 3)?;
        Ok(TileAtlas { data, tile, slots })
    }

    /// An atlas of `slots` layer tiles of `tile` pixels square, zeroed.
    pub fn layer_atlas(
        &self,
        lane: Lane,
        tile: u32,
        slots: usize,
    ) -> Result<LayerAtlas, ComputeError> {
        if tile == 0 || slots == 0 {
            return Err(ComputeError::Cuda("an atlas of no layers".to_owned()));
        }
        let n = slots * tile as usize * tile as usize * 4;
        let data = self.stream(lane).alloc_zeros::<u16>(n)?;
        Ok(LayerAtlas { data, tile, slots })
    }

    /// Copies a tile's samples into `slot`, in half precision.
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
        let staged = self.copy_up(lane, samples)?;
        let mut view = atlas.data.slice_mut(slot * n..(slot + 1) * n);
        let count = n as u64;
        let mut launch = self.stream(lane).launch_builder(&self.kernels.to_half);
        launch.arg(&staged).arg(&count).arg(&mut view);
        let config = LaunchConfig {
            grid_dim: ((n as u32).div_ceil(BLOCK), 1, 1),
            block_dim: (BLOCK, 1, 1),
            shared_mem_bytes: 0,
        };
        // SAFETY: the arguments are the kernel's, in kernels/tissue.cu:
        // `staged` holds `n` floats, and `view` is one slot of `n` halves.
        // Each thread writes one of its own, below `n`.
        unsafe { launch.launch(config) }?;
        Ok(())
    }

    /// The samples of the tile in `slot`, as floats: for tests.
    pub fn download_tile(
        &self,
        lane: Lane,
        atlas: &TileAtlas,
        slot: usize,
    ) -> Result<Vec<f32>, ComputeError> {
        let n = atlas.slot_len();
        if slot >= atlas.slots {
            return Err(ComputeError::Cuda(format!("no slot {slot}")));
        }
        let view = atlas.data.slice(slot * n..(slot + 1) * n);
        let bits = self.stream(lane).clone_dtoh(&view)?;
        Ok(bits.into_iter().map(half_to_f32).collect())
    }

    /// Makes the H&E section's layer tiles `jobs` into `atlas`, at `grid`'s
    /// pixel: `kernels/tissue.cu`'s `he_layers`.
    pub fn he_layers(
        &self,
        lane: Lane,
        atlas: &mut LayerAtlas,
        model: &HeModel<'_>,
        grid: TileGrid,
        jobs: &[LayerJob],
    ) -> Result<(), ComputeError> {
        if jobs.iter().any(|job| job.slot >= atlas.slots) {
            return Err(ComputeError::Cuda(
                "a layer job outside the atlas".to_owned(),
            ));
        }
        let islands = 16 + model.islands as usize * 6;
        if model.numbers.len() < islands || model.salts.len() < 20 {
            return Err(ComputeError::Cuda("an H&E model too short".to_owned()));
        }
        let numbers = self.copy_up(lane, model.numbers)?;
        let salts = self.copy_up(lane, model.salts)?;
        let tile = atlas.tile;
        for chunk in jobs.chunks(MOST_IMAGES) {
            let origins: Vec<i64> = chunk.iter().flat_map(|job| job.origin).collect();
            let slots: Vec<u32> = chunk.iter().map(|job| job.slot as u32).collect();
            let origins = self.copy_up(lane, &origins)?;
            let slots = self.copy_up(lane, &slots)?;
            let [slide_x, slide_y] = grid.slide;
            let mut launch = self.stream(lane).launch_builder(&self.kernels.he_layers);
            launch.arg(&numbers).arg(&salts).arg(&model.islands);
            launch.arg(&grid.pixel).arg(&slide_x).arg(&slide_y);
            launch
                .arg(&origins)
                .arg(&slots)
                .arg(&tile)
                .arg(&mut atlas.data);
            // SAFETY: the arguments are the kernel's, in kernels/tissue.cu.
            // `numbers` holds the islands it reads, and `salts` its 20
            // seeds, checked above; `origins` and `slots` hold one entry a
            // job, and every slot is within the atlas, so each thread
            // writes its own pixel of its own slot.
            unsafe { launch.launch(pixels(tile, tile, chunk.len())) }?;
        }
        Ok(())
    }

    /// Makes tiles from layer tiles: each tile's neighbourhood gathered as
    /// transmittances, each layer blurred, the layers multiplied, then the
    /// glass and the colour response, as `oscope-sim`'s world renders them.
    pub fn he_tiles(
        &self,
        lane: Lane,
        layers: &LayerAtlas,
        atlas: &mut TileAtlas,
        grid: TileGrid,
        batch: &TileBatch<'_>,
    ) -> Result<(), ComputeError> {
        let tile = atlas.tile;
        // A margin under a tile reads only the 3 × 3 neighbourhood.
        if layers.tile != tile || batch.margin >= tile {
            return Err(ComputeError::Cuda(
                "a tile batch whose margin or layers don't fit its tiles".to_owned(),
            ));
        }
        let outside = |slot: i32| slot >= layers.slots as i32;
        let wrong = batch
            .jobs
            .iter()
            .any(|job| job.slot >= atlas.slots || job.neighbours.iter().any(|slot| outside(*slot)));
        if wrong {
            return Err(ComputeError::Cuda(
                "a tile job outside its atlas".to_owned(),
            ));
        }
        let width = tile + 1 + 2 * batch.margin;
        let plane = width as usize * width as usize * 3;
        let per_job = 2 * 2 * plane * std::mem::size_of::<f32>();
        let stream = self.stream(lane);
        let vectors = self.copy_up(lane, &batch.vectors)?;
        let glass = self.copy_up(lane, &batch.glass)?;
        let respond = u32::from(batch.response.is_some());
        let response = self.copy_up(lane, &batch.response.unwrap_or([0.0; 9]))?;
        let mut weights: [Option<CudaSlice<f32>>; 2] = [None, None];
        for (layer, blur) in batch.blurs.iter().enumerate() {
            match blur {
                Blur::Weights(w) if w.is_empty() => {
                    return Err(ComputeError::Cuda("a blur of no weights".to_owned()));
                }
                Blur::Weights(w) => weights[layer] = Some(self.copy_up(lane, w)?),
                _ => {}
            }
        }
        let chunk = (SCRATCH / per_job).clamp(1, MOST_IMAGES);
        for jobs in batch.jobs.chunks(chunk) {
            let n = jobs.len();
            let neighbours: Vec<i32> = jobs.iter().flat_map(|job| job.neighbours).collect();
            let origins: Vec<i64> = jobs.iter().flat_map(|job| job.origin).collect();
            let slots: Vec<u32> = jobs.iter().map(|job| job.slot as u32).collect();
            let neighbours = self.copy_up(lane, &neighbours)?;
            let origins = self.copy_up(lane, &origins)?;
            let slots = self.copy_up(lane, &slots)?;
            // SAFETY: `gather_layers` writes every sample of every plane of `a`
            // before anything reads it, and each pass of a blur writes every
            // sample of its layer's planes of its output before the next
            // pass reads them.
            let mut a = unsafe { stream.alloc::<f32>(n * 2 * plane) }?;
            // SAFETY: as above.
            let mut b = unsafe { stream.alloc::<f32>(n * 2 * plane) }?;
            let mut launch = stream.launch_builder(&self.kernels.gather_layers);
            launch
                .arg(&layers.data)
                .arg(&neighbours)
                .arg(&tile)
                .arg(&width);
            launch.arg(&batch.margin).arg(&vectors).arg(&mut a);
            // SAFETY: the arguments are the kernel's, in kernels/tissue.cu:
            // `neighbours` holds 9 slots a job, each within the layers'
            // atlas or −1, and a margin of at most a tile reads only those
            // 9 tiles. `a` holds 2 planes of `width`² pixels of 3 a job.
            unsafe { launch.launch(pixels(width, width, n)) }?;
            for layer in 0..2_u32 {
                let passes: Vec<(u32, Option<u32>)> = match &batch.blurs[layer as usize] {
                    Blur::None => Vec::new(),
                    Blur::Weights(_) => vec![(0, None), (1, None)],
                    Blur::Boxes(radii) => [0, 1]
                        .into_iter()
                        .flat_map(|axis| radii.iter().map(move |r| (axis, Some(*r))))
                        .collect(),
                };
                // Each pass from one buffer to the other; an even number of
                // passes, as many along each axis, leaves the result in `a`.
                for (k, (columns, radius)) in passes.into_iter().enumerate() {
                    let (from, to) = match k % 2 {
                        0 => (&a, &mut b),
                        _ => (&b, &mut a),
                    };
                    match (radius, &weights[layer as usize]) {
                        (Some(radius), _) => {
                            let mut launch = stream.launch_builder(&self.kernels.box_line);
                            launch.arg(from).arg(to).arg(&width).arg(&layer);
                            launch.arg(&columns).arg(&radius);
                            let config = LaunchConfig {
                                grid_dim: (width.div_ceil(BLOCK), 3, n as u32),
                                block_dim: (BLOCK, 1, 1),
                                shared_mem_bytes: 0,
                            };
                            // SAFETY: the arguments are the kernel's: each
                            // thread runs along its own line and channel of
                            // its own job's plane, reading `from` and
                            // writing `to`, both of `n` pairs of planes.
                            unsafe { launch.launch(config) }?;
                        }
                        (None, Some(weights)) => {
                            let taps = weights.len() as u32;
                            let mut launch = stream.launch_builder(&self.kernels.blur_line);
                            launch.arg(from).arg(to).arg(&width).arg(&layer);
                            launch.arg(&columns).arg(weights).arg(&taps);
                            // SAFETY: the arguments are the kernel's: each
                            // thread writes its own pixel of its job's
                            // plane in `to`, reading `taps` weights and,
                            // clamped to the plane, pixels of `from`.
                            unsafe { launch.launch(pixels(width, width, n)) }?;
                        }
                        (None, None) => {
                            return Err(ComputeError::Cuda("a blur without weights".to_owned()));
                        }
                    }
                }
            }
            let [slide_x, slide_y] = grid.slide;
            let mut launch = stream.launch_builder(&self.kernels.compose);
            launch
                .arg(&a)
                .arg(&width)
                .arg(&batch.margin)
                .arg(&origins)
                .arg(&slots);
            launch.arg(&grid.pixel).arg(&slide_x).arg(&slide_y);
            launch.arg(&glass).arg(&response).arg(&respond).arg(&tile);
            launch.arg(&mut atlas.data);
            // SAFETY: the arguments are the kernel's: `a` holds each job's
            // two planes, read inside their margin; `origins` and `slots`
            // one entry a job, each slot within the atlas, so each thread
            // writes its own pixel of its own slot; `glass` holds 11
            // numbers and `response` 9.
            unsafe { launch.launch(pixels(tile + 1, tile + 1, n)) }?;
        }
        Ok(())
    }

    /// A frame's samples, packed as `exposure.bytes` says, into `out`: the
    /// twin of `oscope-sim`'s camera on the CPU. From pinned memory, the
    /// copy back is a DMA.
    pub fn expose(
        &self,
        lane: Lane,
        atlas: &TileAtlas,
        exposure: &Exposure<'_>,
        out: &mut [u8],
    ) -> Result<(), ComputeError> {
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
        let len = width * height * e.bytes as usize;
        if !matches!(e.bytes, 1 | 2) || out.len() < len {
            return wrong("an output of the wrong size");
        }
        if len == 0 {
            return Ok(());
        }
        let stream = self.stream(lane);
        let grid_slots = self.copy_up(lane, e.grid)?;
        let places = self.copy_up(lane, e.places)?;
        let samples = self.copy_up(lane, e.samples)?;
        // SAFETY: the kernel writes every one of the `len` bytes before the
        // copy back reads them: a sample of `bytes` for each pixel.
        let mut packed = unsafe { stream.alloc::<u8>(len) }?;
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
            .arg(&e.frame);
        let [vx0, vy0, vk, vr2, vs] = e.vignette;
        launch.arg(&vx0).arg(&vy0).arg(&vk).arg(&vr2).arg(&vs);
        launch.arg(&e.bytes).arg(&mut packed);
        // SAFETY: the arguments are the kernel's, in kernels/sim.cu. Every
        // slot in `grid` is within the atlas, which holds `slots` tiles of
        // `tile` + 1 pixels square; the kernel clamps each position to the
        // grid's tiles, and reads at most one pixel past a tile's start,
        // which its overlap holds. `places` holds `height` rows of `most`
        // pairs, and each row reads its first `samples[row]` (1 to `most`).
        // `packed` holds `width` × `height` samples of `bytes`, each thread
        // writing its own.
        unsafe { launch.launch(grid(e.width, e.height)) }?;
        stream.memcpy_dtoh(&packed, &mut out[..len])?;
        Ok(stream.synchronize()?)
    }
}

#[cfg(test)]
mod tests {
    use super::half_to_f32;

    #[test]
    fn halves_read_as_their_values() {
        assert_eq!(half_to_f32(0x3c00), 1.0);
        assert_eq!(half_to_f32(0x3555), 0.333_251_95);
        assert_eq!(half_to_f32(0xc000), -2.0);
        assert_eq!(half_to_f32(0x0001), 2f32.powi(-24));
        assert_eq!(half_to_f32(0x7bff), 65_504.0);
        assert_eq!(half_to_f32(0x7c00), f32::INFINITY);
        assert!(half_to_f32(0x7e00).is_nan());
        assert_eq!(half_to_f32(0x0000), 0.0);
    }
}
