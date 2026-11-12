//! Raw Bayer mosaics to colour (D5). P08 brings demosaicing on the CPU,
//! with three methods ([`Demosaic`]), and the test charts that measure
//! them. P08b adds the colour steps and the writers, and P08c the CUDA twins.

mod chart;
mod demosaic;

pub use chart::{chart, mosaic, psnr};
pub use demosaic::{Demosaic, demosaic};

use oscope_compute::{Compute, ComputeError, Image, Lane, Shape};
use oscope_core::{FrameView, PixelFormat};

/// What the colour functions can't do.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum ColorError {
    /// The frame isn't a Bayer mosaic.
    #[error("{0:?} frames aren't Bayer mosaics")]
    NotAMosaic(PixelFormat),
    /// The image has the wrong number of channels: 1 for a mosaic, 3 for RGB.
    #[error("an image of {0} channels can't be used here")]
    Channels(usize),
    /// Two images that should match have different shapes.
    #[error("an image of {got:?} where {expected:?} was expected")]
    Shape {
        /// The first image's.
        expected: Shape,
        /// The second's.
        got: Shape,
    },
    /// Nothing is left to measure inside the border.
    #[error("the border leaves no pixels to measure")]
    Empty,
    /// The compute layer failed.
    #[error(transparent)]
    Compute(#[from] ComputeError),
}

/// A frame of a Bayer mosaic, in RGB from 0 to 1: converted to floats, then
/// demosaiced, in `lane`.
pub fn demosaic_frame(
    compute: &Compute,
    lane: Lane,
    view: &FrameView<'_>,
    method: Demosaic,
) -> Result<Image, ColorError> {
    let pattern = view
        .format()
        .bayer()
        .ok_or(ColorError::NotAMosaic(view.format()))?;
    let mosaic = compute.to_f32(lane, view)?;
    compute
        .pools()
        .install(lane, || demosaic(&mosaic, pattern, method))
}
