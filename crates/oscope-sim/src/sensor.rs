//! The sensor's electronics: how the light on a pixel becomes its counts,
//! with the MIChrome 20's output (P02b).
//!
//! A pixel collects electrons in proportion to its light and the exposure.
//! It adds dark current, then shot noise (Poisson) and read noise
//! (Gaussian). A 12-bit ADC digitises it, with a black level, after the
//! gain; at no gain, the full well spans the ADC above black, so the ADC's
//! top is where the pixel saturates. As the MIChrome 20's SDK delivers its
//! raw mosaic, the 12 bits go out aligned high in 16-bit samples, so the
//! black level of 125 counts reads as 2,000.

use std::f64::consts::PI;

use crate::noise::mix;

/// How a pixel turns light into counts.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SensorModel {
    /// Electrons a pixel collects in a millisecond, under unhindered light
    /// at full intensity.
    pub electrons_per_ms: f64,
    /// The electrons a pixel holds before it saturates.
    pub full_well: f64,
    /// The read noise's standard deviation, in electrons.
    pub read_noise: f64,
    /// The dark current, in electrons a second.
    pub dark_current: f64,
    /// The black level, in 12-bit counts.
    pub black: u16,
    /// Whether to add noise. Off, the counts are the signal's mean.
    pub noise: bool,
}

impl Default for SensorModel {
    /// The IMX183's class: a 15,000-electron full well, 2.5 electrons of
    /// read noise, 0.5 electrons a second of dark current; 800 electrons a
    /// millisecond under full light, so a 15 ms exposure fills two thirds of
    /// the well through the slide's glass; a black level of 125 counts.
    fn default() -> Self {
        Self {
            electrons_per_ms: 800.0,
            full_well: 15_000.0,
            read_noise: 2.5,
            dark_current: 0.5,
            black: 125,
            noise: true,
        }
    }
}

/// The ADC's largest count.
pub const ADC_MAX: u16 = 4095;

impl SensorModel {
    /// Counts for each electron at `gain`, the MIChrome 20's 0–240. At 0,
    /// the full well spans the ADC above the black level. Each 20 units of
    /// gain multiply by 10^0.1, so 240 is 24 dB: a model, since Tucsen
    /// doesn't say what its gain's units are.
    pub fn counts_per_electron(&self, gain: f64) -> f64 {
        let span = f64::from(ADC_MAX - self.black);
        span / self.full_well * 10f64.powf(gain / 200.0)
    }

    /// A pixel's counts, from the mean of the electrons its light gives,
    /// `signal`, over `exposure_s` seconds, summed over `pixels` binned
    /// pixels, at `gain`, with noise from `random`.
    pub fn count(
        &self,
        signal: f64,
        exposure_s: f64,
        pixels: u32,
        gain: f64,
        random: &mut Random,
    ) -> u16 {
        let n = f64::from(pixels.max(1));
        let mean = signal.max(0.0) + self.dark_current * exposure_s * n;
        let electrons = if self.noise {
            random.poisson(mean) + random.normal() * self.read_noise * n.sqrt()
        } else {
            mean
        };
        // Read noise can take a dark pixel below black: the black level
        // leaves room for it, and only the ADC's floor clips it.
        let counts = f64::from(self.black) + electrons * self.counts_per_electron(gain);
        counts.round().clamp(0.0, f64::from(ADC_MAX)) as u16
    }
}

/// A small, fast generator for the sensor's noise: SplitMix64's sequence,
/// seeded from a hash of the seed, the frame and the row, so that each row
/// draws the same noise whatever thread renders it.
#[derive(Clone, Debug)]
pub struct Random {
    state: u64,
}

impl Random {
    /// A generator from `seed`.
    pub fn new(seed: u64) -> Self {
        Self { state: mix(seed) }
    }

    fn next(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9e37_79b9_7f4a_7c15);
        mix(self.state)
    }

    /// A number from 0 up to 1.
    pub fn uniform(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1_u64 << 53) as f64
    }

    /// A standard normal number, by the Box–Muller transform.
    pub fn normal(&mut self) -> f64 {
        let u = 1.0 - self.uniform();
        let v = self.uniform();
        (-2.0 * u.ln()).sqrt() * (2.0 * PI * v).cos()
    }

    /// A Poisson number of mean `mean`: by counting uniform products under
    /// e^−mean (Knuth) for small means, and a rounded normal past 30, where
    /// the two agree closely.
    pub fn poisson(&mut self, mean: f64) -> f64 {
        if mean <= 0.0 {
            return 0.0;
        }
        if mean > 30.0 {
            return (mean + mean.sqrt() * self.normal()).round().max(0.0);
        }
        let limit = (-mean).exp();
        let (mut k, mut product) = (0.0, self.uniform());
        while product > limit {
            k += 1.0;
            product *= self.uniform();
        }
        k
    }
}
