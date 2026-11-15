//! The live mosaic's canvas (ADR-0026; plan §5.15): a pyramid of 256-pixel
//! tiles over a rectangle of the stage, which frames are drawn into as they
//! arrive.
//!
//! - **Levels:** level 0 has the canvas's own pixel, and each level above
//!   twice the one below, up to the first whose whole canvas fits a tile.
//!   A level's size is the one below's halved and rounded up, as Deep
//!   Zoom's are.
//! - **A tile** holds 8-bit RGBA by rows. Alpha is how much of a pixel the
//!   frames have covered: 0 or 255 at level 0, and above it the mean of the
//!   four pixels below.
//! - **A frame** is drawn through an affine map from its pixels to level
//!   0's, each canvas pixel sampled bilinearly at its centre. The newest
//!   frame wins, or its edges are feathered into what is there.
//! - **Memory is bounded:** past its budget, the tiles used least recently
//!   go to a folder, and come back when wanted.
//! - **Changes are numbered:** each drawing that changes a tile is a
//!   generation, and each tile keeps the one it last changed in, so a viewer
//!   asks which changed since the last it saw.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A tile's side, in pixels.
pub const TILE: u32 = 256;
/// A tile's bytes: RGBA.
pub const TILE_BYTES: usize = (TILE * TILE * 4) as usize;
/// The fewest tiles a budget holds.
pub const LEAST_TILES: usize = 5;
/// The longest side a canvas has at level 0, in pixels: 8 m of 2 µm.
pub const MOST_PIXELS: u32 = 1 << 22;
/// The canvases made: each names its files by its number, so that two in
/// one folder, one going as another comes, never share a file.
static MADE: AtomicU64 = AtomicU64::new(0);

/// What a canvas can't do.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum CanvasError {
    /// Its geometry or budget can't be, or a blend.
    #[error("{0}")]
    Setup(String),
    /// An image to draw isn't the size it says.
    #[error("{0}")]
    Image(String),
    /// The map to the canvas squashes the image flat.
    #[error("the map to the canvas can't be inverted")]
    Singular,
    /// Its folder couldn't be written or read.
    #[error("the canvas's folder: {0}")]
    Io(#[from] std::io::Error),
}

/// A tile, by its level and its column and row from the top left.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
pub struct TileKey {
    /// Its level: 0 at the canvas's own pixel.
    pub level: u32,
    /// Its column.
    pub col: u32,
    /// Its row.
    pub row: u32,
}

/// Where a canvas lies on the stage, and how fine it is.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CanvasGeometry {
    /// The stage's x at the canvas's left edge, in µm.
    pub x_um: f64,
    /// The stage's y at its top edge.
    pub y_um: f64,
    /// A pixel's side at level 0, in µm.
    pub pixel_um: f64,
    /// Its width at level 0, in pixels.
    pub width: u32,
    /// Its height at level 0.
    pub height: u32,
}

impl CanvasGeometry {
    /// The canvas from `(x_um, y_um)` over `width_um` × `height_um` of the
    /// stage, in pixels of `pixel_um`.
    pub fn covering(
        (x_um, y_um): (f64, f64),
        (width_um, height_um): (f64, f64),
        pixel_um: f64,
    ) -> Result<Self, CanvasError> {
        let (w, h) = ((width_um / pixel_um).ceil(), (height_um / pixel_um).ceil());
        if !(w >= 1.0 && h >= 1.0 && w.max(h) <= f64::from(MOST_PIXELS)) {
            return Err(CanvasError::Setup(format!(
                "{width_um} × {height_um} µm in pixels of {pixel_um} µm"
            )));
        }
        let geometry = Self {
            x_um,
            y_um,
            pixel_um,
            width: w as u32,
            height: h as u32,
        };
        geometry.check()?;
        Ok(geometry)
    }

    /// Checks it: a finite corner, a pixel over 0, and from 1 to
    /// [`MOST_PIXELS`] pixels a side.
    pub fn check(&self) -> Result<(), CanvasError> {
        let finite = self.x_um.is_finite() && self.y_um.is_finite() && self.pixel_um.is_finite();
        let sides = (1..=MOST_PIXELS).contains(&self.width.min(self.height))
            && self.width.max(self.height) <= MOST_PIXELS;
        if !finite || self.pixel_um <= 0.0 || !sides {
            return Err(CanvasError::Setup(format!(
                "a canvas of {} × {} pixels of {} µm at ({}, {}) µm",
                self.width, self.height, self.pixel_um, self.x_um, self.y_um
            )));
        }
        Ok(())
    }

    /// How many levels it has: up to the first that fits a tile.
    pub fn levels(&self) -> u32 {
        let (mut side, mut levels) = (self.width.max(self.height), 1);
        while side > TILE {
            side = side.div_ceil(2);
            levels += 1;
        }
        levels
    }

    /// Its width and height at `level`, in pixels.
    pub fn size(&self, level: u32) -> (u32, u32) {
        let halve = |side: u32| (0..level).fold(side, |side, _| side.div_ceil(2));
        (halve(self.width), halve(self.height))
    }

    /// Its columns and rows of tiles at `level`.
    pub fn tiles(&self, level: u32) -> (u32, u32) {
        let (width, height) = self.size(level);
        (width.div_ceil(TILE), height.div_ceil(TILE))
    }

    /// Where the stage point `(x_um, y_um)` lies at level 0, in pixels from
    /// the top left corner: the first pixel's centre is `(0.5, 0.5)`.
    pub fn pixel_at(&self, x_um: f64, y_um: f64) -> (f64, f64) {
        (
            (x_um - self.x_um) / self.pixel_um,
            (y_um - self.y_um) / self.pixel_um,
        )
    }
}

/// How a frame is drawn over what a canvas holds.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Blend {
    /// The newest frame wins wherever it lies.
    #[default]
    Newest,
    /// Its edges fade into what is there, over `width_px` canvas pixels;
    /// where nothing is, it is drawn whole.
    Feather {
        /// How far in from its edges it fades, in canvas pixels.
        width_px: f64,
    },
}

/// An affine map of the plane, by rows: `(x, y)` to
/// `(a x + b y + c, d x + e y + f)`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Affine(pub [[f64; 3]; 2]);

impl Affine {
    /// Where it takes `(x, y)`.
    pub fn apply(&self, x: f64, y: f64) -> (f64, f64) {
        let [[a, b, c], [d, e, f]] = self.0;
        (a * x + b * y + c, d * x + e * y + f)
    }

    /// Its inverse, if it has one.
    pub fn inverse(&self) -> Option<Self> {
        let [[a, b, c], [d, e, f]] = self.0;
        let det = a * e - b * d;
        if !(det.abs() > 1e-12 && det.is_finite() && c.is_finite() && f.is_finite()) {
            return None;
        }
        let (ia, ib, id, ie) = (e / det, -b / det, -d / det, a / det);
        Some(Self([
            [ia, ib, -(ia * c + ib * f)],
            [id, ie, -(id * c + ie * f)],
        ]))
    }

    /// How much it scales lengths, on average: `√|det|`.
    pub fn scale(&self) -> f64 {
        let [[a, b, _], [d, e, _]] = self.0;
        (a * e - b * d).abs().sqrt()
    }
}

/// An 8-bit RGB image to draw, by rows.
#[derive(Clone, Copy, Debug)]
pub struct Rgb<'a> {
    /// Its samples: red, green and blue for each pixel.
    pub pixels: &'a [u8],
    /// Its width, in pixels.
    pub width: u32,
    /// Its height.
    pub height: u32,
}

/// What changed on a canvas since a generation.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Changes {
    /// The canvas's generation: the one to ask from next.
    pub generation: u64,
    /// Whether any tile may have changed: the canvas was cleared since, or
    /// the generation asked from isn't one it had. A viewer then fetches all
    /// again.
    pub all: bool,
    /// The tiles that changed, each once, when not `all`.
    pub tiles: Vec<TileKey>,
}

struct Resident {
    pixels: Vec<u8>,
    used: u64,
}

/// The level-0 pixels a drawing may cover.
struct Span {
    x: Range<u32>,
    y: Range<u32>,
}

/// A live mosaic's canvas. See [the module](self).
pub struct Canvas {
    geometry: CanvasGeometry,
    levels: u32,
    /// The most tiles in memory.
    budget: usize,
    folder: PathBuf,
    /// Its number, which its files are named by.
    serial: u64,
    resident: HashMap<TileKey, Resident>,
    stored: HashSet<TileKey>,
    uses: u64,
    generation: u64,
    /// Each tile's generation when it last changed.
    changed: HashMap<TileKey, u64>,
    /// The generation it was last cleared in.
    cleared: u64,
}

impl Canvas {
    /// An empty canvas over `geometry`, with at most `budget` bytes of tiles
    /// in memory and the rest in `folder`. It removes the tiles canvases
    /// left in `folder` before.
    pub fn new(
        geometry: CanvasGeometry,
        budget: usize,
        folder: impl Into<PathBuf>,
    ) -> Result<Self, CanvasError> {
        geometry.check()?;
        let tiles = budget / TILE_BYTES;
        if tiles < LEAST_TILES {
            return Err(CanvasError::Setup(format!(
                "a budget of {budget} bytes holds {tiles} tiles, under {LEAST_TILES}"
            )));
        }
        let folder = folder.into();
        empty(&folder)?;
        Ok(Self {
            geometry,
            levels: geometry.levels(),
            budget: tiles,
            folder,
            serial: MADE.fetch_add(1, Ordering::Relaxed),
            resident: HashMap::new(),
            stored: HashSet::new(),
            uses: 0,
            generation: 0,
            changed: HashMap::new(),
            cleared: 0,
        })
    }

    /// Where it lies, and how fine it is.
    pub fn geometry(&self) -> CanvasGeometry {
        self.geometry
    }

    /// Its generation: how many drawings and clearings have changed it.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// The bytes its tiles take in memory.
    pub fn resident_bytes(&self) -> usize {
        self.resident.len() * TILE_BYTES
    }

    /// How many tiles it has, at every level, in memory or in its folder.
    pub fn tile_count(&self) -> usize {
        self.resident.len() + self.stored.len()
    }

    /// Draws `image` through `to_canvas`, which takes the image's
    /// coordinates, `(0.5, 0.5)` its first pixel's centre, to level 0's.
    /// Returns the tiles it changed, at every level: one generation, unless
    /// none.
    pub fn draw(
        &mut self,
        image: Rgb<'_>,
        to_canvas: &Affine,
        blend: Blend,
    ) -> Result<Vec<TileKey>, CanvasError> {
        let (w, h) = (image.width as usize, image.height as usize);
        if w < 2 || h < 2 || image.pixels.len() != w * h * 3 {
            return Err(CanvasError::Image(format!(
                "{} bytes for {w} × {h} RGB, at least 2 × 2",
                image.pixels.len()
            )));
        }
        if let Blend::Feather { width_px } = blend
            && !(width_px > 0.0 && width_px.is_finite())
        {
            return Err(CanvasError::Setup(format!("a feather {width_px} px wide")));
        }
        let from = to_canvas.inverse().ok_or(CanvasError::Singular)?;
        let corners = [
            (0.0, 0.0),
            (w as f64, 0.0),
            (0.0, h as f64),
            (w as f64, h as f64),
        ]
        .map(|(x, y)| to_canvas.apply(x, y));
        // The level-0 pixels within its corners' bounds.
        let bounds = |pick: fn(&(f64, f64)) -> f64, limit: u32| {
            let low = corners.iter().map(pick).fold(f64::INFINITY, f64::min);
            let high = corners.iter().map(pick).fold(f64::NEG_INFINITY, f64::max);
            (low.floor().max(0.0), high.ceil().min(f64::from(limit)))
        };
        let xs = bounds(|c| c.0, self.geometry.width);
        let ys = bounds(|c| c.1, self.geometry.height);
        if !(xs.0 < xs.1 && ys.0 < ys.1) {
            return Ok(Vec::new());
        }
        let span = Span {
            x: xs.0 as u32..xs.1 as u32,
            y: ys.0 as u32..ys.1 as u32,
        };
        let scale = to_canvas.scale();
        let mut changed = Vec::new();
        for row in span.y.start / TILE..=(span.y.end - 1) / TILE {
            for col in span.x.start / TILE..=(span.x.end - 1) / TILE {
                let key = TileKey { level: 0, col, row };
                let taken = self.take(key)?;
                let existed = taken.is_some();
                let mut pixels = taken.unwrap_or_else(|| vec![0; TILE_BYTES]);
                let touched = paint(&mut pixels, key, image, &from, scale, blend, &span);
                if touched || existed {
                    self.insert(key, pixels)?;
                }
                if touched {
                    changed.push(key);
                }
            }
        }
        // Each level above, a quarter of a tile for each tile changed below.
        let mut below: BTreeSet<TileKey> = changed.iter().copied().collect();
        for _ in 1..self.levels {
            let mut above = BTreeSet::new();
            for &key in &below {
                above.insert(self.reduce(key)?);
            }
            changed.extend(above.iter().copied());
            below = above;
        }
        if !changed.is_empty() {
            self.generation += 1;
            let generation = self.generation;
            self.changed
                .extend(changed.iter().map(|&key| (key, generation)));
        }
        Ok(changed)
    }

    /// The tiles changed since `generation`: all, from before it was last
    /// cleared, or from a generation it hasn't had.
    pub fn changes_since(&self, generation: u64) -> Changes {
        if generation < self.cleared || generation > self.generation {
            return Changes {
                generation: self.generation,
                all: true,
                tiles: Vec::new(),
            };
        }
        let tiles: BTreeSet<TileKey> = self
            .changed
            .iter()
            .filter(|(_, g)| **g > generation)
            .map(|(key, _)| *key)
            .collect();
        Changes {
            generation: self.generation,
            all: false,
            tiles: tiles.into_iter().collect(),
        }
    }

    /// Tile `key`'s RGBA, if a frame has been drawn on it.
    pub fn tile(&mut self, key: TileKey) -> Result<Option<Vec<u8>>, CanvasError> {
        Ok(self.pixels(key)?.map(<[u8]>::to_vec))
    }

    /// Empties it: every tile goes, and a viewer is told all changed.
    pub fn clear(&mut self) -> Result<(), CanvasError> {
        self.resident.clear();
        self.stored.clear();
        empty(&self.folder)?;
        self.generation += 1;
        self.changed.clear();
        self.cleared = self.generation;
        Ok(())
    }

    fn file(&self, key: TileKey) -> PathBuf {
        let name = format!("{}-{}-{}-{}.rgba", self.serial, key.level, key.col, key.row);
        self.folder.join(name)
    }

    /// Makes `key` resident, if it has been drawn, and marks it used.
    fn load(&mut self, key: TileKey) -> Result<bool, CanvasError> {
        self.uses += 1;
        if let Some(tile) = self.resident.get_mut(&key) {
            tile.used = self.uses;
            return Ok(true);
        }
        if !self.stored.contains(&key) {
            return Ok(false);
        }
        let pixels = std::fs::read(self.file(key))?;
        if pixels.len() != TILE_BYTES {
            let why = format!("tile {key:?} has {} bytes", pixels.len());
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, why).into());
        }
        self.stored.remove(&key);
        self.insert(key, pixels)?;
        Ok(true)
    }

    fn pixels(&mut self, key: TileKey) -> Result<Option<&[u8]>, CanvasError> {
        Ok(match self.load(key)? {
            true => self.resident.get(&key).map(|t| t.pixels.as_slice()),
            false => None,
        })
    }

    fn take(&mut self, key: TileKey) -> Result<Option<Vec<u8>>, CanvasError> {
        Ok(match self.load(key)? {
            true => self.resident.remove(&key).map(|t| t.pixels),
            false => None,
        })
    }

    /// Keeps `pixels` as `key` in memory, first moving the tiles used least
    /// recently to the folder until it is within its budget.
    fn insert(&mut self, key: TileKey, pixels: Vec<u8>) -> Result<(), CanvasError> {
        while self.resident.len() >= self.budget && !self.resident.contains_key(&key) {
            let oldest = self
                .resident
                .iter()
                .min_by_key(|(_, t)| t.used)
                .map(|(k, _)| *k);
            let Some(tile) = oldest.and_then(|k| self.resident.remove(&k).map(|t| (k, t))) else {
                break;
            };
            std::fs::write(self.file(tile.0), &tile.1.pixels)?;
            self.stored.insert(tile.0);
        }
        self.resident.insert(
            key,
            Resident {
                pixels,
                used: self.uses,
            },
        );
        Ok(())
    }

    /// Makes `key`'s quarter of the tile above it from `key`: each pixel the
    /// mean of four, weighted by how much each is covered. Returns the tile
    /// above.
    fn reduce(&mut self, key: TileKey) -> Result<TileKey, CanvasError> {
        let (side, half) = (TILE as usize, TILE as usize / 2);
        let mut quarter = vec![0_u8; half * half * 4];
        if let Some(pixels) = self.pixels(key)? {
            for (k, out) in quarter.as_chunks_mut::<4>().0.iter_mut().enumerate() {
                let (x, y) = (k % half, k / half);
                let (mut sum, mut weight) = ([0_u32; 3], 0_u32);
                for (sx, sy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                    let at = ((2 * y + sy) * side + 2 * x + sx) * 4;
                    let a = u32::from(pixels[at + 3]);
                    for (s, &p) in sum.iter_mut().zip(&pixels[at..at + 3]) {
                        *s += u32::from(p) * a;
                    }
                    weight += a;
                }
                for (o, s) in out.iter_mut().zip(sum) {
                    *o = (s + weight / 2).checked_div(weight).unwrap_or(0) as u8;
                }
                out[3] = ((weight + 2) / 4) as u8;
            }
        }
        let above = TileKey {
            level: key.level + 1,
            col: key.col / 2,
            row: key.row / 2,
        };
        let mut pixels = self.take(above)?.unwrap_or_else(|| vec![0; TILE_BYTES]);
        let (left, top) = ((key.col % 2) as usize * half, (key.row % 2) as usize * half);
        for (y, row) in quarter.chunks_exact(half * 4).enumerate() {
            let at = ((top + y) * side + left) * 4;
            pixels[at..at + half * 4].copy_from_slice(row);
        }
        self.insert(above, pixels)?;
        Ok(above)
    }
}

/// Draws `image` on tile `key`'s `pixels` within `span`: whether it drew a
/// pixel. `scale` is canvas pixels to the image's.
fn paint(
    pixels: &mut [u8],
    key: TileKey,
    image: Rgb<'_>,
    from: &Affine,
    scale: f64,
    blend: Blend,
    span: &Span,
) -> bool {
    let (left, top) = (key.col * TILE, key.row * TILE);
    let xs = span.x.start.max(left)..span.x.end.min(left + TILE);
    let ys = span.y.start.max(top)..span.y.end.min(top + TILE);
    let (w, h) = (f64::from(image.width), f64::from(image.height));
    let mut touched = false;
    for y in ys {
        for x in xs.clone() {
            let (u, v) = from.apply(f64::from(x) + 0.5, f64::from(y) + 0.5);
            // Between its outer pixels' centres, where bilinear sampling has
            // four pixels to take.
            if !(u >= 0.5 && v >= 0.5 && u <= w - 0.5 && v <= h - 0.5) {
                continue;
            }
            let colour = bilinear(image, u - 0.5, v - 0.5);
            let weight = match blend {
                Blend::Newest => 1.0,
                Blend::Feather { width_px } => u.min(v).min(w - u).min(h - v) * scale / width_px,
            };
            let at = (((y - top) * TILE + (x - left)) * 4) as usize;
            let pixel = &mut pixels[at..at + 4];
            let whole = pixel[3] == 0 || weight >= 1.0;
            for (p, c) in pixel.iter_mut().zip(colour) {
                let old = f64::from(*p);
                let new = if whole { c } else { old + weight * (c - old) };
                *p = new.round().clamp(0.0, 255.0) as u8;
            }
            pixel[3] = 255;
            touched = true;
        }
    }
    touched
}

/// `image` at `(u, v)`, where pixel `(i, j)`'s centre is `(i, j)`, within
/// its outer pixels' centres.
fn bilinear(image: Rgb<'_>, u: f64, v: f64) -> [f64; 3] {
    let (w, h) = (image.width as usize, image.height as usize);
    let (x0, y0) = (
        (u.floor() as usize).min(w - 2),
        (v.floor() as usize).min(h - 2),
    );
    let (fx, fy) = (u - x0 as f64, v - y0 as f64);
    let at = |x: usize, y: usize, c: usize| f64::from(image.pixels[(y * w + x) * 3 + c]);
    std::array::from_fn(|c| {
        let top = at(x0, y0, c) + fx * (at(x0 + 1, y0, c) - at(x0, y0, c));
        let bottom = at(x0, y0 + 1, c) + fx * (at(x0 + 1, y0 + 1, c) - at(x0, y0 + 1, c));
        top + fy * (bottom - top)
    })
}

/// Makes `folder`, and removes the tiles in it.
fn empty(folder: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(folder)?;
    for entry in std::fs::read_dir(folder)? {
        let path = entry?.path();
        if path.extension().is_some_and(|e| e == "rgba") {
            std::fs::remove_file(path)?;
        }
    }
    Ok(())
}
