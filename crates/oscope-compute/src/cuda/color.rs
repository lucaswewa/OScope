//! The colour kernels' launches (`kernels/color.cu`): the twins of
//! `oscope-imaging`'s colour functions, which call them through
//! `oscope_imaging::color::gpu`. Each checks the shapes its kernel relies
//! on before launching it.

use cudarc::driver::{LaunchConfig, PushKernelArg};
use oscope_core::{BayerColour, BayerPattern};

use super::{BLOCK, Cuda, DeviceImage, grid};
use crate::{ComputeError, Lane, Shape};

/// The demosaicing kernels: the twins of `oscope_imaging::color::Demosaic`'s
/// methods.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DemosaicKernel {
    /// Each 2 × 2 cell to one pixel.
    Superpixel,
    /// Bilinear interpolation.
    Bilinear,
    /// Malvar, He and Cutler's filters.
    MalvarHeCutler,
}

/// A pattern's cell for the kernels: two bits a colour (0 red, 1 green, 2
/// blue) for (0, 0), (1, 0), (0, 1) and (1, 1), lowest first.
fn cells(pattern: BayerPattern) -> u32 {
    (0..4)
        .map(|k| {
            let code = match pattern.colour_at(k % 2, k / 2) {
                BayerColour::Red => 0,
                BayerColour::Green => 1,
                BayerColour::Blue => 2,
            };
            code << (2 * k)
        })
        .sum()
}

/// A launch with a thread for each of `len` items, each thread stepping on
/// by the whole grid.
pub(super) fn flat(len: u64) -> LaunchConfig {
    let blocks = len.div_ceil(u64::from(BLOCK)).clamp(1, 1 << 20) as u32;
    LaunchConfig {
        grid_dim: (blocks, 1, 1),
        block_dim: (BLOCK, 1, 1),
        shared_mem_bytes: 0,
    }
}

/// Refuses an image without `channels` channels: a kernel would read past it.
fn need(image: &DeviceImage, channels: usize) -> Result<(), ComputeError> {
    match image.shape.channels == channels {
        true => Ok(()),
        false => Err(ComputeError::Channels {
            expected: channels,
            got: image.shape.channels,
        }),
    }
}

impl Cuda {
    /// RGB from a one-channel mosaic whose top-left pixel is `pattern`'s
    /// first: the twin of `oscope_imaging::color::demosaic`.
    pub fn demosaic(
        &self,
        lane: Lane,
        mosaic: &DeviceImage,
        pattern: BayerPattern,
        kernel: DemosaicKernel,
    ) -> Result<DeviceImage, ComputeError> {
        need(mosaic, 1)?;
        let (width, height) = (mosaic.shape.width, mosaic.shape.height);
        let shape = match kernel {
            DemosaicKernel::Superpixel => Shape::new(width / 2, height / 2, 3),
            _ => Shape::new(width, height, 3),
        };
        let mut out = self.alloc(lane, shape)?;
        if !shape.is_empty() {
            let cells = cells(pattern);
            let stream = self.stream(lane);
            if kernel == DemosaicKernel::Superpixel {
                let (out_width, out_height) = (shape.width, shape.height);
                let mut launch = stream.launch_builder(&self.kernels.superpixel);
                launch.arg(&mosaic.data).arg(&width).arg(&cells);
                launch.arg(&mut out).arg(&out_width).arg(&out_height);
                // SAFETY: the arguments are the kernel's, in kernels/color.cu:
                // the mosaic holds `width` × 2 `out_height` samples, and `out`
                // `out_width` × `out_height` RGB pixels. Each thread writes its
                // own pixels.
                unsafe { launch.launch(grid(out_width, out_height)) }?;
            } else {
                let method = u32::from(kernel == DemosaicKernel::MalvarHeCutler);
                let mut launch = stream.launch_builder(&self.kernels.demosaic);
                launch
                    .arg(&mosaic.data)
                    .arg(&width)
                    .arg(&height)
                    .arg(&cells)
                    .arg(&method);
                launch.arg(&mut out);
                // SAFETY: as above: the mosaic holds `width` × `height`
                // samples, and every index is mirrored into it; `out` holds as
                // many RGB pixels.
                unsafe { launch.launch(grid(width, height)) }?;
            }
        }
        Ok(DeviceImage { shape, data: out })
    }

    /// `(v − black) / (1 − black)`, and 0 below black, in place: the twin of
    /// `oscope_imaging::color::subtract_black`, which checks `black`.
    pub fn subtract_black(
        &self,
        lane: Lane,
        image: &mut DeviceImage,
        black: f32,
    ) -> Result<(), ComputeError> {
        let len = image.shape.len() as u64;
        if len > 0 {
            let scale = 1.0 / (1.0 - black);
            let mut launch = self
                .stream(lane)
                .launch_builder(&self.kernels.subtract_black);
            launch
                .arg(&mut image.data)
                .arg(&len)
                .arg(&black)
                .arg(&scale);
            // SAFETY: the kernel touches `len` samples, the image's.
            unsafe { launch.launch(flat(len)) }?;
        }
        Ok(())
    }

    /// White-balance gains, then a 3 × 3 matrix by rows, in place: the twin
    /// of `oscope_imaging::color::correct`.
    pub fn correct(
        &self,
        lane: Lane,
        rgb: &mut DeviceImage,
        gains: [f32; 3],
        matrix: [[f32; 3]; 3],
    ) -> Result<(), ComputeError> {
        need(rgb, 3)?;
        let pixels = (rgb.shape.len() / 3) as u64;
        if pixels > 0 {
            let mut launch = self.stream(lane).launch_builder(&self.kernels.correct);
            launch.arg(&mut rgb.data).arg(&pixels);
            for value in gains.iter().chain(matrix.iter().flatten()) {
                launch.arg(value);
            }
            // SAFETY: the kernel touches `pixels` RGB pixels, the image's.
            unsafe { launch.launch(flat(pixels)) }?;
        }
        Ok(())
    }

    /// Each sample through a table of 65,536 levels, made by
    /// `oscope_imaging::color`'s display transform: its 8-bit samples.
    pub fn display(
        &self,
        lane: Lane,
        image: &DeviceImage,
        table: &[u8],
    ) -> Result<Vec<u8>, ComputeError> {
        if table.len() != 65_536 {
            return Err(ComputeError::Size {
                expected: 65_536,
                got: table.len(),
            });
        }
        let len = image.shape.len() as u64;
        if len == 0 {
            return Ok(Vec::new());
        }
        let stream = self.stream(lane);
        let table = self.copy_up(lane, table)?;
        let mut out = stream.alloc_zeros::<u8>(len as usize)?;
        let mut launch = stream.launch_builder(&self.kernels.display);
        launch.arg(&image.data).arg(&len).arg(&table).arg(&mut out);
        // SAFETY: the kernel reads `len` samples of the image and writes as
        // many bytes; it indexes the 65,536-entry table with values clamped
        // to 0–65,535.
        unsafe { launch.launch(flat(len)) }?;
        Ok(stream.clone_dtoh(&out)?)
    }
}
