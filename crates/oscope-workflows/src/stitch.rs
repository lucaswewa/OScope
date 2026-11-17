//! The stitch job (P39; plan §5.15; P39's spec): a scan's acquisition, its
//! tiles registered over their overlaps and solved for their offsets,
//! blended, and written out of core as a result of its project.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use oscope_compute::{Image, Shape, cpu};
use oscope_core::{AcquisitionId, ProjectId};
use oscope_imaging::registration::{RegisterOptions, register_batch};
use oscope_jobs::{Factory, Job, JobCtx, JobSpec, Resource, Stop};
use oscope_stitch::{
    Flat, Link, Part, PixelBox, Placed, SolveOptions, Tiles, blend, flat_field, solve,
};
use oscope_store::{
    CHUNK, ImageInfo, ImageReader, Input, NodeVersion, Provenance, ResultRecord, SampleType,
    Samples, Store, StoreError, halve_samples,
};
use rayon::prelude::*;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::scan::Plan;

/// The stitch job's kind.
pub const STITCH: &str = "stitch.run";

/// The most tiles the flat field is estimated from.
const FLAT_TILES: usize = 64;

/// How a stitch runs, each with a default (P39's spec).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct StitchOptions {
    /// The least overlap two tiles' nominal frames must have, along both
    /// axes, to be registered, in pixels: 32.
    pub min_overlap_px: u32,
    /// How far from the stage's shift to search, in µm: 25.
    pub radius_um: f64,
    /// The least confidence a pair's registration must have to count: 0.5.
    pub min_confidence: f64,
    /// The global solve.
    pub solve: SolveOptions,
    /// Whether to estimate the tiles' flat field and divide it out.
    pub flat_field: bool,
    /// The most memory the job's parts take at once, in MB: 256.
    pub memory_mb: u32,
}

impl Default for StitchOptions {
    fn default() -> Self {
        Self {
            min_overlap_px: 32,
            radius_um: 25.0,
            min_confidence: 0.5,
            solve: SolveOptions::default(),
            flat_field: false,
            memory_mb: 256,
        }
    }
}

/// What a stitch made, and how well, as `stitch.json` beside its image
/// keeps it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct StitchReport {
    /// The acquisition stitched.
    pub acquisition: AcquisitionId,
    /// Its tiles.
    pub tiles: usize,
    /// The pairs registered.
    pub pairs: usize,
    /// Those kept by the solve.
    pub kept: usize,
    /// Those under the least confidence.
    pub dropped: usize,
    /// Those the solve rejected.
    pub rejected: usize,
    /// The kept pairs' misses' root mean square, in pixels.
    pub rms_px: f64,
    /// The offsets' root mean square change from the stage's, each less
    /// the mean change, in pixels.
    pub moved_px: f64,
    /// The mosaic's width and height, in pixels.
    pub size: (u64, u64),
    /// The most bytes its parts took at once while it was written.
    pub peak_bytes: u64,
    /// Each tile's offset, in pixels, from the first tile's nominal place.
    pub offsets: Vec<[f64; 2]>,
}

/// What can't be stitched.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum StitchError {
    /// The acquisition isn't a scan that can be stitched.
    #[error("{0}")]
    Refused(String),
    /// The store failed.
    #[error(transparent)]
    Store(#[from] StoreError),
    /// A registration failed.
    #[error(transparent)]
    Registration(#[from] oscope_imaging::registration::RegistrationError),
    /// It was cancelled.
    #[error("cancelled")]
    Cancelled,
}

fn refused(why: impl Into<String>) -> StitchError {
    StitchError::Refused(why.into())
}

/// A scan's tiles as the acquisition has them: which frame each registers
/// on, the stage's M, and its image.
struct Scan {
    tiles: Tiles,
    /// The planes of each tile, and the one nearest its z.
    planes: u32,
    nearest: u32,
    pixel_um: f64,
    info: ImageInfo,
    reader: ImageReader,
}

impl Scan {
    fn frame(&self, tile: usize, plane: u32) -> u32 {
        tile as u32 * self.planes + plane
    }
}

/// The acquisition's scan, from its plan and frames.
fn scan(store: &Store, project: ProjectId, id: AcquisitionId) -> Result<Scan, StitchError> {
    let contents = store.contents(project)?;
    let record = contents
        .acquisitions
        .into_iter()
        .find(|a| a.id == id)
        .ok_or_else(|| refused(format!("project {project} has no acquisition {id}")))?;
    if record.image.volume {
        return Err(refused(format!(
            "acquisition {id} is a volume, a stack at one place: there is nothing to stitch"
        )));
    }
    let plan: Plan = serde_json::from_value(record.plan.clone()).map_err(|e| {
        refused(format!(
            "acquisition {id} has no scan's plan to stitch by: {e}"
        ))
    })?;
    let info = record.image.clone();
    let (w, h) = (info.width as f64, info.height as f64);
    // M from the field's corners about the frame's centre (P35): from the
    // top left, round by the top right, then the bottom right and left.
    let [tl, tr, _, bl] = plan.field.corners_um;
    let m = [
        [(tr.x - tl.x) / w, (bl.x - tl.x) / h],
        [(tr.y - tl.y) / w, (bl.y - tl.y) / h],
    ];
    let det = m[0][0] * m[1][1] - m[0][1] * m[1][0];
    if !det.is_normal() {
        return Err(refused(format!("the plan's field gives a flat M: {m:?}")));
    }
    let offsets = plan.planes();
    let planes = offsets.len() as u32;
    let nearest = (0..offsets.len())
        .min_by(|&a, &b| offsets[a].abs().total_cmp(&offsets[b].abs()))
        .unwrap_or(0) as u32;
    let count = record.frames.len() / planes as usize;
    if count == 0 || count * planes as usize != record.frames.len() {
        let frames = record.frames.len();
        return Err(refused(format!(
            "{frames} frames aren't {planes} planes a tile"
        )));
    }
    let position = |tile: usize| {
        let frame = &record.frames[tile * planes as usize + nearest as usize];
        frame
            .position
            .ok_or_else(|| refused(format!("frame {} has no position", frame.index)))
    };
    let first = position(0)?;
    let nominal = (0..count)
        .map(|tile| {
            let p = position(tile)?;
            let (dx, dy) = (p.x.0 - first.x.0, p.y.0 - first.y.0);
            // M⁻¹ (p − p₀).
            Ok([
                (m[1][1] * dx - m[0][1] * dy) / det,
                (-m[1][0] * dx + m[0][0] * dy) / det,
            ])
        })
        .collect::<Result<Vec<_>, StitchError>>()?;
    let raw = info.channel_names == ["raw"];
    let path = store
        .folder(project)
        .join("acquisitions")
        .join(id.to_string())
        .join("raw.ome.zarr");
    Ok(Scan {
        tiles: Tiles {
            width: info.width as u32,
            height: info.height as u32,
            nominal,
            raw,
        },
        planes,
        nearest,
        pixel_um: det.abs().sqrt(),
        reader: ImageReader::open(&path)?,
        info,
    })
}

/// A chunk of a frame: the frame, and the chunk's column and row.
type ChunkKey = (u32, u64, u64);

/// A chunk decoded, and when it was last used.
type Held = (Arc<Vec<f32>>, u64);

/// The acquisition's chunks, decoded, as the writer wants them: each read
/// once while it is wanted, those used least recently let go past
/// `budget` bytes. Two workers wanting one chunk at once may both read it.
struct Chunks {
    held: Mutex<HashMap<ChunkKey, Held>>,
    uses: AtomicU64,
    bytes: AtomicUsize,
    peak: AtomicUsize,
    budget: usize,
}

impl Chunks {
    fn new(budget: usize) -> Self {
        Self {
            held: Mutex::new(HashMap::new()),
            uses: AtomicU64::new(0),
            bytes: AtomicUsize::new(0),
            peak: AtomicUsize::new(0),
            budget,
        }
    }

    /// Frame `frame`'s chunk `(cx, cy)`, from 0 to 1, and its width.
    fn chunk(
        &self,
        scan: &Scan,
        frame: u32,
        (cx, cy): (u64, u64),
    ) -> Result<(Arc<Vec<f32>>, u64), StitchError> {
        let (w, h) = (scan.info.width, scan.info.height);
        let (x, y) = (cx * CHUNK, cy * CHUNK);
        let (cw, ch) = (CHUNK.min(w - x), CHUNK.min(h - y));
        let used = self.uses.fetch_add(1, Ordering::Relaxed);
        let lock = || self.held.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some((samples, last)) = lock().get_mut(&(frame, cx, cy)) {
            *last = used;
            return Ok((Arc::clone(samples), cw));
        }
        let samples = Arc::new(scan.reader.read(0, frame, (x, y), (cw, ch))?.to_unit());
        let size = samples.len() * 4;
        let mut held = lock();
        if held.contains_key(&(frame, cx, cy)) {
            return Ok((samples, cw));
        }
        // Room first: the chunks used least recently go.
        while self.bytes.load(Ordering::Relaxed) + size > self.budget && !held.is_empty() {
            let oldest = held
                .iter()
                .min_by_key(|(_, (_, last))| *last)
                .map(|(key, _)| *key);
            let Some((gone, _)) = oldest.and_then(|key| held.remove(&key)) else {
                break;
            };
            self.bytes.fetch_sub(gone.len() * 4, Ordering::Relaxed);
        }
        held.insert((frame, cx, cy), (Arc::clone(&samples), used));
        let now = self.bytes.fetch_add(size, Ordering::Relaxed) + size;
        self.peak.fetch_max(now, Ordering::Relaxed);
        Ok((samples, cw))
    }

    /// Tile `tile`'s part `within`, plane `plane`, from 0 to 1, from its
    /// chunks.
    fn part(
        &self,
        scan: &Scan,
        tile: usize,
        plane: u32,
        within: PixelBox,
    ) -> Result<Vec<f32>, StitchError> {
        let c = scan.info.channels as usize;
        let frame = scan.frame(tile, plane);
        let (pw, ph) = (within.width() as usize, within.height() as usize);
        let mut part = vec![0.0_f32; pw * ph * c];
        let chunk = CHUNK as i64;
        for cy in within.y0 / chunk..=(within.y1 - 1) / chunk {
            for cx in within.x0 / chunk..=(within.x1 - 1) / chunk {
                let (samples, cw) = self.chunk(scan, frame, (cx as u64, cy as u64))?;
                // Where the chunk meets the part, in the tile's pixels.
                let x0 = within.x0.max(cx * chunk);
                let x1 = within.x1.min((cx + 1) * chunk);
                let y0 = within.y0.max(cy * chunk);
                let y1 = within.y1.min((cy + 1) * chunk);
                let run = (x1 - x0) as usize * c;
                for y in y0..y1 {
                    let from = (((y - cy * chunk) as usize) * cw as usize
                        + (x0 - cx * chunk) as usize)
                        * c;
                    let to = (((y - within.y0) as usize) * pw + (x0 - within.x0) as usize) * c;
                    part[to..to + run].copy_from_slice(&samples[from..from + run]);
                }
            }
        }
        Ok(part)
    }
}

/// A quarter of a chunk of level 1: which, its size, and its samples.
type Quarter = ((u64, u64), (u64, u64), Samples);

/// A chunk of level 1 made whole: its samples, top left and size.
type Whole = (Samples, (u64, u64), (u64, u64));

/// Level 1's chunks, as level 0's halved come in, a quarter each: each
/// written whole once it has them all, so that the pyramid need not read
/// level 0 back.
struct Quarters {
    pending: Mutex<HashMap<ChunkKey, Vec<Quarter>>>,
    /// Level 0's chunks across and down, and level 1's size.
    chunks: (u64, u64),
    level1: (u64, u64),
    channels: u32,
    sample: SampleType,
}

impl Quarters {
    /// Adds quarter `(qx, qy)` of level 1's chunk `key`, `size` pixels: the
    /// chunk's samples, at its top left and size, once it has them all.
    fn add(
        &self,
        key: ChunkKey,
        quarter: (u64, u64),
        size: (u64, u64),
        samples: Samples,
    ) -> Option<Whole> {
        let (_, cx, cy) = key;
        let expected = (self.chunks.0 - 2 * cx).min(2) * (self.chunks.1 - 2 * cy).min(2);
        let mut pending = self.pending.lock().unwrap_or_else(PoisonError::into_inner);
        let quarters = pending.entry(key).or_default();
        quarters.push((quarter, size, samples));
        if (quarters.len() as u64) < expected {
            return None;
        }
        let quarters = pending.remove(&key)?;
        drop(pending);
        let (x, y) = (cx * CHUNK, cy * CHUNK);
        let (w, h) = (CHUNK.min(self.level1.0 - x), CHUNK.min(self.level1.1 - y));
        let c = self.channels as usize;
        let n = (w * h) as usize * c;
        let mut whole = match self.sample {
            SampleType::U8 => Samples::U8(vec![0; n]),
            SampleType::U16 => Samples::U16(vec![0; n]),
            SampleType::F32 => Samples::F32(vec![0.0; n]),
        };
        for ((qx, qy), (qw, qh), from) in &quarters {
            let at = ((qx * CHUNK / 2) as usize, (qy * CHUNK / 2) as usize);
            paste(
                &mut whole,
                w as usize,
                from,
                (*qw as usize, *qh as usize),
                at,
                c,
            );
        }
        Some((whole, (x, y), (w, h)))
    }
}

/// `from`, `fw` × `fh`, into `into`, `iw` wide, at `(x, y)`.
fn paste(
    into: &mut Samples,
    iw: usize,
    from: &Samples,
    (fw, fh): (usize, usize),
    (x, y): (usize, usize),
    c: usize,
) {
    fn rows<T: Copy>(
        into: &mut [T],
        iw: usize,
        from: &[T],
        (fw, fh): (usize, usize),
        (x, y): (usize, usize),
        c: usize,
    ) {
        for r in 0..fh {
            into[((y + r) * iw + x) * c..][..fw * c].copy_from_slice(&from[r * fw * c..][..fw * c]);
        }
    }
    match (into, from) {
        (Samples::U8(i), Samples::U8(f)) => rows(i, iw, f, (fw, fh), (x, y), c),
        (Samples::U16(i), Samples::U16(f)) => rows(i, iw, f, (fw, fh), (x, y), c),
        (Samples::F32(i), Samples::F32(f)) => rows(i, iw, f, (fw, fh), (x, y), c),
        _ => unreachable!("one image's samples are of one type"),
    }
}

/// A part, grey: the mean of its channels.
fn grey(samples: Vec<f32>, (w, h): (u32, u32), channels: usize) -> Image {
    let data = match channels {
        1 => samples,
        c => samples
            .chunks_exact(c)
            .map(|p| p.iter().sum::<f32>() / c as f32)
            .collect(),
    };
    Image::from_vec(Shape::new(w, h, 1), data).expect("the part's size")
}

/// The links of `scan`'s pairs: registered in batches that fit `budget`
/// bytes, those under `options.min_confidence` dropped. Also how many pairs
/// there were.
fn links(
    scan: &Scan,
    options: &StitchOptions,
    progress: &(dyn Fn(f64, String) -> bool + Sync),
) -> Result<(Vec<Link>, usize), StitchError> {
    let pairs: Vec<_> = scan
        .tiles
        .pairs(options.min_overlap_px)
        .into_iter()
        .filter_map(|p| Some((p, scan.tiles.crops(p)?)))
        .collect();
    let channels = scan.info.channels as usize;
    let budget = (options.memory_mb as usize) << 20;
    let mut found = Vec::new();
    let mut done = 0;
    while done < pairs.len() {
        // As many pairs as fit: each two parts, read and binned.
        let mut end = done;
        let mut bytes = 0;
        while end < pairs.len() && (end == done || bytes < budget / 2) {
            let (w, h) = pairs[end].1.size;
            bytes += 2 * (w as usize * h as usize * channels * 4);
            end += 1;
        }
        let parts = pairs[done..end]
            .par_iter()
            .map(|(pair, crops)| {
                let cut = |tile: usize, (x, y): (u32, u32)| -> Result<Image, StitchError> {
                    // Binned by an even factor, from level 1, a quarter of the
                    // pixels to read, each the mean of two by two: for raw
                    // tiles, their cells.
                    let (level, bin) = match crops.bin % 2 == 0 && scan.info.levels > 1 {
                        true => (1, crops.bin / 2),
                        false => (0, crops.bin),
                    };
                    let size = (crops.size.0 >> level, crops.size.1 >> level);
                    let at = (u64::from(x >> level), u64::from(y >> level));
                    let frame = scan.frame(tile, scan.nearest);
                    let read = scan.reader.read(
                        level,
                        frame,
                        at,
                        (u64::from(size.0), u64::from(size.1)),
                    )?;
                    let part = grey(read.to_unit(), size, channels);
                    Ok(match bin {
                        1 => part,
                        bin => cpu::downsample(&part, bin).expect("binned by a factor over 0"),
                    })
                };
                Ok((cut(pair.a, crops.a)?, cut(pair.b, crops.b)?))
            })
            .collect::<Result<Vec<_>, StitchError>>()?;
        // Each pair searched near its own shift, the pairs in parallel.
        let registered = pairs[done..end]
            .par_iter()
            .zip(&parts)
            .map(|((_, crops), (a, b))| {
                let radius = (options.radius_um / scan.pixel_um / f64::from(crops.bin)).ceil();
                let register = RegisterOptions {
                    expected: Some(crops.expected),
                    radius: radius.max(1.0) as u32,
                    ..RegisterOptions::default()
                };
                Ok(register_batch(&[(a, b)], &register)?[0])
            })
            .collect::<Result<Vec<_>, StitchError>>()?;
        for ((pair, crops), one) in pairs[done..end].iter().zip(registered) {
            if one.confidence >= options.min_confidence {
                found.push(Link {
                    a: pair.a,
                    b: pair.b,
                    shift: crops.measured((one.dx, one.dy)),
                    weight: one.confidence,
                });
            }
        }
        done = end;
        let share = 0.4 * done as f64 / pairs.len() as f64;
        if !progress(share, format!("{done} of {} pairs registered", pairs.len())) {
            return Err(StitchError::Cancelled);
        }
    }
    Ok((found, pairs.len()))
}

/// The flat field of up to 64 of `scan`'s tiles, spread over it, at the
/// level whose shorter side is 64 pixels or more.
fn flat(scan: &Scan) -> Result<Flat, StitchError> {
    let shorter = scan.info.width.min(scan.info.height);
    let mut level = 0;
    while level + 1 < scan.info.levels && shorter >> (level + 1) >= 64 {
        level += 1;
    }
    let (w, h) = scan.info.level_size(level);
    let count = scan.tiles.nominal.len();
    let step = count.div_ceil(FLAT_TILES).max(1);
    let tiles = (0..count)
        .step_by(step)
        .map(|tile| {
            let frame = scan.frame(tile, scan.nearest);
            Ok(scan.reader.read(level, frame, (0, 0), (w, h))?.to_unit())
        })
        .collect::<Result<Vec<_>, StitchError>>()?;
    let views: Vec<&[f32]> = tiles.iter().map(Vec::as_slice).collect();
    Ok(flat_field(
        &views,
        (w as u32, h as u32),
        scan.info.channels,
        1 << level,
    ))
}

/// `values`, from 0 to 1, as samples of `sample`.
fn samples(values: Vec<f32>, sample: SampleType) -> Samples {
    let scaled = |most: f32| {
        values
            .iter()
            .map(move |v| (v * most).round().clamp(0.0, most))
    };
    match sample {
        SampleType::U8 => Samples::U8(scaled(255.0).map(|v| v as u8).collect()),
        SampleType::U16 => Samples::U16(scaled(65_535.0).map(|v| v as u16).collect()),
        SampleType::F32 => Samples::F32(values),
    }
}

/// Stitches acquisition `id` of `project` into a new result of it, as
/// `options` say: the result, and its report. `progress` is told how far
/// it is, from 0 to 1, and what it is doing, and cancels it by answering
/// false.
pub fn stitch(
    store: &Store,
    project: ProjectId,
    id: AcquisitionId,
    options: &StitchOptions,
    progress: &(dyn Fn(f64, String) -> bool + Sync),
) -> Result<(ResultRecord, StitchReport), StitchError> {
    let scan = scan(store, project, id)?;
    let (links, pairs) = links(&scan, options, progress)?;
    let solved = solve(&scan.tiles.nominal, &links, &options.solve);
    let moved = {
        let n = solved.offsets.len() as f64;
        let change: Vec<[f64; 2]> = solved
            .offsets
            .iter()
            .zip(&scan.tiles.nominal)
            .map(|(o, s)| [o[0] - s[0], o[1] - s[1]])
            .collect();
        let mean = [0, 1].map(|axis| change.iter().map(|c| c[axis]).sum::<f64>() / n);
        let sum: f64 = change
            .iter()
            .map(|c| (c[0] - mean[0]).powi(2) + (c[1] - mean[1]).powi(2))
            .sum();
        (sum / n).sqrt()
    };
    if !progress(0.45, "solved".to_owned()) {
        return Err(StitchError::Cancelled);
    }
    let flat = match options.flat_field {
        true => Some(flat(&scan)?),
        false => None,
    };
    let tile = (scan.tiles.width, scan.tiles.height);
    let placed = Placed::new(&solved.offsets, tile, scan.tiles.raw);
    let mut info = ImageInfo::new(
        placed.size.0,
        placed.size.1,
        scan.info.channels,
        scan.info.sample,
    );
    info.planes = scan.planes;
    info.channel_names = scan.info.channel_names.clone();
    info.pixel_size = scan.info.pixel_size;
    let acquisition_name = store
        .contents(project)?
        .acquisitions
        .into_iter()
        .find(|a| a.id == id)
        .map_or_else(|| id.to_string(), |a| a.name);
    let provenance = Provenance {
        recipe: json!({ "stitch": options }),
        nodes: vec![NodeVersion {
            node: "stitch.mosaic".to_owned(),
            version: env!("CARGO_PKG_VERSION").to_owned(),
            extension: None,
        }],
        inputs: vec![Input::Acquisition(id)],
        hashes: Default::default(),
        software: format!("OScope {}", env!("CARGO_PKG_VERSION")),
    };
    let name = format!("{acquisition_name} stitched");
    let draft = store.result(project, &name, provenance, Some(&info))?;
    let writer = draft.image().expect("a result with an image has a writer");
    // The chunks, as many at once as the budget holds, each from its
    // tiles' parts.
    let channels = scan.info.channels as usize;
    // Room for level 1's quarters waiting for their chunks' others, at most
    // two rows of level 0's chunks of them; then half the rest for the
    // chunks decoded, half for those being made.
    let chunk_bytes = (CHUNK * CHUNK) as usize * channels * 4;
    let sample_bytes = match scan.info.sample {
        SampleType::U8 => 1,
        SampleType::U16 => 2,
        SampleType::F32 => 4,
    };
    let across = placed.size.0.div_ceil(CHUNK) as usize;
    let quarters = across.div_ceil(2) * (CHUNK * CHUNK) as usize * channels * sample_bytes;
    let budget = ((options.memory_mb as usize) << 20).saturating_sub(quarters);
    let cache = Chunks::new(budget / 2);
    // A chunk holds up to four tiles' parts and its sums, in overlaps.
    let at_once = (budget / 2 / (6 * chunk_bytes)).max(1);
    let chunks: Vec<(u32, PixelBox)> = (0..scan.planes)
        .flat_map(|plane| {
            let (w, h) = (placed.size.0 as i64, placed.size.1 as i64);
            (0..h)
                .step_by(CHUNK as usize)
                .flat_map(move |y| (0..w).step_by(CHUNK as usize).map(move |x| (x, y)))
                .map(move |(x, y)| {
                    let edge = (x + CHUNK as i64).min(w);
                    let bottom = (y + CHUNK as i64).min(h);
                    (
                        plane,
                        PixelBox {
                            x0: x,
                            y0: y,
                            x1: edge,
                            y1: bottom,
                        },
                    )
                })
        })
        .collect();
    let (held, peak, written) = (AtomicUsize::new(0), AtomicUsize::new(0), AtomicU64::new(0));
    let hold = |bytes: usize| {
        let now = held.fetch_add(bytes, Ordering::Relaxed) + bytes;
        peak.fetch_max(now, Ordering::Relaxed);
    };
    // As many workers as the budget holds chunks, each making one at a
    // time: the pool bounds the parts in hand. The chunks go a band of two
    // rows at a time, a row of level 1's, so that level 1's quarters wait
    // for one band's others at most, which their room holds: rayon would
    // otherwise work on far bands at once, each with its quarters waiting.
    let band = |&(plane, chunk): &(u32, PixelBox)| (plane, chunk.y0 / (2 * CHUNK as i64));
    let threads = std::thread::available_parallelism().map_or(1, |n| n.get());
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(at_once.min(threads))
        .build()
        .map_err(|e| refused(format!("the writers' pool: {e}")))?;
    let every = (chunks.len() / 50).max(1);
    let level1 = (writer.info().levels > 1).then(|| Quarters {
        pending: Mutex::new(HashMap::new()),
        chunks: (placed.size.0.div_ceil(CHUNK), placed.size.1.div_ceil(CHUNK)),
        level1: writer.info().level_size(1),
        channels: scan.info.channels,
        sample: scan.info.sample,
    });
    pool.install(|| {
        chunks
            .chunk_by(|a, b| band(a) == band(b))
            .try_for_each(|rows| {
                rows.par_iter().try_for_each(|&(plane, chunk)| {
                    let over = placed.over(chunk);
                    let mut owned = Vec::with_capacity(over.len());
                    for (tile, within) in over {
                        let part = cache.part(&scan, tile, plane, within)?;
                        hold(part.len() * 4);
                        let (x, y) = placed.at[tile];
                        let to = (
                            (x as i64 + within.x0 - chunk.x0) as u32,
                            (y as i64 + within.y0 - chunk.y0) as u32,
                        );
                        owned.push((within, part, to));
                    }
                    let parts: Vec<Part> = owned
                        .iter()
                        .map(|(within, samples, to)| Part {
                            within: *within,
                            samples,
                            to: *to,
                        })
                        .collect();
                    let size = (chunk.width() as u32, chunk.height() as u32);
                    hold(size.0 as usize * size.1 as usize * (channels + 1) * 4);
                    let mosaic = blend(size, scan.info.channels, tile, &parts, flat.as_ref());
                    let out = samples(mosaic, scan.info.sample);
                    let at = (chunk.x0 as u64, chunk.y0 as u64);
                    writer.write(plane, at, (u64::from(size.0), u64::from(size.1)), &out)?;
                    if let Some(level1) = &level1 {
                        let (w, h) = (u64::from(size.0), u64::from(size.1));
                        let half = halve_samples(&out, w, h, scan.info.channels);
                        hold(half.len() * sample_bytes);
                        let (cx, cy) = (at.0 / CHUNK, at.1 / CHUNK);
                        let key = (plane, cx / 2, cy / 2);
                        let whole =
                            level1.add(key, (cx % 2, cy % 2), (w.div_ceil(2), h.div_ceil(2)), half);
                        if let Some((whole, at, size)) = whole {
                            writer.write_level(1, plane, at, size, &whole)?;
                            let made = (size.0 * size.1) as usize * channels * sample_bytes;
                            held.fetch_sub(made, Ordering::Relaxed);
                        }
                    }
                    let freed: usize = owned.iter().map(|(_, p, _)| p.len() * 4).sum::<usize>()
                        + size.0 as usize * size.1 as usize * (channels + 1) * 4;
                    held.fetch_sub(freed, Ordering::Relaxed);
                    let done = written.fetch_add(1, Ordering::Relaxed) + 1;
                    if done % every as u64 == 0 {
                        let share = 0.45 + 0.5 * done as f64 / chunks.len() as f64;
                        if !progress(share, format!("{done} of {} chunks written", chunks.len())) {
                            return Err(StitchError::Cancelled);
                        }
                    }
                    Ok::<(), StitchError>(())
                })
            })
    })?;
    if level1.is_some() {
        writer.made(1);
    }
    let report = StitchReport {
        acquisition: id,
        tiles: scan.tiles.nominal.len(),
        pairs,
        kept: solved.kept.len(),
        dropped: pairs - links.len(),
        rejected: solved.rejected.len(),
        rms_px: solved.rms_px,
        moved_px: moved,
        size: placed.size,
        peak_bytes: (peak.load(Ordering::Relaxed) + cache.peak.load(Ordering::Relaxed)) as u64,
        offsets: solved.offsets,
    };
    let text = serde_json::to_string_pretty(&report).expect("a report serialises");
    let path = draft.folder().join("stitch.json");
    std::fs::write(&path, text).map_err(|source| StoreError::Io { path, source })?;
    progress(0.95, "the pyramid".to_owned());
    Ok((draft.commit()?, report))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Params {
    project: ProjectId,
    acquisition: AcquisitionId,
    #[serde(default)]
    options: StitchOptions,
}

/// Makes stitch jobs, whose parameters are `project`, `acquisition` and,
/// optionally, `options`. Each stitches the acquisition into a new result
/// of the project.
pub fn stitch_jobs(store: Arc<Store>) -> Factory {
    Arc::new(move |value| {
        let params = Params::deserialize(value).map_err(|e| e.to_string())?;
        let spec = JobSpec {
            kind: STITCH.to_owned(),
            title: format!("stitch acquisition {}", params.acquisition),
            account: None,
            params: value.clone(),
            resource: Resource::Compute,
            pausable: false,
            gpu_mib: 0,
        };
        let store = Arc::clone(&store);
        Ok(Job {
            spec,
            run: Box::new(move |ctx| Box::pin(run(store, params, ctx))),
        })
    })
}

async fn run(store: Arc<Store>, params: Params, ctx: JobCtx) -> Result<(), Stop> {
    let job = ctx.clone();
    let project = params.project;
    let work = tokio::task::spawn_blocking(move || {
        let progress = |share: f64, what: String| {
            job.progress(Some(share), what);
            !job.is_cancelled()
        };
        stitch(
            &store,
            project,
            params.acquisition,
            &params.options,
            &progress,
        )
    });
    match work.await {
        Ok(Ok((result, report))) => {
            ctx.progress(Some(1.0), "stitched");
            ctx.add_result(format!("projects/{project}/results/{}", result.id));
            ctx.add_result(format!(
                "{} pairs kept of {}, {} rejected; {:.2} px rms; moved {:.2} px rms from the stage",
                report.kept, report.pairs, report.rejected, report.rms_px, report.moved_px
            ));
            Ok(())
        }
        Ok(Err(StitchError::Cancelled)) => Err(Stop::Cancelled),
        Ok(Err(error)) => Err(Stop::failed(error)),
        Err(error) => Err(Stop::failed(error)),
    }
}

/// Where a stitched result's image is, and its `stitch.json` beside it.
pub fn result_image(
    store: &Store,
    project: ProjectId,
    result: &ResultRecord,
) -> (PathBuf, PathBuf) {
    let folder = store
        .folder(project)
        .join("results")
        .join(result.id.to_string());
    (folder.join("image.ome.zarr"), folder.join("stitch.json"))
}
