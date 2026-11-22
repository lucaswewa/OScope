//! The filter kernels' launches (`kernels/filters.cu`): the twins of
//! [`crate::filters`]. Each checks its settings with the CPU's code, so
//! both refuse the same.

use cudarc::driver::{LaunchConfig, PushKernelArg};

use super::{BLOCK, Cuda, DeviceImage, grid};
use crate::filters::{self, Clahe, MEDIAN_RADIUS};
use crate::{ComputeError, Lane, Shape};

impl Cuda {
    /// One pass of a convolution with `weights` along rows or down columns.
    fn convolve(
        &self,
        lane: Lane,
        image: &DeviceImage,
        weights: &[f32],
        along_rows: bool,
    ) -> Result<DeviceImage, ComputeError> {
        let shape = image.shape;
        let stream = self.stream(lane);
        let taps = stream.clone_htod(weights)?;
        let mut out = self.alloc(lane, shape)?;
        let (width, height, channels) = (shape.width, shape.height, shape.channels as u32);
        let (radius, along) = ((weights.len() / 2) as u32, u32::from(along_rows));
        let mut launch = stream.launch_builder(&self.kernels.convolve);
        launch
            .arg(&image.data)
            .arg(&mut out)
            .arg(&width)
            .arg(&height)
            .arg(&channels)
            .arg(&taps)
            .arg(&radius)
            .arg(&along);
        // SAFETY: the kernel reads the image's samples and the weights, and
        // writes as many samples to `out`, which is the image's size.
        unsafe { launch.launch(grid(shape.row_len() as u32, height)) }?;
        Ok(DeviceImage { shape, data: out })
    }

    /// A Gaussian blur of `sigma` pixels: the twin of
    /// [`filters::gaussian`].
    pub fn gaussian(
        &self,
        lane: Lane,
        image: &DeviceImage,
        sigma: f32,
    ) -> Result<DeviceImage, ComputeError> {
        let weights = filters::gaussian_weights(sigma)?;
        if image.shape.is_empty() {
            return self.copy(lane, image);
        }
        let across = self.convolve(lane, image, &weights, true)?;
        self.convolve(lane, &across, &weights, false)
    }

    /// The median of each window: the twin of [`filters::median`].
    pub fn median(
        &self,
        lane: Lane,
        image: &DeviceImage,
        radius: u32,
    ) -> Result<DeviceImage, ComputeError> {
        if radius > MEDIAN_RADIUS {
            let message = format!("a median's radius, {radius}, is at most {MEDIAN_RADIUS}");
            return Err(ComputeError::Setting(message));
        }
        let shape = image.shape;
        if shape.is_empty() {
            return self.copy(lane, image);
        }
        let mut out = self.alloc(lane, shape)?;
        let (width, height, channels) = (shape.width, shape.height, shape.channels as u32);
        let stream = self.stream(lane);
        let mut launch = stream.launch_builder(&self.kernels.median);
        launch
            .arg(&image.data)
            .arg(&mut out)
            .arg(&width)
            .arg(&height)
            .arg(&channels)
            .arg(&radius);
        // SAFETY: the kernel reads the image's samples, and writes as many to
        // `out`; its window holds 15 × 15 samples, and the radius is at most
        // 7.
        unsafe { launch.launch(grid(shape.row_len() as u32, height)) }?;
        Ok(DeviceImage { shape, data: out })
    }

    /// The bilateral filter: the twin of [`filters::bilateral`].
    pub fn bilateral(
        &self,
        lane: Lane,
        image: &DeviceImage,
        sigma_space: f32,
        sigma_range: f32,
    ) -> Result<DeviceImage, ComputeError> {
        let (spatial, radius) = filters::bilateral_weights(sigma_space)?;
        if sigma_range.is_nan() || sigma_range <= 0.0 {
            let message =
                format!("a bilateral filter's range sigma, {sigma_range}, must be above 0");
            return Err(ComputeError::Setting(message));
        }
        let shape = image.shape;
        if shape.channels > 4 {
            return Err(ComputeError::Channels {
                expected: 4,
                got: shape.channels,
            });
        }
        if shape.is_empty() {
            return self.copy(lane, image);
        }
        let stream = self.stream(lane);
        let weights = stream.clone_htod(&spatial)?;
        let mut out = self.alloc(lane, shape)?;
        let (width, height, channels) = (shape.width, shape.height, shape.channels as u32);
        let inverse = 1.0 / (2.0 * sigma_range * sigma_range);
        let mut launch = stream.launch_builder(&self.kernels.bilateral);
        launch
            .arg(&image.data)
            .arg(&mut out)
            .arg(&width)
            .arg(&height)
            .arg(&channels)
            .arg(&weights)
            .arg(&radius)
            .arg(&inverse);
        // SAFETY: the kernel reads the image's pixels, at most four channels
        // each, and the (2r + 1)² weights, and writes as many samples to
        // `out`.
        unsafe { launch.launch(grid(width, height)) }?;
        Ok(DeviceImage { shape, data: out })
    }

    /// Contrast-limited adaptive histogram equalisation: the twin of
    /// [`filters::clahe`].
    pub fn clahe(
        &self,
        lane: Lane,
        image: &DeviceImage,
        clahe: &Clahe,
    ) -> Result<DeviceImage, ComputeError> {
        let shape = image.shape;
        let (tiles_x, tiles_y) = clahe.tiles_for(shape)?;
        if shape.is_empty() {
            return self.copy(lane, image);
        }
        let (width, height, channels, bins) =
            (shape.width, shape.height, shape.channels as u32, clahe.bins);
        let stream = self.stream(lane);
        let tables = Shape::new(bins, tiles_x * tiles_y, shape.channels);
        let mut maps = self.alloc(lane, tables)?;
        let clip = clahe.clip;
        let mut launch = stream.launch_builder(&self.kernels.clahe_maps);
        launch
            .arg(&image.data)
            .arg(&width)
            .arg(&height)
            .arg(&channels)
            .arg(&tiles_x)
            .arg(&tiles_y)
            .arg(&bins)
            .arg(&clip)
            .arg(&mut maps);
        let config = LaunchConfig {
            grid_dim: (tiles_x * tiles_y, channels, 1),
            block_dim: (BLOCK, 1, 1),
            shared_mem_bytes: bins * 4,
        };
        // SAFETY: each block reads its tile's samples of one channel, counts
        // them in `bins` counters of shared memory, and writes its `bins`
        // values to its place in `maps`, which holds them all.
        unsafe { launch.launch(config) }?;
        let mut out = self.alloc(lane, shape)?;
        let mut launch = stream.launch_builder(&self.kernels.clahe_apply);
        launch
            .arg(&image.data)
            .arg(&mut out)
            .arg(&width)
            .arg(&height)
            .arg(&channels)
            .arg(&tiles_x)
            .arg(&tiles_y)
            .arg(&bins)
            .arg(&maps);
        // SAFETY: the kernel reads the image's samples and the tables, and
        // writes as many samples to `out`.
        unsafe { launch.launch(grid(shape.row_len() as u32, height)) }?;
        Ok(DeviceImage { shape, data: out })
    }
}
