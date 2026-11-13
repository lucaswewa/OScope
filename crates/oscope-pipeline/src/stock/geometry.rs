//! Geometry: binning, resizing and cropping; and the resampler, which
//! also fits references to the part of the frame an image shows.

use std::sync::Arc;

use oscope_compute::{Image, Shape, cpu};
use oscope_core::PixelRect;
use oscope_imaging::color;
use rayon::prelude::*;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{WHOLE, WHOLE_GPU};
use crate::node::{Descriptor, Device, Inputs, Node, NodeError, StepCtx, node, try_node};
use crate::value::{PortType, Value, Values, outputs};

/// The part of the camera's frame an image shows, in the frame's pixels:
/// what a `region` input takes, as JSON.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Region {
    /// Its left edge.
    pub x: f64,
    /// Its top edge.
    pub y: f64,
    /// Its width.
    pub width: f64,
    /// Its height.
    pub height: f64,
    /// The whole frame's width.
    pub frame_width: f64,
    /// The whole frame's height.
    pub frame_height: f64,
}

impl Region {
    pub(crate) fn from_json(json: &serde_json::Value) -> Result<Region, NodeError> {
        let region = Region::deserialize(json).map_err(|error| format!("the region: {error}"))?;
        let sizes = [
            region.width,
            region.height,
            region.frame_width,
            region.frame_height,
        ];
        if !sizes.iter().all(|size| size.is_finite() && *size > 0.0) {
            return Err("the region's sizes must be above 0".into());
        }
        Ok(region)
    }

    /// The window of `reference`, an image of the whole frame, that the
    /// region is, as `[x, y, width, height]` in its pixels.
    pub(crate) fn window(&self, reference: &Image) -> [f64; 4] {
        let across = f64::from(reference.width()) / self.frame_width;
        let down = f64::from(reference.height()) / self.frame_height;
        [
            self.x * across,
            self.y * down,
            self.width * across,
            self.height * down,
        ]
    }
}

/// For each output sample along an axis, the source samples it takes and
/// their weights. One that covers a source span of a sample or more
/// averages it, weighting each source sample by its overlap. One that
/// covers less interpolates linearly at its centre, so enlarging is smooth.
fn taps(start: f64, span: f64, source: u32, out: u32) -> Vec<Vec<(usize, f32)>> {
    let step = span / f64::from(out);
    let last = f64::from(source.max(1) - 1);
    (0..out)
        .map(|i| {
            let (a, b) = (start + f64::from(i) * step, start + f64::from(i + 1) * step);
            if step >= 1.0 {
                let first = a.floor().max(0.0) as usize;
                let end = (b.ceil().max(0.0) as usize).min(source as usize);
                let overlap = |k: usize| b.min(k as f64 + 1.0) - a.max(k as f64);
                let taps: Vec<(usize, f64)> = (first..end)
                    .map(|k| (k, overlap(k)))
                    .filter(|(_, w)| *w > 0.0)
                    .collect();
                let total: f64 = taps.iter().map(|(_, w)| w).sum();
                taps.into_iter()
                    .map(|(k, w)| (k, (w / total) as f32))
                    .collect()
            } else {
                let centre = ((a + b) / 2.0 - 0.5).clamp(0.0, last);
                let low = centre.floor();
                let f = (centre - low) as f32;
                vec![(low as usize, 1.0 - f), ((low + 1.0).min(last) as usize, f)]
            }
        })
        .collect()
}

/// The `[x, y, width, height]` window of `image`, in its pixels, resampled
/// to `width` × `height`: averaged where it shrinks, interpolated linearly
/// where it grows.
pub(crate) fn resample(image: &Image, window: [f64; 4], width: u32, height: u32) -> Image {
    let channels = image.channels();
    let mut out = Image::zeros(Shape::new(width, height, channels));
    if out.shape().is_empty() || image.shape().is_empty() {
        return out;
    }
    let across = taps(window[0], window[2], image.width(), width);
    let down = taps(window[1], window[3], image.height(), height);
    let rows = out
        .data_mut()
        .par_chunks_mut(width as usize * channels)
        .enumerate();
    rows.for_each(|(y, row)| {
        for &(sy, wy) in &down[y] {
            let from = image.row(sy as u32);
            for (x, taps) in across.iter().enumerate() {
                for &(sx, wx) in taps {
                    let weight = wx * wy;
                    for c in 0..channels {
                        row[x * channels + c] += weight * from[sx * channels + c];
                    }
                }
            }
        }
    });
    out
}

/// `resize.bin`'s parameters.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Factor {
    /// The blocks' side, in pixels.
    #[schemars(range(min = 1, max = 64))]
    factor: u32,
}

/// `resize.bin`: each `factor` × `factor` block averaged into one pixel
/// (`oscope_compute::cpu::downsample`, or its CUDA twin).
pub(super) fn bin() -> Arc<dyn Node> {
    let descriptor = Descriptor::new("resize.bin", 1, "resize", "Box averaging by a whole factor")
        .input("image", PortType::Image)
        .output("image", PortType::Image)
        .params::<Factor>()
        .capabilities(WHOLE_GPU);
    node(descriptor, |Factor { factor }| {
        move |ctx: &StepCtx<'_>, inputs: &Inputs<'_>| -> Result<Values, NodeError> {
            let binned = match ctx.device() {
                Device::Cpu => Value::image(cpu::downsample(inputs.image("image")?, factor)?),
                #[cfg(feature = "cuda")]
                Device::Cuda => {
                    let image = ctx.cuda()?.downsample(
                        ctx.lane(),
                        inputs.device_image("image")?,
                        factor,
                    )?;
                    Value::DeviceImage(Arc::new(image))
                }
                #[cfg(not(feature = "cuda"))]
                Device::Cuda => return Err(super::no_cuda()),
            };
            Ok(outputs([("image", binned)]))
        }
    })
}

/// `resize.resize`'s parameters.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Size {
    /// The width, in pixels.
    #[schemars(range(min = 1, max = 65536))]
    width: u32,
    /// The height.
    #[schemars(range(min = 1, max = 65536))]
    height: u32,
}

/// `resize.resize`: the image at `width` × `height`, averaged where it
/// shrinks and interpolated linearly where it grows.
pub(super) fn resize() -> Arc<dyn Node> {
    let descriptor = Descriptor::new("resize.resize", 1, "resize", "To a size")
        .input("image", PortType::Image)
        .output("image", PortType::Image)
        .params::<Size>()
        .capabilities(WHOLE);
    node(descriptor, |Size { width, height }| {
        move |_: &StepCtx<'_>, inputs: &Inputs<'_>| -> Result<Values, NodeError> {
            let image = inputs.image("image")?;
            let whole = [
                0.0,
                0.0,
                f64::from(image.width()),
                f64::from(image.height()),
            ];
            Ok(outputs([(
                "image",
                Value::image(resample(image, whole, width, height)),
            )]))
        }
    })
}

/// `geometry.crop`: a rectangle of the image, which must lie inside it
/// (`oscope_imaging::color::crop`).
pub(super) fn crop() -> Arc<dyn Node> {
    let descriptor = Descriptor::new("geometry.crop", 1, "geometry", "A rectangle of the image")
        .input("image", PortType::Image)
        .output("image", PortType::Image)
        .params::<PixelRect>()
        .capabilities(WHOLE);
    try_node(descriptor, |rect: PixelRect| {
        if rect.width == 0 || rect.height == 0 {
            return Err("the rectangle is empty".into());
        }
        Ok(
            move |_: &StepCtx<'_>, inputs: &Inputs<'_>| -> Result<Values, NodeError> {
                Ok(outputs([(
                    "image",
                    Value::image(color::crop(inputs.image("image")?, rect)?),
                )]))
            },
        )
    })
}
