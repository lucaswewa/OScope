//! Depth from focus (P42), a node for each step over `oscope_volume`: a
//! stack's focus measures, the heights and their confidence from them, and
//! the all-in-focus image. `export.mesh`, in `export`, writes the surface.
//! And deconvolution (P43): Born and Wolf's PSF for a stack, and
//! Richardson–Lucy by it.

use std::sync::Arc;

use oscope_volume::deconvolve::{DeconvolveOptions, Volume};
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

fn three() -> u32 {
    3
}

/// `volume.focus`'s parameters.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Measure {
    /// The modified Laplacian's step: its neighbours this many pixels away.
    #[serde(default = "two")]
    #[schemars(range(min = 1))]
    step: u32,
    /// The window's half-width: the measure is summed over 2r + 1 pixels
    /// square.
    #[serde(default = "three")]
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

/// `volume.psf`'s parameters.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct PsfParams {
    /// The objective's numerical aperture.
    na: f64,
    /// The emission's wavelength, in µm.
    wavelength_um: f64,
    /// The medium's refractive index: 1 for a dry objective.
    medium: f64,
    /// A voxel's size, in µm: x, y and z.
    voxel_um: [f64; 3],
}

/// `volume.psf`: Born and Wolf's widefield PSF for the stack, sampled on
/// its voxels over its depth either way, summing to 1, as a stack.
pub(super) fn psf() -> Arc<dyn Node> {
    let descriptor = Descriptor::new("volume.psf", 1, "volume", "Born and Wolf's widefield PSF")
        .input("stack", PortType::Stack)
        .output("psf", PortType::Stack)
        .params::<PsfParams>()
        .capabilities(STACKS);
    try_node(descriptor, |p: PsfParams| {
        let optics = oscope_volume::Optics {
            na: p.na,
            wavelength_um: p.wavelength_um,
            medium: p.medium,
        };
        Ok(
            move |_: &StepCtx<'_>, inputs: &Inputs<'_>| -> Result<Values, NodeError> {
                let stack = Volume::from_planes(inputs.stack("stack")?).map_err(failed)?;
                let kernel = oscope_volume::psf(&optics, p.voxel_um, stack.size).map_err(failed)?;
                Ok(outputs([("psf", Value::stack(kernel.planes()))]))
            },
        )
    })
}

/// `volume.deconvolve`'s parameters.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Deconvolve {
    /// Richardson–Lucy's iterations.
    #[serde(default = "twenty")]
    #[schemars(range(min = 1, max = 500))]
    iterations: u32,
    /// Total variation's weight: 0 for none, 0.002 typical.
    #[serde(default)]
    tv: f32,
}

fn twenty() -> u32 {
    20
}

/// `volume.deconvolve`: the stack deconvolved by the PSF, Richardson–Lucy
/// over it padded by mirroring, on the GPU when there is one.
pub(super) fn deconvolve() -> Arc<dyn Node> {
    let descriptor = Descriptor::new(
        "volume.deconvolve",
        1,
        "volume",
        "Richardson–Lucy deconvolution",
    )
    .input("stack", PortType::Stack)
    .input("psf", PortType::Stack)
    .output("stack", PortType::Stack)
    .params::<Deconvolve>()
    .capabilities(STACKS);
    try_node(descriptor, |Deconvolve { iterations, tv }| {
        let options = DeconvolveOptions { iterations, tv };
        Ok(
            move |ctx: &StepCtx<'_>, inputs: &Inputs<'_>| -> Result<Values, NodeError> {
                let stack = Volume::from_planes(inputs.stack("stack")?).map_err(failed)?;
                let kernel = Volume::from_planes(inputs.stack("psf")?).map_err(failed)?;
                let mut going = |_: u32| !ctx.is_cancelled();
                let sharpened =
                    oscope_volume::deconvolve(&stack, &kernel, &options, ctx.compute(), &mut going)
                        .map_err(failed)?
                        .ok_or(NodeError::Cancelled)?;
                Ok(outputs([("stack", Value::stack(sharpened.planes()))]))
            },
        )
    })
}
