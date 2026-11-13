//! Trivial nodes, for tests and examples: the stock nodes come in P22 and
//! P23.
//!
//! | Node | What it does | Runs |
//! |---|---|---|
//! | `test.box_blur` | The mean over a box, the edge pixels repeated beyond the image | CPU; live; tiles, with a halo of its radius |
//! | `test.add` | `a × first + b × second`, pixel by pixel | CPU; live; tiles |
//! | `test.threshold` | A mask where the channels' mean reaches a level | CPU; live; tiles |
//! | `test.statistics` | Each channel's mean, least and greatest, as a table, and the mean as JSON | CPU; live |
//! | `test.bin2x2` | 2 × 2 binning, summing, with `oscope-compute`'s kernels | CPU and CUDA; live |
//! | `test.sleep` | The image as it is, after waiting, cancellable | CPU; live; tiles |

use std::sync::Arc;
use std::time::{Duration, Instant};

use oscope_compute::{Image, cpu};
use rayon::prelude::*;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::json;

use crate::node::{Capabilities, Descriptor, Device, Inputs, Node, NodeError, Step, StepCtx, node};
use crate::registry::Registry;
use crate::value::{Mask, PortType, Table, Value, Values, outputs};

/// A registry of the nodes here.
pub fn registry() -> Registry {
    let mut registry = Registry::new();
    for node in [
        box_blur(),
        add(),
        threshold(),
        statistics(),
        bin2x2(),
        sleep(),
    ] {
        registry
            .register(node)
            .expect("the test nodes are well formed");
    }
    registry
}

/// Runs on the CPU, live, and tile by tile.
const TILEABLE: Capabilities = Capabilities {
    cpu: true,
    cuda: false,
    live: true,
    tileable: true,
    halo: 0,
};

/// No parameters.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Nothing {}

/// `test.box_blur`'s parameters.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Radius {
    /// The box's half-width, in pixels.
    #[schemars(range(max = 64))]
    radius: u32,
}

/// `test.box_blur`: each pixel the mean over the box `2 × radius + 1`
/// pixels wide around it, the edge pixels repeated beyond the image. Each
/// sum is taken in the same order wherever the pixel is, so a tile gives
/// exactly what the whole image does.
pub fn box_blur() -> Arc<dyn Node> {
    let descriptor = Descriptor::new(
        "test.box_blur",
        1,
        "filter",
        "The mean over a box around each pixel",
    )
    .input("image", PortType::Image)
    .output("image", PortType::Image)
    .params::<Radius>()
    .capabilities(Capabilities {
        halo: 64,
        ..TILEABLE
    });
    node(descriptor, |params: Radius| BoxBlur(params.radius))
}

struct BoxBlur(u32);

impl Step for BoxBlur {
    fn run(&self, _: &StepCtx<'_>, inputs: &Inputs<'_>) -> Result<Values, NodeError> {
        let image = inputs.image("image")?;
        Ok(outputs([("image", Value::image(blur(image, self.0)))]))
    }

    fn halo(&self) -> Option<u32> {
        Some(self.0)
    }
}

fn blur(image: &Image, radius: u32) -> Image {
    let shape = image.shape();
    if shape.is_empty() {
        return image.clone();
    }
    let (width, height, channels) = (shape.width as isize, shape.height as isize, shape.channels);
    let (radius, n) = (radius as isize, (2 * radius + 1) as f32);
    let mut across = Image::zeros(shape);
    across
        .data_mut()
        .par_chunks_mut(shape.row_len())
        .enumerate()
        .for_each(|(y, row)| {
            let from = image.row(y as u32);
            for x in 0..width {
                for c in 0..channels {
                    let at = |dx: isize| from[(x + dx).clamp(0, width - 1) as usize * channels + c];
                    let sum: f32 = (-radius..=radius).map(at).sum();
                    row[x as usize * channels + c] = sum / n;
                }
            }
        });
    let mut out = Image::zeros(shape);
    out.data_mut()
        .par_chunks_mut(shape.row_len())
        .enumerate()
        .for_each(|(y, row)| {
            for dy in -radius..=radius {
                let from = across.row((y as isize + dy).clamp(0, height - 1) as u32);
                row.iter_mut()
                    .zip(from)
                    .for_each(|(sum, value)| *sum += value);
            }
            row.iter_mut().for_each(|sum| *sum /= n);
        });
    out
}

/// `test.add`'s parameters.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Weights {
    /// The first image's weight.
    #[serde(default = "one")]
    a: f32,
    /// The second's.
    #[serde(default = "one")]
    b: f32,
}

fn one() -> f32 {
    1.0
}

/// `test.add`: `a × first + b × second`, pixel by pixel, for two images
/// of one shape.
pub fn add() -> Arc<dyn Node> {
    let descriptor = Descriptor::new("test.add", 1, "arithmetic", "A weighted sum of two images")
        .input("first", PortType::Image)
        .input("second", PortType::Image)
        .output("image", PortType::Image)
        .params::<Weights>()
        .capabilities(TILEABLE);
    node(descriptor, |weights: Weights| {
        move |_: &StepCtx<'_>, inputs: &Inputs<'_>| -> Result<Values, NodeError> {
            let (first, second) = (inputs.image("first")?, inputs.image("second")?);
            if first.shape() != second.shape() {
                return Err("the images' shapes differ".into());
            }
            let mut sum = first.clone();
            let pairs = sum.data_mut().par_iter_mut().zip(second.data());
            pairs.for_each(|(x, y)| *x = weights.a * *x + weights.b * y);
            Ok(outputs([("image", Value::image(sum))]))
        }
    })
}

/// `test.threshold`'s parameters.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Level {
    /// The channels' mean that a pixel must reach.
    level: f32,
}

/// `test.threshold`: a mask of the pixels whose channels' mean reaches
/// `level`.
pub fn threshold() -> Arc<dyn Node> {
    let descriptor = Descriptor::new(
        "test.threshold",
        1,
        "segmentation",
        "The pixels at or above a level",
    )
    .input("image", PortType::Image)
    .output("mask", PortType::Mask)
    .params::<Level>()
    .capabilities(TILEABLE);
    node(descriptor, |Level { level }| {
        move |_: &StepCtx<'_>, inputs: &Inputs<'_>| -> Result<Values, NodeError> {
            let image = inputs.image("image")?;
            let channels = image.channels().max(1);
            let mut mask = Mask::zeros(image.width(), image.height());
            let pixels = mask
                .data_mut()
                .par_iter_mut()
                .zip(image.data().par_chunks(channels));
            pixels.for_each(|(bit, pixel)| {
                *bit = u8::from(pixel.iter().sum::<f32>() / channels as f32 >= level)
            });
            Ok(outputs([("mask", Value::mask(mask))]))
        }
    })
}

/// `test.statistics`: each channel's mean, least and greatest value, as a
/// table, and the mean of every sample, as JSON.
pub fn statistics() -> Arc<dyn Node> {
    let descriptor = Descriptor::new(
        "test.statistics",
        1,
        "measurement",
        "Each channel's mean, least and greatest",
    )
    .input("image", PortType::Image)
    .output("table", PortType::Table)
    .output("mean", PortType::Json)
    .params::<Nothing>()
    .capabilities(Capabilities {
        tileable: false,
        ..TILEABLE
    });
    node(descriptor, |_: Nothing| {
        |_: &StepCtx<'_>, inputs: &Inputs<'_>| -> Result<Values, NodeError> {
            let image = inputs.image("image")?;
            let channels = image.channels();
            let mut table = Table {
                columns: ["channel", "mean", "least", "greatest"]
                    .map(String::from)
                    .to_vec(),
                rows: Vec::new(),
            };
            let mut total = 0.0;
            for c in 0..channels {
                let values = image
                    .data()
                    .iter()
                    .skip(c)
                    .step_by(channels.max(1))
                    .map(|&v| f64::from(v));
                let (sum, least, greatest) = values
                    .fold((0.0, f64::INFINITY, f64::NEG_INFINITY), |(s, l, g), v| {
                        (s + v, l.min(v), g.max(v))
                    });
                total += sum;
                let mean = sum / (image.data().len() / channels.max(1)).max(1) as f64;
                table
                    .rows
                    .push(vec![json!(c), json!(mean), json!(least), json!(greatest)]);
            }
            let mean = total / image.data().len().max(1) as f64;
            Ok(outputs([
                ("table", Value::table(table)),
                ("mean", Value::json(json!(mean))),
            ]))
        }
    })
}

/// `test.bin2x2`: each 2 × 2 block of pixels summed into one, as a sensor
/// bins, with `oscope-compute`'s kernel on the CPU or its CUDA twin.
pub fn bin2x2() -> Arc<dyn Node> {
    let descriptor = Descriptor::new("test.bin2x2", 1, "resize", "2 × 2 binning")
        .input("image", PortType::Image)
        .output("image", PortType::Image)
        .params::<Nothing>()
        .capabilities(Capabilities {
            cuda: true,
            tileable: false,
            ..TILEABLE
        });
    node(descriptor, |_: Nothing| binned)
}

fn binned(ctx: &StepCtx<'_>, inputs: &Inputs<'_>) -> Result<Values, NodeError> {
    let binned = match ctx.device() {
        Device::Cpu => Value::image(cpu::bin2x2(inputs.image("image")?)),
        #[cfg(feature = "cuda")]
        Device::Cuda => {
            let image = ctx
                .cuda()?
                .bin2x2(ctx.lane(), inputs.device_image("image")?)?;
            Value::DeviceImage(Arc::new(image))
        }
        #[cfg(not(feature = "cuda"))]
        Device::Cuda => return Err("this build has no CUDA".into()),
    };
    Ok(outputs([("image", binned)]))
}

/// `test.sleep`'s parameters.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Wait {
    /// How long to wait, in milliseconds.
    #[schemars(range(max = 60_000))]
    ms: u32,
}

/// `test.sleep`: the image as it is, after waiting `ms`, a slow step for
/// tests. It stops waiting when the run is cancelled.
pub fn sleep() -> Arc<dyn Node> {
    let descriptor = Descriptor::new("test.sleep", 1, "test", "The image as it is, after a wait")
        .input("image", PortType::Image)
        .output("image", PortType::Image)
        .params::<Wait>()
        .capabilities(TILEABLE);
    node(descriptor, |Wait { ms }| {
        move |ctx: &StepCtx<'_>, inputs: &Inputs<'_>| -> Result<Values, NodeError> {
            let end = Instant::now() + Duration::from_millis(ms.into());
            while let Some(left) = end.checked_duration_since(Instant::now()) {
                if ctx.is_cancelled() {
                    return Err(NodeError::Cancelled);
                }
                std::thread::sleep(left.min(Duration::from_millis(1)));
            }
            let image = inputs.get("image").cloned().ok_or("no image")?;
            Ok(Values::from([("image".to_owned(), image)]))
        }
    })
}
