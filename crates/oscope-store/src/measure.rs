//! Measurements (P44, plan §5.17): what a record's annotations measure, in
//! µm: a line's length, a polygon's area, perimeter and centre, and the
//! statistics of the pixels a polygon holds, read tile by tile from the
//! full-resolution image, and the same whatever the tiles' size.
//!
//! A raw image, a camera's Bayer mosaic, has its pixels' statistics by the
//! colour of their filters. The statistics are in the image's own units:
//! counts from 0 to 255 or 65,535, or its floats.

use std::ops::Range;

use oscope_core::{BayerColour, BayerPattern, ProjectId};
use oscope_imaging::shapes::{self, Point};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::annotations::{Geometry, plane};
#[cfg(doc)]
use crate::image::CHUNK;
use crate::image::{ImageReader, Samples};
use crate::records::RecordKind;
use crate::zoom::record_image;
use crate::{Result, Store};

/// What an annotation measures: in µm, or on an image of unknown pixel
/// size, in pixels.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Measurement {
    /// Its layer.
    pub layer: String,
    /// Its ID.
    pub id: String,
    /// Its name, if it has one.
    pub name: Option<String>,
    /// Its shape: `Point`, `LineString` or `Polygon`.
    pub geometry: String,
    /// The unit of its lengths: `µm`, or `px`.
    pub unit: String,
    /// A line's length, or a polygon's perimeter, holes and all.
    pub length: f64,
    /// A polygon's area, without its holes.
    pub area: f64,
    /// Its centre: a point's place, a line's middle by its length, or a
    /// polygon's by its area.
    pub centroid: [f64; 2],
    /// A polygon's pixels, channel by channel: those whose centres are
    /// inside.
    pub channels: Vec<ChannelStats>,
}

/// The statistics of a polygon's pixels in one channel.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ChannelStats {
    /// The channel: a raw image's are `red`, `green` and `blue`, by the
    /// colours of their filters.
    pub name: String,
    /// How many pixels.
    pub pixels: u64,
    /// Their mean.
    pub mean: f64,
    /// Their standard deviation, as a population's.
    pub sd: f64,
    /// The least.
    pub min: f64,
    /// The greatest.
    pub max: f64,
}

/// A row's running sums, in its pixels' order left to right.
#[derive(Clone, Copy)]
struct Row {
    n: u64,
    sum: f64,
    squares: f64,
    min: f64,
    max: f64,
}

impl Default for Row {
    fn default() -> Self {
        let (min, max) = (f64::INFINITY, f64::NEG_INFINITY);
        Row {
            n: 0,
            sum: 0.0,
            squares: 0.0,
            min,
            max,
        }
    }
}

impl Row {
    fn add(&mut self, v: f64) {
        self.n += 1;
        self.sum += v;
        self.squares += v * v;
        self.min = self.min.min(v);
        self.max = self.max.max(v);
    }
}

/// Statistics so far: the rows taken in order, top to bottom, each by
/// Chan, Golub and LeVeque's pairwise update (1979).
#[derive(Clone, Copy, Default)]
struct Total {
    n: u64,
    mean: f64,
    m2: f64,
    row: Row,
}

impl Total {
    fn take(&mut self, row: &Row) {
        if row.n == 0 {
            return;
        }
        let (na, nb) = (self.n as f64, row.n as f64);
        let mean = row.sum / nb;
        let m2 = (row.squares - row.sum * mean).max(0.0);
        let delta = mean - self.mean;
        let n = na + nb;
        self.mean += delta * nb / n;
        self.m2 += m2 + delta * delta * na * nb / n;
        self.n += row.n;
        self.row.min = self.row.min.min(row.min);
        self.row.max = self.row.max.max(row.max);
    }

    fn stats(&self, name: &str) -> ChannelStats {
        let seen = self.n > 0;
        let n = self.n.max(1) as f64;
        ChannelStats {
            name: name.to_owned(),
            pixels: self.n,
            mean: self.mean,
            sd: (self.m2 / n).sqrt(),
            min: if seen { self.row.min } else { 0.0 },
            max: if seen { self.row.max } else { 0.0 },
        }
    }
}

/// `samples` as numbers in their own units.
fn values(samples: &Samples) -> Vec<f64> {
    match samples {
        Samples::U8(s) => s.iter().map(|&v| f64::from(v)).collect(),
        Samples::U16(s) => s.iter().map(|&v| f64::from(v)).collect(),
        Samples::F32(s) => s.iter().map(|&v| f64::from(v)).collect(),
    }
}

/// The statistics of the pixels of plane `plane` of `reader`'s full
/// resolution that the polygon `rings`, in pixels, holds: by `pattern`'s
/// colours for a raw image. Read in tiles of `tile` pixels a side on the
/// image's grid, a band of them at a time, left to right: each row's sums
/// run on from tile to tile, so the result is the same whatever the tiles.
pub fn polygon_stats(
    reader: &ImageReader,
    pattern: Option<BayerPattern>,
    rings: &[Vec<Point>],
    plane: u32,
    tile: u64,
) -> Result<Vec<ChannelStats>> {
    let info = reader.info();
    let names: Vec<String> = match pattern {
        Some(_) => ["red", "green", "blue"].map(str::to_owned).to_vec(),
        None => (0..info.channels as usize)
            .map(|c| match info.channel_names.get(c) {
                Some(name) => name.clone(),
                None => format!("channel {c}"),
            })
            .collect(),
    };
    let (columns, rows) = shapes::pixel_bounds(rings);
    let clip = |r: Range<u64>, end: u64| r.start.min(end)..r.end.min(end);
    let (columns, rows) = (clip(columns, info.width), clip(rows, info.height));
    let (tile, channels) = (tile.max(1), info.channels as usize);
    let mut totals = vec![Total::default(); names.len()];
    let mut top = rows.start / tile * tile;
    while top < rows.end {
        let band = top.max(rows.start)..(top + tile).min(rows.end);
        let runs: Vec<Vec<Range<u64>>> = band
            .clone()
            .map(|y| shapes::row_spans(rings, y, columns.clone()))
            .collect();
        let mut sums = vec![vec![Row::default(); names.len()]; runs.len()];
        let mut left = columns.start / tile * tile;
        while left < columns.end {
            let cols = left.max(columns.start)..(left + tile).min(columns.end);
            left += tile;
            let touched = |run: &Range<u64>| run.start < cols.end && run.end > cols.start;
            if !runs.iter().flatten().any(touched) {
                continue;
            }
            let size = (cols.end - cols.start, band.end - band.start);
            let tile_values = values(&reader.read(0, plane, (cols.start, band.start), size)?);
            for (r, (row_runs, row_sums)) in runs.iter().zip(&mut sums).enumerate() {
                let y = band.start + r as u64;
                for run in row_runs {
                    for x in run.start.max(cols.start)..run.end.min(cols.end) {
                        let at = (r * size.0 as usize + (x - cols.start) as usize) * channels;
                        match pattern {
                            Some(p) => {
                                let c = match p.colour_at(x as u32, y as u32) {
                                    BayerColour::Red => 0,
                                    BayerColour::Green => 1,
                                    BayerColour::Blue => 2,
                                };
                                row_sums[c].add(tile_values[at]);
                            }
                            None => {
                                for (c, sum) in row_sums.iter_mut().enumerate() {
                                    sum.add(tile_values[at + c]);
                                }
                            }
                        }
                    }
                }
            }
        }
        for row_sums in &sums {
            for (total, row) in totals.iter_mut().zip(row_sums) {
                total.take(row);
            }
        }
        top += tile;
    }
    Ok(totals.iter().zip(&names).map(|(t, n)| t.stats(n)).collect())
}

/// What the annotations of `kind` record `id` of `project` measure: layer
/// `layer`'s, or every layer's. Polygons' pixels are read from plane
/// `plane_index`, in tiles of `tile` pixels a side: [`CHUNK`], the image's
/// chunks, reads each chunk once.
pub fn measure(
    store: &Store,
    project: ProjectId,
    kind: RecordKind,
    id: &str,
    layer: Option<&str>,
    plane_index: u32,
    tile: u64,
) -> Result<Vec<Measurement>> {
    let annotations = store.annotations(project, kind, id)?;
    let (path, pattern) = record_image(store, project, kind, id)?;
    let reader = ImageReader::open(&path)?;
    let (unit, (sx, sy)) = match reader.info().pixel_size {
        Some(size) => ("µm", (size.x, size.y)),
        None => ("px", (1.0, 1.0)),
    };
    let to_pixels = |p: &Point| [p[0] / sx, p[1] / sy];
    let mut measured = Vec::new();
    for (name, kept) in &annotations.layers {
        if layer.is_some_and(|l| l != name) {
            continue;
        }
        for feature in &kept.features {
            let (length, area, centroid, channels) = match &feature.geometry {
                Geometry::Point(p) => (0.0, 0.0, [p[0], p[1]], Vec::new()),
                Geometry::LineString(line) => {
                    let line = plane(line);
                    let centre = shapes::path_centroid(&line);
                    (shapes::path_length(&line), 0.0, centre, Vec::new())
                }
                Geometry::Polygon(rings) => {
                    let rings: Vec<Vec<Point>> = rings.iter().map(|r| plane(r)).collect();
                    let pixels: Vec<Vec<Point>> = rings
                        .iter()
                        .map(|r| r.iter().map(to_pixels).collect())
                        .collect();
                    let stats = polygon_stats(&reader, pattern, &pixels, plane_index, tile)?;
                    let area = shapes::polygon_area(&rings);
                    let centre = shapes::polygon_centroid(&rings);
                    (shapes::polygon_perimeter(&rings), area, centre, stats)
                }
            };
            measured.push(Measurement {
                layer: name.clone(),
                id: feature.id.clone().unwrap_or_default(),
                name: feature
                    .properties
                    .get("name")
                    .and_then(|n| n.as_str())
                    .map(str::to_owned),
                geometry: feature.geometry.name().to_owned(),
                unit: unit.to_owned(),
                length,
                area,
                centroid,
                channels,
            });
        }
    }
    Ok(measured)
}

/// A CSV field: quoted if it holds a comma, a quote or a line break.
fn field(text: &str) -> String {
    match text.contains([',', '"', '\n', '\r']) {
        true => format!("\"{}\"", text.replace('"', "\"\"")),
        false => text.to_owned(),
    }
}

/// `measurements` as CSV (RFC 4180): a row each, with the statistics of
/// every channel any of them has, in µm (`um`) or pixels (`px`).
pub fn measurements_csv(measurements: &[Measurement]) -> String {
    let mut channels: Vec<&str> = Vec::new();
    for c in measurements.iter().flat_map(|m| &m.channels) {
        if !channels.contains(&c.name.as_str()) {
            channels.push(&c.name);
        }
    }
    let unit = match measurements.first().map(|m| m.unit.as_str()) {
        Some("px") => "px",
        _ => "um",
    };
    let mut header = format!(
        "layer,id,name,geometry,length_{unit},area_{unit}2,centroid_x_{unit},centroid_y_{unit}"
    );
    for c in &channels {
        for stat in ["pixels", "mean", "sd", "min", "max"] {
            header += &format!(",{}", field(&format!("{c} {stat}")));
        }
    }
    let mut csv = header + "\r\n";
    for m in measurements {
        let name = m.name.as_deref().unwrap_or_default();
        csv += &format!(
            "{},{},{},{},{},{},{},{}",
            field(&m.layer),
            field(&m.id),
            field(name),
            m.geometry,
            m.length,
            m.area,
            m.centroid[0],
            m.centroid[1]
        );
        for c in &channels {
            match m.channels.iter().find(|s| s.name == *c) {
                Some(s) => csv += &format!(",{},{},{},{},{}", s.pixels, s.mean, s.sd, s.min, s.max),
                None => csv += ",,,,,",
            }
        }
        csv += "\r\n";
    }
    csv
}
