//! Autofocus (P34's spec): the depth of field and the defaults by light; the
//! stepped sweep, coarse then fine, its peak by the three-point Gaussian
//! estimate; the continuous sweep, its frames put at z by their exposure
//! midpoints, its peak a parabola's vertex; and the refusals.

use std::sync::Arc;
use std::time::{Duration, Instant};

use oscope_compute::focus::{FocusMeasure, FocusMeasures};
use oscope_compute::{Image, Lane};
use oscope_core::{Um, Xyz};
use oscope_hal::{LightKind, UmPerS};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::frames::{frame_after, middle_row, prepare};
use super::{AutofocusError, FocusRig, check, device, failed};
use crate::calibration::{Region, Report};

/// Every sweep's first position is approached from this far below, in µm.
const APPROACH_UM: f64 = 10.0;
/// The coarse sweep reaches this many depths of field either side.
const RANGE: f64 = 6.0;
/// A field whose sharpest frame measures under this many times its median
/// is empty.
const EMPTY: f64 = 1.2;
/// The depth of field's wavelength in transmitted and reflected light,
/// green's, and in fluorescence, a green emission's, in µm.
const GREEN_UM: f64 = 0.55;
const EMISSION_UM: f64 = 0.52;

/// A focus measure.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Measure {
    /// Brenner's gradient: transmitted light's default.
    Brenner,
    /// Tenengrad: reflected light's.
    Tenengrad,
    /// The variance of the Laplacian: fluorescence's.
    Laplacian,
}

impl Measure {
    fn kind(self) -> FocusMeasure {
        match self {
            Measure::Brenner => FocusMeasure::Brenner,
            Measure::Tenengrad => FocusMeasure::Tenengrad,
            Measure::Laplacian => FocusMeasure::Laplacian,
        }
    }
}

/// How a run sweeps z.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Sweep {
    /// Position by position: a coarse sweep, then a fine one.
    #[default]
    Stepped,
    /// One pass at a constant speed, frames matched to z by their times.
    Continuous,
}

/// What an autofocus run takes, each optional.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct FocusOptions {
    /// The measure: the light's by default.
    pub measure: Option<Measure>,
    /// Stepped, or continuous.
    pub sweep: Sweep,
    /// How far to look either side of where the stage is, in µm: 6 depths
    /// of field by default.
    pub range_um: Option<f64>,
    /// The coarse step, in µm: the depth of field by default. The fine step
    /// is a quarter of it.
    pub step_um: Option<f64>,
    /// The frame's region looked at: its middle half each way by default.
    pub region: Option<Region>,
    /// How far below a sweep's first position it is approached from, in µm.
    pub approach_um: f64,
    /// How long after the stage arrives a frame may start, in ms.
    pub settle_ms: f64,
}

impl Default for FocusOptions {
    /// The defaults: the light's measure, a stepped sweep of ±6 depths of
    /// field, approached from 10 µm below, frames 50 ms on.
    fn default() -> Self {
        Self {
            measure: None,
            sweep: Sweep::Stepped,
            range_um: None,
            step_um: None,
            region: None,
            approach_um: APPROACH_UM,
            settle_ms: 50.0,
        }
    }
}

impl FocusOptions {
    /// Checks it: a range and a step that are positive, an approach and a
    /// settling time that aren't negative, a region that isn't empty.
    pub fn check(&self) -> Result<(), String> {
        let positive = |v: Option<f64>| v.is_none_or(|v| v.is_finite() && v > 0.0);
        let at_least_0 = |v: f64| v.is_finite() && v >= 0.0;
        let region = self.region.is_none_or(|r| r.width > 0 && r.height > 0);
        match positive(self.range_um)
            && positive(self.step_um)
            && at_least_0(self.approach_um)
            && at_least_0(self.settle_ms)
            && region
        {
            true => Ok(()),
            false => Err(format!("these options can't be: {self:?}")),
        }
    }
}

/// A point of a focus curve: a frame's z and its measure.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CurvePoint {
    /// The stage's z, in µm.
    pub z_um: f64,
    /// The measure.
    pub value: f64,
}

/// What an autofocus run did.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct FocusReport {
    /// Where it left the stage, in µm: the focus found, or where it started.
    pub z_um: f64,
    /// Whether it found a focus.
    pub found: bool,
    /// Where the stage was, in µm, when it started.
    pub start_um: f64,
    /// The measure used.
    pub measure: Measure,
    /// How it swept.
    pub sweep: Sweep,
    /// The depth of field, in µm.
    pub depth_of_field_um: f64,
    /// The coarse sweep's curve, or a continuous sweep's frames, in z.
    pub coarse: Vec<CurvePoint>,
    /// The fine sweep's curve; none for a continuous sweep.
    pub fine: Vec<CurvePoint>,
    /// How many frames it measured.
    pub frames: u32,
    /// How long it took, in ms of the server's clock.
    pub took_ms: f64,
    /// For a continuous sweep, its speed, in µm/s.
    pub speed_um_s: Option<f64>,
    /// Why it found no focus, if it didn't.
    pub refused: Option<String>,
}

/// The defaults now, from the fitted objective, the camera's pixel and
/// binning, and the light on.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct FocusDefaults {
    /// The light on, by kind, if one is.
    pub light: Option<LightKind>,
    /// Its measure.
    pub measure: Measure,
    /// The depth of field, in µm.
    pub depth_of_field_um: f64,
    /// The sweep's reach either side, in µm.
    pub range_um: f64,
    /// The coarse step, in µm.
    pub coarse_um: f64,
    /// The fine step, in µm.
    pub fine_um: f64,
    /// The approach, in µm.
    pub approach_um: f64,
}

/// The depth of field, in µm: d = λ n / NA² + n e / (M NA) (Inoué and
/// Spring, 1997), for a dry objective (n = 1) of `na` at `magnification` on
/// the instrument, with the camera's pixel `pixel_um` (its binning
/// included), in light of `wavelength_um`.
pub fn depth_of_field(na: f64, magnification: f64, pixel_um: f64, wavelength_um: f64) -> f64 {
    wavelength_um / (na * na) + pixel_um / (magnification * na)
}

/// The defaults now.
pub async fn focus_defaults(rig: &FocusRig) -> Result<FocusDefaults, AutofocusError> {
    let objective = rig
        .objectives
        .fitted()
        .await
        .map_err(device)?
        .ok_or_else(|| failed("no objective is fitted: fit one first"))?;
    let magnification = rig.objectives.catalogue().magnification(&objective);
    let settings = rig.camera.settings().await.map_err(device)?;
    let camera = rig.camera.capabilities();
    let binning = camera.mode(&settings.mode).map_or(1, |m| m.binning.max(1));
    let states = rig.lights.state().await.map_err(device)?;
    let channels = &rig.lights.capabilities().channels;
    let light = states
        .iter()
        .find(|s| s.on && s.intensity > 0.0)
        .and_then(|s| channels.iter().find(|c| c.name == s.channel))
        .map(|c| c.kind);
    let (measure, wavelength) = match light {
        Some(LightKind::Reflected) => (Measure::Tenengrad, GREEN_UM),
        Some(LightKind::Other) => (Measure::Laplacian, EMISSION_UM),
        _ => (Measure::Brenner, GREEN_UM),
    };
    let pixel = camera.pixel_size.0 * f64::from(binning);
    let d = depth_of_field(objective.na, magnification, pixel, wavelength);
    Ok(FocusDefaults {
        light,
        measure,
        depth_of_field_um: d,
        range_um: RANGE * d,
        coarse_um: d,
        fine_um: d / 4.0,
        approach_um: APPROACH_UM,
    })
}

/// A frame's three focus measures, where the stage was.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct FrameMeasures {
    /// Brenner's gradient.
    pub brenner: f64,
    /// Tenengrad.
    pub tenengrad: f64,
    /// The variance of the Laplacian.
    pub laplacian: f64,
    /// The stage's z, in µm.
    pub z_um: f64,
}

/// The next frame's three measures over `region`, without moving.
pub async fn current_measures(
    rig: &FocusRig,
    region: Option<Region>,
) -> Result<FrameMeasures, AutofocusError> {
    let at = rig.stage.position().await.map_err(device)?;
    let frame = frame_after(&rig.bus, at.time).await?;
    let image = prepare(&frame, region)?;
    drop(frame);
    let m = measures(rig, &image)?;
    Ok(FrameMeasures {
        brenner: m.brenner,
        tenengrad: m.tenengrad,
        laplacian: m.laplacian,
        z_um: at.position.z.0,
    })
}

/// How a run looks at its frames.
struct How {
    measure: Measure,
    region: Option<Region>,
    approach: f64,
    settle: Duration,
}

/// `image`'s measures: with the compute layer when there is one, the GPU's
/// if it has one; on the CPU otherwise.
fn measures(rig: &FocusRig, image: &Image) -> Result<FocusMeasures, AutofocusError> {
    let measured = match &rig.compute {
        Some(compute) => compute.focus_measures(Lane::Live, image),
        None => oscope_compute::focus::measures(image),
    };
    measured.map_err(|e| failed(format!("the focus measures: {e}")))
}

fn measure(rig: &FocusRig, image: &Image, measure: Measure) -> Result<f64, AutofocusError> {
    Ok(measures(rig, image)?.get(measure.kind()))
}

/// Moves to `z` moving up, from `approach` below it: where the stage says
/// it arrived.
async fn up_to(
    rig: &FocusRig,
    xy: (f64, f64),
    z: f64,
    approach: f64,
) -> Result<f64, AutofocusError> {
    let to = |z: f64| Xyz::new(Um(xy.0), Um(xy.1), Um(z));
    rig.stage
        .move_to(to(z - approach), None)
        .await
        .map_err(device)?;
    let arrived = rig.stage.move_to(to(z), None).await.map_err(device)?;
    Ok(arrived.position.z.0)
}

/// From `from` to `to` in steps of about `step`, both ends included.
fn positions(from: f64, to: f64, step: f64) -> Vec<f64> {
    let n = ((to - from) / step).round().max(2.0) as usize;
    (0..=n)
        .map(|i| from + (to - from) * i as f64 / n as f64)
        .collect()
}

/// How far each position commanded lay above where the stage said it
/// arrived: a stage that reports its true position lags, approached from
/// below, by half its backlash.
#[derive(Default)]
struct Swept {
    lags: Vec<f64>,
}

impl Swept {
    /// The commanded z that brings the stage, approached from below, to
    /// `z` as it reports it.
    fn command(&self, z: f64) -> f64 {
        match self.lags.len() {
            0 => z,
            n => z + self.lags.iter().sum::<f64>() / n as f64,
        }
    }
}

/// The frame at each of `positions`, each reached moving up, the first from
/// the approach below it: their measures, at the z the stage reported.
async fn stepped(
    rig: &FocusRig,
    xy: (f64, f64),
    positions: &[f64],
    how: &How,
    report: &dyn Report,
    (from, share): (f64, f64),
    swept: &mut Swept,
) -> Result<Vec<CurvePoint>, AutofocusError> {
    let to = |z: f64| Xyz::new(Um(xy.0), Um(xy.1), Um(z));
    rig.stage
        .move_to(to(positions[0] - how.approach), None)
        .await
        .map_err(device)?;
    let mut curve = Vec::with_capacity(positions.len());
    for (i, &z) in positions.iter().enumerate() {
        check(report)?;
        let arrived = rig.stage.move_to(to(z), None).await.map_err(device)?;
        swept.lags.push(z - arrived.position.z.0);
        let frame = frame_after(&rig.bus, arrived.time + how.settle).await?;
        let image = prepare(&frame, how.region)?;
        drop(frame);
        curve.push(CurvePoint {
            z_um: arrived.position.z.0,
            value: measure(rig, &image, how.measure)?,
        });
        let done = (i + 1) as f64 / positions.len() as f64;
        report.progress(from + share * done, "sweeping through focus");
    }
    Ok(curve)
}

/// The curve's sharpest point.
fn best(curve: &[CurvePoint]) -> usize {
    (0..curve.len())
        .max_by(|&a, &b| curve[a].value.total_cmp(&curve[b].value))
        .unwrap_or(0)
}

/// Whether the curve is an empty field's: its best under 1.2 times its
/// median, or nothing measured at all.
fn empty(curve: &[CurvePoint]) -> bool {
    let mut values: Vec<f64> = curve.iter().map(|p| p.value).collect();
    values.sort_by(f64::total_cmp);
    let (best, median) = (values[values.len() - 1], values[values.len() / 2]);
    best <= 0.0 || best < EMPTY * median
}

/// The peak of a curve sampled evenly in z, by the three-point Gaussian
/// estimate (Willert and Gharib, 1991): the best point moved by
/// s (ln F₋ − ln F₊) / (2 (ln F₋ − 2 ln F₀ + ln F₊)). At an end, or where
/// the logarithms don't make a peak, the best point as it is.
fn gaussian_peak(curve: &[CurvePoint]) -> f64 {
    let k = best(curve);
    if k == 0 || k + 1 == curve.len() {
        return curve[k].z_um;
    }
    let (a, b, c) = (curve[k - 1].value, curve[k].value, curve[k + 1].value);
    if a <= 0.0 || b <= 0.0 || c <= 0.0 {
        return curve[k].z_um;
    }
    let (a, b, c) = (a.ln(), b.ln(), c.ln());
    let denominator = 2.0 * (a - 2.0 * b + c);
    if denominator >= 0.0 {
        return curve[k].z_um;
    }
    // The positions' spacing, as the stage reported them.
    let step = (curve[k + 1].z_um - curve[k - 1].z_um) / 2.0;
    curve[k].z_um + step * (a - c) / denominator
}

/// The vertex of the least-squares parabola through the points' z and
/// ln measure; `None` if it doesn't open downwards, or a measure isn't
/// positive.
fn parabola_peak(points: &[CurvePoint]) -> Option<f64> {
    if points.len() < 3 || points.iter().any(|p| p.value <= 0.0) {
        return None;
    }
    let z0 = points[points.len() / 2].z_um;
    // The normal equations of y = a u² + b u + c, u = z − z0.
    let (mut s, mut t) = ([0.0; 5], [0.0; 3]);
    for p in points {
        let (u, y) = (p.z_um - z0, p.value.ln());
        let mut power = 1.0;
        for k in 0..5 {
            s[k] += power;
            if k < 3 {
                t[k] += y * power;
            }
            power *= u;
        }
    }
    let m = [[s[4], s[3], s[2]], [s[3], s[2], s[1]], [s[2], s[1], s[0]]];
    let det = |m: [[f64; 3]; 3]| {
        m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
            - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
            + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0])
    };
    let d = det(m);
    if d == 0.0 || !d.is_finite() {
        return None;
    }
    // Cramer's rule, for a and b.
    let with = |col: usize| {
        let mut n = m;
        for (row, v) in [t[2], t[1], t[0]].into_iter().enumerate() {
            n[row][col] = v;
        }
        det(n) / d
    };
    let (a, b) = (with(0), with(1));
    (a < 0.0).then(|| z0 - b / (2.0 * a))
}

/// Why a sweep's curve gives no focus: an empty field, or its peak at an
/// end.
fn refusal(curve: &[CurvePoint], range: f64) -> Option<String> {
    if empty(curve) {
        return Some(format!(
            "the field is empty: its sharpest frame measures under {EMPTY} times the median. \
             Move to the sample"
        ));
    }
    let k = best(curve);
    if k == 0 || k + 1 == curve.len() {
        let way = if k == 0 { "below" } else { "above" };
        return Some(format!(
            "the focus lies {way} the sweep, more than {range:.1} µm from where it started: \
             focus nearer by hand, or widen the range"
        ));
    }
    None
}

/// The stepped sweep: the focus, as the stage reports z, or `None` with why
/// in the report.
async fn stepped_run(
    rig: &FocusRig,
    (xy, z0, range, coarse): ((f64, f64), f64, f64, f64),
    how: &How,
    report: &dyn Report,
    (out, swept): (&mut FocusReport, &mut Swept),
) -> Result<Option<f64>, AutofocusError> {
    let wide = positions(z0 - range, z0 + range, coarse);
    let curve = stepped(rig, xy, &wide, how, report, (0.0, 0.6), swept).await?;
    out.frames += curve.len() as u32;
    out.coarse.clone_from(&curve);
    if let Some(why) = refusal(&curve, range) {
        out.refused = Some(why);
        return Ok(None);
    }
    // Commanded positions around the coarse peak, as the stage reported it.
    let centre = swept.command(curve[best(&curve)].z_um);
    let fine_positions = positions(centre - coarse, centre + coarse, coarse / 4.0);
    let fine = stepped(rig, xy, &fine_positions, how, report, (0.6, 0.35), swept).await?;
    out.frames += fine.len() as u32;
    out.fine.clone_from(&fine);
    Ok(Some(gaussian_peak(&fine)))
}

/// One pass of the continuous sweep, from `from` up to `to` at `speed`, its
/// start approached from below: each frame at the z the stage's track gives
/// for its exposure midpoint, in z; and the interval at which the frames
/// came, in s, if two did.
async fn pass(
    rig: &FocusRig,
    xy: (f64, f64),
    (from, to, speed): (f64, f64, f64),
    how: &How,
    report: &dyn Report,
    swept: &mut Swept,
) -> Result<(Vec<CurvePoint>, Option<f64>), AutofocusError> {
    let arrived = up_to(rig, xy, from, how.approach).await?;
    swept.lags.push(from - arrived);
    let stage = Arc::clone(&rig.stage);
    let end = Xyz::new(Um(xy.0), Um(xy.1), Um(to));
    let moving = tokio::spawn(async move { stage.move_to(end, Some(UmPerS(speed))).await });
    let mut after = rig.stage.position().await.map_err(device)?.time;
    let mut samples = Vec::new();
    while !moving.is_finished() && !report.cancelled() {
        let frame = frame_after(&rig.bus, after).await?;
        let meta = frame.meta().clone();
        after = meta.exposure_start + Duration::from_micros(1);
        let line = meta
            .line_time
            .and_then(|l| l.to_duration())
            .unwrap_or_default();
        let exposure = meta.exposure.to_duration().unwrap_or_default();
        let middle = meta.exposure_start + exposure / 2 + line * middle_row(&meta, how.region);
        let image = prepare(&frame, how.region)?;
        drop(frame);
        samples.push((
            meta.exposure_start,
            middle,
            measure(rig, &image, how.measure)?,
        ));
        report.progress(
            0.1 + 0.8 * (samples.len() as f64 / 64.0).min(1.0),
            "sweeping through focus",
        );
    }
    moving
        .await
        .map_err(|e| failed(format!("the sweep's move: {e}")))?
        .map_err(device)?;
    check(report)?;
    let track = rig.stage.track();
    let mut curve: Vec<CurvePoint> = samples
        .iter()
        .filter_map(|(_, middle, value)| {
            track.at(*middle).map(|p| CurvePoint {
                z_um: p.z.0,
                value: *value,
            })
        })
        .collect();
    curve.sort_by(|a, b| a.z_um.total_cmp(&b.z_um));
    let interval = match samples.as_slice() {
        [first, .., last] => {
            let span = last.0.0.saturating_sub(first.0.0) as f64 / 1e9;
            Some(span / (samples.len() - 1) as f64)
        }
        _ => None,
    };
    Ok((curve, interval))
}

/// The continuous sweep: the focus, as the stage reports z, or `None` with
/// why in the report.
///
/// A camera that gives frames more slowly while the stage moves (the
/// simulator renders each depth afresh) leaves them sparse: fewer than 9
/// within 2 depths of field of the best frame, and the stepped sweep's fine
/// pass about it finds the peak; fewer than 3 in all, and the run sweeps
/// stepped.
async fn continuous_run(
    rig: &FocusRig,
    (xy, z0, range, fine): ((f64, f64), f64, f64, f64),
    how: &How,
    report: &dyn Report,
    (out, swept): (&mut FocusReport, &mut Swept),
) -> Result<Option<f64>, AutofocusError> {
    // The camera's frame interval, from two of its frames.
    let now = rig.stage.position().await.map_err(device)?.time;
    let first = frame_after(&rig.bus, now).await?.meta().exposure_start;
    let second = frame_after(&rig.bus, first + Duration::from_micros(1))
        .await?
        .meta()
        .exposure_start;
    let interval = second.0.saturating_sub(first.0) as f64 / 1e9;
    if interval <= 0.0 {
        return Err(failed("the camera's frames come at no interval"));
    }
    let fastest = rig.stage.capabilities().max_speed.0;
    let speed = (fine / interval).clamp(0.1, fastest);
    out.speed_um_s = Some(speed);
    let (curve, _came) = pass(rig, xy, (z0 - range, z0 + range, speed), how, report, swept).await?;
    out.frames = curve.len() as u32;
    out.coarse.clone_from(&curve);
    // Too few frames to judge at all: sweep stepped instead.
    if curve.len() < 3 {
        out.fine.clear();
        return stepped_run(rig, (xy, z0, range, 4.0 * fine), how, report, (out, swept)).await;
    }
    if let Some(why) = refusal(&curve, range) {
        out.refused = Some(why);
        return Ok(None);
    }
    let k = best(&curve);
    let centre = curve[k].z_um;
    let near = curve
        .iter()
        .filter(|p| (p.z_um - centre).abs() <= 8.0 * fine)
        .count();
    if near >= 9 {
        let around = &curve[k.saturating_sub(2)..(k + 3).min(curve.len())];
        return Ok(Some(parabola_peak(around).unwrap_or(centre)));
    }
    // Sparse about the best frame: the stepped sweep's fine pass about it.
    let around = swept.command(centre);
    let fine_positions = positions(around - 4.0 * fine, around + 4.0 * fine, fine);
    let curve = stepped(rig, xy, &fine_positions, how, report, (0.6, 0.35), swept).await?;
    out.frames += curve.len() as u32;
    out.fine.clone_from(&curve);
    Ok(Some(gaussian_peak(&curve)))
}

/// Autofocuses where the stage is: sweeps, finds the sharpest z, and leaves
/// the stage there, approached moving up. A refusal (an empty field, the
/// focus out of reach) leaves it where it started, and says why in the
/// report. The report's z is where the stage says it is.
pub async fn autofocus(
    rig: &FocusRig,
    options: &FocusOptions,
    report: &dyn Report,
) -> Result<FocusReport, AutofocusError> {
    options.check().map_err(failed)?;
    let started = Instant::now();
    let defaults = focus_defaults(rig).await?;
    let measure = options.measure.unwrap_or(defaults.measure);
    let d = defaults.depth_of_field_um;
    let coarse = options.step_um.unwrap_or(d);
    let range = options.range_um.unwrap_or(RANGE * d);
    let here = rig.stage.position().await.map_err(device)?.position;
    let (xy, z0) = ((here.x.0, here.y.0), here.z.0);
    let how = How {
        measure,
        region: options.region,
        approach: options.approach_um,
        settle: Duration::from_secs_f64(options.settle_ms / 1e3),
    };
    let mut out = FocusReport {
        z_um: z0,
        found: false,
        start_um: z0,
        measure,
        sweep: options.sweep,
        depth_of_field_um: d,
        coarse: Vec::new(),
        fine: Vec::new(),
        frames: 0,
        took_ms: 0.0,
        speed_um_s: None,
        refused: None,
    };
    let mut swept = Swept::default();
    let found = match options.sweep {
        Sweep::Stepped => {
            let span = (xy, z0, range, coarse);
            stepped_run(rig, span, &how, report, (&mut out, &mut swept)).await?
        }
        Sweep::Continuous => {
            let span = (xy, z0, range, coarse / 4.0);
            continuous_run(rig, span, &how, report, (&mut out, &mut swept)).await?
        }
    };
    // To the focus, or back to the start, approached from below.
    let target = swept.command(found.unwrap_or(z0));
    let arrived = up_to(rig, xy, target, how.approach).await?;
    // Where the stage is once it has settled: a frame after.
    let at = rig.stage.position().await.map_err(device)?.time;
    drop(frame_after(&rig.bus, at + how.settle).await?);
    out.z_um = rig.stage.position().await.map_err(device)?.position.z.0;
    let _ = arrived;
    out.found = found.is_some();
    out.took_ms = started.elapsed().as_secs_f64() * 1e3;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gaussian(z: f64, centre: f64, sigma: f64) -> CurvePoint {
        CurvePoint {
            z_um: z,
            value: 1.0 + 4.0 * (-(z - centre).powi(2) / (2.0 * sigma * sigma)).exp(),
        }
    }

    #[test]
    fn the_peaks_are_found_between_samples() {
        // A pure Gaussian: the three-point estimate is exact.
        let curve: Vec<CurvePoint> = (0..9)
            .map(|i| {
                let z = 100.0 + 2.0 * f64::from(i);
                CurvePoint {
                    z_um: z,
                    value: (-(z - 107.3_f64).powi(2) / 18.0).exp(),
                }
            })
            .collect();
        assert!((gaussian_peak(&curve) - 107.3).abs() < 1e-9);
        // A Gaussian on a floor, unevenly sampled: the parabola through the
        // logarithms lands near.
        let zs = [100.0, 101.3, 103.1, 104.0, 105.8, 107.2, 109.0];
        let points: Vec<CurvePoint> = zs.iter().map(|z| gaussian(*z, 104.6, 2.5)).collect();
        let peak = parabola_peak(&points[1..6]).unwrap();
        assert!((peak - 104.6).abs() < 0.2, "{peak}");
        // At an end, the end.
        assert_eq!(gaussian_peak(&curve[..4]), 106.0);
    }

    #[test]
    fn empty_fields_and_out_of_reach_peaks_are_refused() {
        let flat: Vec<CurvePoint> = (0..13)
            .map(|i| CurvePoint {
                z_um: f64::from(i),
                value: 1.0 + 0.01 * f64::from(i % 3),
            })
            .collect();
        assert!(refusal(&flat, 6.0).unwrap().contains("empty"));
        let rising: Vec<CurvePoint> = (0..13).map(|i| gaussian(f64::from(i), 14.0, 3.0)).collect();
        assert!(refusal(&rising, 6.0).unwrap().contains("above"));
        let peaked: Vec<CurvePoint> = (0..13).map(|i| gaussian(f64::from(i), 6.0, 2.0)).collect();
        assert_eq!(refusal(&peaked, 6.0), None);
        assert!(empty(
            &[CurvePoint {
                z_um: 0.0,
                value: 0.0
            }; 3]
        ));
    }

    #[test]
    fn the_depth_of_field_is_the_plan_s() {
        // The MIChrome 20's 2.4 µm pixels behind Mitutoyo's 5×, 10× and 50×.
        let d = |na, m| depth_of_field(na, m, 2.4, 0.55);
        assert!((d(0.14, 5.0) - 31.5).abs() < 0.1);
        assert!((d(0.28, 10.0) - 7.87).abs() < 0.01);
        assert!((d(0.55, 50.0) - 1.9).abs() < 0.01);
        let options = FocusOptions {
            range_um: Some(-1.0),
            ..FocusOptions::default()
        };
        assert!(options.check().is_err() && FocusOptions::default().check().is_ok());
    }
}
