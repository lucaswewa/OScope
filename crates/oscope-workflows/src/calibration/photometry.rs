//! The photometric calibrations (P33): dark frames, flat fields and the
//! white balance they give, and the colour matrix from a colour target.
//!
//! Frames are taken whole, as the camera gives them, in linear units from
//! 0 to 1 (`oscope_compute::cpu::to_f32`). A colour mosaic is kept as it
//! is: each correction works on its sites, colour by colour.

use std::time::Duration;

use oscope_compute::{Image, Shape, cpu};
use oscope_core::{Frame, FrameMeta, LightState, PixelFormat};
use oscope_imaging::color::{ColourTarget, apply3, delta_e76, fit_colour_matrix};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::fit::{apply, inverse};
use super::procedures::{CalibrationError, FRAME_WAIT, Report, Rig, check, device, failed};
use super::set::{
    Colour, Dark, Flat, Instrument, Region, Values, dark_key, flat_key, geometry_key,
};

/// The most a blank field may vary about its own smooth shape: 3 % of its
/// level.
const BLANK: f64 = 0.03;
/// The colour matrix's fit is refused above a mean ΔE*ab of 5.
const FITTED: f64 = 5.0;

/// How photometric calibrations take their frames.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct Frames {
    /// How many frames to average.
    pub frames: u32,
    /// How long after a change the first frame may start, in ms.
    pub settle_ms: f64,
}

impl Default for Frames {
    /// Sixteen frames, 50 ms on.
    fn default() -> Self {
        Self {
            frames: 16,
            settle_ms: 50.0,
        }
    }
}

impl Frames {
    /// Checks it: at least a frame, and a settling time finite and not
    /// negative.
    pub fn check(&self) -> Result<(), String> {
        let settle = self.settle_ms;
        match self.frames >= 1 && settle.is_finite() && settle >= 0.0 {
            true => Ok(()),
            false => Err(format!("{} frames, {settle} ms on", self.frames)),
        }
    }
}

/// A mean of frames, with the last one's metadata.
struct Mean {
    image: Image,
    meta: FrameMeta,
}

/// The colour of a mosaic's site `(x, y)`: 0 red, 1 green, 2 blue; or the
/// channel itself in an image of colour channels.
fn site(format: PixelFormat, x: u32, y: u32) -> Option<usize> {
    match format {
        PixelFormat::Bayer8 { pattern } | PixelFormat::Bayer16 { pattern, .. } => {
            Some(match pattern.colour_at(x % 2, y % 2) {
                oscope_core::BayerColour::Red => 0,
                oscope_core::BayerColour::Green => 1,
                oscope_core::BayerColour::Blue => 2,
            })
        }
        _ => None,
    }
}

/// The black level of `meta`'s frames, in linear units.
fn black(meta: &FrameMeta) -> f32 {
    let full = if meta.format.sample_bytes() == 2 {
        65_535.0
    } else {
        255.0
    };
    (meta.black_level / full) as f32
}

impl Rig {
    /// The instrument now, at `binning`, with the objective fitted if one
    /// is.
    async fn state(&self, binning: u32) -> Result<Instrument, CalibrationError> {
        let objective = self.objectives.fitted().await.map_err(device)?;
        let catalogue = self.objectives.catalogue();
        let camera = self.camera.capabilities();
        Ok(Instrument {
            camera: camera.model.clone(),
            sensor_pixel_um: camera.pixel_size.0,
            binning,
            stage: self.stage.capabilities().model.clone(),
            objective,
            tube_lens_um: catalogue.tube_lens.0,
        })
    }

    /// The lights on, by channel, joined with `+`; or a refusal.
    async fn lit(&self) -> Result<String, CalibrationError> {
        let on: Vec<String> = self
            .lights
            .state()
            .await
            .map_err(device)?
            .into_iter()
            .filter(|l| l.on && l.intensity > 0.0)
            .map(|l| l.channel)
            .collect();
        match on.is_empty() {
            true => Err(failed("no light is on: turn the light on first")),
            false => Ok(on.join("+")),
        }
    }

    /// The mean of `how.frames` frames, the first exposed from `settle_ms`
    /// after now on the instrument's clock.
    async fn mean(&self, how: &Frames, report: &dyn Report) -> Result<Mean, CalibrationError> {
        let now = self.stage.position().await.map_err(device)?.time;
        let mut after = now + Duration::from_secs_f64(how.settle_ms / 1e3);
        let mut sum: Option<Vec<f64>> = None;
        let mut last: Option<(Shape, FrameMeta)> = None;
        for _ in 0..how.frames {
            check(report)?;
            let next = self.bus.next_frame_after(after);
            let frame: Frame = tokio::time::timeout(FRAME_WAIT, next)
                .await
                .map_err(|_| failed("no frame came within 30 s: is the camera streaming?"))?
                .ok_or_else(|| failed("the camera's frames stopped"))?;
            let meta = frame.meta().clone();
            after = meta.exposure_start + Duration::from_micros(1);
            let image = cpu::to_f32(&frame.view()).map_err(|e| failed(format!("a frame: {e}")))?;
            let sum = sum.get_or_insert_with(|| vec![0.0; image.data().len()]);
            if sum.len() != image.data().len() {
                return Err(failed("the frames' size changed while they were taken"));
            }
            for (s, v) in sum.iter_mut().zip(image.data()) {
                *s += f64::from(*v);
            }
            last = Some((image.shape(), meta));
        }
        let ((shape, meta), sum) = (last.expect("a frame"), sum.expect("a frame"));
        let n = f64::from(how.frames);
        let data = sum.into_iter().map(|s| (s / n) as f32).collect();
        let image = Image::from_vec(shape, data).map_err(|e| failed(e.to_string()))?;
        Ok(Mean { image, meta })
    }

    /// `mean` less the active dark frame for its frames, if one matches
    /// them, or else their black level: and whether it was a dark frame.
    fn without_dark(&self, mean: &Mean) -> Result<(Image, bool), CalibrationError> {
        let meta = &mean.meta;
        let key = dark_key(&meta.mode, meta.gain, meta.exposure.0);
        let dark = self
            .store
            .active(&key)
            .and_then(|set| self.store.image(set.id).ok());
        let mut image = mean.image.clone();
        match dark.filter(|d| d.shape() == image.shape()) {
            Some(dark) => {
                for (v, d) in image.data_mut().iter_mut().zip(dark.data()) {
                    *v -= d;
                }
                Ok((image, true))
            }
            None => {
                let black = black(meta);
                image.data_mut().iter_mut().for_each(|v| *v -= black);
                Ok((image, false))
            }
        }
    }
}

/// The mean of the values at `image`'s sites of colour `c`, within the
/// region from `(x, y)` of `w` × `h`; the channel `c` of an image of colour
/// channels; `None` if none is there.
fn colour_mean(
    image: &Image,
    format: PixelFormat,
    c: usize,
    (x, y, w, h): (u32, u32, u32, u32),
) -> Option<f64> {
    let (width, channels) = (image.width(), image.channels());
    let (mut sum, mut n) = (0.0, 0.0);
    for v in y..y + h {
        for u in x..x + w {
            let i = (v * width + u) as usize * channels;
            let value = match (site(format, u, v), channels) {
                (Some(colour), _) if colour == c => image.data()[i],
                (None, 1) => image.data()[i],
                (None, k) if c < k => image.data()[i + c],
                _ => continue,
            };
            sum += f64::from(value);
            n += 1.0;
        }
    }
    (n > 0.0).then(|| sum / n)
}

/// A dark frame's options.
pub type DarkOptions = Frames;

/// Takes a dark frame: the lights turned off, frames averaged from their
/// settling on, and the lights restored as they were. For the camera's
/// mode, gain and octave of exposure.
pub async fn measure_dark(
    rig: &Rig,
    options: &DarkOptions,
    report: &dyn Report,
) -> Result<(Dark, Instrument, Image), CalibrationError> {
    options.check().map_err(failed)?;
    let lights = rig.lights.state().await.map_err(device)?;
    report.progress(0.0, "turning the lights off");
    for light in lights.iter().filter(|l| l.on) {
        let off = LightState {
            on: false,
            ..light.clone()
        };
        rig.lights.set(off).await.map_err(device)?;
    }
    report.progress(0.1, "averaging dark frames");
    let taken = rig.mean(options, report).await;
    report.progress(0.9, "turning the lights back on");
    for light in lights {
        rig.lights.set(light).await.map_err(device)?;
    }
    let Mean { image, meta } = taken?;
    let n = image.data().len() as f64;
    let mean = image.data().iter().map(|v| f64::from(*v)).sum::<f64>() / n;
    let variance = image
        .data()
        .iter()
        .map(|v| (f64::from(*v) - mean).powi(2))
        .sum::<f64>()
        / n;
    let dark = Dark {
        mode: meta.mode.clone(),
        gain: meta.gain,
        exposure_ms: super::set::exposure_octave(meta.exposure.0),
        taken_ms: meta.exposure.0,
        region: region(&meta),
        frames: options.frames,
        mean,
        spread: variance.sqrt(),
    };
    Ok((dark, rig.state(meta.binning.max(1)).await?, image))
}

fn region(meta: &FrameMeta) -> Region {
    Region {
        x: meta.roi.x,
        y: meta.roi.y,
        width: meta.width,
        height: meta.height,
    }
}

/// The values of `image`'s sites of colour `c` as an image of their own,
/// a site a pixel: half a mosaic's width and height, its two greens
/// averaged; or the channel itself.
fn plane(image: &Image, format: PixelFormat, c: usize) -> Image {
    let (w, h) = (image.width(), image.height());
    if site(format, 0, 0).is_none() {
        let k = image.channels();
        let data = image.data().chunks(k).map(|p| p[c.min(k - 1)]).collect();
        return Image::from_vec(Shape::new(w, h, 1), data).expect("a plane");
    }
    let (pw, ph) = (w / 2, h / 2);
    let mut data = Vec::with_capacity((pw * ph) as usize);
    for y in 0..ph {
        for x in 0..pw {
            let cell = [(0, 0), (1, 0), (0, 1), (1, 1)].map(|(dx, dy)| (2 * x + dx, 2 * y + dy));
            let of_c: Vec<f32> = cell
                .iter()
                .filter(|(u, v)| site(format, *u, *v) == Some(c))
                .map(|(u, v)| image.data()[(v * w + u) as usize])
                .collect();
            data.push(of_c.iter().sum::<f32>() / of_c.len() as f32);
        }
    }
    Image::from_vec(Shape::new(pw, ph, 1), data).expect("a plane")
}

/// `plane` blurred by a box of `radius`, one axis then the other, the edges
/// repeated.
fn smooth(plane: &Image, radius: usize) -> Vec<f64> {
    let (w, h) = (plane.width() as usize, plane.height() as usize);
    let pass = |data: &[f64], w: usize, h: usize| -> Vec<f64> {
        let mut out = vec![0.0; data.len()];
        for y in 0..h {
            let row = &data[y * w..(y + 1) * w];
            for x in 0..w {
                let (lo, hi) = (x.saturating_sub(radius), (x + radius).min(w - 1));
                out[y * w + x] = row[lo..=hi].iter().sum::<f64>() / (hi - lo + 1) as f64;
            }
        }
        out
    };
    let data: Vec<f64> = plane.data().iter().map(|v| f64::from(*v)).collect();
    let across = pass(&data, w, h);
    // Down: transpose, across, transpose back.
    let mut turned = vec![0.0; w * h];
    for y in 0..h {
        for x in 0..w {
            turned[x * h + y] = across[y * w + x];
        }
    }
    let down = pass(&turned, h, w);
    let mut out = vec![0.0; w * h];
    for x in 0..w {
        for y in 0..h {
            out[y * w + x] = down[x * h + y];
        }
    }
    out
}

/// A flat field's options.
pub type FlatOptions = Frames;

/// Takes a flat field at a blank part of the slide, through the fitted
/// objective in the lights that are on: frames averaged, the dark taken
/// away, refused if the field isn't blank, and each colour's sites scaled
/// to a mean of 1. The blank field's colour gives the white balance.
pub async fn measure_flat(
    rig: &Rig,
    options: &FlatOptions,
    report: &dyn Report,
) -> Result<(Flat, Instrument, Image), CalibrationError> {
    options.check().map_err(failed)?;
    let light = rig.lit().await?;
    report.progress(0.0, "averaging the blank field's frames");
    let mean = rig.mean(options, report).await?;
    let instrument = rig.state(mean.meta.binning.max(1)).await?;
    let objective = instrument
        .objective
        .clone()
        .ok_or_else(|| failed("no objective is fitted: fit one first"))?;
    report.progress(0.9, "the flat field");
    let (mut image, dark) = rig.without_dark(&mean)?;
    let format = mean.meta.format;
    // Blank: green varies about its own smooth shape by little.
    let green = plane(&image, format, 1);
    let radius = (green.width().max(green.height()) / 32).max(4) as usize;
    let smoothed = smooth(&green, radius);
    let level = smoothed.iter().sum::<f64>() / smoothed.len() as f64;
    if level <= 0.0 {
        return Err(failed(
            "the field is dark: is the light on, and the exposure long enough?",
        ));
    }
    let residual = green
        .data()
        .iter()
        .zip(&smoothed)
        .map(|(v, s)| (f64::from(*v) - s).powi(2))
        .sum::<f64>()
        / smoothed.len() as f64;
    let rough = residual.sqrt() / level;
    if rough > BLANK {
        return Err(failed(format!(
            "the field isn't blank: it varies by {:.1} % about its shape, more than {:.0} %. \
             Move to an empty part of the slide",
            100.0 * rough,
            100.0 * BLANK
        )));
    }
    let spread =
        (smoothed.iter().map(|s| (s - level).powi(2)).sum::<f64>() / smoothed.len() as f64).sqrt();
    // Each colour's sites to a mean of 1; the means give the white balance.
    let (w, h) = (image.width(), image.height());
    let whole = (0, 0, w, h);
    let means = [0, 1, 2].map(|c| colour_mean(&image, format, c, whole).unwrap_or(level));
    let channels = image.channels();
    for (i, v) in image.data_mut().iter_mut().enumerate() {
        let (pixel, k) = (i / channels, i % channels);
        let (x, y) = ((pixel as u32) % w, (pixel as u32) / w);
        let c = site(format, x, y).unwrap_or(k.min(2));
        *v /= means[c] as f32;
    }
    if means.iter().any(|m| *m <= 0.0) {
        return Err(failed("a colour of the field is dark: is the light on?"));
    }
    let flat = Flat {
        objective: objective.name,
        light,
        mode: mean.meta.mode.clone(),
        binning: mean.meta.binning.max(1),
        region: region(&mean.meta),
        frames: options.frames,
        dark,
        gains: [means[1] / means[0], 1.0, means[1] / means[2]],
        uneven: spread / level,
    };
    Ok((flat, instrument, image))
}

/// A colour calibration's options.
pub type ColourOptions = Frames;

/// Calibrates colour from OScope's colour target, centred in the field:
/// each patch found through the active geometry, its middle half's colours
/// measured, the dark taken away and the active flat field divided out;
/// white-balanced on the lightest grey, and the colour matrix fitted
/// (Hong, Luo and Rhodes, 2001). Refused if it fits poorly.
pub async fn measure_colour(
    rig: &Rig,
    options: &ColourOptions,
    report: &dyn Report,
) -> Result<(Colour, Instrument), CalibrationError> {
    options.check().map_err(failed)?;
    let light = rig.lit().await?;
    report.progress(0.0, "averaging the target's frames");
    let mean = rig.mean(options, report).await?;
    let binning = mean.meta.binning.max(1);
    let instrument = rig.state(binning).await?;
    let objective = instrument
        .objective
        .clone()
        .ok_or_else(|| failed("no objective is fitted: fit one first"))?;
    let geometry = match rig
        .store
        .active(&geometry_key(&objective.name, binning))
        .map(|s| s.values)
    {
        Some(Values::Geometry(g)) => g,
        _ => {
            return Err(failed(format!(
                "no geometry is active for the {} at binning {binning}: calibrate it first",
                objective.name
            )));
        }
    };
    report.progress(0.8, "measuring the patches");
    let (mut image, _) = rig.without_dark(&mean)?;
    let format = mean.meta.format;
    let flat_set = rig
        .store
        .active(&flat_key(&objective.name, &light, binning));
    let flat = flat_set.and_then(|set| rig.store.image(set.id).ok());
    if let Some(flat) = flat.filter(|f| f.shape() == image.shape()) {
        for (v, f) in image.data_mut().iter_mut().zip(flat.data()) {
            *v /= f.max(1e-6);
        }
    }
    let to_image =
        inverse(geometry.matrix).ok_or_else(|| failed("the geometry can't be inverted"))?;
    let target = ColourTarget::oscope();
    let (w, h) = (image.width(), image.height());
    let side = (target.side_um / 2.0 / geometry.pixel_um).max(2.0) as u32;
    let mut measured = Vec::new();
    for patch in target.patches() {
        // The patch's centre, from the image's middle, in pixels: M⁻¹ s.
        let d = apply(to_image, (patch.x_um, patch.y_um));
        let (cx, cy) = (f64::from(w) / 2.0 + d.0, f64::from(h) / 2.0 + d.1);
        let (x, y) = (cx - f64::from(side) / 2.0, cy - f64::from(side) / 2.0);
        let fits = x >= 0.0
            && y >= 0.0
            && x + f64::from(side) <= f64::from(w)
            && y + f64::from(side) <= f64::from(h);
        if !fits {
            return Err(failed(
                "the colour target isn't all in the frame: centre it, through the 5× or the 10×",
            ));
        }
        let at = (x as u32, y as u32, side, side);
        let rgb = [0, 1, 2].map(|c| colour_mean(&image, format, c, at).unwrap_or(0.0));
        measured.push(rgb);
    }
    // White balance on the lightest grey, which the target holds first.
    let patches = target.patches();
    let reference = patches[0].rgb;
    if measured[0].iter().any(|v| *v <= 0.0) {
        return Err(failed(
            "the target's lightest grey is dark: is the light on?",
        ));
    }
    let gains = [0, 1, 2].map(|c| reference[c] / measured[0][c]);
    let balanced: Vec<[f64; 3]> = measured
        .iter()
        .map(|rgb| [0, 1, 2].map(|c| rgb[c] * gains[c]))
        .collect();
    let truth: Vec<[f64; 3]> = patches.iter().map(|p| p.rgb).collect();
    let matrix = fit_colour_matrix(&balanced, &truth)
        .ok_or_else(|| failed("the target's colours gave no matrix"))?;
    let mean_of = |values: Vec<f64>| values.iter().sum::<f64>() / values.len() as f64;
    let before: Vec<f64> = balanced
        .iter()
        .zip(&truth)
        .map(|(b, t)| delta_e76(*b, *t))
        .collect();
    let after: Vec<f64> = balanced
        .iter()
        .zip(&truth)
        .map(|(b, t)| delta_e76(apply3(matrix, *b), *t))
        .collect();
    let largest = after.iter().copied().fold(0.0, f64::max);
    let (before, after) = (mean_of(before), mean_of(after));
    if after > FITTED {
        return Err(failed(format!(
            "the colours fit to a mean ΔE*ab of {after:.1}, more than {FITTED}: \
             is the target centred, in focus and lit?"
        )));
    }
    let colour = Colour {
        light,
        gains: [gains[0] / gains[1], 1.0, gains[2] / gains[1]],
        matrix,
        patches: patches.len() as u32,
        delta_e_before: before,
        delta_e_mean: after,
        delta_e_max: largest,
    };
    Ok((colour, instrument))
}
