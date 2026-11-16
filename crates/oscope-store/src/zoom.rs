//! Deep zoom from the store (plan §5.15, ADR-0028): a record's image as the
//! tiles of [`oscope_imaging::zoom`], read through a cache of decompressed
//! chunks, kept in a tile cache on the scratch disk, and exported as a
//! static DZI folder.
//!
//! - **Raw images,** a camera's Bayer mosaic, show in colour. At full size
//!   each tile is demosaiced bilinearly, with the 2 pixels around it the
//!   filters need. Halved, they come from a colour overview: an OME-Zarr
//!   image whose pixels are the mosaic's 2 × 2 cells, as the superpixel
//!   demosaic makes them, with its own levels ([`build_overview`]).
//! - **The tile cache's layout,** which the desktop app reads straight from
//!   disk (§5.18): `<root>/<image>/<look>/<level>/<col>_<row>.jpg`, each a
//!   JPEG of 256 pixels square or less at an edge, as Deep Zoom numbers its
//!   levels and tiles; and `<root>/<image>/overview.ome.zarr`, a raw
//!   image's colour overview.

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use oscope_compute::{Image, Shape};
use oscope_core::{BayerPattern, PixelFormat, PixelRect, ProjectId};
use oscope_imaging::color::{Curve, Demosaic, Display, DisplayTable, crop, demosaic};
use oscope_imaging::zoom::{Pyramid, ZoomError, write_dzi};

use crate::image::CHUNK;
use crate::records::{Input, RecordKind};
use crate::{ImageInfo, ImageReader, ImageWriter, Result, Samples, Store, StoreError};

fn failed(error: impl std::fmt::Display) -> StoreError {
    StoreError::Zarr(format!("deep zoom: {error}"))
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// A plane of an image in the store, opened.
pub struct StoredImage {
    path: PathBuf,
    plane: u32,
    reader: ImageReader,
}

impl StoredImage {
    /// The image at `path`, its plane `plane`.
    pub fn open(path: &Path, plane: u32) -> Result<Self> {
        let reader = ImageReader::open(path)?;
        if plane >= reader.info().planes {
            return Err(StoreError::Refused(format!(
                "plane {plane} of an image of {}",
                reader.info().planes
            )));
        }
        Ok(StoredImage {
            path: path.to_owned(),
            plane,
            reader,
        })
    }

    /// What it holds.
    pub fn info(&self) -> &ImageInfo {
        self.reader.info()
    }
}

type ChunkKey = (PathBuf, u32, u32, u64, u64);

/// Each chunk's samples, and when it was last used.
type Held = HashMap<ChunkKey, (Arc<Vec<f32>>, u64)>;

/// Decompressed chunks, shared by every image served, those used least
/// recently let go past its budget: a chunk's other tiles then cost only
/// their showing and encoding.
pub struct ChunkCache {
    held: Mutex<Held>,
    uses: AtomicU64,
    bytes: AtomicUsize,
    budget: usize,
    hits: AtomicU64,
    misses: AtomicU64,
}

impl ChunkCache {
    /// A cache of `budget` bytes of samples.
    pub fn new(budget: usize) -> Self {
        ChunkCache {
            held: Mutex::new(HashMap::new()),
            uses: AtomicU64::new(0),
            bytes: AtomicUsize::new(0),
            budget,
            hits: AtomicU64::new(0),
            misses: AtomicU64::new(0),
        }
    }

    /// The chunks found in it, and those read.
    pub fn counts(&self) -> (u64, u64) {
        (
            self.hits.load(Ordering::Relaxed),
            self.misses.load(Ordering::Relaxed),
        )
    }

    /// Chunk `(cx, cy)` of `image`'s `level`, from 0 to 1, and its width.
    fn chunk(
        &self,
        image: &StoredImage,
        level: u32,
        (cx, cy): (u64, u64),
    ) -> Result<(Arc<Vec<f32>>, u64)> {
        let (w, h) = image.info().level_size(level);
        let (x, y) = (cx * CHUNK, cy * CHUNK);
        let (cw, ch) = (CHUNK.min(w - x), CHUNK.min(h - y));
        let key = (image.path.clone(), level, image.plane, cx, cy);
        let used = self.uses.fetch_add(1, Ordering::Relaxed);
        if let Some((samples, last)) = lock(&self.held).get_mut(&key) {
            *last = used;
            self.hits.fetch_add(1, Ordering::Relaxed);
            return Ok((Arc::clone(samples), cw));
        }
        self.misses.fetch_add(1, Ordering::Relaxed);
        let samples = Arc::new(
            image
                .reader
                .read(level, image.plane, (x, y), (cw, ch))?
                .to_unit(),
        );
        let size = samples.len() * 4;
        let mut held = lock(&self.held);
        while self.bytes.load(Ordering::Relaxed) + size > self.budget && !held.is_empty() {
            let oldest = held
                .iter()
                .min_by_key(|(_, (_, last))| *last)
                .map(|(key, _)| key.clone());
            let Some((gone, _)) = oldest.and_then(|key| held.remove(&key)) else {
                break;
            };
            self.bytes.fetch_sub(gone.len() * 4, Ordering::Relaxed);
        }
        if held.insert(key, (Arc::clone(&samples), used)).is_none() {
            self.bytes.fetch_add(size, Ordering::Relaxed);
        }
        Ok((samples, cw))
    }

    /// The `size` pixels at `at` of `image`'s `level`, from 0 to 1, its
    /// channels interleaved, from its chunks.
    fn region(
        &self,
        image: &StoredImage,
        level: u32,
        at: (u64, u64),
        size: (u64, u64),
    ) -> Result<Vec<f32>> {
        let c = image.info().channels as usize;
        let mut out = vec![0.0_f32; size.0 as usize * size.1 as usize * c];
        for cy in at.1 / CHUNK..=(at.1 + size.1 - 1) / CHUNK {
            for cx in at.0 / CHUNK..=(at.0 + size.0 - 1) / CHUNK {
                let (samples, cw) = self.chunk(image, level, (cx, cy))?;
                let x0 = at.0.max(cx * CHUNK);
                let x1 = (at.0 + size.0).min((cx + 1) * CHUNK);
                let y0 = at.1.max(cy * CHUNK);
                let y1 = (at.1 + size.1).min((cy + 1) * CHUNK);
                let width = (x1 - x0) as usize * c;
                for y in y0..y1 {
                    let from = ((y - cy * CHUNK) * cw + (x0 - cx * CHUNK)) as usize * c;
                    let to = ((y - at.1) * size.0 + (x0 - at.0)) as usize * c;
                    out[to..to + width].copy_from_slice(&samples[from..from + width]);
                }
            }
        }
        Ok(out)
    }
}

/// A plane of a stored image as deep zoom's [`Pyramid`]: as stored, or a
/// raw image in colour.
pub struct StoredPyramid {
    image: Arc<StoredImage>,
    cache: Arc<ChunkCache>,
    raw: Option<Raw>,
}

struct Raw {
    pattern: BayerPattern,
    overview: Option<Arc<StoredImage>>,
}

impl StoredPyramid {
    /// `image` through `cache`: in colour if `raw` gives its mosaic's
    /// pattern, halved from `overview` once one is built.
    pub fn new(
        image: Arc<StoredImage>,
        cache: Arc<ChunkCache>,
        raw: Option<(BayerPattern, Option<Arc<StoredImage>>)>,
    ) -> Result<Self> {
        let channels = image.info().channels;
        if raw.is_some() && channels != 1 {
            return Err(StoreError::Refused(format!(
                "an image of {channels} channels isn't a raw mosaic"
            )));
        }
        let raw = raw.map(|(pattern, overview)| Raw { pattern, overview });
        Ok(StoredPyramid { image, cache, raw })
    }

    /// The `size` pixels at `at` of the raw mosaic at full size, in colour.
    fn demosaiced(
        &self,
        pattern: BayerPattern,
        at: (u64, u64),
        size: (u64, u64),
    ) -> Result<Vec<f32>> {
        const MARGIN: u64 = 2;
        let info = self.image.info();
        let (x0, y0) = (at.0.saturating_sub(MARGIN), at.1.saturating_sub(MARGIN));
        let x1 = (at.0 + size.0 + MARGIN).min(info.width);
        let y1 = (at.1 + size.1 + MARGIN).min(info.height);
        let around = self
            .cache
            .region(&self.image, 0, (x0, y0), (x1 - x0, y1 - y0))?;
        let shape = Shape::new((x1 - x0) as u32, (y1 - y0) as u32, 1);
        let mosaic = Image::from_vec(shape, around).map_err(failed)?;
        let shifted = pattern.shifted(x0 as u32, y0 as u32);
        let rgb = demosaic(&mosaic, shifted, Demosaic::Bilinear).map_err(failed)?;
        let rect = PixelRect::new(
            (at.0 - x0) as u32,
            (at.1 - y0) as u32,
            size.0 as u32,
            size.1 as u32,
        );
        Ok(crop(&rgb, rect).map_err(failed)?.data().to_vec())
    }

    /// The `size` pixels at `at` of the colour overview's `level`: the
    /// last column or row repeated where the mosaic's size was odd.
    fn overview_region(
        &self,
        overview: &StoredImage,
        level: u32,
        at: (u64, u64),
        size: (u64, u64),
    ) -> Result<Vec<f32>> {
        let (w, h) = overview.info().level_size(level);
        let (x, y) = (at.0.min(w - 1), at.1.min(h - 1));
        let got = ((at.0 + size.0).min(w) - x, (at.1 + size.1).min(h) - y);
        let samples = self.cache.region(overview, level, (x, y), got)?;
        if got == size {
            return Ok(samples);
        }
        let mut out = Vec::with_capacity(size.0 as usize * size.1 as usize * 3);
        for row in 0..size.1 {
            let r = row.min(got.1 - 1) as usize;
            for col in 0..size.0 {
                let c = col.min(got.0 - 1) as usize;
                let i = (r * got.0 as usize + c) * 3;
                out.extend_from_slice(&samples[i..i + 3]);
            }
        }
        Ok(out)
    }
}

impl Pyramid for StoredPyramid {
    fn size(&self) -> (u64, u64) {
        (self.image.info().width, self.image.info().height)
    }

    fn channels(&self) -> u32 {
        match self.raw {
            Some(_) => 3,
            None => self.image.info().channels,
        }
    }

    fn coarsest(&self) -> u32 {
        match &self.raw {
            Some(Raw {
                overview: Some(overview),
                ..
            }) => overview.info().levels,
            _ => self.image.info().levels - 1,
        }
    }

    fn read(
        &self,
        halvings: u32,
        at: (u64, u64),
        size: (u64, u64),
    ) -> std::result::Result<Vec<f32>, ZoomError> {
        let source = |e: StoreError| ZoomError::Source(e.to_string());
        match &self.raw {
            None => self.cache.region(&self.image, halvings, at, size),
            Some(raw) if halvings == 0 => self.demosaiced(raw.pattern, at, size),
            Some(Raw {
                overview: Some(overview),
                ..
            }) => self.overview_region(overview, halvings - 1, at, size),
            // No colour yet: the mosaic's levels, each pixel a cell's mean,
            // in grey.
            Some(Raw { overview: None, .. }) => self
                .cache
                .region(&self.image, halvings, at, size)
                .map(|grey| grey.iter().flat_map(|&g| [g; 3]).collect()),
        }
        .map_err(source)
    }
}

/// Builds `image`'s colour overview at `out`, an OME-Zarr image: each 2 × 2
/// cell of its mosaic, of `pattern`, one RGB pixel, and its levels made by
/// halving. A band of cells at a time; written beside `out`, then renamed.
pub fn build_overview(image: &StoredImage, pattern: BayerPattern, out: &Path) -> Result<()> {
    let info = image.info();
    let (width, height) = (info.width / 2, info.height / 2);
    if width == 0 || height == 0 {
        return Err(StoreError::Refused(format!(
            "a mosaic of {} × {} has no whole cells",
            info.width, info.height
        )));
    }
    let mut colour = ImageInfo::new(width, height, 3, info.sample);
    colour.channel_names = ["red", "green", "blue"].map(str::to_owned).to_vec();
    let part = out.with_extension("part");
    if part.exists() {
        fs::remove_dir_all(&part).map_err(crate::io(&part))?;
    }
    let writer = ImageWriter::create(&part, &colour, "colour overview")?;
    for y in (0..height).step_by(CHUNK as usize) {
        let rows = CHUNK.min(height - y);
        let cells = image
            .reader
            .read(0, image.plane, (0, 2 * y), (2 * width, 2 * rows))?
            .to_unit();
        let shape = Shape::new(2 * width as u32, 2 * rows as u32, 1);
        let mosaic = Image::from_vec(shape, cells).map_err(failed)?;
        let rgb = demosaic(&mosaic, pattern, Demosaic::Superpixel).map_err(failed)?;
        let samples = Samples::from_unit(info.sample, rgb.data());
        writer.write(0, (0, y), (width, rows), &samples)?;
    }
    writer.finish()?;
    if out.exists() {
        fs::remove_dir_all(out).map_err(crate::io(out))?;
    }
    fs::rename(&part, out).map_err(crate::io(out))
}

/// How `image` shows when nothing else is asked: as its thumbnail does,
/// stretched over its coarsest level's 0.5th to 99.5th percentile unless
/// 8-bit.
pub fn default_display(image: &StoredImage) -> Result<Display> {
    let info = image.info();
    let level = info.levels - 1;
    let (w, h) = info.level_size(level);
    let unit = image
        .reader
        .read(level, image.plane, (0, 0), (w, h))?
        .to_unit();
    Ok(crate::thumbnail::display(info.sample, &unit))
}

/// A display's key in the tile cache: its levels and its curve.
pub fn look_key(display: &Display) -> String {
    let curve = match display.curve {
        Curve::Linear => "linear".to_owned(),
        Curve::Srgb => "srgb".to_owned(),
        Curve::Gamma { gamma } => format!("gamma{gamma:.3}"),
    };
    format!("{:.5}-{:.5}-{curve}", display.black, display.white)
}

/// Where a tile is kept: its image's key, its look's, its level and its
/// column and row.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct TileKey {
    /// The image's key, a folder's name.
    pub image: String,
    /// The look's key, from [`look_key`].
    pub look: String,
    /// The Deep Zoom level.
    pub level: u32,
    /// The tile's column and row.
    pub tile: (u64, u64),
}

impl TileKey {
    /// Its file, under the cache's root.
    pub fn file(&self) -> PathBuf {
        Path::new(&self.image)
            .join(&self.look)
            .join(self.level.to_string())
            .join(format!("{}_{}.jpg", self.tile.0, self.tile.1))
    }
}

/// What a tile cache has done.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TileCounts {
    /// Tiles found.
    pub hits: u64,
    /// Tiles asked for and not found.
    pub misses: u64,
    /// Tiles kept.
    pub tiles: u64,
    /// Their bytes.
    pub bytes: u64,
}

#[derive(Default)]
struct TileIndex {
    /// Each file's bytes and when it was last used.
    files: HashMap<PathBuf, (u64, u64)>,
    /// The files by when they were last used.
    order: BTreeMap<u64, PathBuf>,
    clock: u64,
    counts: TileCounts,
}

impl TileIndex {
    fn touch(&mut self, file: &Path) {
        self.clock += 1;
        if let Some((_, used)) = self.files.get_mut(file) {
            self.order.remove(used);
            *used = self.clock;
            self.order.insert(self.clock, file.to_owned());
        }
    }

    fn add(&mut self, file: PathBuf, bytes: u64) {
        self.clock += 1;
        if let Some((old, used)) = self.files.insert(file.clone(), (bytes, self.clock)) {
            self.order.remove(&used);
            self.counts.bytes -= old;
            self.counts.tiles -= 1;
        }
        self.order.insert(self.clock, file);
        self.counts.bytes += bytes;
        self.counts.tiles += 1;
    }
}

/// Tiles on the scratch disk, those used least recently deleted past its
/// budget of bytes.
pub struct TileCache {
    root: PathBuf,
    budget: u64,
    index: Mutex<TileIndex>,
}

impl TileCache {
    /// The cache at `root`, taking in the tiles already there, the oldest
    /// as used least recently.
    pub fn open(root: &Path, budget: u64) -> Result<Self> {
        fs::create_dir_all(root).map_err(crate::io(root))?;
        let mut found = Vec::new();
        let mut folders = vec![root.to_owned()];
        while let Some(folder) = folders.pop() {
            for entry in fs::read_dir(&folder).map_err(crate::io(&folder))?.flatten() {
                let path = entry.path();
                let Ok(meta) = entry.metadata() else {
                    continue;
                };
                if meta.is_dir() && path.extension().is_none_or(|e| e != "zarr") {
                    folders.push(path);
                } else if path.extension().is_some_and(|e| e == "jpg") {
                    let file = path.strip_prefix(root).unwrap_or(&path).to_owned();
                    found.push((meta.modified().ok(), file, meta.len()));
                }
            }
        }
        found.sort_by_key(|(modified, ..)| *modified);
        let mut index = TileIndex::default();
        for (_, file, bytes) in found {
            index.add(file, bytes);
        }
        let cache = TileCache {
            root: root.to_owned(),
            budget,
            index: Mutex::new(index),
        };
        cache.evict(&mut lock(&cache.index))?;
        Ok(cache)
    }

    /// Where it keeps its tiles.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The tile `key`, if kept.
    pub fn get(&self, key: &TileKey) -> Option<Vec<u8>> {
        let file = key.file();
        let mut index = lock(&self.index);
        if !index.files.contains_key(&file) {
            index.counts.misses += 1;
            return None;
        }
        match fs::read(self.root.join(&file)) {
            Ok(bytes) => {
                index.touch(&file);
                index.counts.hits += 1;
                Some(bytes)
            }
            Err(_) => {
                // Gone from the disk: forgotten.
                if let Some((bytes, used)) = index.files.remove(&file) {
                    index.order.remove(&used);
                    index.counts.bytes -= bytes;
                    index.counts.tiles -= 1;
                }
                index.counts.misses += 1;
                None
            }
        }
    }

    /// Keeps tile `key`, written beside and renamed whole; past the budget,
    /// the tiles used least recently go.
    pub fn put(&self, key: &TileKey, jpeg: &[u8]) -> Result<()> {
        let file = key.file();
        let path = self.root.join(&file);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(crate::io(parent))?;
        }
        let part = path.with_extension("part");
        fs::write(&part, jpeg).map_err(crate::io(&part))?;
        fs::rename(&part, &path).map_err(crate::io(&path))?;
        let mut index = lock(&self.index);
        index.add(file, jpeg.len() as u64);
        self.evict(&mut index)
    }

    /// What it has done, and holds.
    pub fn counts(&self) -> TileCounts {
        lock(&self.index).counts
    }

    fn evict(&self, index: &mut TileIndex) -> Result<()> {
        while index.counts.bytes > self.budget {
            let Some((_, file)) = index.order.pop_first() else {
                break;
            };
            if let Some((bytes, _)) = index.files.remove(&file) {
                index.counts.bytes -= bytes;
                index.counts.tiles -= 1;
            }
            let path = self.root.join(&file);
            match fs::remove_file(&path) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(crate::io(&path)(e)),
            }
        }
        Ok(())
    }
}

/// The key of `kind` record `id`'s image of `project`, plane `plane`: a
/// folder's name.
pub fn image_key(project: ProjectId, kind: RecordKind, id: &str, plane: u32) -> String {
    format!("{project}-{}-{id}-{plane}", kind.name())
}

/// `kind` record `id`'s image of `project`, and its mosaic's pattern if it
/// is raw: its camera's, or for a result, its acquisition's.
pub fn record_image(
    store: &Store,
    project: ProjectId,
    kind: RecordKind,
    id: &str,
) -> Result<(PathBuf, Option<BayerPattern>)> {
    let (path, _) = crate::formats::record_image(store, project, kind, id)?;
    let contents = store.contents(project)?;
    let acquisition = match kind {
        RecordKind::Acquisition => contents
            .acquisitions
            .iter()
            .find(|a| a.id.to_string() == id),
        _ => {
            let result = contents.results.iter().find(|r| r.id.to_string() == id);
            let input = result.and_then(|r| {
                r.provenance.inputs.iter().find_map(|i| match i {
                    Input::Acquisition(a) => Some(*a),
                    _ => None,
                })
            });
            input.and_then(|a| contents.acquisitions.iter().find(|r| r.id == a))
        }
    };
    let raw = ImageReader::open(&path)?.info().channel_names == ["raw"];
    let pattern = acquisition
        .and_then(|a| serde_json::from_value::<PixelFormat>(a.camera["format"].clone()).ok())
        .and_then(PixelFormat::bayer)
        .filter(|_| raw);
    Ok((path, pattern))
}

/// Exports `kind` record `id`'s image of `project` as a static DZI folder,
/// `exports/<id>_dzi/` in its folder: `image.dzi`, and its tiles in
/// `image_files/`, through `display`, or as its thumbnail shows. A raw
/// image's colour overview is built for it, and deleted after. The
/// folder's path in the project's folder, as the data route serves it.
pub fn export_dzi(
    store: &Store,
    project: ProjectId,
    kind: RecordKind,
    id: &str,
    display: Option<Display>,
) -> Result<String> {
    let (path, pattern) = record_image(store, project, kind, id)?;
    let image = Arc::new(StoredImage::open(&path, 0)?);
    let display = match display {
        Some(display) => display,
        None => default_display(&image)?,
    };
    let exports = store
        .folder(project)
        .join(kind.folder())
        .join(id)
        .join("exports");
    fs::create_dir_all(&exports).map_err(crate::io(&exports))?;
    let overview_path = exports.join(".overview.ome.zarr");
    let raw = match pattern {
        Some(pattern) => {
            build_overview(&image, pattern, &overview_path)?;
            Some((
                pattern,
                Some(Arc::new(StoredImage::open(&overview_path, 0)?)),
            ))
        }
        None => None,
    };
    let cache = Arc::new(ChunkCache::new(256 << 20));
    let pyramid = StoredPyramid::new(image, cache, raw)?;
    let name = format!("{id}_dzi");
    let part = exports.join(format!(".{name}"));
    if part.exists() {
        fs::remove_dir_all(&part).map_err(crate::io(&part))?;
    }
    let table = DisplayTable::new(&display).map_err(failed)?;
    let written = write_dzi(&pyramid, &table, &part, "image").map_err(failed);
    drop(pyramid);
    if overview_path.exists() {
        fs::remove_dir_all(&overview_path).map_err(crate::io(&overview_path))?;
    }
    written?;
    let folder = exports.join(&name);
    if folder.exists() {
        fs::remove_dir_all(&folder).map_err(crate::io(&folder))?;
    }
    fs::rename(&part, &folder).map_err(crate::io(&folder))?;
    Ok(format!("{}/{id}/exports/{name}", kind.folder()))
}
