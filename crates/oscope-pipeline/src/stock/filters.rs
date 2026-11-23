//! Filters and enhancement (P23): the Gaussian, median and bilateral
//! filters, the unsharp mask and CLAHE, over `oscope_compute::filters` and
//! their CUDA twins.

use std::sync::Arc;

#[cfg(feature = "cuda")]
use oscope_compute::Lane;
#[cfg(feature = "cuda")]
use oscope_compute::cuda::{Cuda, DeviceImage};
use oscope_compute::filters::{self, Clahe};
use oscope_compute::{ComputeError, Image};
use rayon::prelude::*;
use schemars::JsonSchema;
use serde::Deserialize;

use super::{PIXELWISE, WHOLE_GPU};
use crate::node::{
    Capabilities, Descriptor, Device, Inputs, Node, NodeError, Step, StepCtx, try_node,
};
use crate::value::{PortType, Value, Values, outputs};

/// Pixel by pixel, with a CUDA step, and tileable with a halo its step
/// gives.
const NEIGHBOURHOOD_GPU: Capabilities = Capabilities {
    cuda: true,
    ..PIXELWISE
};

/// A filter on the CPU.
type CpuFilter = Box<dyn Fn(&Image) -> Result<Image, ComputeError> + Send + Sync>;

/// Its CUDA twin.
#[cfg(feature = "cuda")]
type GpuFilter =
    Box<dyn Fn(&Cuda, Lane, &DeviceImage) -> Result<DeviceImage, ComputeError> + Send + Sync>;

/// An image filter, on the CPU or its CUDA twin, with the halo it reads.
struct Filter {
    cpu: CpuFilter,
    #[cfg(feature = "cuda")]
    gpu: GpuFilter,
    halo: u32,
}

impl Step for Filter {
    fn run(&self, ctx: &StepCtx<'_>, inputs: &Inputs<'_>) -> Result<Values, NodeError> {
        let out = match ctx.device() {
            Device::Cpu => Value::image((self.cpu)(inputs.image("image")?)?),
            #[cfg(feature = "cuda")]
            Device::Cuda => {
                let image = (self.gpu)(ctx.cuda()?, ctx.lane(), inputs.device_image("image")?)?;
                Value::DeviceImage(Arc::new(image))
            }
            #[cfg(not(feature = "cuda"))]
            Device::Cuda => return Err(super::no_cuda()),
        };
        Ok(outputs([("image", out)]))
    }

    fn halo(&self) -> Option<u32> {
        Some(self.halo)
    }
}

fn descriptor(id: &str, description: &str, capabilities: Capabilities) -> Descriptor {
    Descriptor::new(id, 1, id.split('.').next().unwrap_or_default(), description)
        .input("image", PortType::Image)
        .output("image", PortType::Image)
        .capabilities(capabilities)
}

/// `filter.gaussian`'s parameters.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Sigma {
    /// The Gaussian's standard deviation, in pixels.
    #[schemars(range(max = 32.0))]
    sigma: f32,
}

/// `filter.gaussian`: a Gaussian blur, separable, `⌈3σ⌉` pixels each side
/// (`oscope_compute::filters::gaussian`, or its CUDA twin).
pub(super) fn gaussian() -> Arc<dyn Node> {
    let descriptor =
        descriptor("filter.gaussian", "A Gaussian blur", NEIGHBOURHOOD_GPU).params::<Sigma>();
    try_node(descriptor, |Sigma { sigma }| {
        let weights = filters::gaussian_weights(sigma).map_err(|e| e.to_string())?;
        Ok(Filter {
            cpu: Box::new(move |image| filters::gaussian(image, sigma)),
            #[cfg(feature = "cuda")]
            gpu: Box::new(move |cuda, lane, image| cuda.gaussian(lane, image, sigma)),
            halo: (weights.len() / 2) as u32,
        })
    })
}

/// `filter.median`'s parameters.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Radius {
    /// The window's half-width, in pixels: a window `2r + 1` across.
    #[schemars(range(min = 1, max = 7))]
    radius: u32,
}

/// `filter.median`: each sample the median of its square window
/// (`oscope_compute::filters::median`, or its CUDA twin).
pub(super) fn median() -> Arc<dyn Node> {
    let descriptor =
        descriptor("filter.median", "The median of a window", NEIGHBOURHOOD_GPU).params::<Radius>();
    try_node(descriptor, |Radius { radius }| {
        Ok(Filter {
            cpu: Box::new(move |image| filters::median(image, radius)),
            #[cfg(feature = "cuda")]
            gpu: Box::new(move |cuda, lane, image| cuda.median(lane, image, radius)),
            halo: radius,
        })
    })
}

/// `filter.bilateral`'s parameters.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Bilateral {
    /// The distance weights' standard deviation, in pixels: a window
    /// `⌈2σ⌉` each side.
    #[schemars(range(max = 8.0))]
    sigma_space: f32,
    /// The difference weights' standard deviation, in the samples' units.
    sigma_range: f32,
}

/// `filter.bilateral`: an edge-keeping blur, weighting each neighbour by
/// its distance and by its colour's difference
/// (`oscope_compute::filters::bilateral`, or its CUDA twin).
pub(super) fn bilateral() -> Arc<dyn Node> {
    let descriptor = descriptor(
        "filter.bilateral",
        "An edge-keeping blur",
        NEIGHBOURHOOD_GPU,
    )
    .params::<Bilateral>();
    try_node(
        descriptor,
        |Bilateral {
             sigma_space,
             sigma_range,
         }| {
            let (_, radius) = filters::bilateral_weights(sigma_space).map_err(|e| e.to_string())?;
            if sigma_range.is_nan() || sigma_range <= 0.0 {
                return Err(format!("the range sigma, {sigma_range}, must be above 0"));
            }
            Ok(Filter {
                cpu: Box::new(move |image| filters::bilateral(image, sigma_space, sigma_range)),
                #[cfg(feature = "cuda")]
                gpu: Box::new(move |cuda, lane, image| {
                    cuda.bilateral(lane, image, sigma_space, sigma_range)
                }),
                halo: radius,
            })
        },
    )
}

fn tiles() -> [u32; 2] {
    [8, 8]
}

fn clip() -> f32 {
    2.0
}

fn bins() -> u32 {
    256
}

/// `enhance.clahe`'s parameters.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ClaheParams {
    /// The tiles across and down: 8 × 8 unless given.
    #[serde(default = "tiles")]
    tiles: [u32; 2],
    /// How far above the mean a histogram's bin may count, its excess
    /// shared: 2 unless given; 1 equalises nothing.
    #[serde(default = "clip")]
    clip: f32,
    /// The histograms' bins over 0–1: 256 unless given.
    #[serde(default = "bins")]
    #[schemars(range(min = 2, max = 65536))]
    bins: u32,
}

/// `enhance.clahe`: contrast-limited adaptive histogram equalisation,
/// channel by channel over 0–1 (`oscope_compute::filters::clahe`, or its
/// CUDA twin).
pub(super) fn clahe() -> Arc<dyn Node> {
    let descriptor = descriptor(
        "enhance.clahe",
        "Contrast-limited adaptive histogram equalisation",
        WHOLE_GPU,
    )
    .params::<ClaheParams>();
    try_node(descriptor, |ClaheParams { tiles, clip, bins }| {
        let settings = Clahe {
            tiles: (tiles[0], tiles[1]),
            clip,
            bins,
        };
        let shape = oscope_compute::Shape::new(u32::MAX, u32::MAX, 1);
        settings.tiles_for(shape).map_err(|e| e.to_string())?;
        Ok(Filter {
            cpu: Box::new(move |image| filters::clahe(image, &settings)),
            #[cfg(feature = "cuda")]
            gpu: Box::new(move |cuda, lane, image| cuda.clahe(lane, image, &settings)),
            halo: 0,
        })
    })
}

/// `enhance.unsharp`'s parameters.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Unsharp {
    /// The blur's standard deviation, in pixels.
    #[schemars(range(max = 32.0))]
    sigma: f32,
    /// How much of the detail is added back: 1 doubles it.
    amount: f32,
}

/// `enhance.unsharp`: the unsharp mask, `v + amount × (v − blurred)`, with
/// a Gaussian blur.
pub(super) fn unsharp() -> Arc<dyn Node> {
    let descriptor =
        descriptor("enhance.unsharp", "The unsharp mask", PIXELWISE).params::<Unsharp>();
    try_node(descriptor, |Unsharp { sigma, amount }| {
        let weights = filters::gaussian_weights(sigma).map_err(|e| e.to_string())?;
        Ok(Filter {
            cpu: Box::new(move |image| {
                let blurred = filters::gaussian(image, sigma)?;
                let mut out = blurred;
                let samples = out.data_mut().par_iter_mut().zip(image.data().par_iter());
                samples.for_each(|(blur, v)| *blur = v + amount * (v - *blur));
                Ok(out)
            }),
            #[cfg(feature = "cuda")]
            gpu: Box::new(|_, _, _| {
                Err(ComputeError::Cuda(
                    "the unsharp mask runs on the CPU".into(),
                ))
            }),
            halo: (weights.len() / 2) as u32,
        })
    })
}
