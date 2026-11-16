//! A scan's tiles, before and after the solve (P39; plan §5.15): which
//! overlap, the parts of each pair to register, and where each lands in the
//! mosaic, in the camera's pixels.

use serde::{Deserialize, Serialize};

/// The longest side a pair's parts are registered at, binned down to it.
pub const MOST_REGISTERED: u32 = 2048;

/// A rectangle of pixels, `[x0, x1) × [y0, y1)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PixelBox {
    /// Its left edge.
    pub x0: i64,
    /// Its top edge.
    pub y0: i64,
    /// One past its right edge.
    pub x1: i64,
    /// One past its bottom edge.
    pub y1: i64,
}

impl PixelBox {
    /// Its width.
    pub fn width(&self) -> i64 {
        self.x1 - self.x0
    }

    /// Its height.
    pub fn height(&self) -> i64 {
        self.y1 - self.y0
    }

    /// Where it meets `other`, if it does.
    pub fn meet(&self, other: &PixelBox) -> Option<PixelBox> {
        let met = PixelBox {
            x0: self.x0.max(other.x0),
            y0: self.y0.max(other.y0),
            x1: self.x1.min(other.x1),
            y1: self.y1.min(other.y1),
        };
        (met.x0 < met.x1 && met.y0 < met.y1).then_some(met)
    }
}

/// A scan's tiles: their size, and where the stage put each, in pixels.
#[derive(Clone, Debug, PartialEq)]
pub struct Tiles {
    /// A tile's width, in pixels.
    pub width: u32,
    /// Its height.
    pub height: u32,
    /// Each tile's nominal offset: M⁻¹ times its recorded position less the
    /// first tile's.
    pub nominal: Vec<[f64; 2]>,
    /// Whether the tiles are a colour camera's raw Bayer mosaic: registered
    /// on their 2 × 2 cells, and placed at even offsets.
    pub raw: bool,
}

/// Two tiles whose nominal frames overlap.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pair {
    /// The first tile.
    pub a: usize,
    /// The second.
    pub b: usize,
}

/// Where to cut a pair's parts to register, and what the stage predicts.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Crops {
    /// The part's top left in tile `a`, in its pixels.
    pub a: (u32, u32),
    /// In tile `b`.
    pub b: (u32, u32),
    /// The parts' width and height, in pixels, a multiple of `bin`.
    pub size: (u32, u32),
    /// How much the parts are binned before they are registered.
    pub bin: u32,
    /// The shift the stage predicts, in binned pixels.
    pub expected: (f64, f64),
}

impl Tiles {
    /// A tile's frame, at offset `at`.
    fn frame(&self, at: [f64; 2]) -> PixelBox {
        let (x, y) = (at[0].round() as i64, at[1].round() as i64);
        PixelBox {
            x0: x,
            y0: y,
            x1: x + i64::from(self.width),
            y1: y + i64::from(self.height),
        }
    }

    /// The pairs whose nominal frames overlap by `least` pixels along both
    /// axes, and by half a frame along one: side by side, not corner to
    /// corner. Each once, the lower index first.
    pub fn pairs(&self, least: u32) -> Vec<Pair> {
        let least = i64::from(least);
        let mut pairs = Vec::new();
        for a in 0..self.nominal.len() {
            for b in a + 1..self.nominal.len() {
                let met = self
                    .frame(self.nominal[a])
                    .meet(&self.frame(self.nominal[b]));
                let (half_w, half_h) = (i64::from(self.width) / 2, i64::from(self.height) / 2);
                let beside = |m: &PixelBox| m.width() >= half_w || m.height() >= half_h;
                if met.is_some_and(|m| m.width() >= least && m.height() >= least && beside(&m)) {
                    pairs.push(Pair { a, b });
                }
            }
        }
        pairs
    }

    /// The parts of `pair` to register: the same rectangle of the mosaic, as
    /// the stage has it, in each tile, binned to at most
    /// [`MOST_REGISTERED`] pixels a side, by 2 at least for raw tiles, and
    /// cut at even pixels for them.
    pub fn crops(&self, pair: Pair) -> Option<Crops> {
        let (oa, ob) = (self.nominal[pair.a], self.nominal[pair.b]);
        let met = self.frame(oa).meet(&self.frame(ob))?;
        let longest = met.width().max(met.height()) as u32;
        let mut bin = longest.div_ceil(MOST_REGISTERED).max(1);
        if self.raw {
            bin = bin.max(2).next_multiple_of(2);
        }
        let step = if self.raw { 2 } else { 1 };
        // Each part's corner in its tile, even for raw tiles: the frames are
        // at whole pixels, so the corners differ by whole pixels too.
        let corner = |o: [f64; 2]| {
            let (x, y) = (met.x0 - o[0].round() as i64, met.y0 - o[1].round() as i64);
            (x + x.rem_euclid(step), y + y.rem_euclid(step))
        };
        let (ca, cb) = (corner(oa), corner(ob));
        let room = |c: (i64, i64)| (i64::from(self.width) - c.0, i64::from(self.height) - c.1);
        let (ra, rb) = (room(ca), room(cb));
        let fit = |w: i64, r1: i64, r2: i64| w.min(r1).min(r2) / i64::from(bin) * i64::from(bin);
        let size = (fit(met.width(), ra.0, rb.0), fit(met.height(), ra.1, rb.1));
        if size.0 < 2 * i64::from(bin) || size.1 < 2 * i64::from(bin) {
            return None;
        }
        // The stage predicts that the same point is at `c_a` in a and `c_b`
        // in b, less the offsets' difference: what is left, rounding.
        let left = |axis: usize, ca: i64, cb: i64| {
            ((ca - cb) as f64 - (ob[axis] - oa[axis])) / f64::from(bin)
        };
        Some(Crops {
            a: (ca.0 as u32, ca.1 as u32),
            b: (cb.0 as u32, cb.1 as u32),
            size: (size.0 as u32, size.1 as u32),
            bin,
            expected: (left(0, ca.0, cb.0), left(1, ca.1, cb.1)),
        })
    }
}

impl Crops {
    /// The offsets' difference, `x_b − x_a`, that a registration of the parts
    /// found: B's content shifted by `(dx, dy)` binned pixels from A's.
    pub fn measured(&self, (dx, dy): (f64, f64)) -> [f64; 2] {
        let bin = f64::from(self.bin);
        [
            f64::from(self.a.0) - f64::from(self.b.0) - dx * bin,
            f64::from(self.a.1) - f64::from(self.b.1) - dy * bin,
        ]
    }
}

/// The tiles placed: each offset rounded to a whole pixel, or an even one
/// for raw tiles, from the mosaic's top left.
#[derive(Clone, Debug, PartialEq)]
pub struct Placed {
    /// Each tile's top left in the mosaic.
    pub at: Vec<(u64, u64)>,
    /// A tile's width and height.
    pub tile: (u32, u32),
    /// The mosaic's width and height.
    pub size: (u64, u64),
}

impl Placed {
    /// `offsets` placed, for tiles `tile` pixels, raw or not.
    pub fn new(offsets: &[[f64; 2]], tile: (u32, u32), raw: bool) -> Self {
        let step = if raw { 2.0 } else { 1.0 };
        let snapped: Vec<(i64, i64)> = offsets
            .iter()
            .map(|o| {
                let snap = |v: f64| ((v / step).round() * step) as i64;
                (snap(o[0]), snap(o[1]))
            })
            .collect();
        let (x0, y0) = snapped
            .iter()
            .fold((i64::MAX, i64::MAX), |(x, y), s| (x.min(s.0), y.min(s.1)));
        let at: Vec<(u64, u64)> = snapped
            .iter()
            .map(|s| ((s.0 - x0) as u64, (s.1 - y0) as u64))
            .collect();
        let (w, h) = at.iter().fold((0, 0), |(w, h), a| {
            (
                w.max(a.0 + u64::from(tile.0)),
                h.max(a.1 + u64::from(tile.1)),
            )
        });
        Self {
            at,
            tile,
            size: (w, h),
        }
    }

    /// The tiles over `chunk`, each with the part of it over the chunk, in
    /// the tile's pixels.
    pub fn over(&self, chunk: PixelBox) -> Vec<(usize, PixelBox)> {
        let (tw, th) = (i64::from(self.tile.0), i64::from(self.tile.1));
        self.at
            .iter()
            .enumerate()
            .filter_map(|(k, &(x, y))| {
                let (x, y) = (x as i64, y as i64);
                let frame = PixelBox {
                    x0: x,
                    y0: y,
                    x1: x + tw,
                    y1: y + th,
                };
                let met = frame.meet(&chunk)?;
                let part = PixelBox {
                    x0: met.x0 - x,
                    y0: met.y0 - y,
                    x1: met.x1 - x,
                    y1: met.y1 - y,
                };
                Some((k, part))
            })
            .collect()
    }
}
