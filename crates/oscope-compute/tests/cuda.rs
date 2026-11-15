//! The CUDA backend on the GPU, with `OSCOPE_CUDA=1`: the twins match their
//! CPU references to the bit, at random sizes and at 20 MP; images go up and
//! down unchanged; and the frame pool's pinned memory is pinned.

#![cfg(feature = "cuda")]

use oscope_compute::equivalence::{Tolerance, assert_equivalent};
use oscope_compute::{Backend, Compute, ComputeConfig, ComputeError, Image, Lane, Shape, cpu};
use oscope_core::{BayerPattern, FramePool, FrameView, PixelFormat, PixelRect};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};

/// The GPU, when `OSCOPE_CUDA=1` asks for the tests that need one.
fn gpu() -> Option<Compute> {
    if std::env::var("OSCOPE_CUDA").as_deref() != Ok("1") {
        eprintln!("skipped: set OSCOPE_CUDA=1 to run the tests that need a GPU");
        return None;
    }
    let compute = Compute::new(&ComputeConfig::default()).unwrap();
    let backends = &compute.info().backends;
    assert!(
        compute.cuda().is_some(),
        "OSCOPE_CUDA=1, but no GPU: {backends:?}"
    );
    Some(compute)
}

#[test]
fn the_twins_match_the_cpu_to_the_bit_on_200_seeded_cases() {
    let Some(compute) = gpu() else { return };
    let cuda = compute.cuda().unwrap();
    let formats = [
        PixelFormat::Mono8,
        PixelFormat::Mono16,
        PixelFormat::Rgb8,
        PixelFormat::Rgb16,
        PixelFormat::Rgba8,
        PixelFormat::Bayer16 {
            pattern: BayerPattern::Grbg,
            significant_bits: 12,
        },
    ];
    let mut random = StdRng::seed_from_u64(7);
    for case in 0..200 {
        let format = formats[case % formats.len()];
        let (width, height) = (random.random_range(1..=300), random.random_range(1..=300));
        let bytes: Vec<u8> = (0..format.frame_bytes(width, height))
            .map(|_| random.random())
            .collect();
        let whole = FrameView::new(&bytes, format, width, height).unwrap();
        // Half the cases are a rectangle inside, so rows are a stride apart.
        let view = match case % 2 {
            0 => whole,
            _ => {
                let (x, y) = (
                    random.random_range(0..width),
                    random.random_range(0..height),
                );
                let rect = PixelRect::new(
                    x,
                    y,
                    random.random_range(1..=width - x),
                    random.random_range(1..=height - y),
                );
                whole.roi(rect).unwrap()
            }
        };
        let lane = if case % 3 == 0 {
            Lane::Batch
        } else {
            Lane::Live
        };
        let expected = cpu::to_f32(&view).unwrap();
        let converted = cuda.to_f32(lane, &view).unwrap();
        assert_equivalent(
            &expected,
            &cuda.download(lane, &converted).unwrap(),
            Tolerance::EXACT,
        );
        let binned = cuda.bin2x2(lane, &converted).unwrap();
        assert_equivalent(
            &cpu::bin2x2(&expected),
            &cuda.download(lane, &binned).unwrap(),
            Tolerance::EXACT,
        );
        let factor = random.random_range(1..=9);
        let reference = cpu::downsample(&expected, factor).unwrap();
        let downsampled = cuda.downsample(lane, &converted, factor).unwrap();
        assert_equivalent(
            &reference,
            &cuda.download(lane, &downsampled).unwrap(),
            Tolerance::EXACT,
        );
    }
}

#[test]
fn a_20_mp_frame_matches_on_the_gpu() {
    let Some(compute) = gpu() else { return };
    let cuda = compute.cuda().unwrap();
    let (width, height) = (5472, 3648);
    let format = PixelFormat::Bayer16 {
        pattern: BayerPattern::Rggb,
        significant_bits: 16,
    };
    let bytes: Vec<u8> = (0..width * height)
        .flat_map(|i| ((i * 13) as u16).to_le_bytes())
        .collect();
    let view = FrameView::new(&bytes, format, width, height).unwrap();
    let expected = cpu::to_f32(&view).unwrap();
    let converted = cuda.to_f32(Lane::Live, &view).unwrap();
    assert_equivalent(
        &expected,
        &cuda.download(Lane::Live, &converted).unwrap(),
        Tolerance::EXACT,
    );
    let binned = cuda
        .download(Lane::Live, &cuda.bin2x2(Lane::Live, &converted).unwrap())
        .unwrap();
    assert_equivalent(&cpu::bin2x2(&expected), &binned, Tolerance::EXACT);
    let preview = cuda.downsample(Lane::Live, &converted, 4).unwrap();
    let reference = cpu::downsample(&expected, 4).unwrap();
    assert_equivalent(
        &reference,
        &cuda.download(Lane::Live, &preview).unwrap(),
        Tolerance::EXACT,
    );
}

#[test]
fn images_go_up_and_down_unchanged_and_errors_are_kept() {
    let Some(compute) = gpu() else { return };
    let cuda = compute.cuda().unwrap();
    let data: Vec<f32> = (0..60).map(|v| v as f32 * 0.25).collect();
    let image = Image::from_vec(Shape::new(4, 5, 3), data).unwrap();
    let up = cuda.upload(Lane::Batch, &image).unwrap();
    assert_eq!(cuda.download(Lane::Batch, &up).unwrap(), image);
    assert_eq!(
        cuda.downsample(Lane::Batch, &up, 0).unwrap_err(),
        ComputeError::Factor
    );

    let thin = cuda
        .upload(Lane::Live, &Image::zeros(Shape::new(1, 5, 1)))
        .unwrap();
    let binned = cuda.bin2x2(Lane::Live, &thin).unwrap();
    assert_eq!(
        cuda.download(Lane::Live, &binned).unwrap().shape(),
        Shape::new(0, 2, 1)
    );

    let planar = PixelFormat::PlanarF32 { planes: 1 };
    let view = FrameView::new(&[0; 16], planar, 2, 2).unwrap();
    assert_eq!(
        cuda.to_f32(Lane::Live, &view).unwrap_err(),
        ComputeError::Unsupported(planar)
    );
}

#[test]
fn the_frame_pool_gets_pinned_memory_and_the_info_names_the_gpu() {
    let Some(compute) = gpu() else { return };
    let cuda = compute.cuda().unwrap();
    let pool = FramePool::new(40 << 20, 2, &cuda.pinned_allocator());
    let mut buffer = pool.take().unwrap();
    assert!(cuda.is_pinned(buffer.bytes()));
    assert!(!cuda.is_pinned(&vec![0_u8; 1 << 20]));
    // An upload from pinned memory is whole when it returns: the slab can
    // take the next frame at once.
    buffer
        .bytes_mut()
        .iter_mut()
        .enumerate()
        .for_each(|(i, b)| *b = i as u8);
    let sent = buffer.bytes().to_vec();
    let device = cuda.upload_bytes(Lane::Live, buffer.bytes()).unwrap();
    buffer.bytes_mut().fill(0xEE);
    let received = compute
        .cuda()
        .unwrap()
        .download_bytes(Lane::Live, &device)
        .unwrap();
    assert!(received == sent, "the upload read the next frame");

    let info = compute.info();
    let gpu = info.gpu.as_ref().unwrap();
    assert_eq!(info.backend(), Backend::Cuda);
    assert!(!gpu.name.is_empty() && gpu.memory_bytes > 0, "{gpu:?}");
    assert!(gpu.compute_capability >= (7, 5), "{gpu:?}");
    assert!(
        gpu.driver_cuda
            .split('.')
            .next()
            .unwrap()
            .parse::<u32>()
            .unwrap()
            >= 12
    );
}

/// The filters' twins (P23) against their CPU references, on 60 seeded
/// images of 1 to 3 channels and up to 300 × 300, settings drawn too: the
/// Gaussian, median and CLAHE to the bit, and the bilateral filter, whose
/// exponentials may differ by an ulp or two, within 1e-5.
#[test]
fn the_filters_twins_match_the_cpu() {
    use oscope_compute::filters::{self, Clahe};
    let Some(compute) = gpu() else { return };
    let cuda = compute.cuda().unwrap();
    let lane = Lane::Batch;
    let mut random = StdRng::seed_from_u64(23);
    for case in 0..60 {
        let channels = [1, 3, 2][case % 3];
        let (width, height) = (random.random_range(1..=300), random.random_range(1..=300));
        let shape = Shape::new(width, height, channels);
        let data = (0..shape.len())
            .map(|_| random.random_range(-0.1..1.1))
            .collect();
        let image = Image::from_vec(shape, data).unwrap();
        let up = cuda.upload(lane, &image).unwrap();
        let down = |result: Result<_, ComputeError>| cuda.download(lane, &result.unwrap()).unwrap();
        let sigma = random.random_range(0.3..4.0);
        assert_equivalent(
            &filters::gaussian(&image, sigma).unwrap(),
            &down(cuda.gaussian(lane, &up, sigma)),
            Tolerance::EXACT,
        );
        let radius = random.random_range(0..=7);
        assert_equivalent(
            &filters::median(&image, radius).unwrap(),
            &down(cuda.median(lane, &up, radius)),
            Tolerance::EXACT,
        );
        let (space, range) = (
            random.random_range(0.5..3.0),
            random.random_range(0.05..0.5),
        );
        let tolerance = Tolerance {
            abs: 1e-6,
            rel: 1e-5,
        };
        let bilateral = cuda.bilateral(lane, &up, space, range);
        assert_equivalent(
            &filters::bilateral(&image, space, range).unwrap(),
            &down(bilateral),
            tolerance,
        );
        let clahe = Clahe {
            tiles: (random.random_range(1..=8), random.random_range(1..=8)),
            clip: random.random_range(1.0..4.0),
            bins: [16, 64, 256][case % 3],
        };
        assert_equivalent(
            &filters::clahe(&image, &clahe).unwrap(),
            &down(cuda.clahe(lane, &up, &clahe)),
            Tolerance::EXACT,
        );
    }
    // Settings out of range are refused alike.
    let image = cuda
        .upload(lane, &Image::zeros(Shape::new(8, 8, 1)))
        .unwrap();
    assert!(matches!(
        cuda.median(lane, &image, 8),
        Err(ComputeError::Setting(_))
    ));
    assert!(matches!(
        cuda.gaussian(lane, &image, 0.0),
        Err(ComputeError::Setting(_))
    ));
}

#[test]
fn the_focus_measures_twin_matches_the_cpu() {
    use oscope_compute::focus;
    let Some(compute) = gpu() else { return };
    let cuda = compute.cuda().unwrap();
    let lane = Lane::Live;
    let mut random = StdRng::seed_from_u64(34);
    for case in 0..40 {
        let (w, h) = (random.random_range(3..300), random.random_range(3..300));
        let data = (0..w * h).map(|_| random.random_range(0.0..1.0)).collect();
        let image = Image::from_vec(Shape::new(w, h, 1), data).unwrap();
        let cpu = focus::measures(&image).unwrap();
        let gpu = cuda
            .focus_measures(lane, &cuda.upload(lane, &image).unwrap())
            .unwrap();
        assert_eq!(cpu, gpu, "case {case}: {w} × {h}");
        // And through the compute layer, which takes the GPU.
        assert_eq!(compute.focus_measures(lane, &image).unwrap(), cpu);
    }
    // A colour image is refused alike.
    let colour = cuda
        .upload(lane, &Image::zeros(Shape::new(8, 8, 3)))
        .unwrap();
    assert!(matches!(
        cuda.focus_measures(lane, &colour),
        Err(ComputeError::Channels { .. })
    ));
}
