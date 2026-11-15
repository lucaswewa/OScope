//! The procedures (P32's spec): frames as grey, positions reached one way,
//! and the measurements of the geometry, the stage and the objective.

use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use oscope_acquire::FrameBus;
use oscope_compute::{Image, Shape, cpu};
use oscope_core::{Frame, PixelFormat, Um, Xyz};
use oscope_hal::{Camera, Illumination, Objective, ObjectiveMount, Stage};
use oscope_imaging::registration::{self, RegisterOptions};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::fit::{Decomposition, apply, decompose, fit_matrix};
use super::set::{Geometry, Instrument, PerAxis, SoftLimits, StageValues, geometry_key};
use super::store::CalibrationStore;

/// How long each frame is waited for: a camera gives one within a frame's
/// interval, but after a long move the simulator renders its 20 MP frames
/// afresh, which takes it seconds.
pub(super) const FRAME_WAIT: Duration = Duration::from_secs(30);
/// The least confidence a registration may have.
const CONFIDENCE: f64 = 0.5;
/// The widest and tallest part of a frame used, in pixels.
const PART: u32 = 2048;
/// The largest residual of the geometry's fit, in pixels.
const RESIDUAL_PX: f64 = 0.5;
/// How far a measured pixel size may be from the nominal: 25 %.
const FROM_NOMINAL: f64 = 0.25;
/// How far the aspect may be from 1, and the skew from 0.
const ASPECT: f64 = 0.02;
const SKEW: f64 = 0.02;
/// The objective check passes within 20 % of the pixel size expected.
const PLAUSIBLE: f64 = 1.2;

/// Why a calibration didn't finish.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum CalibrationError {
    /// It was asked to stop.
    #[error("cancelled")]
    Cancelled,
    /// It failed, for this reason.
    #[error("{0}")]
    Failed(String),
}

pub(super) fn failed(why: impl Into<String>) -> CalibrationError {
    CalibrationError::Failed(why.into())
}

pub(super) fn device(error: oscope_hal::Error) -> CalibrationError {
    failed(error.to_string())
}

/// What calibration drives: the stage, the camera's frames, what the
/// instrument is, and where the sets go.
#[derive(Clone)]
pub struct Rig {
    /// The stage.
    pub stage: Arc<dyn Stage>,
    /// The camera, for what it is: its frames come on the bus.
    pub camera: Arc<dyn Camera>,
    /// The objective mount.
    pub objectives: Arc<dyn ObjectiveMount>,
    /// The camera's frames.
    pub bus: Arc<FrameBus>,
    /// The calibration sets.
    pub store: Arc<CalibrationStore>,
    /// The lights, which a dark frame turns off and back on (P33).
    pub lights: Arc<dyn Illumination>,
}

/// How a calibration moves: each position reached from `approach_um`
/// before it, and its frame exposed from `settle_ms` after the stage
/// arrives.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct Motion {
    /// How far before each position the stage comes from, in µm.
    pub approach_um: f64,
    /// How long after the stage arrives its frame may start, in ms.
    pub settle_ms: f64,
}

impl Motion {
    /// Checks it: an approach and a settling time, finite and not negative.
    pub fn check(&self) -> Result<(), String> {
        let (a, s) = (self.approach_um, self.settle_ms);
        match a.is_finite() && a >= 0.0 && s.is_finite() && s >= 0.0 {
            true => Ok(()),
            false => Err(format!(
                "an approach of {a} µm and a settling time of {s} ms"
            )),
        }
    }
}

impl Default for Motion {
    /// 50 µm, and 50 ms.
    fn default() -> Self {
        Self {
            approach_um: 50.0,
            settle_ms: 50.0,
        }
    }
}

/// Where a procedure reports how far it has got, and learns that it should
/// stop.
pub trait Report: Sync {
    /// How far, from 0 to 1, and what it is doing.
    fn progress(&self, fraction: f64, stage: &str);
    /// Whether it should stop.
    fn cancelled(&self) -> bool;
}

/// A report to no one.
#[derive(Debug, Default)]
pub struct Quiet;

impl Report for Quiet {
    fn progress(&self, _: f64, _: &str) {}
    fn cancelled(&self) -> bool {
        false
    }
}

/// A frame as grey, for registration.
struct Grey {
    image: Arc<Image>,
    /// The frame's pixels a grey pixel spans: 2 for a mosaic's cells.
    span: f64,
    binning: u32,
}

/// `frame` as grey: its middle, at most [`PART`] pixels a side; a colour
/// mosaic's 2 × 2 cells summed, free of the mosaic; red, green and blue
/// averaged.
fn grey(frame: &Frame) -> Result<Grey, CalibrationError> {
    let meta = frame.meta();
    let image = cpu::to_f32(&frame.view()).map_err(|e| failed(format!("a frame: {e}")))?;
    let mosaic = matches!(
        meta.format,
        PixelFormat::Bayer8 { .. } | PixelFormat::Bayer16 { .. }
    );
    let even = |v: u32| if mosaic { v & !1 } else { v };
    let (w, h, channels) = (image.width(), image.height(), image.channels());
    let (pw, ph) = (even(w.min(PART)), even(h.min(PART)));
    let (x0, y0) = (even((w - pw) / 2), even((h - ph) / 2));
    let at = |x: u32, y: u32, c: usize| image.data()[(y * w + x) as usize * channels + c];
    let (gw, gh, span) = match mosaic {
        true => (pw / 2, ph / 2, 2.0),
        false => (pw, ph, 1.0),
    };
    let mut data = Vec::with_capacity((gw * gh) as usize);
    for gy in 0..gh {
        for gx in 0..gw {
            data.push(if mosaic {
                let (x, y) = (x0 + 2 * gx, y0 + 2 * gy);
                at(x, y, 0) + at(x + 1, y, 0) + at(x, y + 1, 0) + at(x + 1, y + 1, 0)
            } else if channels >= 3 {
                let (x, y) = (x0 + gx, y0 + gy);
                (at(x, y, 0) + at(x, y, 1) + at(x, y, 2)) / 3.0
            } else {
                at(x0 + gx, y0 + gy, 0)
            });
        }
    }
    let image = Image::from_vec(Shape::new(gw, gh, 1), data).map_err(|e| failed(e.to_string()))?;
    Ok(Grey {
        image: Arc::new(image),
        span,
        binning: meta.binning.max(1),
    })
}

/// P31's weight for `objective`: its in-focus blur, 0.21 λ / NA in green
/// light, in grey pixels, and at least 2 (P31's notes).
fn sigma(objective: &Objective, pixel_um: f64, span: f64) -> f64 {
    let blur = 0.21 * 0.55 / objective.na;
    (blur / (pixel_um * span)).max(2.0)
}

/// How far `b`'s content lies from `a`'s, in the frame's pixels, by P31's
/// registration with every shift considered. A confidence under 0.5 fails,
/// saying `which`.
async fn shift(
    a: &Grey,
    b: &Grey,
    sigma: f64,
    which: &str,
) -> Result<(f64, f64), CalibrationError> {
    let (first, second) = (Arc::clone(&a.image), Arc::clone(&b.image));
    let options = RegisterOptions {
        sigma,
        ..RegisterOptions::default()
    };
    let found =
        tokio::task::spawn_blocking(move || registration::register(&first, &second, &options))
            .await
            .map_err(|e| failed(format!("registering {which}: {e}")))?
            .map_err(|e| failed(format!("registering {which}: {e}")))?;
    if found.confidence < CONFIDENCE {
        return Err(failed(format!(
            "the frames {which} didn't register (a confidence of {:.2}, under {CONFIDENCE}): \
             is the sample in focus, and textured there?",
            found.confidence
        )));
    }
    Ok((found.dx * a.span, found.dy * a.span))
}

fn offset(at: Xyz, x: f64, y: f64) -> Xyz {
    Xyz::new(Um(at.x.0 + x), Um(at.y.0 + y), at.z)
}

impl Rig {
    async fn here(&self) -> Result<Xyz, CalibrationError> {
        Ok(self.stage.position().await.map_err(device)?.position)
    }

    /// Moves to `target`, coming from `before` µm before it along x and y.
    async fn go(
        &self,
        target: Xyz,
        before: (f64, f64),
    ) -> Result<oscope_hal::PositionSample, CalibrationError> {
        let from = offset(target, -before.0, -before.1);
        self.stage.move_to(from, None).await.map_err(device)?;
        self.stage.move_to(target, None).await.map_err(device)
    }

    /// Moves to `target` as [`Rig::go`] does, and takes the first frame
    /// exposed once it has settled, as grey.
    async fn visit(
        &self,
        target: Xyz,
        before: (f64, f64),
        motion: Motion,
    ) -> Result<Grey, CalibrationError> {
        let arrived = self.go(target, before).await?;
        let settle = Duration::from_secs_f64(motion.settle_ms.max(0.0) / 1e3);
        let next = self.bus.next_frame_after(arrived.time + settle);
        let frame = tokio::time::timeout(FRAME_WAIT, next)
            .await
            .map_err(|_| failed("no frame came within 30 s: is the camera streaming?"))?
            .ok_or_else(|| failed("the camera's frames stopped"))?;
        grey(&frame)
    }

    /// The instrument now, at `binning`; the objective fitted; and its
    /// nominal pixel size.
    async fn instrument(
        &self,
        binning: u32,
    ) -> Result<(Instrument, Objective, f64), CalibrationError> {
        let fitted = self.objectives.fitted().await.map_err(device)?;
        let objective = fitted.ok_or_else(|| failed("no objective is fitted: fit one first"))?;
        let catalogue = self.objectives.catalogue();
        let camera = self.camera.capabilities();
        let sensor = camera.pixel_size.0;
        let nominal = sensor * f64::from(binning) / catalogue.magnification(&objective);
        let instrument = Instrument {
            camera: camera.model.clone(),
            sensor_pixel_um: sensor,
            binning,
            stage: self.stage.capabilities().model.clone(),
            objective: Some(objective.clone()),
            tube_lens_um: catalogue.tube_lens.0,
        };
        Ok((instrument, objective, nominal))
    }
}

pub(super) fn check(report: &dyn Report) -> Result<(), CalibrationError> {
    match report.cancelled() {
        true => Err(CalibrationError::Cancelled),
        false => Ok(()),
    }
}

/// What a geometry calibration takes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct GeometryOptions {
    /// The moves' step, in µm: if not given, a quarter of the part used's
    /// smaller side at the nominal pixel size.
    pub step_um: Option<f64>,
    /// How it moves.
    #[serde(flatten)]
    pub motion: Motion,
}

impl GeometryOptions {
    /// Checks it: a step, if given, finite and more than 0, and the motion.
    pub fn check(&self) -> Result<(), String> {
        if let Some(step) = self.step_um
            && !(step.is_finite() && step > 0.0)
        {
            return Err(format!("a step of {step} µm"));
        }
        self.motion.check()
    }
}

/// Measures the fitted objective's geometry at the camera's binning: a
/// 3 × 3 grid of positions around where the stage is, each reached from the
/// same side, each frame registered against the middle's, and the matrix
/// fitted (P32's spec). Refused if the fit is poor, or the pixel size far
/// from the nominal.
pub async fn measure_geometry(
    rig: &Rig,
    options: &GeometryOptions,
    report: &dyn Report,
) -> Result<(Geometry, Instrument), CalibrationError> {
    options.check().map_err(failed)?;
    let motion = options.motion;
    let before = (motion.approach_um, motion.approach_um);
    let start = rig.here().await?;
    report.progress(0.0, "the middle's frame");
    let middle = rig.visit(start, before, motion).await?;
    let (instrument, objective, nominal) = rig.instrument(middle.binning).await?;
    let side = f64::from(middle.image.width().min(middle.image.height())) * middle.span;
    let step = options
        .step_um
        .unwrap_or_else(|| (side / 4.0 * nominal).round().max(1.0));
    let sigma = sigma(&objective, nominal, middle.span);
    // A snake through the grid, so that each move is short.
    let grid = [
        (-1, -1),
        (0, -1),
        (1, -1),
        (1, 0),
        (-1, 0),
        (-1, 1),
        (0, 1),
        (1, 1),
    ];
    let (mut moves, mut shifts) = (Vec::new(), Vec::new());
    for (n, (i, j)) in grid.into_iter().enumerate() {
        check(report)?;
        let which = format!("{i}, {j} steps from the middle");
        report.progress((n + 1) as f64 / 10.0, &format!("the frame {which}"));
        let s = (f64::from(i) * step, f64::from(j) * step);
        let frame = rig.visit(offset(start, s.0, s.1), before, motion).await?;
        shifts.push(shift(&middle, &frame, sigma, &format!("at {which}")).await?);
        moves.push(s);
    }
    report.progress(0.9, "back to the middle, and the fit");
    rig.go(start, before).await?;
    let (matrix, residual) =
        fit_matrix(&moves, &shifts).ok_or_else(|| failed("the moves gave no matrix"))?;
    let parts = decompose(matrix).ok_or_else(|| failed("the fitted matrix can't be inverted"))?;
    let name = &objective.name;
    verdict(&parts, residual, nominal, name).map_err(failed)?;
    let geometry = Geometry {
        objective: name.clone(),
        binning: middle.binning,
        nominal_pixel_um: nominal,
        matrix,
        pixel_um: parts.pixel,
        rotation_deg: parts.rotation_deg,
        mirrored: parts.mirrored,
        aspect: parts.aspect,
        skew: parts.skew,
        residual_px: residual,
        step_um: step,
        moves: moves.len() as u32,
    };
    Ok((geometry, instrument))
}

/// Whether a fitted geometry may be kept (P32's spec): refused if the fit's
/// residual is above 0.5 px, the pixel size more than 25 % from the
/// objective's nominal, the aspect more than 2 % from 1, or the skew more
/// than 0.02.
fn verdict(parts: &Decomposition, residual: f64, nominal: f64, name: &str) -> Result<(), String> {
    if residual > RESIDUAL_PX {
        return Err(format!(
            "the fit's residual, {residual:.2} px, is above {RESIDUAL_PX} px: did the sample or the stage move?"
        ));
    }
    if (parts.pixel / nominal - 1.0).abs() > FROM_NOMINAL {
        return Err(format!(
            "the pixel size measured, {:.4} µm, is {:.2} times the {name}'s nominal {nominal:.4} µm: \
             is the {name} in the light path? The objective check names the one that is",
            parts.pixel,
            parts.pixel / nominal
        ));
    }
    if (parts.aspect - 1.0).abs() > ASPECT || parts.skew.abs() > SKEW {
        return Err(format!(
            "an aspect of {:.4} and a skew of {:.4}: a stage axis or the frames are at fault",
            parts.aspect, parts.skew
        ));
    }
    Ok(())
}

/// What a stage calibration takes.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct StageOptions {
    /// Whether to home first, if the stage can.
    pub home: bool,
    /// How many times each axis's backlash is measured: its median counts.
    pub cycles: u32,
    /// How it moves.
    #[serde(flatten)]
    pub motion: Motion,
}

impl StageOptions {
    /// Checks it: at least one cycle, and the motion.
    pub fn check(&self) -> Result<(), String> {
        match self.cycles {
            0 => Err("no cycles of backlash to measure".to_owned()),
            _ => self.motion.check(),
        }
    }
}

impl Default for StageOptions {
    /// Homing, and three cycles.
    fn default() -> Self {
        Self {
            home: true,
            cycles: 3,
            motion: Motion::default(),
        }
    }
}

fn median(mut values: Vec<f64>) -> f64 {
    values.sort_by(f64::total_cmp);
    let n = values.len();
    match n % 2 {
        1 => values[n / 2],
        _ => (values[n / 2 - 1] + values[n / 2]) / 2.0,
    }
}

/// Measures the stage: x's and y's backlash where it is, from frames
/// reached from below and from above (ISO 230-2's reversal value); then
/// homes it if asked and it can, records the soft limits, and takes it back.
/// It needs the active geometry of the fitted objective at the camera's
/// binning.
pub async fn measure_stage(
    rig: &Rig,
    options: &StageOptions,
    report: &dyn Report,
) -> Result<(StageValues, Instrument), CalibrationError> {
    options.check().map_err(failed)?;
    let motion = options.motion;
    let a = motion.approach_um;
    let start = rig.here().await?;
    report.progress(0.0, "the first frame");
    let first = rig.visit(start, (a, a), motion).await?;
    let (instrument, objective, _) = rig.instrument(first.binning).await?;
    let key = geometry_key(&objective.name, first.binning);
    let geometry = match rig.store.active(&key).map(|set| set.values) {
        Some(super::set::Values::Geometry(geometry)) => geometry,
        _ => {
            return Err(failed(format!(
                "no geometry is active for the {} at binning {}: calibrate it first",
                objective.name, first.binning
            )));
        }
    };
    let sigma = sigma(&objective, geometry.pixel_um, first.span);
    let mut backlash = [0.0; 2];
    let total = 2 * options.cycles;
    for (axis, name) in [(0, "x"), (1, "y")] {
        let mut found = Vec::new();
        for cycle in 0..options.cycles {
            check(report)?;
            let done = axis as u32 * options.cycles + cycle;
            let fraction = 0.8 * f64::from(done) / f64::from(total);
            report.progress(fraction, &format!("{name}'s backlash, cycle {}", cycle + 1));
            let (below, above) = match axis {
                0 => ((a, 0.0), (-a, 0.0)),
                _ => ((0.0, a), (0.0, -a)),
            };
            let low = rig.visit(start, below, motion).await?;
            let high = rig.visit(start, above, motion).await?;
            let which = format!("of {name}'s backlash, cycle {}", cycle + 1);
            let d = shift(&low, &high, sigma, &which).await?;
            // The stage's true move from the first to the second is −M · d.
            let moved = apply(geometry.matrix, d);
            found.push(-[moved.0, moved.1][axis]);
        }
        backlash[axis] = median(found);
    }
    // Homing last: no frame is needed after its long way there and back.
    let capabilities = rig.stage.capabilities().clone();
    let homed_at = match options.home && capabilities.homing {
        true => {
            check(report)?;
            report.progress(0.8, "homing");
            Some(rig.stage.home().await.map_err(device)?.position)
        }
        false => None,
    };
    report.progress(0.9, "back where it was");
    rig.go(start, (a, a)).await?;
    let soft_limits = SoftLimits {
        x: capabilities.x,
        y: capabilities.y,
        z: capabilities.z,
    };
    let values = StageValues {
        backlash_um: PerAxis {
            x: backlash[0],
            y: backlash[1],
        },
        soft_limits,
        homed_at,
    };
    Ok((values, instrument))
}

/// The objective check's report.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CheckReport {
    /// The objective fitted.
    pub objective: String,
    /// The camera's binning.
    pub binning: u32,
    /// The pixel size expected, in µm: the active geometry's, or else the
    /// nominal.
    pub expected_pixel_um: f64,
    /// Whether the pixel size expected is a calibration's.
    pub calibrated: bool,
    /// The pixel size measured, in µm.
    pub measured_pixel_um: f64,
    /// Measured over expected.
    pub ratio: f64,
    /// Whether it is within 20 % of the expected.
    pub plausible: bool,
    /// The catalogue's objective whose nominal pixel size is nearest the
    /// measured one.
    pub nearest: String,
    /// When it was checked.
    pub checked: DateTime<Utc>,
}

/// Checks the fitted objective: moves along x and along y, a 32nd of the
/// part used at the pixel size expected, register to a pixel size, whatever
/// the camera's turn; within 20 % of the expected it is plausible, and
/// otherwise the catalogue's nearest objective is named (P32's spec).
pub async fn check_objective(
    rig: &Rig,
    motion: &Motion,
    report: &dyn Report,
) -> Result<CheckReport, CalibrationError> {
    motion.check().map_err(failed)?;
    let motion = *motion;
    let before = (motion.approach_um, motion.approach_um);
    let start = rig.here().await?;
    report.progress(0.0, "the first frame");
    let first = rig.visit(start, before, motion).await?;
    let (_, objective, nominal) = rig.instrument(first.binning).await?;
    let key = geometry_key(&objective.name, first.binning);
    let (expected, calibrated) = match rig.store.active(&key).map(|set| set.values) {
        Some(super::set::Values::Geometry(geometry)) => (geometry.pixel_um, true),
        _ => (nominal, false),
    };
    let side = f64::from(first.image.width().min(first.image.height())) * first.span;
    let step = ((side / 32.0 * expected) * 10.0).round().max(1.0) / 10.0;
    let sigma = sigma(&objective, expected, first.span);
    let mut pixels = Vec::new();
    for (n, (x, y)) in [(step, 0.0), (0.0, step)].into_iter().enumerate() {
        check(report)?;
        report.progress(0.3 + 0.3 * n as f64, "a move");
        let moved = rig.visit(offset(start, x, y), before, motion).await?;
        let d = shift(&first, &moved, sigma, &format!("{step} µm apart")).await?;
        pixels.push(step / d.0.hypot(d.1));
    }
    rig.go(start, before).await?;
    let measured = (pixels[0] + pixels[1]) / 2.0;
    let ratio = measured / expected;
    let catalogue = rig.objectives.catalogue();
    let sensor = rig.camera.capabilities().pixel_size.0 * f64::from(first.binning);
    let off = |o: &Objective| {
        (measured / (sensor / catalogue.magnification(o)))
            .ln()
            .abs()
    };
    let nearest = catalogue
        .objectives
        .iter()
        .min_by(|a, b| off(a).total_cmp(&off(b)))
        .map_or_else(String::new, |o| o.name.clone());
    Ok(CheckReport {
        objective: objective.name,
        binning: first.binning,
        expected_pixel_um: expected,
        calibrated,
        measured_pixel_um: measured,
        ratio,
        plausible: (1.0 / PLAUSIBLE..=PLAUSIBLE).contains(&ratio),
        nearest,
        checked: Utc::now(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parts(pixel: f64, aspect: f64, skew: f64) -> Decomposition {
        let (sx, sy) = (pixel * aspect.sqrt(), pixel / aspect.sqrt());
        Decomposition {
            rotation_deg: 30.0,
            mirrored: true,
            sx,
            sy,
            k: skew * pixel,
            pixel,
            aspect,
            skew,
        }
    }

    #[test]
    fn a_geometry_is_refused_outside_the_spec_s_bounds() {
        let kept = |p: Decomposition, residual: f64| verdict(&p, residual, 0.24, "10x");
        assert_eq!(kept(parts(0.24, 1.0, 0.0), 0.0), Ok(()));
        assert_eq!(kept(parts(0.24 * 1.24, 1.019, 0.019), 0.49), Ok(()));
        let refused = |p: Decomposition, residual: f64, says: &str| {
            let why = kept(p, residual).unwrap_err();
            assert!(why.contains(says), "{why:?} doesn't say {says:?}");
        };
        refused(parts(0.24, 1.0, 0.0), 0.51, "residual");
        refused(
            parts(0.24 * 1.26, 1.0, 0.0),
            0.0,
            "is the 10x in the light path",
        );
        refused(
            parts(0.24 * 0.74, 1.0, 0.0),
            0.0,
            "is the 10x in the light path",
        );
        refused(parts(0.24, 1.021, 0.0), 0.0, "an aspect");
        refused(parts(0.24, 1.0, -0.021), 0.0, "a skew");
    }
}
