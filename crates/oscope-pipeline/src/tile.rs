//! Tiled runs: rasters cut into tiles, each read with the halo its steps
//! need, run, and put back together (ADR-0017).

use std::collections::BTreeMap;
use std::time::Instant;

use oscope_compute::{Image, Shape};
use oscope_core::PixelRect;

use crate::plan::Plan;
use crate::run::{Engine, Report, Run, RunError, RunOptions};
use crate::value::{Mask, Value, Values};

/// A tile of a tiled run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tile {
    /// The pixels it gives.
    pub core: PixelRect,
    /// The pixels it reads: its core, and the halo around it within the
    /// raster.
    pub read: PixelRect,
}

/// The tiles covering a `width` × `height` raster, row by row: `size`
/// pixels square, smaller at the right and bottom edges, each reading
/// `halo` pixels around it within the raster.
pub fn tiles(width: u32, height: u32, size: u32, halo: u32) -> Vec<Tile> {
    let size = size.max(1);
    let mut tiles = Vec::new();
    for y in (0..height).step_by(size as usize) {
        for x in (0..width).step_by(size as usize) {
            let core = PixelRect::new(x, y, size.min(width - x), size.min(height - y));
            let (left, top) = (x.saturating_sub(halo), y.saturating_sub(halo));
            let right = (x + core.width).saturating_add(halo).min(width);
            let bottom = (y + core.height).saturating_add(halo).min(height);
            let read = PixelRect::new(left, top, right - left, bottom - top);
            tiles.push(Tile { core, read });
        }
    }
    tiles
}

impl Engine {
    /// Runs `plan` tile by tile (ADR-0017): its raster inputs, all of one
    /// size, cut into tiles `size` pixels square, each read with the plan's
    /// halo ([`Plan::halo`]); each tile run; and the cores of the outputs
    /// put together. Other inputs go whole to every tile. Every step must
    /// be tileable. `progress` hears of each tile done, and of how many
    /// there are.
    pub fn run_tiled(
        &self,
        plan: &Plan,
        inputs: &Values,
        size: u32,
        options: &RunOptions,
        mut progress: impl FnMut(usize, usize),
    ) -> Result<Run, RunError> {
        let started = Instant::now();
        let refused = plan.not_tileable();
        if !refused.is_empty() {
            return Err(RunError::Unsuitable(format!(
                "{} can't run tile by tile",
                refused.join(", ")
            )));
        }
        let mut size_of_all = None;
        for (name, value) in inputs
            .iter()
            .filter(|(_, value)| value.port_type().is_raster())
        {
            let size = raster_size(value);
            let size = size.ok_or_else(|| {
                RunError::Input(name.clone(), "tiled runs take rasters in memory".into())
            })?;
            if *size_of_all.get_or_insert(size) != size {
                return Err(RunError::Input(
                    name.clone(),
                    "it isn't the other rasters' size".into(),
                ));
            }
        }
        let message = "the inputs have no raster to cut into tiles";
        let (width, height) = size_of_all.ok_or_else(|| RunError::Unsuitable(message.into()))?;
        let tiles = tiles(width, height, size, plan.halo());
        let mut wholes = BTreeMap::new();
        let mut report = Report::default();
        for (n, tile) in tiles.iter().enumerate() {
            let cut: Values = inputs
                .iter()
                .map(|(name, value)| (name.clone(), crop(value, tile.read)))
                .collect();
            let run = self.run(plan, &cut, options)?;
            let inner = PixelRect::new(
                tile.core.x - tile.read.x,
                tile.core.y - tile.read.y,
                tile.core.width,
                tile.core.height,
            );
            for (name, value) in run.outputs {
                let whole = wholes
                    .entry(name.clone())
                    .or_insert_with(|| Whole::like(&value, width, height));
                if !whole.paste(&value, tile.read, inner, tile.core) {
                    let message = format!("output `{name}` isn't a raster the size of its tile");
                    return Err(RunError::Unsuitable(message));
                }
            }
            if n == 0 {
                report.steps = run.report.steps;
            } else {
                for (sum, step) in report.steps.iter_mut().zip(run.report.steps) {
                    sum.time += step.time;
                    sum.cached &= step.cached;
                }
            }
            report.transfers += run.report.transfers;
            progress(n + 1, tiles.len());
        }
        report.tiles = tiles.len() as u32;
        report.time = started.elapsed();
        let outputs = wholes
            .into_iter()
            .map(|(name, whole)| (name, whole.into_value()));
        Ok(Run {
            outputs: outputs.collect(),
            report,
        })
    }
}

/// A raster's width and height, if it is one in memory.
fn raster_size(value: &Value) -> Option<(u32, u32)> {
    match value {
        Value::Image(image) => Some((image.width(), image.height())),
        Value::Mask(mask) => Some((mask.width(), mask.height())),
        _ => None,
    }
}

/// A raster's pixels in `rect`; other values as they are.
fn crop(value: &Value, rect: PixelRect) -> Value {
    let (x, width) = (rect.x as usize, rect.width as usize);
    let rows = rect.y..rect.y + rect.height;
    match value {
        Value::Image(image) => {
            let channels = image.channels();
            let mut tile = Image::zeros(Shape::new(rect.width, rect.height, channels));
            let row_len = width * channels;
            for (out, y) in tile.data_mut().chunks_exact_mut(row_len.max(1)).zip(rows) {
                out.copy_from_slice(&image.row(y)[x * channels..x * channels + row_len]);
            }
            Value::image(tile)
        }
        Value::Mask(mask) => {
            let mut tile = Mask::zeros(rect.width, rect.height);
            let stride = mask.width() as usize;
            for (out, y) in tile.data_mut().chunks_exact_mut(width.max(1)).zip(rows) {
                let start = y as usize * stride + x;
                out.copy_from_slice(&mask.data()[start..start + width]);
            }
            Value::mask(tile)
        }
        other => other.clone(),
    }
}

/// An output being put together from its tiles.
enum Whole {
    Image(Image),
    Mask(Mask),
    Other(Value),
}

impl Whole {
    fn like(value: &Value, width: u32, height: u32) -> Whole {
        match value {
            Value::Image(image) => {
                Whole::Image(Image::zeros(Shape::new(width, height, image.channels())))
            }
            Value::Mask(_) => Whole::Mask(Mask::zeros(width, height)),
            other => Whole::Other(other.clone()),
        }
    }

    /// Copies the pixels of a tile's output, read from `read`, that lie in
    /// `inner` within it, to `at`; false if it isn't a raster of `read`'s
    /// size.
    fn paste(&mut self, value: &Value, read: PixelRect, inner: PixelRect, at: PixelRect) -> bool {
        let copy = |whole: &mut [f32], tile: &[f32], stride: usize, channels: usize| {
            let row_len = inner.width as usize * channels;
            for row in 0..inner.height as usize {
                let from =
                    ((inner.y as usize + row) * read.width as usize + inner.x as usize) * channels;
                let to = ((at.y as usize + row) * stride + at.x as usize) * channels;
                whole[to..to + row_len].copy_from_slice(&tile[from..from + row_len]);
            }
        };
        let fits = |size: Option<(u32, u32)>| size == Some((read.width, read.height));
        match (self, value) {
            (Whole::Image(whole), Value::Image(tile))
                if fits(raster_size(value)) && tile.channels() == whole.channels() =>
            {
                let (stride, channels) = (whole.width() as usize, whole.channels());
                copy(whole.data_mut(), tile.data(), stride, channels);
                true
            }
            (Whole::Mask(whole), Value::Mask(tile)) if fits(raster_size(value)) => {
                let stride = whole.width() as usize;
                for row in 0..inner.height as usize {
                    let from = (inner.y as usize + row) * read.width as usize + inner.x as usize;
                    let to = (at.y as usize + row) * stride + at.x as usize;
                    let len = inner.width as usize;
                    whole.data_mut()[to..to + len].copy_from_slice(&tile.data()[from..from + len]);
                }
                true
            }
            (Whole::Other(_), _) => !value.port_type().is_raster(),
            _ => false,
        }
    }

    fn into_value(self) -> Value {
        match self {
            Whole::Image(image) => Value::image(image),
            Whole::Mask(mask) => Value::mask(mask),
            Whole::Other(value) => value,
        }
    }
}
