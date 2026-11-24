//! The calibration in the preview (W8.2): the active set's dark frame and
//! flat field applied to the raw mosaic before it is demosaiced, and its
//! colour calibration as the colour correction, each when the look asks for
//! it. The sets are the calibration's (P33, ADR-0023); finding a frame's is
//! the server's, through [`Calibrating`].

use std::fmt;
use std::sync::Arc;

use oscope_compute::Image;
use oscope_core::FrameMeta;
use oscope_imaging::color::ColourCorrection;
use rayon::prelude::*;

/// What the look asks the calibration for.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Wanted {
    /// The dark frame taken off, in place of the black level.
    pub dark: bool,
    /// The flat field divided out.
    pub flat: bool,
    /// The colour calibration's white balance and matrix.
    pub colour: bool,
}

impl Wanted {
    /// Whether it asks for anything.
    pub fn any(self) -> bool {
        self.dark || self.flat || self.colour
    }
}

/// A dark frame's or a flat field's image: the sensor's pixels from `(x,
/// y)`, in the frame's own pixels, as the frame it was made from had them,
/// in linear units.
#[derive(Clone, Debug)]
pub struct Calibrated {
    /// The image.
    pub image: Image,
    /// Its left, on the sensor.
    pub x: u32,
    /// Its top.
    pub y: u32,
}

impl Calibrated {
    /// Whether it covers the `width` × `height` pixels from `(x, y)`, with
    /// `channels` each.
    fn covers(&self, (x, y): (u32, u32), (width, height, channels): (u32, u32, usize)) -> bool {
        let inside = |from: u32, n: u32, own: u32, size: u32| from >= own && from + n <= own + size;
        self.image.channels() == channels
            && inside(x, width, self.x, self.image.width())
            && inside(y, height, self.y, self.image.height())
    }
}

/// What the calibration gives a frame, as far as the look asks.
#[derive(Clone, Debug, Default)]
pub struct Calibration {
    /// The active dark frame for its mode, gain and exposure.
    pub dark: Option<Arc<Calibrated>>,
    /// The active flat field for its objective, light and binning.
    pub flat: Option<Arc<Calibrated>>,
    /// The active colour calibration for its light, as the correction.
    pub colour: Option<ColourCorrection>,
}

/// Finds a frame's calibration: what the look asks for, as far as the
/// active sets have it.
#[derive(Clone)]
pub struct Calibrating(pub Arc<dyn Fn(&FrameMeta, Wanted) -> Calibration + Send + Sync>);

impl fmt::Debug for Calibrating {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Calibrating")
    }
}

/// `raw`, the sensor's pixels from `at` in linear units, a mosaic or not,
/// corrected as the calibration measured its sets (P33): its dark frame
/// taken off, or else `black`, and then divided by its flat field. An
/// image that doesn't cover all of `raw` isn't used. Whether either was:
/// otherwise `raw` is as it was, its black level still on it.
pub fn correct(
    raw: &mut Image,
    at: (u32, u32),
    black: f32,
    dark: Option<&Calibrated>,
    flat: Option<&Calibrated>,
) -> bool {
    let size = (raw.width(), raw.height(), raw.channels());
    let dark = dark.filter(|d| d.covers(at, size));
    let flat = flat.filter(|f| f.covers(at, size));
    if dark.is_none() && flat.is_none() {
        return false;
    }
    let (width, channels) = (size.0 as usize, size.2);
    let row = width * channels;
    // Where pixel `(u, v)`'s channel `c` is in a calibration image.
    let index = |image: &Calibrated, u: usize, v: usize, c: usize| {
        let (x, y) = ((at.0 - image.x) as usize + u, (at.1 - image.y) as usize + v);
        (y * image.image.width() as usize + x) * channels + c
    };
    raw.data_mut()
        .par_chunks_mut(row)
        .enumerate()
        .for_each(|(v, samples)| {
            for (k, sample) in samples.iter_mut().enumerate() {
                let (u, c) = (k / channels, k % channels);
                let base = dark.map_or(black, |d| d.image.data()[index(d, u, v, c)]);
                let mut value = *sample - base;
                if let Some(f) = flat {
                    let gain = f.image.data()[index(f, u, v, c)];
                    // A site the flat field saw dark is left alone, not blown up.
                    if gain > 1e-3 {
                        value /= gain;
                    }
                }
                *sample = value;
            }
        });
    true
}
