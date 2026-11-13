//! Measurement and conversion: histograms with each channel's statistics,
//! and image formats.

use std::sync::Arc;

use oscope_compute::{Image, Shape};
use rayon::prelude::*;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::json;

use super::{PIXELWISE, WHOLE};
use crate::node::{Descriptor, Inputs, Node, NodeError, StepCtx, try_node};
use crate::value::{PortType, Table, Value, Values, outputs};

fn bins() -> u32 {
    256
}

fn one() -> f32 {
    1.0
}

/// `measure.histogram`'s parameters.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Bins {
    /// How many bins: 256 unless given.
    #[serde(default = "bins")]
    #[schemars(range(min = 2, max = 65536))]
    bins: u32,
    /// The low end of the first bin.
    #[serde(default)]
    low: f32,
    /// The high end of the last: above low.
    #[serde(default = "one")]
    high: f32,
}

/// A channel's name in a table: `grey` for one, `red`, `green` and `blue`
/// for three.
fn channel(c: usize, of: usize) -> String {
    match (of, c) {
        (1, _) => "grey".into(),
        (3, c) => ["red", "green", "blue"][c].into(),
        (_, c) => format!("channel {c}"),
    }
}

/// `measure.histogram`: each channel's histogram, values outside
/// `low`–`high` counted in the end bins, as a table with a row a bin (its
/// low end, then a count for each channel); and each channel's statistics,
/// as a table with a row a channel: its mean, standard deviation, least
/// and greatest values, and its 1st, 50th and 99th percentiles, at their
/// bins' centres.
pub(super) fn histogram() -> Arc<dyn Node> {
    let descriptor = Descriptor::new(
        "measure.histogram",
        1,
        "measurement",
        "Histograms and statistics",
    )
    .input("image", PortType::Image)
    .output("histogram", PortType::Table)
    .output("statistics", PortType::Table)
    .params::<Bins>()
    .capabilities(WHOLE);
    try_node(descriptor, |Bins { bins, low, high }| {
        if high.partial_cmp(&low) != Some(std::cmp::Ordering::Greater) {
            return Err(format!("high ({high}) must be above low ({low})"));
        }
        Ok(
            move |_: &StepCtx<'_>, inputs: &Inputs<'_>| -> Result<Values, NodeError> {
                let image = inputs.image("image")?;
                let (histogram, statistics) = measure(image, bins as usize, low, high);
                Ok(outputs([
                    ("histogram", Value::table(histogram)),
                    ("statistics", Value::table(statistics)),
                ]))
            },
        )
    })
}

/// Each channel's counts, and its sum, sum of squares, least and greatest.
#[derive(Clone)]
struct Tally {
    counts: Vec<u64>,
    sum: f64,
    squares: f64,
    least: f32,
    greatest: f32,
}

fn measure(image: &Image, bins: usize, low: f32, high: f32) -> (Table, Table) {
    let channels = image.channels().max(1);
    let scale = bins as f32 / (high - low);
    let empty = || {
        let tally = Tally {
            counts: vec![0; bins],
            sum: 0.0,
            squares: 0.0,
            least: f32::INFINITY,
            greatest: f32::NEG_INFINITY,
        };
        vec![tally; channels]
    };
    let tallies = image
        .data()
        .par_chunks(channels * 4096)
        .fold(empty, |mut tallies, chunk| {
            for pixel in chunk.chunks(channels) {
                for (tally, &v) in tallies.iter_mut().zip(pixel) {
                    let bin = (((v - low) * scale) as i64).clamp(0, bins as i64 - 1);
                    tally.counts[bin as usize] += 1;
                    tally.sum += f64::from(v);
                    tally.squares += f64::from(v) * f64::from(v);
                    tally.least = tally.least.min(v);
                    tally.greatest = tally.greatest.max(v);
                }
            }
            tallies
        })
        .reduce(empty, |mut a, b| {
            for (a, b) in a.iter_mut().zip(b) {
                a.counts.iter_mut().zip(b.counts).for_each(|(a, b)| *a += b);
                a.sum += b.sum;
                a.squares += b.squares;
                a.least = a.least.min(b.least);
                a.greatest = a.greatest.max(b.greatest);
            }
            a
        });
    let width = f64::from(high - low) / bins as f64;
    let names: Vec<String> = (0..channels).map(|c| channel(c, channels)).collect();
    let mut histogram = Table {
        columns: std::iter::once("from".to_owned())
            .chain(names.iter().cloned())
            .collect(),
        rows: Vec::with_capacity(bins),
    };
    for bin in 0..bins {
        let from = f64::from(low) + bin as f64 * width;
        let counts = tallies.iter().map(|tally| json!(tally.counts[bin]));
        histogram
            .rows
            .push(std::iter::once(json!(from)).chain(counts).collect());
    }
    let columns = [
        "channel", "mean", "sd", "least", "greatest", "p1", "p50", "p99",
    ];
    let mut statistics = Table {
        columns: columns.map(String::from).to_vec(),
        rows: Vec::with_capacity(channels),
    };
    for (tally, name) in tallies.iter().zip(names) {
        let total: u64 = tally.counts.iter().sum();
        let n = total.max(1) as f64;
        let mean = tally.sum / n;
        let sd = (tally.squares / n - mean * mean).max(0.0).sqrt();
        let percentile = |p: f64| {
            let wanted = (p * total as f64).ceil().max(1.0) as u64;
            let mut seen = 0;
            let bin = tally.counts.iter().position(|&count| {
                seen += count;
                seen >= wanted
            });
            f64::from(low) + (bin.unwrap_or(0) as f64 + 0.5) * width
        };
        statistics.rows.push(vec![
            json!(name),
            json!(mean),
            json!(sd),
            json!(tally.least),
            json!(tally.greatest),
            json!(percentile(0.01)),
            json!(percentile(0.5)),
            json!(percentile(0.99)),
        ]);
    }
    (histogram, statistics)
}

/// What channels `convert.format` gives.
#[derive(Clone, Copy, Default, PartialEq, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum Channels {
    /// The image's, as they are.
    #[default]
    Same,
    /// One: the luminance of RGB (Rec. 709's weights, on linear light), or
    /// grey as it is.
    Grey,
    /// Three: grey repeated, or RGB as it is.
    Rgb,
    /// RGB's red alone.
    Red,
    /// RGB's green alone.
    Green,
    /// RGB's blue alone.
    Blue,
}

/// `convert.format`'s parameters.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Format {
    /// The channels it gives.
    #[serde(default)]
    channels: Channels,
    /// The bit depth each sample is rounded to, from 1 to 16, from 0 to 1:
    /// none unless given.
    #[serde(default)]
    bits: Option<u8>,
}

/// `convert.format`: the image as grey, RGB or one of its channels, and its
/// samples rounded to a bit depth, as a file of that depth would hold them.
pub(super) fn format() -> Arc<dyn Node> {
    let descriptor = Descriptor::new(
        "convert.format",
        1,
        "conversion",
        "Grey, RGB or a channel, at a bit depth",
    )
    .input("image", PortType::Image)
    .output("image", PortType::Image)
    .params::<Format>()
    .capabilities(PIXELWISE);
    try_node(descriptor, |Format { channels, bits }| {
        let levels = match bits {
            Some(bits @ 1..=16) => Some(f32::from((1_u32 << bits) as u16 - 1)),
            Some(bits) => return Err(format!("{bits} bits: from 1 to 16")),
            None => None,
        };
        Ok(
            move |_: &StepCtx<'_>, inputs: &Inputs<'_>| -> Result<Values, NodeError> {
                let image = inputs.image("image")?;
                let mut out = convert(image, channels)?;
                if let Some(levels) = levels {
                    out.data_mut()
                        .par_iter_mut()
                        .for_each(|v| *v = (v.clamp(0.0, 1.0) * levels).round() / levels);
                }
                Ok(outputs([("image", Value::image(out))]))
            },
        )
    })
}

fn convert(image: &Image, to: Channels) -> Result<Image, NodeError> {
    let from = image.channels();
    let pick = |count: usize, f: &(dyn Fn(&[f32], &mut [f32]) + Sync)| {
        let mut out = Image::zeros(Shape::new(image.width(), image.height(), count));
        let pixels = out
            .data_mut()
            .par_chunks_mut(count)
            .zip(image.data().par_chunks(from.max(1)));
        pixels.for_each(|(out, pixel)| f(pixel, out));
        out
    };
    Ok(match (to, from) {
        (Channels::Same, _) | (Channels::Grey, 1) | (Channels::Rgb, 3) => super::copied(image),
        (Channels::Grey, 3) => pick(1, &|p, out| {
            out[0] = 0.2126 * p[0] + 0.7152 * p[1] + 0.0722 * p[2]
        }),
        (Channels::Rgb, 1) => pick(3, &|p, out| out.fill(p[0])),
        (Channels::Red, 3) => pick(1, &|p, out| out[0] = p[0]),
        (Channels::Green, 3) => pick(1, &|p, out| out[0] = p[1]),
        (Channels::Blue, 3) => pick(1, &|p, out| out[0] = p[2]),
        (_, from) => {
            return Err(format!("an image of {from} channels can't be converted so").into());
        }
    })
}
