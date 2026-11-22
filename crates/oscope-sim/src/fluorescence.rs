//! Simulated fluorescence, which the instrument itself doesn't have (plan
//! §5.10, D8): dyes that an excitation line excites, emitting in colours
//! the colour sensor sees, in widefield or with a confocal's sectioning.
//!
//! **Widefield** collects a dye's light from every depth: a plane out of
//! focus is blurred, but its light all arrives. **A confocal's pinhole**
//! passes less of it the further the plane is from focus. Its optical
//! section, the full width at half maximum of its response to a thin
//! sheet, is the wave-optical width 0.88 λ / (n − √(n² − NA²)) in
//! quadrature with the pinhole's geometric width √2 n PH / NA, for a
//! pinhole PH across at the sample (Wilhelm et al., *Confocal laser
//! scanning microscopy: principles*, Carl Zeiss; Cole, Jinadasa and Brown,
//! *Nature Protocols*, 2011).

use oscope_core::Um;
use serde::{Deserialize, Serialize};

use crate::optics::Optics;

/// The confocal's pinhole, in Airy units: one, the usual setting.
pub const PINHOLE_AIRY_UNITS: f64 = 1.0;

/// The width of a dye's excitation band: the standard deviation of the
/// Gaussian that stands in for its excitation spectrum.
const EXCITATION_WIDTH: f64 = 0.025;

/// An excitation line: the band a laser or an LED excites dyes in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Excitation {
    /// 405 nm.
    Violet,
    /// 488 nm.
    Blue,
    /// 561 nm.
    Green,
    /// 640 nm.
    Red,
}

impl Excitation {
    /// Every line.
    pub const ALL: [Self; 4] = [Self::Violet, Self::Blue, Self::Green, Self::Red];

    /// Its wavelength: one of the common laser lines.
    pub fn wavelength(self) -> Um {
        Um(match self {
            Self::Violet => 0.405,
            Self::Blue => 0.488,
            Self::Green => 0.561,
            Self::Red => 0.640,
        })
    }
}

/// A fluorescent dye, by the peaks of its excitation and emission spectra.
/// In a configuration: `dapi`, `green`, `orange` or `far-red`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Dye {
    /// A DNA stain like DAPI: excited at 358 nm, emitting at 461 nm.
    Dapi,
    /// A green dye like Alexa Fluor 488: 495 nm and 519 nm.
    Green,
    /// An orange dye like Alexa Fluor 568: 578 nm and 603 nm.
    Orange,
    /// A far-red dye like Alexa Fluor 647: 650 nm and 665 nm.
    FarRed,
}

impl Dye {
    /// Every dye.
    pub const ALL: [Self; 4] = [Self::Dapi, Self::Green, Self::Orange, Self::FarRed];

    /// Its excitation and emission peaks, from the dyes' published spectra.
    pub fn peaks(self) -> (Um, Um) {
        let (excitation, emission) = match self {
            Self::Dapi => (0.358, 0.461),
            Self::Green => (0.495, 0.519),
            Self::Orange => (0.578, 0.603),
            Self::FarRed => (0.650, 0.665),
        };
        (Um(excitation), Um(emission))
    }

    /// How well `line` excites it, from 0 to 1: a Gaussian of 25 nm around
    /// its excitation peak, at the line. A model: real spectra fall more
    /// slowly on their blue side.
    pub fn excited_by(self, line: Excitation) -> f64 {
        let d = (line.wavelength().0 - self.peaks().0.0) / EXCITATION_WIDTH;
        (-0.5 * d * d).exp()
    }

    /// The red, green and blue the colour sensor records of its light.
    pub fn colour(self) -> [f64; 3] {
        sensor_response(self.peaks().1)
    }
}

/// What the colour sensor records, in red, green and blue from 0 to 1, of
/// a dye's light that peaks at `wavelength`. A model of a Bayer sensor
/// behind an infrared-cut filter: Gaussian channels peaking at 610, 535 and
/// 460 nm, widened by a dye's emission band, and red cut off from 650 nm.
pub fn sensor_response(wavelength: Um) -> [f64; 3] {
    let channel = |peak: f64, width: f64| {
        let d = (wavelength.0 - peak) / width;
        (-0.5 * d * d).exp()
    };
    let cut = 1.0 / (1.0 + ((wavelength.0 - 0.650) / 0.010).exp());
    [
        channel(0.610, 0.050) * cut,
        channel(0.535, 0.045),
        channel(0.460, 0.040),
    ]
}

/// The confocal's optical section through `optics` at `emission`: the
/// full width at half maximum of its response to a thin fluorescent sheet,
/// with a pinhole of [`PINHOLE_AIRY_UNITS`].
pub fn optical_section(optics: &Optics, emission: Um) -> Um {
    let n = optics.medium;
    let na = optics.na.min(0.999 * n);
    let lambda = emission.0;
    let wave = 0.88 * lambda / (n - (n * n - na * na).sqrt());
    let pinhole = PINHOLE_AIRY_UNITS * 1.22 * lambda / na;
    let geometric = 2f64.sqrt() * n * pinhole / na;
    Um(wave.hypot(geometric))
}

/// The share of a thin sheet's light, `dz` from focus, that a confocal
/// with an optical section of `section` passes: 1 / (1 + (2 dz /
/// section)²), half at half the section from focus. Its tails fall as
/// 1 / dz², as a pinhole's share of a sheet's light does once the sheet's
/// blur is much wider than the pinhole. Widefield passes all of it.
pub fn sectioning(dz: Um, section: Um) -> f64 {
    let t = 2.0 * dz.0 / section.0;
    1.0 / (1.0 + t * t)
}
