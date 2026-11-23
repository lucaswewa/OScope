//! Thresholds (P23): by a level given, or by Otsu's method.

use std::sync::Arc;

use oscope_compute::Image;
use rayon::prelude::*;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::json;

use super::{PIXELWISE, WHOLE};
use crate::node::{Descriptor, Inputs, Node, NodeError, StepCtx, node, try_node};
use crate::value::{Mask, PortType, Value, Values, outputs};

/// Each pixel's intensity: the mean of its channels.
fn intensities(image: &Image) -> Vec<f32> {
    let channels = image.channels().max(1);
    let pixels = image.data().par_chunks(channels);
    pixels
        .map(|pixel| pixel.iter().sum::<f32>() / channels as f32)
        .collect()
}

/// The mask of the intensities at or above `level`.
fn mask(image: &Image, intensities: &[f32], level: f32) -> Mask {
    let mut mask = Mask::zeros(image.width(), image.height());
    let bits = mask.data_mut().par_iter_mut().zip(intensities.par_iter());
    bits.for_each(|(bit, &v)| *bit = u8::from(v >= level));
    mask
}

/// `threshold.manual`'s parameters.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Level {
    /// The intensity a pixel must reach: the mean of its channels.
    level: f32,
}

/// `threshold.manual`: a mask of the pixels whose intensity, the mean of
/// their channels, reaches `level`.
pub(super) fn manual() -> Arc<dyn Node> {
    let descriptor = Descriptor::new(
        "threshold.manual",
        1,
        "threshold",
        "The pixels at or above a level",
    )
    .input("image", PortType::Image)
    .output("mask", PortType::Mask)
    .params::<Level>()
    .capabilities(PIXELWISE);
    node(descriptor, |Level { level }| {
        move |_: &StepCtx<'_>, inputs: &Inputs<'_>| -> Result<Values, NodeError> {
            let image = inputs.image("image")?;
            Ok(outputs([(
                "mask",
                Value::mask(mask(image, &intensities(image), level)),
            )]))
        }
    })
}

fn bins() -> u32 {
    256
}

/// `threshold.otsu`'s parameters.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Bins {
    /// The histogram's bins over 0–1: 256 unless given.
    #[serde(default = "bins")]
    #[schemars(range(min = 2, max = 65536))]
    bins: u32,
}

/// The level by Otsu's method (1979): of the intensities' histogram over
/// 0–1, the split that maximises the variance between the two classes, the
/// first where several tie, as across a gap. The level is the upper edge of
/// the last bin below it.
pub(crate) fn otsu(intensities: &[f32], bins: usize) -> f32 {
    let mut histogram = vec![0_u64; bins];
    for &v in intensities {
        histogram[((v.clamp(0.0, 1.0) * bins as f32) as usize).min(bins - 1)] += 1;
    }
    let total = intensities.len() as f64;
    let sum: f64 = histogram
        .iter()
        .enumerate()
        .map(|(b, &n)| b as f64 * n as f64)
        .sum();
    let (mut below, mut below_sum, mut best, mut split) = (0.0, 0.0, -1.0, 0);
    for (b, &n) in histogram.iter().enumerate().take(bins - 1) {
        below += n as f64;
        below_sum += b as f64 * n as f64;
        let above = total - below;
        if below == 0.0 || above == 0.0 {
            continue;
        }
        let (mean_below, mean_above) = (below_sum / below, (sum - below_sum) / above);
        let between = below * above * (mean_below - mean_above).powi(2);
        if between > best {
            (best, split) = (between, b);
        }
    }
    (split + 1) as f32 / bins as f32
}

/// `threshold.otsu`: a mask of the pixels whose intensity reaches the level
/// Otsu's method finds, and the level, as JSON.
pub(super) fn otsu_node() -> Arc<dyn Node> {
    let descriptor = Descriptor::new(
        "threshold.otsu",
        1,
        "threshold",
        "The pixels above Otsu's level",
    )
    .input("image", PortType::Image)
    .output("mask", PortType::Mask)
    .output("level", PortType::Json)
    .params::<Bins>()
    .capabilities(WHOLE);
    try_node(descriptor, |Bins { bins }| {
        if bins < 2 {
            return Err(format!("{bins} bins: two at least"));
        }
        Ok(
            move |_: &StepCtx<'_>, inputs: &Inputs<'_>| -> Result<Values, NodeError> {
                let image = inputs.image("image")?;
                let intensities = intensities(image);
                let level = otsu(&intensities, bins as usize);
                Ok(outputs([
                    ("mask", Value::mask(mask(image, &intensities, level))),
                    ("level", Value::json(json!(level))),
                ]))
            },
        )
    })
}
