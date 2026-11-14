//! Phase correlation on the GPU (P31): the twin of `oscope-imaging`'s
//! registration steps, which calls it through
//! `oscope_imaging::registration::gpu`. The kernels are in
//! `kernels/registration.cu`, and cuFFT's batched plans make the transforms.

use cudarc::cufft::CudaFft;
use cudarc::cufft::sys::{cufftType, float2};
use cudarc::driver::PushKernelArg;

use super::color::flat;
use super::{Cuda, Image};
use crate::{ComputeError, Lane};

/// What phase correlation takes besides its images, as `oscope-imaging`
/// sets it up.
#[derive(Clone, Copy, Debug)]
pub struct Correlation<'a> {
    /// Each image's mean, A's then B's for each pair.
    pub means: &'a [f32],
    /// The window across a row and down a column.
    pub window: (&'a [f32], &'a [f32]),
    /// The spectral weight by frequency index: across, for the half
    /// spectrum's `width / 2 + 1`; down, for all `height`.
    pub weight: (&'a [f32], &'a [f32]),
    /// The surface's scale.
    pub scale: f32,
    /// The region's first shift, across and down.
    pub from: (i64, i64),
    /// The region's width and height.
    pub size: (usize, usize),
}

fn cufft(error: cudarc::cufft::result::CufftError) -> ComputeError {
    ComputeError::Cuda(format!("cuFFT: {error:?}"))
}

fn setting(what: &str, expected: usize, got: usize) -> ComputeError {
    ComputeError::Setting(format!("{got} {what} where {expected} are needed"))
}

impl Cuda {
    /// The region of each pair's correlation surface, pair after pair, `size`
    /// values a row: the twin of `oscope_imaging::registration`'s steps,
    /// which check the images and set up `correlation`.
    pub fn phase_correlation(
        &self,
        lane: Lane,
        pairs: &[(&Image, &Image)],
        correlation: &Correlation<'_>,
    ) -> Result<Vec<f32>, ComputeError> {
        let Some((first, _)) = pairs.first() else {
            return Ok(Vec::new());
        };
        if !self.info.cufft {
            return Err(ComputeError::Cuda(
                "cuFFT's library wasn't found".to_owned(),
            ));
        }
        let shape = first.shape();
        for image in pairs.iter().flat_map(|(a, b)| [a, b]) {
            if image.shape() != shape {
                let got = image.shape();
                return Err(ComputeError::Shape {
                    expected: shape,
                    got,
                });
            }
        }
        if shape.channels != 1 {
            let got = shape.channels;
            return Err(ComputeError::Channels { expected: 1, got });
        }
        let (width, height) = (shape.width as usize, shape.height as usize);
        let (half, n) = (width / 2 + 1, pairs.len());
        let c = correlation;
        let lengths = [
            ("means", 2 * n, c.means.len()),
            ("window values across", width, c.window.0.len()),
            ("window values down", height, c.window.1.len()),
            ("weights across", half, c.weight.0.len()),
            ("weights down", height, c.weight.1.len()),
        ];
        if let Some((what, expected, got)) = lengths.into_iter().find(|(_, e, g)| e != g) {
            return Err(setting(what, expected, got));
        }
        let (region_width, region_height) = c.size;
        let region = region_width * region_height;
        if region == 0 {
            return Err(ComputeError::Setting("an empty region".to_owned()));
        }
        let stream = self.stream(lane);
        let len = width * height;
        let mut images = stream.alloc_zeros::<f32>(2 * n * len)?;
        for (i, image) in pairs.iter().flat_map(|(a, b)| [a, b]).enumerate() {
            stream.memcpy_htod(image.data(), &mut images.slice_mut(i * len..(i + 1) * len))?;
        }
        let means = stream.clone_htod(c.means)?;
        let window = (
            stream.clone_htod(c.window.0)?,
            stream.clone_htod(c.window.1)?,
        );
        let weight = (
            stream.clone_htod(c.weight.0)?,
            stream.clone_htod(c.weight.1)?,
        );
        let (w, h, count) = (shape.width, shape.height, 2 * n as u32);
        let mut launch = stream.launch_builder(&self.kernels.prepare);
        launch
            .arg(&mut images)
            .arg(&means)
            .arg(&window.0)
            .arg(&window.1);
        launch.arg(&w).arg(&h).arg(&count);
        // SAFETY: the arguments are the kernel's, in kernels/registration.cu:
        // `images` holds `count` images of `w` × `h`, `means` one for each,
        // and the window `w` values across and `h` down. Each thread writes
        // its own samples.
        unsafe { launch.launch(flat((2 * n * len) as u64)) }?;

        // The transforms, as one batched plan each way: `height` rows of
        // `width`, the half spectrum `half` complex values a row.
        let dims = [h as i32, w as i32];
        let (real, complex) = ((len) as i32, (height * half) as i32);
        let forward = CudaFft::plan_many(
            &dims,
            None,
            1,
            real,
            None,
            1,
            complex,
            cufftType::CUFFT_R2C,
            count as i32,
            stream.clone(),
        )
        .map_err(cufft)?;
        let mut spectra = stream.alloc_zeros::<float2>(2 * n * height * half)?;
        forward.exec_r2c(&images, &mut spectra).map_err(cufft)?;
        drop(images);

        let mut cross = stream.alloc_zeros::<float2>(n * height * half)?;
        let (half32, pairs32) = (half as u32, n as u32);
        let mut launch = stream.launch_builder(&self.kernels.cross_power);
        launch.arg(&spectra).arg(&weight.0).arg(&weight.1);
        launch.arg(&half32).arg(&h).arg(&pairs32).arg(&mut cross);
        // SAFETY: `spectra` holds 2 `pairs32` half spectra of `half32` × `h`,
        // `cross` `pairs32` of them, and the weight `half32` values across
        // and `h` down. Each thread writes its own value.
        unsafe { launch.launch(flat((n * height * half) as u64)) }?;
        drop(spectra);

        let inverse = CudaFft::plan_many(
            &dims,
            None,
            1,
            complex,
            None,
            1,
            real,
            cufftType::CUFFT_C2R,
            pairs32 as i32,
            stream.clone(),
        )
        .map_err(cufft)?;
        let mut surfaces = stream.alloc_zeros::<f32>(n * len)?;
        inverse.exec_c2r(&mut cross, &mut surfaces).map_err(cufft)?;

        let mut out = stream.alloc_zeros::<f32>(n * region)?;
        let (x0, y0) = c.from;
        let (rw, rh) = (region_width as u32, region_height as u32);
        let mut launch = stream.launch_builder(&self.kernels.gather);
        launch.arg(&surfaces).arg(&w).arg(&h).arg(&pairs32);
        launch
            .arg(&x0)
            .arg(&y0)
            .arg(&rw)
            .arg(&rh)
            .arg(&c.scale)
            .arg(&mut out);
        // SAFETY: `surfaces` holds `pairs32` surfaces of `w` × `h`, and every
        // index is wrapped into one; `out` holds `pairs32` regions of `rw` ×
        // `rh`, and each thread writes its own value.
        unsafe { launch.launch(flat((n * region) as u64)) }?;
        Ok(stream.clone_dtoh(&out)?)
    }
}
