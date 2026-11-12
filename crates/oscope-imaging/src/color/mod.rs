//! Raw Bayer mosaics to colour (D5), in this order:
//!
//! 1. **the black level,** on the mosaic ([`subtract_black`]);
//! 2. **demosaicing,** with three methods ([`Demosaic`]), of the whole
//!    mosaic or of a region, binned ([`demosaic_region`]);
//! 3. **white balance and the colour matrix** ([`correct`]);
//! 4. **the display transform** to 8 bits ([`to_display`]);
//! 5. **files:** PNG for viewing, and 16-bit TIFF.
//!
//! The test charts measure the methods ([`chart`], [`psnr`]). With the
//! `cuda` feature, the module `gpu` runs the same steps on the GPU, through their
//! CUDA twins in `oscope-compute`.

mod chart;
mod demosaic;
mod files;
#[cfg(feature = "cuda")]
pub mod gpu;
mod region;
mod tone;

pub use chart::{chart, mosaic, psnr};
pub use demosaic::{Demosaic, demosaic};
pub use files::{encode_png, encode_tiff16};
pub use region::{crop, demosaic_region};
pub use tone::{ColourCorrection, Curve, Display, correct, subtract_black, to_display};

use oscope_compute::{Compute, ComputeError, Image, Lane, Shape};
use oscope_core::{FrameView, PixelFormat, PixelRect};

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
    /// The rectangle is empty, or not inside the image.
    #[error("{0:?} isn't a non-empty rectangle inside {1:?}")]
    Region(PixelRect, PixelRect),
    /// A setting is out of its range.
    #[error("{0}")]
    Setting(String),
    /// A file couldn't be written.
    #[error("the file: {0}")]
    File(String),
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
