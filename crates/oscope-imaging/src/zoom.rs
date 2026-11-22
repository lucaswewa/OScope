//! Deep zoom (plan §5.15, ADR-0028): an image cut into the tiles of
//! Microsoft's Deep Zoom format (DZI), 256 pixels square without overlap,
//! each shown through a display transform and encoded as JPEG.
//!
//! - **Levels:** level `n` of a DZI image `w` pixels wide is `⌈w / 2^(N − n)⌉`
//!   wide, `N` its last, full-size level; level 0 is a pixel. An OME-Zarr
//!   pyramid's level `k`, each level half the one before rounded up, is the
//!   same size as DZI level `N − k`.
//! - **Tiles** start at multiples of 256 at each level, the last of a row or
//!   column smaller, so that a 512-pixel chunk holds 2 × 2 of them.
//! - **Past a pyramid's coarsest level,** a tile is cut from that level,
//!   box-averaged further.

use std::path::{Path, PathBuf};

use oscope_compute::{ComputeError, Image, Shape, cpu};
use oscope_core::PixelRect;
use rayon::prelude::*;

use crate::color::{ColorError, DisplayTable, crop, encode_jpeg};

/// A tile's side, in pixels.
pub const TILE: u64 = 256;

/// The JPEG quality of tiles.
pub const QUALITY: u8 = 85;

/// What deep zoom can't do.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ZoomError {
    /// Showing or encoding a tile failed.
    #[error(transparent)]
    Color(#[from] ColorError),
    /// Averaging a tile failed.
    #[error(transparent)]
    Compute(#[from] ComputeError),
    /// The pyramid couldn't be read.
    #[error("{0}")]
    Source(String),
    /// A file couldn't be written.
    #[error("{path}: {source}")]
    Io {
        /// The file or folder.
        path: PathBuf,
        /// Why.
        source: std::io::Error,
    },
}

/// The levels and tiles of a DZI image of a size.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Dzi {
    /// Its full width.
    pub width: u64,
    /// Its full height.
    pub height: u64,
}

/// A rectangle of a level, in its pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Area {
    /// Its left edge.
    pub x: u64,
    /// Its top edge.
    pub y: u64,
    /// Its width.
    pub width: u64,
    /// Its height.
    pub height: u64,
}

impl Dzi {
    /// Its levels: from a pixel to the full size, `⌈log₂ longest⌉ + 1`.
    pub fn levels(&self) -> u32 {
        let longest = self.width.max(self.height).max(1);
        u64::BITS - (longest - 1).leading_zeros() + 1
    }

    /// How many times `level` is halved from the full size.
    pub fn halvings(&self, level: u32) -> u32 {
        self.levels() - 1 - level
    }

    /// `level`'s size.
    pub fn size(&self, level: u32) -> (u64, u64) {
        let factor = 1_u64 << self.halvings(level);
        (self.width.div_ceil(factor), self.height.div_ceil(factor))
    }

    /// The tiles across and down `level`.
    pub fn tiles(&self, level: u32) -> (u64, u64) {
        let (width, height) = self.size(level);
        (width.div_ceil(TILE), height.div_ceil(TILE))
    }

    /// Tile `(col, row)` of `level`, in its pixels: none if there is no such
    /// tile.
    pub fn tile(&self, level: u32, (col, row): (u64, u64)) -> Option<Area> {
        if level >= self.levels() {
            return None;
        }
        let (width, height) = self.size(level);
        let (x, y) = (col.checked_mul(TILE)?, row.checked_mul(TILE)?);
        (x < width && y < height).then(|| Area {
            x,
            y,
            width: TILE.min(width - x),
            height: TILE.min(height - y),
        })
    }

    /// Its descriptor, the `.dzi` file's XML.
    pub fn descriptor(&self) -> String {
        format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<Image xmlns=\"http://schemas.microsoft.com/deepzoom/2008\" \
             TileSize=\"{TILE}\" Overlap=\"0\" Format=\"jpg\"><Size Width=\"{}\" Height=\"{}\"/></Image>\n",
            self.width, self.height
        )
    }
}

/// What tiles are cut from: an image's samples, from 0 to 1, at its full
/// size and halved, each halving rounded up.
pub trait Pyramid: Sync {
    /// Its full size.
    fn size(&self) -> (u64, u64);
    /// The channels it gives: 1 for grey, 3 for RGB.
    fn channels(&self) -> u32;
    /// The most halvings it reads; tiles past them are averaged further.
    fn coarsest(&self) -> u32;
    /// The `size` pixels at `at` of the image halved `halvings` times, its
    /// channels interleaved.
    fn read(&self, halvings: u32, at: (u64, u64), size: (u64, u64)) -> Result<Vec<f32>, ZoomError>;
}

/// Tile `(col, row)` of `level` of `pyramid`'s DZI image, as RGB through
/// `display`'s table, and its size: none if there is no such tile.
pub fn tile_rgb(
    pyramid: &dyn Pyramid,
    level: u32,
    tile: (u64, u64),
    display: &DisplayTable,
) -> Result<Option<(Vec<u8>, u32, u32)>, ZoomError> {
    let (width, height) = pyramid.size();
    let dzi = Dzi { width, height };
    let Some(area) = dzi.tile(level, tile) else {
        return Ok(None);
    };
    let halvings = dzi.halvings(level);
    let direct = halvings.min(pyramid.coarsest());
    let factor = 1_u64 << (halvings - direct);
    // The tile's pixels at the level read: `factor` times as many, within
    // that level, which rounds as the tile's own level does.
    let (wide, high) = (width.div_ceil(1 << direct), height.div_ceil(1 << direct));
    let (x, y) = (area.x * factor, area.y * factor);
    let size = (
        (area.width * factor).min(wide - x),
        (area.height * factor).min(high - y),
    );
    let channels = pyramid.channels();
    let samples = pyramid.read(direct, (x, y), size)?;
    let shape = Shape::new(size.0 as u32, size.1 as u32, channels as usize);
    let mut image = Image::from_vec(shape, samples)?;
    if factor > 1 {
        image = cpu::downsample(&image, factor as u32)?;
    }
    let shown = display.apply(&image);
    Ok(Some((rgb(&shown, channels), image.width(), image.height())))
}

/// Tile `(col, row)` of `level` of `pyramid`'s DZI image through
/// `display`'s table, as JPEG: none if there is no such tile.
pub fn tile(
    pyramid: &dyn Pyramid,
    level: u32,
    tile: (u64, u64),
    display: &DisplayTable,
) -> Result<Option<Vec<u8>>, ZoomError> {
    let Some((rgb, width, height)) = tile_rgb(pyramid, level, tile, display)? else {
        return Ok(None);
    };
    Ok(Some(encode_jpeg(&rgb, width, height, QUALITY)?))
}

/// `shown`'s pixels of `channels` as RGB: grey to all three, two channels
/// as red and green, and a fourth left out.
fn rgb(shown: &[u8], channels: u32) -> Vec<u8> {
    shown
        .chunks_exact(channels as usize)
        .flat_map(|p| match p.len() {
            1 => [p[0]; 3],
            2 => [p[0], p[1], 0],
            _ => [p[0], p[1], p[2]],
        })
        .collect()
}

/// Writes `pyramid`'s DZI image to `folder`, as `name.dzi` and its tiles in
/// `name_files/<level>/<col>_<row>.jpg`, every level, through `display`:
/// how many tiles.
pub fn write_dzi(
    pyramid: &dyn Pyramid,
    display: &DisplayTable,
    folder: &Path,
    name: &str,
) -> Result<u64, ZoomError> {
    let io = |path: &Path| {
        let path = path.to_owned();
        move |source| ZoomError::Io { path, source }
    };
    let (width, height) = pyramid.size();
    let dzi = Dzi { width, height };
    std::fs::create_dir_all(folder).map_err(io(folder))?;
    let descriptor = folder.join(format!("{name}.dzi"));
    std::fs::write(&descriptor, dzi.descriptor()).map_err(io(&descriptor))?;
    let mut count = 0;
    for level in 0..dzi.levels() {
        let dir = folder.join(format!("{name}_files")).join(level.to_string());
        std::fs::create_dir_all(&dir).map_err(io(&dir))?;
        let (cols, rows) = dzi.tiles(level);
        let tiles: Vec<(u64, u64)> = (0..rows)
            .flat_map(|row| (0..cols).map(move |col| (col, row)))
            .collect();
        tiles.par_iter().try_for_each(|&(col, row)| {
            let jpeg = tile(pyramid, level, (col, row), display)?.unwrap_or_default();
            let path = dir.join(format!("{col}_{row}.jpg"));
            std::fs::write(&path, jpeg).map_err(io(&path))
        })?;
        count += tiles.len() as u64;
    }
    Ok(count)
}

/// An image in memory, and its levels, each half the one before, rounded
/// up, down to a tile.
pub struct InMemory {
    levels: Vec<Image>,
}

impl InMemory {
    /// `image`, its samples from 0 to 1, and its levels.
    pub fn new(image: Image) -> Result<Self, ZoomError> {
        let mut levels = vec![image];
        while let Some(last) = levels.last()
            && u64::from(last.width().max(last.height())) > TILE
        {
            let half = cpu::downsample(last, 2)?;
            levels.push(half);
        }
        Ok(InMemory { levels })
    }
}

impl Pyramid for InMemory {
    fn size(&self) -> (u64, u64) {
        (
            u64::from(self.levels[0].width()),
            u64::from(self.levels[0].height()),
        )
    }

    fn channels(&self) -> u32 {
        self.levels[0].channels() as u32
    }

    fn coarsest(&self) -> u32 {
        self.levels.len() as u32 - 1
    }

    fn read(&self, halvings: u32, at: (u64, u64), size: (u64, u64)) -> Result<Vec<f32>, ZoomError> {
        let level = self
            .levels
            .get(halvings as usize)
            .ok_or_else(|| ZoomError::Source(format!("no level halved {halvings} times")))?;
        let rect = PixelRect::new(at.0 as u32, at.1 as u32, size.0 as u32, size.1 as u32);
        Ok(crop(level, rect)?.data().to_vec())
    }
}
