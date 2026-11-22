//! Richardson–Lucy on the GPU (P43): the twin of `oscope-volume`'s
//! `deconvolve`, which calls it with the volume padded and the PSF's
//! transfer function made. cuFFT's 3D real transforms make the
//! convolutions; `kernels/deconvolve.cu` the steps between them.

use std::sync::Arc;

use cudarc::cufft::CudaFft;
use cudarc::cufft::sys::{cufftType, float2};
use cudarc::driver::PushKernelArg;

use super::Cuda;
use super::color::flat;
use crate::{ComputeError, Lane};

fn cufft(error: cudarc::cufft::result::CufftError) -> ComputeError {
    ComputeError::Cuda(format!("cuFFT: {error:?}"))
}

impl Cuda {
    /// Richardson–Lucy over `data`, a padded volume of `size`, x fastest,
    /// periodic, by `otf`, the PSF's transfer function's half spectrum
    /// along x: the estimate after `iterations`, from the data, with total
    /// variation of weight `tv` if above 0. `progress` is told each
    /// iteration done and stops it by answering false: then `None`.
    #[allow(clippy::too_many_arguments)]
    pub fn richardson_lucy(
        &self,
        lane: Lane,
        data: &[f32],
        otf: &[[f32; 2]],
        size: [usize; 3],
        iterations: u32,
        tv: f32,
        progress: &mut dyn FnMut(u32) -> bool,
    ) -> Result<Option<Vec<f32>>, ComputeError> {
        if !self.info.cufft {
            return Err(ComputeError::Cuda(
                "cuFFT's library wasn't found".to_owned(),
            ));
        }
        let [nx, ny, nz] = size;
        let (n, half) = (nx * ny * nz, (nx / 2 + 1) * ny * nz);
        if data.len() != n || otf.len() != half || n == 0 {
            return Err(ComputeError::Setting(format!(
                "{} values and {} of a transfer function, for a volume of {size:?}",
                data.len(),
                otf.len()
            )));
        }
        let stream = self.stream(lane);
        let dims = [nz, ny, nx].map(|d| d as i32);
        let plan = |kind| CudaFft::plan_3d(dims[0], dims[1], dims[2], kind, Arc::clone(stream));
        let forward = plan(cufftType::CUFFT_R2C).map_err(cufft)?;
        let inverse = plan(cufftType::CUFFT_C2R).map_err(cufft)?;
        let measured = stream.clone_htod(data)?;
        let transfer: Vec<float2> = otf.iter().map(|&[x, y]| float2 { x, y }).collect();
        let transfer = stream.clone_htod(&transfer)?;
        let mut u = stream.clone_htod(data)?;
        let mut spectrum = stream.alloc_zeros::<float2>(half)?;
        let mut work = stream.alloc_zeros::<f32>(n)?;
        let mut normals = stream.alloc_zeros::<f32>(if tv > 0.0 { 3 * n } else { 3 })?;
        let floor = 1e-12 * data.iter().copied().fold(0.0_f32, f32::max);
        let (scale, half64, n64) = (1.0 / n as f32, half as u64, n as u64);
        let (x32, y32, z32, with_tv) = (nx as u32, ny as u32, nz as u32, i32::from(tv > 0.0));
        let spectrum_times = |spectrum: &mut cudarc::driver::CudaSlice<float2>, mirrored: i32| {
            let mut launch = stream.launch_builder(&self.kernels.rl_spectrum);
            launch
                .arg(spectrum)
                .arg(&transfer)
                .arg(&half64)
                .arg(&mirrored)
                .arg(&scale);
            // SAFETY: `spectrum` and `transfer` both hold `half64` values,
            // and each thread writes its own.
            unsafe { launch.launch(flat(half64)) }.map(|_| ())
        };
        for k in 0..iterations {
            forward.exec_r2c(&u, &mut spectrum).map_err(cufft)?;
            spectrum_times(&mut spectrum, 0)?;
            inverse.exec_c2r(&mut spectrum, &mut work).map_err(cufft)?;
            let mut launch = stream.launch_builder(&self.kernels.rl_ratio);
            launch.arg(&measured).arg(&mut work).arg(&n64).arg(&floor);
            // SAFETY: `measured` and `work` both hold `n64` values, and each
            // thread writes its own.
            unsafe { launch.launch(flat(n64)) }?;
            forward.exec_r2c(&work, &mut spectrum).map_err(cufft)?;
            spectrum_times(&mut spectrum, 1)?;
            inverse.exec_c2r(&mut spectrum, &mut work).map_err(cufft)?;
            if tv > 0.0 {
                let mut launch = stream.launch_builder(&self.kernels.rl_normals);
                launch
                    .arg(&u)
                    .arg(&mut normals)
                    .arg(&x32)
                    .arg(&y32)
                    .arg(&z32);
                // SAFETY: `u` holds the volume's `n64` values and `normals`
                // three for each; each thread writes its own three.
                unsafe { launch.launch(flat(n64)) }?;
            }
            let mut launch = stream.launch_builder(&self.kernels.rl_update);
            launch.arg(&mut u).arg(&work).arg(&normals);
            launch.arg(&x32).arg(&y32).arg(&z32).arg(&tv).arg(&with_tv);
            // SAFETY: `u` and `work` hold `n64` values, `normals` three for
            // each when `with_tv` reads them; each thread writes its own.
            unsafe { launch.launch(flat(n64)) }?;
            if !progress(k + 1) {
                return Ok(None);
            }
        }
        Ok(Some(stream.clone_dtoh(&u)?))
    }
}
