//! Stains (P45): each stain's amount by colour deconvolution, Macenko's
//! estimate of two stains' vectors, and DAB's classes and score.

use std::sync::Arc;

use oscope_compute::Image;
use oscope_imaging::stain::{DabCounts, DabThresholds, StainChoice, dab_classes, macenko};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::json;

use super::{PIXELWISE, WHOLE};
use crate::node::{Descriptor, Inputs, Node, NodeError, StepCtx, try_node};
use crate::value::{Mask, PortType, Table, Value, Values, outputs};

fn one() -> [f32; 3] {
    [1.0; 3]
}

/// An image's pixels, if it is RGB.
fn rgb(image: &Image) -> Result<&[f32], NodeError> {
    match image.channels() {
        3 => Ok(image.data()),
        n => Err(format!("stains are separated in RGB, not in {n} channels").into()),
    }
}

/// `stain.deconvolve`'s parameters.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Deconvolve {
    /// The stains: `he`, `hdab`, or `{"vectors": [[r, g, b], …]}`. H&E
    /// unless given.
    #[serde(default)]
    stains: StainChoice,
    /// The light through glass, in the image's units: 1 unless given.
    #[serde(default = "one")]
    white: [f32; 3],
}

/// `stain.deconvolve`: each stain's amount in each pixel, by colour
/// deconvolution (Ruifrok and Johnston, 2001): the first stain's, the
/// second's, and the third's or the residual's.
pub(super) fn deconvolve() -> Arc<dyn Node> {
    let descriptor = Descriptor::new(
        "stain.deconvolve",
        1,
        "stain",
        "Each stain's amount, by colour deconvolution",
    )
    .input("image", PortType::Image)
    .output("amounts", PortType::Image)
    .params::<Deconvolve>()
    .capabilities(PIXELWISE);
    try_node(descriptor, |Deconvolve { stains, white }| {
        let stains = stains.stains().map_err(|e| e.to_string())?;
        Ok(
            move |_: &StepCtx<'_>, inputs: &Inputs<'_>| -> Result<Values, NodeError> {
                let image = inputs.image("image")?;
                let amounts = stains.deconvolve(rgb(image)?, white);
                let amounts = Image::from_vec(image.shape(), amounts).map_err(|e| e.to_string())?;
                Ok(outputs([("amounts", Value::image(amounts))]))
            },
        )
    })
}

fn beta() -> f32 {
    0.15
}

fn percentile() -> f64 {
    1.0
}

/// `stain.estimate`'s parameters.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Estimate {
    /// The light through glass, in the image's units: 1 unless given.
    #[serde(default = "one")]
    white: [f32; 3],
    /// The least optical density a pixel used has: 0.15 unless given.
    #[serde(default = "beta")]
    beta: f32,
    /// The percentile of the angles at which the vectors are taken: 1
    /// unless given.
    #[serde(default = "percentile")]
    #[schemars(range(min = 0.0, max = 50.0))]
    percentile: f64,
}

/// `stain.estimate`: Macenko's estimate of the two stains' vectors (2009),
/// the one with more red first: `{"vectors": [[r, g, b], [r, g, b]]}`.
pub(super) fn estimate() -> Arc<dyn Node> {
    let descriptor = Descriptor::new(
        "stain.estimate",
        1,
        "stain",
        "Two stains' vectors, by Macenko's method",
    )
    .input("image", PortType::Image)
    .output("vectors", PortType::Json)
    .params::<Estimate>()
    .capabilities(WHOLE);
    try_node(descriptor, |p: Estimate| {
        Ok(
            move |_: &StepCtx<'_>, inputs: &Inputs<'_>| -> Result<Values, NodeError> {
                let image = inputs.image("image")?;
                let vectors = macenko(rgb(image)?, p.white, p.beta, p.percentile)
                    .map_err(|e| e.to_string())?;
                Ok(outputs([(
                    "vectors",
                    Value::json(json!({ "vectors": vectors })),
                )]))
            },
        )
    })
}

fn hdab() -> StainChoice {
    StainChoice::Hdab
}

/// `stain.dab`'s parameters.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Dab {
    /// The stains, DAB the second: H-DAB unless given.
    #[serde(default = "hdab")]
    stains: StainChoice,
    /// The light through glass, in the image's units: 1 unless given.
    #[serde(default = "one")]
    white: [f32; 3],
    /// The DAB amounts from which tissue is weak, moderate and strong:
    /// 0.15, 0.35 and 0.60 unless given.
    #[serde(default)]
    thresholds: DabThresholds,
}

/// `stain.dab`: each pixel's class, 0 glass, 1 negative, 2 weak, 3
/// moderate and 4 strong, by its summed optical density and its DAB.
pub(super) fn dab() -> Arc<dyn Node> {
    let descriptor = Descriptor::new(
        "stain.dab",
        1,
        "stain",
        "Each pixel's class by its DAB: glass, negative, weak, moderate, strong",
    )
    .input("image", PortType::Image)
    .output("classes", PortType::Mask)
    .params::<Dab>()
    .capabilities(PIXELWISE);
    try_node(descriptor, |p: Dab| {
        let stains = p.stains.stains().map_err(|e| e.to_string())?;
        Ok(
            move |_: &StepCtx<'_>, inputs: &Inputs<'_>| -> Result<Values, NodeError> {
                let image = inputs.image("image")?;
                let classes = dab_classes(&stains, rgb(image)?, p.white, &p.thresholds);
                let mut mask = Mask::zeros(image.width(), image.height());
                mask.data_mut().copy_from_slice(&classes);
                Ok(outputs([("classes", Value::mask(mask))]))
            },
        )
    })
}

/// `stain.dab_score`: of `stain.dab`'s classes, a row of the pixels in
/// each, the tissue's, the positive percentage and the H-score.
pub(super) fn dab_score() -> Arc<dyn Node> {
    let descriptor = Descriptor::new(
        "stain.dab_score",
        1,
        "stain",
        "DAB's counts, positive percentage and H-score",
    )
    .input("classes", PortType::Mask)
    .output("score", PortType::Table)
    .params::<super::Nothing>()
    .capabilities(WHOLE);
    try_node(descriptor, |_: super::Nothing| {
        Ok(
            move |_: &StepCtx<'_>, inputs: &Inputs<'_>| -> Result<Values, NodeError> {
                let mut counts = DabCounts::default();
                for &class in inputs.mask("classes")?.data() {
                    counts.add(class);
                }
                let c = counts;
                let columns = [
                    "glass",
                    "negative",
                    "weak",
                    "moderate",
                    "strong",
                    "tissue",
                    "positive %",
                    "H-score",
                ];
                let row = vec![
                    json!(c.glass),
                    json!(c.negative),
                    json!(c.weak),
                    json!(c.moderate),
                    json!(c.strong),
                    json!(c.tissue()),
                    json!(c.positive_percent()),
                    json!(c.h_score()),
                ];
                let table = Table {
                    columns: columns.map(str::to_owned).to_vec(),
                    rows: vec![row],
                };
                Ok(outputs([("score", Value::table(table))]))
            },
        )
    })
}
