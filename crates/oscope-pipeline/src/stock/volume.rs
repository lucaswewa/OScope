//! Depth from focus (P42), a node for each step over `oscope_volume`: a
//! stack's focus measures, the heights and their confidence from them, and
//! the all-in-focus image. `export.mesh`, in `export`, writes the surface.

use std::sync::Arc;

use oscope_volume::depth::{self, AllInFocus, Peaks, Planes};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::node::{Capabilities, Descriptor, Inputs, Node, NodeError, StepCtx, try_node};
use crate::value::{PortType, Value, Values, outputs};

/// A stack's work, on the whole and not live. The focus measure takes the
/// GPU, through the compute layer, when there is one.
const STACKS: Capabilities = Capabilities {
    cpu: true,
    cuda: false,
    live: false,
    tileable: false,
    halo: 0,
};

fn one() -> u32 {
    1
}

fn two() -> u32 {
    2
}

/// `volume.focus`'s parameters.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Measure {
    /// The modified Laplacian's step: its neighbours this many pixels away.
    #[serde(default = "one")]
    #[schemars(range(min = 1))]
    step: u32,
    /// The window's half-width: the measure is summed over 2r + 1 pixels
    /// square.
    #[serde(default = "two")]
    #[schemars(range(max = 15))]
    radius: u32,
}

/// The planes' heights on input `planes`: `{z0_um, step_um}`.
fn planes(inputs: &Inputs<'_>) -> Result<Planes, NodeError> {
    let json = inputs.json("planes")?.clone();
    serde_json::from_value(json).map_err(|e| format!("the planes: {e}").into())
}

fn failed(error: oscope_volume::VolumeError) -> NodeError {
    NodeError::Failed(error.to_string())
}

/// `volume.focus`: each grey plane's sum-modified-Laplacian (Nayar and
/// Nakagawa, 1994), the stack's focus measures.
pub(super) fn focus() -> Arc<dyn Node> {
    let descriptor = Descriptor::new("volume.focus", 1, "volume", "A stack's focus measures")
        .input("stack", PortType::Stack)
        .output("measures", PortType::Stack)
        .params::<Measure>()
        .capabilities(STACKS);
    try_node(descriptor, |Measure { step, radius }| {
        if step == 0 {
            return Err("the step must be 1 or more".to_owned());
        }
        Ok(
            move |ctx: &StepCtx<'_>, inputs: &Inputs<'_>| -> Result<Values, NodeError> {
                let mut measures = Vec::new();
                for plane in inputs.stack("stack")? {
                    if ctx.is_cancelled() {
                        return Err(NodeError::Cancelled);
                    }
                    measures.push(ctx.compute().focus_sml(ctx.lane(), plane, step, radius)?);
                }
                Ok(outputs([("measures", Value::stack(measures))]))
            },
        )
    })
}

/// `volume.height`'s parameters.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Height {
    /// The median's half-width over the heights: 0 for none.
    #[serde(default = "one")]
    #[schemars(range(max = 7))]
    median: u32,
}

/// `volume.height`: from a stack's focus measures and its planes'
/// heights, each pixel's height in µm, at the vertex of the parabola about
/// its peak, its median taken, and its confidence, from 0 to 1.
pub(super) fn height() -> Arc<dyn Node> {
    let descriptor = Descriptor::new("volume.height", 1, "volume", "Heights from focus measures")
        .input("measures", PortType::Stack)
        .input("planes", PortType::Json)
        .output("heights", PortType::Image)
        .output("confidence", PortType::Image)
        .params::<Height>()
        .capabilities(STACKS);
    try_node(descriptor, |Height { median }| {
        Ok(
            move |_: &StepCtx<'_>, inputs: &Inputs<'_>| -> Result<Values, NodeError> {
                let measures = inputs.stack("measures")?;
                let first = measures.first().ok_or("an empty stack has no heights")?;
                let mut peaks = Peaks::new(first.width(), first.height());
                for plane in measures {
                    peaks.push(plane).map_err(failed)?;
                }
                let found = peaks.finish(planes(inputs)?).map_err(failed)?;
                let heights = depth::median(&found.heights, median).map_err(failed)?;
                Ok(outputs([
                    ("heights", Value::image(heights)),
                    ("confidence", Value::image(found.confidence)),
                ]))
            },
        )
    })
}

/// `volume.all_in_focus`: each pixel from the planes either side of its
/// height, blended by where it lies between them.
pub(super) fn all_in_focus() -> Arc<dyn Node> {
    let descriptor = Descriptor::new("volume.all_in_focus", 1, "volume", "The all-in-focus image")
        .input("stack", PortType::Stack)
        .input("heights", PortType::Image)
        .input("planes", PortType::Json)
        .output("image", PortType::Image)
        .params::<super::Nothing>()
        .capabilities(STACKS);
    try_node(descriptor, |_: super::Nothing| {
        Ok(
            move |_: &StepCtx<'_>, inputs: &Inputs<'_>| -> Result<Values, NodeError> {
                let stack = inputs.stack("stack")?;
                let channels = stack.first().map_or(1, |p| p.channels());
                let heights = inputs.image("heights")?;
                let count = stack.len() as u32;
                let mut image =
                    AllInFocus::new(heights, planes(inputs)?, count, channels).map_err(failed)?;
                for plane in stack {
                    image.push(plane).map_err(failed)?;
                }
                Ok(outputs([(
                    "image",
                    Value::image(image.finish().map_err(failed)?),
                )]))
            },
        )
    })
}
