//! How a frame stores its pixels.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// The colour of one filter in a Bayer mosaic.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BayerColour {
    /// Red.
    Red,
    /// Green.
    Green,
    /// Blue.
    Blue,
}

/// The colours of a mosaic's first 2 × 2 cell, row by row from its top-left
/// pixel.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "UPPERCASE")]
pub enum BayerPattern {
    /// Red, green; green, blue. The MIChrome 20's.
    Rggb,
    /// Green, red; blue, green.
    Grbg,
    /// Green, blue; red, green.
    Gbrg,
    /// Blue, green; green, red.
    Bggr,
}

impl BayerPattern {
    const ALL: [BayerPattern; 4] = [Self::Rggb, Self::Grbg, Self::Gbrg, Self::Bggr];

    /// The colour of pixel `(x, y)` in a mosaic that starts with this pattern.
    pub fn colour_at(self, x: u32, y: u32) -> BayerColour {
        use BayerColour::{Blue, Green, Red};
        let cell = match self {
            Self::Rggb => [Red, Green, Green, Blue],
            Self::Grbg => [Green, Red, Blue, Green],
            Self::Gbrg => [Green, Blue, Red, Green],
            Self::Bggr => [Blue, Green, Green, Red],
        };
        cell[(y % 2 * 2 + x % 2) as usize]
    }

    /// The pattern of a mosaic that starts `dx` columns and `dy` rows into
    /// this one, as an ROI does.
    pub fn shifted(self, dx: u32, dy: u32) -> BayerPattern {
        let same = |p: &BayerPattern| {
            (0..2).all(|y| (0..2).all(|x| p.colour_at(x, y) == self.colour_at(x + dx, y + dy)))
        };
        Self::ALL
            .into_iter()
            .find(same)
            .expect("a shifted Bayer pattern is a Bayer pattern")
    }
}

/// How a frame stores its pixels: interleaved samples, row by row, or planes
/// one after another. Samples of more than one byte are little-endian.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PixelFormat {
    /// A Bayer mosaic in 16-bit samples, of which `significant_bits` carry
    /// data, aligned high.
    Bayer16 {
        /// The mosaic's first cell.
        pattern: BayerPattern,
        /// How many of the 16 bits carry data.
        significant_bits: u8,
    },
    /// A Bayer mosaic in 8-bit samples.
    Bayer8 {
        /// The mosaic's first cell.
        pattern: BayerPattern,
    },
    /// Grey, 8 bits.
    Mono8,
    /// Grey, 16 bits.
    Mono16,
    /// Red, green and blue, 8 bits each.
    Rgb8,
    /// Red, green and blue, 16 bits each.
    Rgb16,
    /// Red, green, blue and alpha, 8 bits each.
    Rgba8,
    /// Planes of 32-bit floats, one after another.
    PlanarF32 {
        /// How many planes.
        planes: u8,
    },
}

impl PixelFormat {
    /// Samples per pixel, within a plane.
    pub fn channels(self) -> usize {
        match self {
            Self::Rgb8 | Self::Rgb16 => 3,
            Self::Rgba8 => 4,
            _ => 1,
        }
    }

    /// Bytes per sample.
    pub fn sample_bytes(self) -> usize {
        match self {
            Self::Bayer16 { .. } | Self::Mono16 | Self::Rgb16 => 2,
            Self::PlanarF32 { .. } => 4,
            _ => 1,
        }
    }

    /// Bytes per pixel, within a plane.
    pub fn pixel_bytes(self) -> usize {
        self.channels() * self.sample_bytes()
    }

    /// How many planes: more than one only for a planar format.
    pub fn planes(self) -> usize {
        match self {
            Self::PlanarF32 { planes } => usize::from(planes),
            _ => 1,
        }
    }

    /// The bytes a tightly packed frame of this size needs, saturating
    /// rather than overflowing.
    pub fn frame_bytes(self, width: u32, height: u32) -> usize {
        (width as usize)
            .saturating_mul(height as usize)
            .saturating_mul(self.pixel_bytes())
            .saturating_mul(self.planes())
    }

    /// A mosaic's pattern, or `None` for any other format.
    pub fn bayer(self) -> Option<BayerPattern> {
        match self {
            Self::Bayer16 { pattern, .. } | Self::Bayer8 { pattern } => Some(pattern),
            _ => None,
        }
    }

    /// The same format, starting `dx` columns and `dy` rows further on: a
    /// mosaic's pattern shifts, other formats don't change.
    pub fn shifted(self, dx: u32, dy: u32) -> PixelFormat {
        match self {
            Self::Bayer16 {
                pattern,
                significant_bits,
            } => Self::Bayer16 {
                pattern: pattern.shifted(dx, dy),
                significant_bits,
            },
            Self::Bayer8 { pattern } => Self::Bayer8 {
                pattern: pattern.shifted(dx, dy),
            },
            other => other,
        }
    }
}
