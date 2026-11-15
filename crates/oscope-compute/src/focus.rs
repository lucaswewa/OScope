//! Focus measures (P34; ADR-0024) over a grey image: Brenner's gradient,
//! Tenengrad and the variance of the Laplacian. Each is divided by the
//! image's mean squared, so the light's level and the exposure don't change
//! it (Groen, Young and Ligthart, 1985).
//!
//! The CUDA twin (`cuda/focus.rs`, `kernels/focus.cu`) computes each
//! pixel's terms with the same arithmetic, and both sum them in
//! [`reduce`], so the two agree to the bit.

use crate::{ComputeError, Image};

/// A focus measure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FocusMeasure {
    /// Brenner's gradient, along x and y (Brenner et al., 1976).
    Brenner,
    /// Tenengrad: Sobel's gradient, squared (Tenenbaum, 1970).
    Tenengrad,
    /// The variance of the Laplacian (Pech-Pacheco et al., 2000).
    Laplacian,
}

/// An image's three focus measures.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FocusMeasures {
    /// Brenner's gradient.
    pub brenner: f64,
    /// Tenengrad.
    pub tenengrad: f64,
    /// The variance of the Laplacian.
    pub laplacian: f64,
}

impl FocusMeasures {
    /// The one asked for.
    pub fn get(&self, measure: FocusMeasure) -> f64 {
        match measure {
            FocusMeasure::Brenner => self.brenner,
            FocusMeasure::Tenengrad => self.tenengrad,
            FocusMeasure::Laplacian => self.laplacian,
        }
    }
}

/// Pixel `(x, y)`'s terms in an image of `w` × `h`: Brenner's, the
/// differences two pixels on along x and y squared, where they're inside;
/// Tenengrad's and the Laplacian, where its 3 × 3 neighbourhood is inside,
/// and 0 elsewhere. The kernel's arithmetic, step for step.
#[inline]
fn terms(data: &[f32], w: usize, h: usize, x: usize, y: usize) -> [f32; 3] {
    let i = y * w + x;
    let c = data[i];
    let mut b = 0.0_f32;
    if x + 2 < w {
        let d = data[i + 2] - c;
        b += d * d;
    }
    if y + 2 < h {
        let d = data[i + 2 * w] - c;
        b += d * d;
    }
    if x == 0 || y == 0 || x + 1 >= w || y + 1 >= h {
        return [b, 0.0, 0.0];
    }
    let (a00, a01, a02) = (data[i - w - 1], data[i - w], data[i - w + 1]);
    let (a10, a12) = (data[i - 1], data[i + 1]);
    let (a20, a21, a22) = (data[i + w - 1], data[i + w], data[i + w + 1]);
    let gx = (a02 + 2.0 * a12 + a22) - (a00 + 2.0 * a10 + a20);
    let gy = (a20 + 2.0 * a21 + a22) - (a00 + 2.0 * a01 + a02);
    let laplacian = a01 + a21 + a10 + a12 - 4.0 * c;
    [b, gx * gx + gy * gy, laplacian]
}

/// Checks `image` is grey and at least 3 × 3.
pub(crate) fn check(image: &Image) -> Result<(), ComputeError> {
    if image.channels() != 1 {
        return Err(ComputeError::Channels {
            expected: 1,
            got: image.channels(),
        });
    }
    if image.width() < 3 || image.height() < 3 {
        return Err(ComputeError::Setting(
            "a focus measure needs an image of at least 3 × 3 pixels".to_owned(),
        ));
    }
    Ok(())
}

/// The measures of `image`, grey and at least 3 × 3.
pub fn measures(image: &Image) -> Result<FocusMeasures, ComputeError> {
    check(image)?;
    let (w, h) = (image.width() as usize, image.height() as usize);
    let data = image.data();
    let n = w * h;
    let (mut b, mut t, mut l) = (vec![0.0; n], vec![0.0; n], vec![0.0; n]);
    for y in 0..h {
        for x in 0..w {
            [b[y * w + x], t[y * w + x], l[y * w + x]] = terms(data, w, h, x, y);
        }
    }
    Ok(reduce(data, &b, &t, &l, w, h))
}

/// The measures from each pixel's terms: Brenner's and Tenengrad's sums
/// over N μ²; the Laplacian's variance over the pixels inside, over μ². All
/// zero for an image with no light.
pub(crate) fn reduce(
    data: &[f32],
    brenner: &[f32],
    tenengrad: &[f32],
    laplacian: &[f32],
    w: usize,
    h: usize,
) -> FocusMeasures {
    let n = data.len() as f64;
    let mean = data.iter().map(|v| f64::from(*v)).sum::<f64>() / n;
    if mean <= 0.0 || !mean.is_finite() {
        return FocusMeasures {
            brenner: 0.0,
            tenengrad: 0.0,
            laplacian: 0.0,
        };
    }
    let sum = |terms: &[f32]| terms.iter().map(|v| f64::from(*v)).sum::<f64>();
    let (mut s, mut s2) = (0.0, 0.0);
    for y in 1..h - 1 {
        for v in &laplacian[y * w + 1..y * w + w - 1] {
            let v = f64::from(*v);
            s += v;
            s2 += v * v;
        }
    }
    let m = ((w - 2) * (h - 2)) as f64;
    let variance = (s2 / m - (s / m).powi(2)).max(0.0);
    FocusMeasures {
        brenner: sum(brenner) / (n * mean * mean),
        tenengrad: sum(tenengrad) / (n * mean * mean),
        laplacian: variance / (mean * mean),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Shape;

    /// Vertical stripes `period` pixels wide, blurred into a sine of
    /// `contrast` about 0.5.
    fn stripes(contrast: f32, period: f32) -> Image {
        let (w, h) = (64, 48);
        let data = (0..w * h)
            .map(|i| {
                let x = (i % w) as f32;
                0.5 + contrast * (std::f32::consts::TAU * x / period).sin()
            })
            .collect();
        Image::from_vec(Shape::new(w as u32, h as u32, 1), data).unwrap()
    }

    #[test]
    fn sharper_images_measure_higher_and_the_light_s_level_doesn_t_matter() {
        let (sharp, soft) = (
            measures(&stripes(0.4, 8.0)).unwrap(),
            measures(&stripes(0.1, 8.0)).unwrap(),
        );
        for measure in [
            FocusMeasure::Brenner,
            FocusMeasure::Tenengrad,
            FocusMeasure::Laplacian,
        ] {
            assert!(
                sharp.get(measure) > 10.0 * soft.get(measure),
                "{measure:?}: {sharp:?} {soft:?}"
            );
        }
        // Twice the light, the same measures.
        let image = stripes(0.4, 8.0);
        let doubled = Image::from_vec(
            image.shape(),
            image.data().iter().map(|v| 2.0 * v).collect(),
        )
        .unwrap();
        let (a, b) = (measures(&image).unwrap(), measures(&doubled).unwrap());
        for measure in [
            FocusMeasure::Brenner,
            FocusMeasure::Tenengrad,
            FocusMeasure::Laplacian,
        ] {
            assert!(
                (a.get(measure) / b.get(measure) - 1.0).abs() < 1e-6,
                "{measure:?}"
            );
        }
    }

    #[test]
    fn a_flat_image_measures_nothing_and_shapes_are_checked() {
        let flat = Image::from_vec(Shape::new(8, 8, 1), vec![0.3; 64]).unwrap();
        let m = measures(&flat).unwrap();
        assert_eq!((m.brenner, m.tenengrad, m.laplacian), (0.0, 0.0, 0.0));
        let dark = Image::zeros(Shape::new(8, 8, 1));
        assert_eq!(measures(&dark).unwrap().brenner, 0.0);
        assert!(measures(&Image::zeros(Shape::new(8, 8, 3))).is_err());
        assert!(measures(&Image::zeros(Shape::new(2, 8, 1))).is_err());
    }

    #[test]
    fn the_measures_are_their_definitions() {
        // A single bright pixel in the middle of a 5 × 5 field of 1s.
        let mut data = vec![1.0_f32; 25];
        data[12] = 2.0;
        let image = Image::from_vec(Shape::new(5, 5, 1), data).unwrap();
        let m = measures(&image).unwrap();
        let mean = 26.0 / 25.0;
        // Brenner: (2 − 1)² from (0, 2), (2, 0) and from the pixel to (4, 2)
        // and (2, 4): four terms of 1.
        assert!(
            (m.brenner - 4.0 / (25.0 * mean * mean)).abs() < 1e-9,
            "{m:?}"
        );
        // A ramp along x, I = x + 1: Sobel's x gradient is 8 at each of the
        // 4 × 3 pixels inside, its y gradient 0; the Laplacian 0.
        let data = (0..30).map(|i| (i % 6) as f32 + 1.0).collect();
        let ramp = measures(&Image::from_vec(Shape::new(6, 5, 1), data).unwrap()).unwrap();
        let level = 30.0 * 3.5 * 3.5;
        assert!(
            (ramp.tenengrad - 64.0 * 12.0 / level).abs() < 1e-9,
            "{ramp:?}"
        );
        assert!((ramp.brenner - 4.0 * 20.0 / level).abs() < 1e-9, "{ramp:?}");
        assert!(ramp.laplacian.abs() < 1e-12, "{ramp:?}");
        // Tenengrad: Sobel's gradient squared is 2 at the middle's four
        // diagonal neighbours and 4 at its four others, 0 elsewhere inside.
        assert!(
            (m.tenengrad - 24.0 / (25.0 * mean * mean)).abs() < 1e-9,
            "{m:?}"
        );
        // Laplacian: −4 at the middle, 1 at its four neighbours, 0 at the
        // other four inside: mean 0, variance (16 + 4) / 9.
        assert!(
            (m.laplacian - (20.0 / 9.0) / (mean * mean)).abs() < 1e-9,
            "{m:?}"
        );
    }
}
