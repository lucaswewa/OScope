//! The colour steps on the GPU, with the same settings as the CPU's: each
//! calls its CUDA twin in `oscope-compute` (`oscope_compute::cuda`), which
//! gives the same result to the bit. Images stay on the GPU between steps,
//! as [`DeviceImage`]s.

use oscope_compute::Lane;
use oscope_compute::cuda::{Cuda, DemosaicKernel, DeviceImage};
use oscope_core::{BayerPattern, FrameView};

use super::tone::{check_black, display_table};
use super::{ColorError, ColourCorrection, Demosaic, Display};

/// [`demosaic`](super::demosaic) on the GPU.
pub fn demosaic(
    cuda: &Cuda,
    lane: Lane,
    mosaic: &DeviceImage,
    pattern: BayerPattern,
    method: Demosaic,
) -> Result<DeviceImage, ColorError> {
    let kernel = match method {
        Demosaic::Superpixel => DemosaicKernel::Superpixel,
        Demosaic::Bilinear => DemosaicKernel::Bilinear,
        Demosaic::MalvarHeCutler => DemosaicKernel::MalvarHeCutler,
    };
    Ok(cuda.demosaic(lane, mosaic, pattern, kernel)?)
}

/// [`demosaic_frame`](super::demosaic_frame) on the GPU: the frame is
/// uploaded, converted to floats and demosaiced there.
pub fn demosaic_frame(
    cuda: &Cuda,
    lane: Lane,
    view: &FrameView<'_>,
    method: Demosaic,
) -> Result<DeviceImage, ColorError> {
    let pattern = view
        .format()
        .bayer()
        .ok_or(ColorError::NotAMosaic(view.format()))?;
    let mosaic = cuda.to_f32(lane, view)?;
    demosaic(cuda, lane, &mosaic, pattern, method)
}

/// [`subtract_black`](super::subtract_black) on the GPU.
pub fn subtract_black(
    cuda: &Cuda,
    lane: Lane,
    image: &mut DeviceImage,
    black: f32,
) -> Result<(), ColorError> {
    check_black(black)?;
    Ok(cuda.subtract_black(lane, image, black)?)
}

/// [`correct`](super::correct) on the GPU.
pub fn correct(
    cuda: &Cuda,
    lane: Lane,
    rgb: &mut DeviceImage,
    correction: &ColourCorrection,
) -> Result<(), ColorError> {
    Ok(cuda.correct(lane, rgb, correction.gains, correction.matrix)?)
}

/// [`to_display`](super::to_display) on the GPU: the 8-bit samples, copied
/// back.
pub fn to_display(
    cuda: &Cuda,
    lane: Lane,
    image: &DeviceImage,
    display: &Display,
) -> Result<Vec<u8>, ColorError> {
    let table = display_table(display)?;
    Ok(cuda.display(lane, image, &table)?)
}
