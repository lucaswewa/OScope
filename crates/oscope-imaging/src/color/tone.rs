//! The colour steps around demosaicing: the black level, white balance, the
//! colour matrix, and the display transform to 8 bits.

use oscope_compute::Image;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use super::ColorError;

/// Subtracts the sensor's black level, on the same 0–1 scale as the image,
/// and stretches the rest back to 0–1: `(v − black) / (1 − black)`, and 0
/// below black. It works on a mosaic, before demosaicing, as on RGB.
pub fn subtract_black(image: &mut Image, black: f32) -> Result<(), ColorError> {
    check_black(black)?;
    let scale = 1.0 / (1.0 - black);
    image
        .data_mut()
        .par_iter_mut()
        .for_each(|v| *v = ((*v - black) * scale).max(0.0));
    Ok(())
}

/// Refuses a black level outside 0 to below 1.
pub(crate) fn check_black(black: f32) -> Result<(), ColorError> {
    match (0.0..1.0).contains(&black) {
        true => Ok(()),
        false => Err(ColorError::Setting(format!(
            "a black level of {black}: it must be from 0 to below 1"
        ))),
    }
}

/// White-balance gains and a colour matrix, for RGB: each pixel becomes
/// `matrix × (gains ⊙ rgb)`. The default changes nothing.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ColourCorrection {
    /// Red, green and blue's gains.
    pub gains: [f32; 3],
    /// The 3 × 3 matrix, by rows: from the camera's RGB to the output's.
    pub matrix: [[f32; 3]; 3],
}

impl Default for ColourCorrection {
    fn default() -> Self {
        Self {
            gains: [1.0; 3],
            matrix: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        }
    }
}

/// Applies white balance and the colour matrix to an RGB image. The results
/// aren't clamped: the display transform does that.
pub fn correct(rgb: &mut Image, correction: &ColourCorrection) -> Result<(), ColorError> {
    if rgb.channels() != 3 {
        return Err(ColorError::Channels(rgb.channels()));
    }
    let [gr, gg, gb] = correction.gains;
    let m = correction.matrix;
    rgb.data_mut().par_chunks_mut(3).for_each(|pixel| {
        let (r, g, b) = (pixel[0] * gr, pixel[1] * gg, pixel[2] * gb);
        for (out, row) in pixel.iter_mut().zip(m) {
            *out = row[0] * r + row[1] * g + row[2] * b;
        }
    });
    Ok(())
}

/// The tone curve a display expects.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Curve {
    /// None.
    Linear,
    /// `t^(1 / gamma)`, such as 2.2.
    Gamma {
        /// The display's gamma.
        gamma: f32,
    },
    /// sRGB's (IEC 61966-2-1): linear near black, then a 1/2.4 power.
    Srgb,
}

impl Curve {
    /// The curve at `t`, from 0 to 1.
    pub fn apply(self, t: f32) -> f32 {
        match self {
            Self::Linear => t,
            Self::Gamma { gamma } => t.powf(1.0 / gamma),
            Self::Srgb if t <= 0.003_130_8 => 12.92 * t,
            Self::Srgb => 1.055 * t.powf(1.0 / 2.4) - 0.055,
        }
    }
}

/// How to show an image on an 8-bit display.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Display {
    /// The level shown as black.
    pub black: f32,
    /// The level shown as white.
    pub white: f32,
    /// The tone curve between them.
    pub curve: Curve,
}

impl Default for Display {
    /// The whole range, through sRGB's curve.
    fn default() -> Self {
        Self {
            black: 0.0,
            white: 1.0,
            curve: Curve::Srgb,
        }
    }
}

/// The display transform's table: for each of the 65,536 16-bit levels,
/// its 8-bit value.
pub(crate) fn display_table(display: &Display) -> Result<Vec<u8>, ColorError> {
    let Display {
        black,
        white,
        curve,
    } = *display;
    if white <= black || white.is_nan() || black.is_nan() {
        return Err(ColorError::Setting(format!(
            "white ({white}) must be above black ({black})"
        )));
    }
    Ok((0..=u16::MAX)
        .map(|code| {
            let t = ((f32::from(code) / 65_535.0 - black) / (white - black)).clamp(0.0, 1.0);
            (curve.apply(t) * 255.0).round() as u8
        })
        .collect())
}

/// An image's samples for an 8-bit display, interleaved as they are.
///
/// Each sample is first taken to 16 bits, from 0–1, and then looked up in a
/// table of 65,536 entries. Each entry stretches `black`–`white` to 0–1,
/// clamps it, applies the curve, and rounds to 0–255.
pub fn to_display(image: &Image, display: &Display) -> Result<Vec<u8>, ColorError> {
    let table = display_table(display)?;
    Ok(image
        .data()
        .par_iter()
        .map(|v| table[(v.clamp(0.0, 1.0) * 65_535.0).round() as usize])
        .collect())
}
