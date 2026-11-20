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

use std::sync::LazyLock;

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
        self.readout(exposure_s, pixels, gain).count(signal, random)
    }

    /// How pixels exposed for `exposure_s` seconds, `pixels` of them
    /// binned, are read at `gain`: what each count takes, worked out once
    /// for all of a frame's pixels.
    pub fn readout(&self, exposure_s: f64, pixels: u32, gain: f64) -> Readout {
        let n = f64::from(pixels.max(1));
        Readout {
            dark: self.dark_current * exposure_s * n,
            read: self.read_noise * n.sqrt(),
            black: f64::from(self.black),
            per_electron: self.counts_per_electron(gain),
            noise: self.noise,
        }
    }
}

/// A sensor's readout at one exposure, binning and gain
/// ([`SensorModel::readout`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Readout {
    /// The dark current's electrons.
    dark: f64,
    /// The read noise's standard deviation, in electrons.
    read: f64,
    /// The black level, in counts.
    black: f64,
    /// Counts for each electron.
    per_electron: f64,
    /// Whether to add noise.
    noise: bool,
}

impl Readout {
    /// A pixel's counts, from the mean of the electrons its light gives,
    /// `signal`, with noise from `random`.
    ///
    /// Past a mean of 30 electrons, where the shot noise is drawn as a
    /// normal number, it and the read noise are one normal number with both
    /// their variances: the sum of two independent normal numbers. Below,
    /// the shot noise is Knuth's Poisson number, and the read noise is added.
    pub fn count(&self, signal: f64, random: &mut Random) -> u16 {
        let mean = signal.max(0.0) + self.dark;
        let electrons = match self.noise {
            true if mean > POISSON_AS_NORMAL => {
                mean + (mean + self.read * self.read).sqrt() * random.normal()
            }
            true => random.poisson(mean) + random.normal() * self.read,
            false => mean,
        };
        // Read noise can take a dark pixel below black: the black level
        // leaves room for it, and only the ADC's floor clips it.
        let counts = self.black + electrons * self.per_electron;
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

    /// A standard normal number, by the ziggurat method (Marsaglia and
    /// Tsang, 2000), as Doornik (2005) corrected it: the layer's number and
    /// the place in it from separate bits of one draw.
    ///
    /// The density's curve is covered by 128 layers of equal area, the
    /// lowest of them reaching into the tail past 3.44. A draw picks a layer
    /// and a place across it; inside the narrower layer above, as nearly all
    /// are, the place is the number. Otherwise it is kept only under the
    /// curve, and the lowest layer's draws past 3.44 come from the tail
    /// (Marsaglia, 1964). Its numbers have the normal distribution exactly,
    /// a table look-up and a comparison at a time, where the Box–Muller
    /// transform takes a logarithm, a square root and a cosine for each.
    pub fn normal(&mut self) -> f64 {
        let zig = &*ZIGGURAT;
        loop {
            let bits = self.next();
            let layer = (bits & 0x7f) as usize;
            let across = 2.0 * ((bits >> 11) as f64 / (1_u64 << 53) as f64) - 1.0;
            if across.abs() < zig.inner[layer] {
                return across * zig.x[layer];
            }
            if layer == 0 {
                return self.tail(across < 0.0);
            }
            let x = across * zig.x[layer];
            let (outer, inner) = (zig.x[layer], zig.x[layer + 1]);
            let top = (-0.5 * (outer * outer - x * x)).exp();
            let bottom = (-0.5 * (inner * inner - x * x)).exp();
            if bottom + self.uniform() * (top - bottom) < 1.0 {
                return x;
            }
        }
    }

    /// A number from the normal distribution's tail past `ZIGGURAT_R`, or
    /// before its negative: Marsaglia's (1964) method.
    fn tail(&mut self, negative: bool) -> f64 {
        loop {
            let x = (1.0 - self.uniform()).ln() / ZIGGURAT_R;
            let y = (1.0 - self.uniform()).ln();
            if -2.0 * y >= x * x {
                return match negative {
                    true => x - ZIGGURAT_R,
                    false => ZIGGURAT_R - x,
                };
            }
        }
    }

    /// A Poisson number of mean `mean`: by counting uniform products under
    /// e^−mean (Knuth) for small means, and a rounded normal past 30, where
    /// the two agree closely.
    pub fn poisson(&mut self, mean: f64) -> f64 {
        if mean <= 0.0 {
            return 0.0;
        }
        if mean > POISSON_AS_NORMAL {
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

/// The mean past which a Poisson number is drawn as a rounded normal one.
const POISSON_AS_NORMAL: f64 = 30.0;

/// Where the ziggurat's lowest layer meets the tail, for 128 layers.
const ZIGGURAT_R: f64 = 3.442_619_855_899;
/// The area of each of its layers, under e^−x²/2.
const ZIGGURAT_AREA: f64 = 9.912_563_035_262_17e-3;

/// The ziggurat's 128 layers: each one's right edge, `x`, and the share of
/// it under the layer above, `inner`.
struct Ziggurat {
    x: [f64; 129],
    inner: [f64; 128],
}

/// The layers, worked out once: the lowest is as wide as its area over the
/// curve's height at `ZIGGURAT_R`, the next ends at `ZIGGURAT_R`, and each
/// above ends where the curve leaves the layer below its area.
static ZIGGURAT: LazyLock<Ziggurat> = LazyLock::new(|| {
    let f = |x: f64| (-0.5 * x * x).exp();
    let mut x = [0.0; 129];
    x[0] = ZIGGURAT_AREA / f(ZIGGURAT_R);
    x[1] = ZIGGURAT_R;
    for i in 2..128 {
        x[i] = (-2.0 * (ZIGGURAT_AREA / x[i - 1] + f(x[i - 1])).ln()).sqrt();
    }
    let inner = std::array::from_fn(|i| x[i + 1] / x[i]);
    Ziggurat { x, inner }
});
