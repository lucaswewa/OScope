//! Correction: demosaicing, the black level, dark frames, flat fields,
//! white balance and the colour matrix.

use std::sync::{Arc, Mutex, PoisonError};

use oscope_compute::Image;
use oscope_core::BayerPattern;
#[cfg(feature = "cuda")]
use oscope_imaging::color::gpu;
use oscope_imaging::color::{self, ColourCorrection, Demosaic};
use rayon::prelude::*;
use schemars::JsonSchema;
use serde::Deserialize;

use super::{Nothing, PIXELWISE, PIXELWISE_GPU, Region, WHOLE, WHOLE_GPU, copied, resample};
use crate::node::{Descriptor, Device, Inputs, Node, NodeError, StepCtx, node};
use crate::value::{PortType, Value, Values, outputs};

/// `correction.demosaic`'s parameters.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct DemosaicParams {
    /// The mosaic's first 2 × 2 cell: `RGGB`, the MIChrome 20's, unless
    /// given.
    #[serde(default = "rggb")]
    pattern: BayerPattern,
    /// The method: `bilinear` unless given.
    #[serde(default)]
    method: Method,
}

fn rggb() -> BayerPattern {
    BayerPattern::Rggb
}

/// A demosaicing method (P08).
#[derive(Clone, Copy, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum Method {
    /// Each 2 × 2 cell to one pixel: an image half the size.
    Superpixel,
    /// Each missing colour from its neighbours, linearly.
    #[default]
    Bilinear,
    /// Malvar, He and Cutler's gradient-corrected interpolation.
    MalvarHeCutler,
}

impl From<Method> for Demosaic {
    fn from(method: Method) -> Self {
        match method {
            Method::Superpixel => Demosaic::Superpixel,
            Method::Bilinear => Demosaic::Bilinear,
            Method::MalvarHeCutler => Demosaic::MalvarHeCutler,
        }
    }
}

/// `correction.demosaic`: a Bayer mosaic, one channel, to RGB
/// (`oscope_imaging::color::demosaic`, or its CUDA twin).
pub(super) fn demosaic() -> Arc<dyn Node> {
    let descriptor = Descriptor::new(
        "correction.demosaic",
        1,
        "correction",
        "A Bayer mosaic to RGB",
    )
    .input("image", PortType::Image)
    .output("image", PortType::Image)
    .params::<DemosaicParams>()
    .capabilities(WHOLE_GPU);
    node(descriptor, |params: DemosaicParams| {
        let (pattern, method) = (params.pattern, Demosaic::from(params.method));
        move |ctx: &StepCtx<'_>, inputs: &Inputs<'_>| -> Result<Values, NodeError> {
            let rgb = match ctx.device() {
                Device::Cpu => {
                    Value::image(color::demosaic(inputs.image("image")?, pattern, method)?)
                }
                #[cfg(feature = "cuda")]
                Device::Cuda => {
                    let mosaic = inputs.device_image("image")?;
                    let rgb = gpu::demosaic(ctx.cuda()?, ctx.lane(), mosaic, pattern, method)?;
                    Value::DeviceImage(Arc::new(rgb))
                }
                #[cfg(not(feature = "cuda"))]
                Device::Cuda => return Err(super::no_cuda()),
            };
            Ok(outputs([("image", rgb)]))
        }
    })
}

/// `correction.black_level`'s parameters.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Black {
    /// The level taken off, on the 0–1 scale.
    #[schemars(range(min = 0.0, max = 0.99))]
    black: f32,
}

/// `correction.black_level`: each sample `(v − black) / (1 − black)`, at
/// least 0 (`oscope_imaging::color::subtract_black`, or its CUDA twin).
pub(super) fn black_level() -> Arc<dyn Node> {
    let descriptor = Descriptor::new(
        "correction.black_level",
        1,
        "correction",
        "The black level taken off",
    )
    .input("image", PortType::Image)
    .output("image", PortType::Image)
    .params::<Black>()
    .capabilities(PIXELWISE_GPU);
    node(descriptor, |Black { black }| {
        move |ctx: &StepCtx<'_>, inputs: &Inputs<'_>| -> Result<Values, NodeError> {
            let out = match ctx.device() {
                Device::Cpu => {
                    let mut image = copied(inputs.image("image")?);
                    color::subtract_black(&mut image, black)?;
                    Value::image(image)
                }
                #[cfg(feature = "cuda")]
                Device::Cuda => {
                    let cuda = ctx.cuda()?;
                    let mut image = cuda.copy(ctx.lane(), inputs.device_image("image")?)?;
                    gpu::subtract_black(cuda, ctx.lane(), &mut image, black)?;
                    Value::DeviceImage(Arc::new(image))
                }
                #[cfg(not(feature = "cuda"))]
                Device::Cuda => return Err(super::no_cuda()),
            };
            Ok(outputs([("image", out)]))
        }
    })
}

/// `correction.dark_frame`: a dark frame taken off, each sample at least
/// 0.
pub(super) fn dark_frame() -> Arc<dyn Node> {
    let descriptor = Descriptor::new(
        "correction.dark_frame",
        1,
        "correction",
        "A dark frame taken off",
    )
    .input("image", PortType::Image)
    .input("dark", PortType::Image)
    .optional_input("region", PortType::Json)
    .output("image", PortType::Image)
    .params::<Nothing>()
    .capabilities(PIXELWISE);
    node(descriptor, |_: Nothing| {
        let last = Sampled::default();
        move |_: &StepCtx<'_>, inputs: &Inputs<'_>| -> Result<Values, NodeError> {
            let image = inputs.image("image")?;
            let dark = matched(inputs, "dark", image, &last)?;
            let out = combine(image, &dark, |value, dark, _| (value - dark).max(0.0))?;
            Ok(outputs([("image", Value::image(out))]))
        }
    })
}

/// `correction.flat_field`: the image divided by a flat field and
/// multiplied by the flat's mean, channel by channel, so that an evenly
/// lit field comes out even, at its mean level. Where the flat is 0, so
/// is the image.
pub(super) fn flat_field() -> Arc<dyn Node> {
    let descriptor = Descriptor::new(
        "correction.flat_field",
        1,
        "correction",
        "Divided by a flat field",
    )
    .input("image", PortType::Image)
    .input("flat", PortType::Image)
    .optional_input("region", PortType::Json)
    .output("image", PortType::Image)
    .params::<Nothing>()
    .capabilities(WHOLE);
    node(descriptor, |_: Nothing| {
        let last = Sampled::default();
        move |_: &StepCtx<'_>, inputs: &Inputs<'_>| -> Result<Values, NodeError> {
            let image = inputs.image("image")?;
            let reference = inputs.image("flat")?;
            // The whole flat's means, so that zooming in keeps the level.
            let channels = reference.channels().max(1);
            let pixels = (reference.data().len() / channels).max(1) as f64;
            let sums = reference
                .data()
                .par_chunks(channels * 4096)
                .map(|chunk| {
                    let mut sums = vec![0.0_f64; channels];
                    for pixel in chunk.chunks(channels) {
                        sums.iter_mut()
                            .zip(pixel)
                            .for_each(|(sum, &v)| *sum += f64::from(v));
                    }
                    sums
                })
                .reduce(
                    || vec![0.0; channels],
                    |mut a, b| {
                        a.iter_mut().zip(b).for_each(|(a, b)| *a += b);
                        a
                    },
                );
            let means: Vec<f32> = sums.iter().map(|sum| (sum / pixels) as f32).collect();
            let flat = matched(inputs, "flat", image, &last)?;
            let out = combine(image, &flat, |value, flat, c| match flat > 1e-6 {
                true => value * means[c.min(channels - 1)] / flat,
                false => 0.0,
            })?;
            Ok(outputs([("image", Value::image(out))]))
        }
    })
}

/// `correction.white_balance`'s parameters.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Gains {
    /// Red, green and blue's gains.
    gains: [f32; 3],
}

/// `correction.white_balance`: red, green and blue multiplied by their
/// gains (`oscope_imaging::color::correct`, or its CUDA twin).
pub(super) fn white_balance() -> Arc<dyn Node> {
    let descriptor = Descriptor::new(
        "correction.white_balance",
        1,
        "correction",
        "White-balance gains",
    )
    .input("image", PortType::Image)
    .output("image", PortType::Image)
    .params::<Gains>()
    .capabilities(PIXELWISE_GPU);
    node(descriptor, |Gains { gains }| {
        let correction = ColourCorrection {
            gains,
            ..ColourCorrection::default()
        };
        move |ctx: &StepCtx<'_>, inputs: &Inputs<'_>| corrected(ctx, inputs, &correction)
    })
}

/// `correction.colour_matrix`'s parameters.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Matrix {
    /// The 3 × 3 matrix, by rows: from the camera's RGB to the output's.
    matrix: [[f32; 3]; 3],
}

/// `correction.colour_matrix`: each pixel's RGB multiplied by a 3 × 3
/// matrix (`oscope_imaging::color::correct`, or its CUDA twin).
pub(super) fn colour_matrix() -> Arc<dyn Node> {
    let descriptor = Descriptor::new(
        "correction.colour_matrix",
        1,
        "correction",
        "A 3 × 3 colour matrix",
    )
    .input("image", PortType::Image)
    .output("image", PortType::Image)
    .params::<Matrix>()
    .capabilities(PIXELWISE_GPU);
    node(descriptor, |Matrix { matrix }| {
        let correction = ColourCorrection {
            matrix,
            ..ColourCorrection::default()
        };
        move |ctx: &StepCtx<'_>, inputs: &Inputs<'_>| corrected(ctx, inputs, &correction)
    })
}

/// `correction` applied to an RGB image, where the step runs.
fn corrected(
    ctx: &StepCtx<'_>,
    inputs: &Inputs<'_>,
    correction: &ColourCorrection,
) -> Result<Values, NodeError> {
    let out = match ctx.device() {
        Device::Cpu => {
            let mut rgb = copied(inputs.image("image")?);
            color::correct(&mut rgb, correction)?;
            Value::image(rgb)
        }
        #[cfg(feature = "cuda")]
        Device::Cuda => {
            let (cuda, input) = (ctx.cuda()?, inputs.device_image("image")?);
            if input.shape().channels != 3 {
                return Err(color::ColorError::Channels(input.shape().channels).into());
            }
            let mut rgb = cuda.copy(ctx.lane(), input)?;
            gpu::correct(cuda, ctx.lane(), &mut rgb, correction)?;
            Value::DeviceImage(Arc::new(rgb))
        }
        #[cfg(not(feature = "cuda"))]
        Device::Cuda => return Err(super::no_cuda()),
    };
    Ok(outputs([("image", out)]))
}

/// The last reference a step sampled, kept while the reference, the
/// region and the size stay the same: in live mode, frame after frame. It
/// holds the reference itself, so its address can't be another's.
#[derive(Default)]
struct Sampled(Mutex<Option<Sample>>);

/// A reference, sampled over a region at a size.
struct Sample {
    from: Arc<Image>,
    region: Option<Region>,
    size: (u32, u32),
    sampled: Arc<Image>,
}

/// The reference on input `port`, at the image's size: as it is, or
/// sampled over the image's region of the frame (input `region`, or the
/// whole frame).
fn matched(
    inputs: &Inputs<'_>,
    port: &str,
    image: &Image,
    last: &Sampled,
) -> Result<Arc<Image>, NodeError> {
    let Some(Value::Image(reference)) = inputs.get(port) else {
        return Err(format!("input `{port}` isn't an image in memory").into());
    };
    let region = inputs.get("region").and_then(Value::as_json);
    let region = region.map(Region::from_json).transpose()?;
    let size = (image.width(), image.height());
    if region.is_none() && (reference.width(), reference.height()) == size {
        return Ok(Arc::clone(reference));
    }
    let mut last = last.0.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(sample) = &*last
        && Arc::ptr_eq(&sample.from, reference)
        && (sample.region, sample.size) == (region, size)
    {
        return Ok(Arc::clone(&sample.sampled));
    }
    let window = match region {
        Some(region) => region.window(reference),
        None => [
            0.0,
            0.0,
            f64::from(reference.width()),
            f64::from(reference.height()),
        ],
    };
    let sampled = Arc::new(resample(reference, window, size.0, size.1));
    *last = Some(Sample {
        from: Arc::clone(reference),
        region,
        size,
        sampled: Arc::clone(&sampled),
    });
    Ok(sampled)
}

/// The image with each sample combined with the reference's at its pixel
/// by `f(value, reference, channel)`. The reference has one channel, or
/// the image's.
fn combine(
    image: &Image,
    reference: &Image,
    f: impl Fn(f32, f32, usize) -> f32 + Sync,
) -> Result<Image, NodeError> {
    let (channels, theirs) = (image.channels().max(1), reference.channels().max(1));
    if theirs != 1 && theirs != channels {
        return Err(format!("a reference of {theirs} channels, for an image of {channels}").into());
    }
    let mut out = Image::zeros(image.shape());
    let pixels = out.data_mut().par_chunks_mut(channels);
    let pixels = pixels.zip(
        image
            .data()
            .par_chunks(channels)
            .zip(reference.data().par_chunks(theirs)),
    );
    pixels.for_each(|(out, (pixel, reference))| {
        for (c, (out, value)) in out.iter_mut().zip(pixel).enumerate() {
            *out = f(*value, reference[c.min(theirs - 1)], c);
        }
    });
    Ok(out)
}
