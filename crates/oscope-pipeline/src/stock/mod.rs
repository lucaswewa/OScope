//! The stock nodes (P22): correction, display, geometry and measurement,
//! the steps the live view and every acquisition need. Where P08–P08c
//! made a function (`oscope_imaging::color`, `oscope_compute::cpu`), the
//! node runs it, and its CUDA twin on the GPU.
//!
//! | Node | What it does | Runs |
//! |---|---|---|
//! | `correction.demosaic` | A Bayer mosaic to RGB: superpixel, bilinear or Malvar–He–Cutler | CPU, CUDA; live |
//! | `correction.black_level` | The black level taken off, the rest stretched back to 1 | CPU, CUDA; live; tiles |
//! | `correction.dark_frame` | A dark frame taken off | CPU; live; tiles |
//! | `correction.flat_field` | Divided by a flat field, scaled by its mean | CPU; live |
//! | `correction.white_balance` | Red, green and blue gains | CPU, CUDA; live; tiles |
//! | `correction.colour_matrix` | A 3 × 3 colour matrix | CPU, CUDA; live; tiles |
//! | `display.levels` | Black and white points and a tone curve, to display values | CPU; live; tiles |
//! | `display.lut` | A lookup table, interpolated | CPU; live; tiles |
//! | `display.false_colour` | Intensity to a colour map | CPU; live; tiles |
//! | `resize.bin` | Box averaging by a whole factor | CPU, CUDA; live |
//! | `resize.resize` | To a size: averaging to shrink, linear to enlarge | CPU; live |
//! | `geometry.crop` | A rectangle | CPU; live |
//! | `measure.histogram` | Each channel's histogram, and its statistics | CPU; live |
//! | `convert.format` | Grey, RGB or one channel, and a bit depth | CPU; live; tiles |
//! | `io.reference` | A reference image, such as a flat field, from the references folder | CPU; live |
//!
//! **References** cover the camera's whole frame. A dark frame or a flat
//! field of another size than the image is sampled over the part of the
//! frame the image shows: the `region` input, `{x, y, width, height,
//! frame_width, frame_height}` in the frame's pixels, or the whole frame
//! without it.

mod correction;
mod display;
mod geometry;
mod measure;
mod reference;

use std::path::PathBuf;

use oscope_compute::Image;
use rayon::prelude::*;
use schemars::JsonSchema;
use serde::Deserialize;

use crate::node::Capabilities;
use crate::registry::Registry;

pub use geometry::Region;
pub(crate) use geometry::resample;

/// No parameters.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Nothing {}

/// A registry of the stock nodes. `io.reference` reads from `references`.
pub fn registry(references: impl Into<PathBuf>) -> Registry {
    let mut registry = Registry::new();
    let nodes = [
        correction::demosaic(),
        correction::black_level(),
        correction::dark_frame(),
        correction::flat_field(),
        correction::white_balance(),
        correction::colour_matrix(),
        display::levels(),
        display::lut(),
        display::false_colour(),
        geometry::bin(),
        geometry::resize(),
        geometry::crop(),
        measure::histogram(),
        measure::format(),
        reference::reference(references.into()),
    ];
    for node in nodes {
        registry
            .register(node)
            .expect("the stock nodes are well formed");
    }
    registry
}

/// Pixel by pixel, on the CPU: live, and by tiles with no halo.
const PIXELWISE: Capabilities = Capabilities {
    cpu: true,
    cuda: false,
    live: true,
    tileable: true,
    halo: 0,
};

/// Pixel by pixel, with a CUDA step too.
const PIXELWISE_GPU: Capabilities = Capabilities {
    cuda: true,
    ..PIXELWISE
};

/// On the whole image, on the CPU: live, but not by tiles.
const WHOLE: Capabilities = Capabilities {
    tileable: false,
    ..PIXELWISE
};

/// On the whole image, with a CUDA step too.
const WHOLE_GPU: Capabilities = Capabilities {
    cuda: true,
    ..WHOLE
};

/// A copy of `image`, made in parallel: for steps that change their input
/// in place. A live image is tens of megabytes.
fn copied(image: &Image) -> Image {
    let mut copy = Image::zeros(image.shape());
    let chunks = copy
        .data_mut()
        .par_chunks_mut(1 << 16)
        .zip(image.data().par_chunks(1 << 16));
    chunks.for_each(|(to, from)| to.copy_from_slice(from));
    copy
}

/// For a step the planner put on the GPU in a build without CUDA, which it
/// never does.
#[cfg(not(feature = "cuda"))]
fn no_cuda() -> crate::node::NodeError {
    "this build has no CUDA".into()
}
