//! The Gaussian blur, exact and by cascaded boxes: the light kept, the
//! spread asked for, the Gaussian's shape, and the edges.

use oscope_compute::{Image, Shape};
use oscope_sim::blur::{BOXES_FROM, box_radii, gaussian};

/// A point of light at the centre of a single-channel image `n` across,
/// blurred by `sigma`.
fn blurred_point(n: u32, sigma: f64) -> Image {
    let mut image = Image::zeros(Shape::new(n, n, 1));
    let centre = (n / 2) as usize;
    image.data_mut()[centre * n as usize + centre] = 1.0;
    gaussian(&mut image, sigma);
    image
}

/// The variance along x of a centred point's blur.
fn variance(image: &Image) -> f64 {
    let n = image.width() as usize;
    let centre = (n / 2) as f64;
    image
        .data()
        .iter()
        .enumerate()
        .map(|(i, v)| f64::from(*v) * ((i % n) as f64 - centre).powi(2))
        .sum()
}

#[test]
fn the_cascade_s_widths_have_the_gaussian_s_variance() {
    // A box of radius r has variance ((2r + 1)² − 1) / 12; four in turn,
    // the sum.
    for sigma in [8.0, 9.7, 15.0, 33.3, 120.0] {
        let total: f64 = box_radii(sigma, 4)
            .iter()
            .map(|r| (((2 * r + 1) * (2 * r + 1) - 1) as f64) / 12.0)
            .sum();
        assert!(
            (total.sqrt() / sigma - 1.0).abs() < 0.02,
            "σ {sigma}: {}",
            total.sqrt()
        );
    }
}

#[test]
fn both_blurs_keep_the_light_and_spread_it_as_asked() {
    // Just under the switch to boxes, and well past it.
    for sigma in [BOXES_FROM - 0.1, BOXES_FROM, 20.0] {
        let image = blurred_point(241, sigma);
        let total: f64 = image.data().iter().map(|v| f64::from(*v)).sum();
        assert!((total - 1.0).abs() < 1e-4, "σ {sigma}: {total}");
        let spread = variance(&image).sqrt();
        assert!((spread / sigma - 1.0).abs() < 0.02, "σ {sigma}: {spread}");
    }
}

#[test]
fn the_cascade_has_a_gaussian_s_shape() {
    // The blur's profile along x, summed over y, against the exact
    // Gaussian of the same σ, relative to its peak: four boxes have a
    // slightly flatter top.
    for sigma in [BOXES_FROM, 20.0, 60.0] {
        let n = 481_usize;
        let image = blurred_point(n as u32, sigma);
        let peak = 1.0 / ((2.0 * std::f64::consts::PI).sqrt() * sigma);
        let worst = (0..n)
            .map(|x| {
                let profile: f64 = (0..n).map(|y| f64::from(image.data()[y * n + x])).sum();
                let d = x as f64 - (n / 2) as f64;
                (profile - peak * (-d * d / (2.0 * sigma * sigma)).exp()).abs() / peak
            })
            .fold(0.0_f64, f64::max);
        assert!(worst < 0.05, "σ {sigma}: {worst}");
    }
}

#[test]
fn edges_repeat_so_an_even_image_stays_even() {
    for sigma in [3.0, 40.0] {
        let mut image = Image::zeros(Shape::new(50, 30, 3));
        image.data_mut().fill(0.25);
        gaussian(&mut image, sigma);
        assert!(
            image.data().iter().all(|v| (v - 0.25).abs() < 1e-6),
            "σ {sigma}"
        );
    }
}
