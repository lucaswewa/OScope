//! Live statistics (plan §5.5), measured on the raw frame, not its 8-bit
//! preview, and sent with a client's preview at most 10 times a second
//! (ADR-0035):
//!
//! - **histograms** of each channel at 12 bits, from a subsample, with
//!   percentiles and the share of samples at full scale;
//! - **a saturation mask** at the preview's resolution, a bit a pixel;
//! - **ROI statistics:** each channel's mean, standard deviation, minimum
//!   and maximum;
//! - **a line profile** across the raw frame, cell by cell;
//! - **a focus value:** the normalised variance of the green cells (Groen,
//!   Young and Ligthart 1985).
//!
//! Values are in the sensor's own units: a 12-bit sensor's run from 0 to
//! 4095, whatever the frame's sample width. A mosaic's channels are its
//! colours, each sample counted in its own; a cell's two greens are
//! averaged where a cell gives one value.

use oscope_compute::cpu;
use oscope_core::{BayerColour, Frame, FrameMeta, FrameView, PixelFormat, PixelRect};
use rayon::prelude::*;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::preview::{PreviewError, region};

/// The levels a histogram is counted at: 12 bits.
const LEVELS: u64 = 4096;
/// The most samples a histogram or a focus value reads: larger frames are
/// subsampled, by whole cells.
const SUBSAMPLE: u64 = 1 << 20;
/// The most samples a profile takes.
const MOST_SAMPLES: usize = 10_000;
/// The percentiles given, in per cent.
pub const PERCENTILES: [f64; 5] = [0.1, 1.0, 50.0, 99.0, 99.9];

/// What a client asks to be measured.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct StatsRequest {
    /// The sensor's pixels to give statistics for, in full-sensor
    /// coordinates, which the focus value is also measured over: the
    /// preview's region when not given.
    pub roi: Option<PixelRect>,
    /// A line to profile, in full-sensor coordinates.
    pub profile: Option<Line>,
    /// The histograms' bins, from 1 to 4096.
    pub bins: u16,
    /// Whether to send the saturation mask.
    pub mask: bool,
}

impl Default for StatsRequest {
    /// Histograms of 256 bins, the mask, and the focus value.
    fn default() -> Self {
        Self {
            roi: None,
            profile: None,
            bins: 256,
            mask: true,
        }
    }
}

/// A line across the sensor, in full-sensor coordinates.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Line {
    /// Where it starts: x and y.
    pub from: [f64; 2],
    /// Where it ends.
    pub to: [f64; 2],
}

/// A frame's statistics.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LiveStats {
    /// The frame measured.
    pub frame: u64,
    /// The channels, in order: `red`, `green` and `blue`, or `grey`.
    pub channels: Vec<String>,
    /// The largest value a sample can have: 4095 for 12 bits.
    pub full_scale: u32,
    /// How many samples of each channel the histograms counted.
    pub counted: Vec<u64>,
    /// Each channel's histogram: the bins asked for, the first from 0, each
    /// `(full_scale + 1) / bins` wide.
    pub histogram: Vec<Vec<u64>>,
    /// Each channel's values at [`PERCENTILES`].
    pub percentiles: Vec<Vec<f64>>,
    /// Each channel's share of samples at full scale, from 0 to 1.
    pub saturated: Vec<f64>,
    /// The ROI's statistics, when asked for.
    pub roi: Option<RoiStats>,
    /// The profile, when asked for.
    pub profile: Option<Profile>,
    /// The focus value: higher when sharper.
    pub focus: f64,
}

/// A region's statistics, channel by channel.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RoiStats {
    /// The sensor's pixels measured: the ROI clipped to the frame, on whole
    /// cells of a mosaic.
    pub rect: PixelRect,
    /// How many samples each channel has.
    pub count: Vec<u64>,
    /// Their mean.
    pub mean: Vec<f64>,
    /// Their standard deviation.
    pub std: Vec<f64>,
    /// The smallest.
    pub min: Vec<u32>,
    /// The largest.
    pub max: Vec<u32>,
}

/// Values along a line, cell by cell.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Profile {
    /// The line asked for.
    pub line: Line,
    /// The sensor pixels between samples.
    pub step: f64,
    /// Each sample's channels, or none where the line is off the frame.
    pub samples: Vec<Option<Vec<f64>>>,
}

/// Where the preview shows samples at full scale: a bit a pixel.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mask {
    /// The preview's width.
    pub width: u32,
    /// Its height.
    pub height: u32,
    /// Rows of ⌈width / 8⌉ bytes, each pixel's bit from the highest down.
    pub bits: Vec<u8>,
}

impl Mask {
    fn stride(width: u32) -> usize {
        width.div_ceil(8) as usize
    }

    /// Whether the pixel at `(x, y)` is set.
    pub fn get(&self, x: u32, y: u32) -> bool {
        let byte = self.bits[y as usize * Self::stride(self.width) + x as usize / 8];
        byte & (0x80 >> (x % 8)) != 0
    }

    /// Its bytes in a block (ADR-0035): width and height as little-endian
    /// `u32`s, then the rows.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(8 + self.bits.len());
        out.extend_from_slice(&self.width.to_le_bytes());
        out.extend_from_slice(&self.height.to_le_bytes());
        out.extend_from_slice(&self.bits);
        out
    }

    /// A mask from a block's bytes, if they are one.
    pub fn from_bytes(bytes: &[u8]) -> Option<Mask> {
        let width = u32::from_le_bytes(bytes.get(..4)?.try_into().ok()?);
        let height = u32::from_le_bytes(bytes.get(4..8)?.try_into().ok()?);
        let bits = bytes.get(8..)?.to_vec();
        (bits.len() == Self::stride(width) * height as usize).then_some(Mask {
            width,
            height,
            bits,
        })
    }
}

/// A frame's samples, in the sensor's units.
struct Raw<'a> {
    view: FrameView<'a>,
    meta: &'a FrameMeta,
    wide: bool,
    shift: u32,
    full_scale: u32,
    /// Samples a pixel: 1, or 3 for RGB.
    per_pixel: usize,
    /// A mosaic's channel at each place in a 2 × 2 cell, row by row.
    colours: Option<[usize; 4]>,
}

impl<'a> Raw<'a> {
    fn new(frame: &'a Frame) -> Result<Self, PreviewError> {
        let meta = frame.meta();
        let format = meta.format;
        if !cpu::converts(format) || !matches!(format.channels(), 1 | 3) {
            return Err(PreviewError::Format(format));
        }
        let bits = match format {
            PixelFormat::Bayer16 {
                significant_bits, ..
            } => u32::from(significant_bits).clamp(1, 16),
            _ => 8 * format.sample_bytes() as u32,
        };
        let colours = format.bayer().map(|pattern| {
            let channel = |x, y| match pattern.colour_at(x, y) {
                BayerColour::Red => 0,
                BayerColour::Green => 1,
                BayerColour::Blue => 2,
            };
            [channel(0, 0), channel(1, 0), channel(0, 1), channel(1, 1)]
        });
        Ok(Self {
            view: frame.view(),
            meta,
            wide: format.sample_bytes() == 2,
            shift: 8 * format.sample_bytes() as u32 - bits,
            full_scale: (1_u32 << bits) - 1,
            per_pixel: format.channels(),
            colours,
        })
    }

    fn channels(&self) -> usize {
        if self.colours.is_some() {
            3
        } else {
            self.per_pixel
        }
    }

    fn names(&self) -> Vec<String> {
        let names: &[&str] = match self.channels() {
            3 => &["red", "green", "blue"],
            _ => &["grey"],
        };
        names.iter().map(|n| (*n).to_owned()).collect()
    }

    /// A cell's side: 2 for a mosaic, 1 otherwise.
    fn cell(&self) -> u32 {
        if self.colours.is_some() { 2 } else { 1 }
    }

    /// The channel of sample `s` of the pixel at `(x, y)`.
    fn channel(&self, x: u32, y: u32, s: usize) -> usize {
        match self.colours {
            Some(colours) => colours[(y % 2 * 2 + x % 2) as usize],
            None => s,
        }
    }

    /// Row `y`'s samples, from column `x0` to `x1`.
    fn row(&self, y: u32, x0: u32, x1: u32, out: &mut Vec<u32>) {
        out.clear();
        let bytes = self.view.row(y).expect("y is inside the frame");
        let (a, b) = (x0 as usize * self.per_pixel, x1 as usize * self.per_pixel);
        if self.wide {
            let samples = &bytes.as_chunks::<2>().0[a..b];
            out.extend(
                samples
                    .iter()
                    .map(|s| u32::from(u16::from_le_bytes(*s)) >> self.shift),
            );
        } else {
            out.extend(bytes[a..b].iter().map(|&s| u32::from(s) >> self.shift));
        }
    }

    /// The cell at `(cx, cy)`, each channel's mean.
    fn cell_values(&self, cx: u32, cy: u32, scratch: &mut Vec<u32>) -> Vec<f64> {
        let c = self.cell();
        let (mut sums, mut counts) = (vec![0.0; self.channels()], vec![0.0; self.channels()]);
        for y in cy * c..(cy + 1) * c {
            self.row(y, cx * c, (cx + 1) * c, scratch);
            for (i, &v) in scratch.iter().enumerate() {
                let (x, s) = (cx * c + (i / self.per_pixel) as u32, i % self.per_pixel);
                let channel = self.channel(x, y, s);
                sums[channel] += f64::from(v);
                counts[channel] += 1.0;
            }
        }
        sums.iter().zip(counts).map(|(s, n)| s / n).collect()
    }

    /// The step, in cells, that keeps a region of `cells` cells to about
    /// [`SUBSAMPLE`] samples.
    fn step(&self, cells: u64) -> u32 {
        let samples = cells * u64::from(self.cell().pow(2)) * self.per_pixel as u64;
        ((samples as f64 / SUBSAMPLE as f64).sqrt().ceil() as u32).max(1)
    }
}

/// Counts of each channel, folded over rows.
#[derive(Clone)]
struct Tally {
    count: Vec<u64>,
    sum: Vec<f64>,
    squares: Vec<f64>,
    min: Vec<u32>,
    max: Vec<u32>,
}

impl Tally {
    fn new(channels: usize) -> Self {
        Self {
            count: vec![0; channels],
            sum: vec![0.0; channels],
            squares: vec![0.0; channels],
            min: vec![u32::MAX; channels],
            max: vec![0; channels],
        }
    }

    fn add(&mut self, channel: usize, value: u32) {
        let v = f64::from(value);
        self.count[channel] += 1;
        self.sum[channel] += v;
        self.squares[channel] += v * v;
        self.min[channel] = self.min[channel].min(value);
        self.max[channel] = self.max[channel].max(value);
    }

    fn merge(mut self, other: Tally) -> Tally {
        for c in 0..self.count.len() {
            self.count[c] += other.count[c];
            self.sum[c] += other.sum[c];
            self.squares[c] += other.squares[c];
            self.min[c] = self.min[c].min(other.min[c]);
            self.max[c] = self.max[c].max(other.max[c]);
        }
        self
    }
}

/// `frame`'s statistics, as `request` asks, for a preview of the sensor's
/// pixels `shown` at `size`; and its saturation mask, if asked for and any
/// sample there is at full scale.
pub fn measure(
    frame: &Frame,
    request: &StatsRequest,
    shown: PixelRect,
    size: (u32, u32),
) -> Result<(LiveStats, Option<Mask>), PreviewError> {
    let raw = Raw::new(frame)?;
    let (counted, levels, saturated) = histograms(&raw);
    let bins = u64::from(request.bins.clamp(1, 4096)).min(levels[0].len() as u64);
    let level_count = levels[0].len() as u64;
    let scale = f64::from(raw.full_scale + 1) / level_count as f64;
    let percentiles = levels
        .iter()
        .zip(&counted)
        .map(|(counts, &total)| {
            PERCENTILES
                .iter()
                .map(|p| {
                    let target = (p / 100.0 * total as f64).ceil().max(1.0) as u64;
                    let mut sum = 0;
                    let level = counts.iter().position(|&n| {
                        sum += n;
                        sum >= target
                    });
                    level.unwrap_or(0) as f64 * scale
                })
                .collect()
        })
        .collect();
    let histogram = levels
        .iter()
        .map(|counts| {
            let mut binned = vec![0; bins as usize];
            for (level, &n) in counts.iter().enumerate() {
                binned[(level as u64 * bins / level_count) as usize] += n;
            }
            binned
        })
        .collect();
    let roi = match request.roi {
        Some(roi) => Some(roi_stats(&raw, roi)?),
        None => None,
    };
    let focus_region = request.roi.unwrap_or(shown);
    let stats = LiveStats {
        frame: raw.meta.id.0,
        channels: raw.names(),
        full_scale: raw.full_scale,
        saturated: saturated
            .iter()
            .zip(&counted)
            .map(|(&n, &total)| n as f64 / total.max(1) as f64)
            .collect(),
        counted,
        histogram,
        percentiles,
        roi,
        profile: request.profile.map(|line| profile(&raw, line)),
        focus: focus(&raw, focus_region)?,
    };
    let mask = match request.mask {
        true => mask(&raw, shown, size)?,
        false => None,
    };
    Ok((stats, mask))
}

/// Each channel's samples counted, its 12-bit histogram (fewer levels for
/// fewer bits), and its samples at full scale, over a subsample of whole
/// cells.
fn histograms(raw: &Raw<'_>) -> (Vec<u64>, Vec<Vec<u64>>, Vec<u64>) {
    let (c, channels) = (raw.cell(), raw.channels());
    let levels = (u64::from(raw.full_scale) + 1).min(LEVELS);
    let (cells_x, cells_y) = (raw.meta.width / c, raw.meta.height / c);
    let step = raw.step(u64::from(cells_x) * u64::from(cells_y));
    let empty = || {
        (
            vec![0_u64; channels],
            vec![vec![0_u64; levels as usize]; channels],
            vec![0_u64; channels],
            Vec::new(),
        )
    };
    let (counted, histogram, saturated, _) = (0..cells_y)
        .into_par_iter()
        .step_by(step as usize)
        .flat_map_iter(|cy| cy * c..(cy + 1) * c)
        .fold(
            empty,
            |(mut counted, mut histogram, mut saturated, mut row), y| {
                raw.row(y, 0, cells_x * c, &mut row);
                for (i, &v) in row.iter().enumerate() {
                    let x = (i / raw.per_pixel) as u32;
                    if !(x / c).is_multiple_of(step) {
                        continue;
                    }
                    let channel = raw.channel(x, y, i % raw.per_pixel);
                    counted[channel] += 1;
                    histogram[channel]
                        [(u64::from(v) * levels / (u64::from(raw.full_scale) + 1)) as usize] += 1;
                    saturated[channel] += u64::from(v >= raw.full_scale);
                }
                (counted, histogram, saturated, row)
            },
        )
        .reduce(empty, |a, b| {
            let add =
                |x: Vec<u64>, y: Vec<u64>| x.iter().zip(y).map(|(p, q)| p + q).collect::<Vec<_>>();
            let histogram = a.1.into_iter().zip(b.1).map(|(p, q)| add(p, q)).collect();
            (add(a.0, b.0), histogram, add(a.2, b.2), Vec::new())
        });
    (counted, histogram, saturated)
}

/// `roi`'s statistics.
fn roi_stats(raw: &Raw<'_>, roi: PixelRect) -> Result<RoiStats, PreviewError> {
    let (rect, sensor) = region(raw.meta, Some(roi), raw.cell())?;
    let channels = raw.channels();
    let tally = (rect.y..rect.y + rect.height)
        .into_par_iter()
        .fold(
            || (Tally::new(channels), Vec::new()),
            |(mut tally, mut row), y| {
                raw.row(y, rect.x, rect.x + rect.width, &mut row);
                for (i, &v) in row.iter().enumerate() {
                    let x = rect.x + (i / raw.per_pixel) as u32;
                    tally.add(raw.channel(x, y, i % raw.per_pixel), v);
                }
                (tally, row)
            },
        )
        .map(|(tally, _)| tally)
        .reduce(|| Tally::new(channels), Tally::merge);
    let mean: Vec<f64> = tally
        .sum
        .iter()
        .zip(&tally.count)
        .map(|(s, &n)| s / n as f64)
        .collect();
    let std = tally
        .squares
        .iter()
        .zip(&tally.count)
        .zip(&mean)
        .map(|((q, &n), m)| (q / n as f64 - m * m).max(0.0).sqrt())
        .collect();
    Ok(RoiStats {
        rect: sensor,
        count: tally.count,
        mean,
        std,
        min: tally.min,
        max: tally.max,
    })
}

/// The cells along `line`, a cell apart.
fn profile(raw: &Raw<'_>, line: Line) -> Profile {
    let meta = raw.meta;
    let (binning, c) = (f64::from(meta.binning.max(1)), raw.cell());
    let cell = f64::from(c) * binning;
    let [dx, dy] = [line.to[0] - line.from[0], line.to[1] - line.from[1]];
    let length = dx.hypot(dy);
    let n = ((length / cell).ceil() as usize + 1).min(MOST_SAMPLES);
    let step = if n > 1 { length / (n - 1) as f64 } else { 0.0 };
    let (cells_x, cells_y) = (meta.width / c, meta.height / c);
    let mut scratch = Vec::new();
    let samples = (0..n)
        .map(|i| {
            // Multiplied before divided, so that a sample on a cell's edge
            // lands on it exactly.
            let along = |d: f64| {
                if n > 1 {
                    d * i as f64 / (n - 1) as f64
                } else {
                    0.0
                }
            };
            let sx = line.from[0] + along(dx) - f64::from(meta.roi.x);
            let sy = line.from[1] + along(dy) - f64::from(meta.roi.y);
            let (cx, cy) = ((sx / cell).floor(), (sy / cell).floor());
            let inside =
                cx >= 0.0 && cy >= 0.0 && cx < f64::from(cells_x) && cy < f64::from(cells_y);
            inside.then(|| raw.cell_values(cx as u32, cy as u32, &mut scratch))
        })
        .collect();
    Profile {
        line,
        step,
        samples,
    }
}

/// The normalised variance of the green cells (or grey, or the green of
/// RGB) over `region`: their variance over their mean, from a subsample.
fn focus(raw: &Raw<'_>, region_shown: PixelRect) -> Result<f64, PreviewError> {
    let (rect, _) = region(raw.meta, Some(region_shown), raw.cell())?;
    let c = raw.cell();
    let (cells_x, cells_y) = (rect.width / c, rect.height / c);
    let step = raw.step(u64::from(cells_x) * u64::from(cells_y));
    let green = if raw.channels() == 3 { 1 } else { 0 };
    let (n, sum, squares) = (0..cells_y)
        .into_par_iter()
        .step_by(step as usize)
        .fold(
            || (0.0, 0.0, 0.0, Vec::new()),
            |(mut n, mut sum, mut squares, mut scratch), cy| {
                for cx in (0..cells_x).step_by(step as usize) {
                    let v = raw.cell_values(rect.x / c + cx, rect.y / c + cy, &mut scratch)[green];
                    n += 1.0;
                    sum += v;
                    squares += v * v;
                }
                (n, sum, squares, scratch)
            },
        )
        .map(|(n, sum, squares, _)| (n, sum, squares))
        .reduce(|| (0.0, 0.0, 0.0), |a, b| (a.0 + b.0, a.1 + b.1, a.2 + b.2));
    let mean = sum / f64::max(n, 1.0);
    Ok(if mean > 0.0 {
        (squares / n - mean * mean).max(0.0) / mean
    } else {
        0.0
    })
}

/// The bits of a `size` preview of the sensor's pixels `shown` where a raw
/// sample under it is at full scale; none if none is.
fn mask(
    raw: &Raw<'_>,
    shown: PixelRect,
    (width, height): (u32, u32),
) -> Result<Option<Mask>, PreviewError> {
    let (rect, sensor) = region(raw.meta, Some(shown), 1)?;
    let stride = Mask::stride(width);
    let mut bits = vec![0_u8; stride * height as usize];
    // A frame pixel's column or row in the preview.
    let place = |offset: u32, extent: u32, size: u32| {
        (u64::from(offset) * u64::from(size) / u64::from(extent)) as u32
    };
    let binning = raw.meta.binning.max(1);
    bits.par_chunks_mut(stride)
        .enumerate()
        .for_each(|(py, out)| {
            let mut row = Vec::new();
            let py = py as u32;
            for y in rect.y..rect.y + rect.height {
                if place((y - rect.y) * binning, sensor.height, height) != py {
                    continue;
                }
                raw.row(y, rect.x, rect.x + rect.width, &mut row);
                for (i, &v) in row.iter().enumerate() {
                    if v >= raw.full_scale {
                        let x = (i / raw.per_pixel) as u32 * binning;
                        let px = place(x, sensor.width, width).min(width - 1);
                        out[px as usize / 8] |= 0x80 >> (px % 8);
                    }
                }
            }
        });
    Ok(bits.iter().any(|&b| b != 0).then_some(Mask {
        width,
        height,
        bits,
    }))
}
