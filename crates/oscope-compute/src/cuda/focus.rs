//! The focus measures' launch (`kernels/focus.cu`): the twin of
//! [`crate::focus::measures`]. The GPU computes each pixel's terms; the CPU
//! sums them as the CPU's measures do.

use cudarc::driver::{LaunchConfig, PushKernelArg};

use super::{BLOCK, Cuda, DeviceImage};
use crate::focus::{self, FocusMeasures};
use crate::{ComputeError, Lane};

impl Cuda {
    /// `image`'s focus measures: the twin of [`focus::measures`].
    pub fn focus_measures(
        &self,
        lane: Lane,
        image: &DeviceImage,
    ) -> Result<FocusMeasures, ComputeError> {
        let host = self.download(lane, image)?;
        focus::check(&host)?;
        let shape = image.shape;
        let n = shape.len();
        let stream = self.stream(lane);
        let mut brenner = stream.alloc_zeros::<f32>(n)?;
        let mut tenengrad = stream.alloc_zeros::<f32>(n)?;
        let mut laplacian = stream.alloc_zeros::<f32>(n)?;
        let (width, height) = (shape.width, shape.height);
        let mut launch = stream.launch_builder(&self.kernels.focus_terms);
        launch
            .arg(&image.data)
            .arg(&mut brenner)
            .arg(&mut tenengrad)
            .arg(&mut laplacian)
            .arg(&width)
            .arg(&height);
        let config = LaunchConfig {
            grid_dim: ((n as u32).div_ceil(BLOCK), 1, 1),
            block_dim: (BLOCK, 1, 1),
            shared_mem_bytes: 0,
        };
        // SAFETY: the kernel reads the image's n samples, and writes n terms
        // to each of the three buffers, each n long.
        unsafe { launch.launch(config) }?;
        let (b, t, l) = (
            stream.clone_dtoh(&brenner)?,
            stream.clone_dtoh(&tenengrad)?,
            stream.clone_dtoh(&laplacian)?,
        );
        let (w, h) = (width as usize, height as usize);
        Ok(focus::reduce(host.data(), &b, &t, &l, w, h))
    }
}
