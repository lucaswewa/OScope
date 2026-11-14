//! Registration on the GPU (P31): the CPU's steps, through their CUDA twin
//! in `oscope-compute` ([`Cuda::phase_correlation`]), which brings back only
//! the region of each surface that the options consider. The peak is then
//! found from it by the same code as the CPU's.

use oscope_compute::cuda::{Correlation, Cuda};
use oscope_compute::{Image, Lane};

use super::{
    Located, RegisterOptions, Registration, RegistrationError, Setup, locate, means, overlap_pass,
};

/// The GPU memory that a call's pairs may take at once, about.
const GPU_MEMORY: usize = 1 << 30;

/// Registers `b` against `a` on the GPU: [`super::register`]'s twin.
pub fn register(
    cuda: &Cuda,
    lane: Lane,
    a: &Image,
    b: &Image,
    options: &RegisterOptions,
) -> Result<Registration, RegistrationError> {
    Ok(register_batch(cuda, lane, &[(a, b)], options)?.remove(0))
}

/// Registers each pair's B against its A on the GPU, their transforms
/// batched, as many pairs at once as fit in 1 GiB: [`super::register_batch`]'s
/// twin. Each pair's overlap pass follows, alone.
pub fn register_batch(
    cuda: &Cuda,
    lane: Lane,
    pairs: &[(&Image, &Image)],
    options: &RegisterOptions,
) -> Result<Vec<Registration>, RegistrationError> {
    let first = first_pass(cuda, lane, pairs, options)?;
    let again = |a: &Image, b: &Image, options: &RegisterOptions| {
        Ok(first_pass(cuda, lane, &[(a, b)], options)?.remove(0))
    };
    let pairs = pairs.iter().zip(first);
    pairs
        .map(|(pair, first)| overlap_pass(*pair, first, options, again))
        .collect()
}

/// The first pass on the GPU: steps 1 to 4 for each pair.
fn first_pass(
    cuda: &Cuda,
    lane: Lane,
    pairs: &[(&Image, &Image)],
    options: &RegisterOptions,
) -> Result<Vec<Located>, RegistrationError> {
    let setup = Setup::new(pairs, options)?;
    let means = means(pairs)?;
    let region = setup.region;
    // A pair's two images, their two half spectra, its cross power, its
    // surface, and cuFFT's work areas: about eight images' worth.
    let per_pair = 8 * setup.width * setup.height * size_of::<f32>();
    let at_once = (GPU_MEMORY / per_pair).max(1);
    let mut out = Vec::with_capacity(pairs.len());
    for (chunk, means) in pairs.chunks(at_once).zip(means.chunks(2 * at_once)) {
        let correlation = Correlation {
            means,
            window: (&setup.window.0, &setup.window.1),
            weight: (&setup.weight.0[..=setup.width / 2], &setup.weight.1),
            scale: setup.scale,
            from: (region.x0, region.y0),
            size: (region.w, region.h),
        };
        let values = cuda.phase_correlation(lane, chunk, &correlation)?;
        let each = values.chunks(region.w * region.h);
        out.extend(each.map(|values| locate(values, region, setup.reach)));
    }
    Ok(out)
}
