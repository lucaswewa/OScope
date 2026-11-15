//! The histological scan (P37, spec-first; its notes give the spec and its
//! sources): an overview of a region, the tissue detected in it, and a mask
//! of where it is, which a snake then scans.

use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use oscope_core::{Point2, Um, Xyz};
use oscope_jobs::{Factory, Job, JobCtx, JobSpec, Resource, Stop};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::plan::{Field, ZPolicy};
use super::planners::snake;
use super::region::{Mask, Region};
use crate::autofocus::frames::{colour_cells, frame_after};
use crate::autofocus::{AutofocusError, FocusRig, check, device, failed};
use crate::calibration::Report;

/// The overview job's kind.
pub const OVERVIEW: &str = "scan.overview";

/// A region's colour at a coarse pixel, from frames a field apart.
#[derive(Clone, Debug, PartialEq)]
pub struct Overview {
    /// Its first pixel's corner, in µm.
    pub origin: Point2,
    /// A pixel's side, in µm.
    pub pixel_um: f64,
    /// Its width, in pixels.
    pub width: u32,
    /// Its height.
    pub height: u32,
    /// Each pixel's mean colour, linear, in shares of full scale, by rows:
    /// black where no frame fell.
    pub rgb: Vec<[f32; 3]>,
    /// The frames it took.
    pub frames: u32,
}

impl Overview {
    /// Its colours in 8 bits, by rows, its lightest 255: for a page.
    pub fn rgb8(&self) -> Vec<u8> {
        let top = self.rgb.iter().flatten().fold(0.0_f32, |m, v| m.max(*v));
        let scale = if top > 0.0 { 255.0 / top } else { 0.0 };
        let bytes = self.rgb.iter().flatten();
        bytes
            .map(|v| (v * scale).round().clamp(0.0, 255.0) as u8)
            .collect()
    }
}

/// How an overview is taken, and its tissue found, each with a default.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct OverviewOptions {
    /// The overview's pixel, in µm.
    pub pixel_um: f64,
    /// How long the stage settles after a move before its frame, in ms.
    pub settle_ms: f64,
    /// How its tissue is found.
    pub tissue: TissueOptions,
}

impl Default for OverviewOptions {
    /// Pixels of 50 µm, 50 ms of settling, and the tissue's defaults.
    fn default() -> Self {
        Self {
            pixel_um: 50.0,
            settle_ms: 50.0,
            tissue: TissueOptions::default(),
        }
    }
}

/// How tissue is found in an overview, each with a default.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct TissueOptions {
    /// The least saturation taken as tissue, whatever Otsu's threshold.
    pub floor: f64,
    /// The least area a piece of tissue keeps, in mm²: dust is less.
    pub min_area_mm2: f64,
    /// How far the tissue found is grown, in µm, so that its edges are in.
    pub margin_um: f64,
}

impl Default for TissueOptions {
    /// A floor of 0.05, pieces of at least 0.05 mm², a margin of 100 µm.
    fn default() -> Self {
        Self {
            floor: 0.05,
            min_area_mm2: 0.05,
            margin_um: 100.0,
        }
    }
}

/// The tissue found in an overview.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Tissue {
    /// Where it is: the overview's pixels, grown by the margin.
    pub mask: Mask,
    /// The saturation that divides it from glass: Otsu's, or the floor.
    pub threshold: f64,
    /// The pieces kept.
    pub kept: u32,
    /// The pieces dropped as smaller than the least area.
    pub dropped: u32,
    /// Its area, before the margin, in mm².
    pub area_mm2: f64,
}

/// Takes an overview of `region`: frames of `field` one field apart, in a
/// snake, at the stage's z, each reduced to its cells' colours and averaged
/// into pixels of `options.pixel_um` (P37's spec, 1).
pub async fn overview(
    rig: &FocusRig,
    region: &Region,
    field: &Field,
    options: &OverviewOptions,
    report: &dyn Report,
) -> Result<Overview, AutofocusError> {
    let here = rig.stage.position().await.map_err(device)?.position;
    let fixed = ZPolicy::Fixed { z_um: here.z.0 };
    let plan = snake(region, field, fixed).map_err(|e| failed(e.to_string()))?;
    let b = region.bounds();
    let pixel = options.pixel_um;
    let (width, height) = (
        (b.size.width / pixel).ceil().max(1.0) as u32,
        (b.size.height / pixel).ceil().max(1.0) as u32,
    );
    let mut sums = vec![[0.0_f64; 4]; (width * height) as usize];
    let settle = Duration::from_secs_f64(options.settle_ms / 1e3);
    let n = plan.tiles.len();
    for (k, tile) in plan.tiles.iter().enumerate() {
        check(report)?;
        report.progress(k as f64 / n as f64, &format!("frame {} of {n}", k + 1));
        let to = Xyz::new(Um(tile.x_um), Um(tile.y_um), here.z);
        let arrived = rig.stage.move_to(to, None).await.map_err(device)?;
        let frame = frame_after(&rig.bus, arrived.time + settle).await?;
        let (w, h) = (frame.meta().width, frame.meta().height);
        let whole = crate::calibration::Region {
            x: 0,
            y: 0,
            width: w,
            height: h,
        };
        let Some((cells, across, down)) = colour_cells(&frame, Some(whole))? else {
            return Err(failed("an overview needs colour frames"));
        };
        // A cell's place on the stage, through the matrix the field's corners
        // give: M · (u, v), from the frame's middle, in its pixels.
        let [c0, c1, _, c3] = field.corners_um;
        let m = [
            [(c1.x - c0.x) / f64::from(w), (c3.x - c0.x) / f64::from(h)],
            [(c1.y - c0.y) / f64::from(w), (c3.y - c0.y) / f64::from(h)],
        ];
        let (sx, sy) = (
            f64::from(w) / f64::from(across),
            f64::from(h) / f64::from(down),
        );
        for (index, rgb) in cells.iter().enumerate() {
            let (a, d) = (
                (index as u32 % across) as f64,
                (index as u32 / across) as f64,
            );
            let (u, v) = (
                (a + 0.5) * sx - f64::from(w) / 2.0,
                (d + 0.5) * sy - f64::from(h) / 2.0,
            );
            let x = tile.x_um + m[0][0] * u + m[0][1] * v;
            let y = tile.y_um + m[1][0] * u + m[1][1] * v;
            let (i, j) = (
                ((x - b.origin.x) / pixel).floor(),
                ((y - b.origin.y) / pixel).floor(),
            );
            if (0.0..f64::from(width)).contains(&i) && (0.0..f64::from(height)).contains(&j) {
                let sum = &mut sums[(j as u32 * width + i as u32) as usize];
                for c in 0..3 {
                    sum[c] += rgb[c];
                }
                sum[3] += 1.0;
            }
        }
    }
    let rgb = sums
        .iter()
        .map(|s| match s[3] {
            0.0 => [0.0; 3],
            count => [0, 1, 2].map(|c| (s[c] / count) as f32),
        })
        .collect();
    Ok(Overview {
        origin: b.origin,
        pixel_um: pixel,
        width,
        height,
        rgb,
        frames: n as u32,
    })
}

/// The tissue in `overview` (P37's spec, 2): its saturation over Otsu's
/// threshold (Otsu, 1979), or the floor; a closing and an opening (Serra,
/// 1982); pieces of 8 neighbours (Rosenfeld and Pfaltz, 1966) under the
/// least area dropped; and the rest grown by the margin.
pub fn detect(overview: &Overview, options: &TissueOptions) -> Tissue {
    let (w, h) = (overview.width as usize, overview.height as usize);
    // Saturation, as HSV's (Smith, 1978).
    let saturation: Vec<f64> = overview
        .rgb
        .iter()
        .map(|c| {
            let (max, min) = (c[0].max(c[1]).max(c[2]), c[0].min(c[1]).min(c[2]));
            if max > 0.0 {
                f64::from((max - min) / max)
            } else {
                0.0
            }
        })
        .collect();
    let threshold = otsu(&saturation).max(options.floor);
    let found: Vec<bool> = saturation.iter().map(|s| *s > threshold).collect();
    let cleaned = dilate(&erode(&erode(&dilate(&found, w, h), w, h), w, h), w, h);
    let pixel_mm2 = (overview.pixel_um / 1e3).powi(2);
    let (kept_mask, kept, dropped) = keep_large(&cleaned, w, h, options.min_area_mm2 / pixel_mm2);
    let area_mm2 = kept_mask.iter().filter(|c| **c).count() as f64 * pixel_mm2;
    let mut mask = kept_mask;
    for _ in 0..(options.margin_um / overview.pixel_um).ceil() as u32 {
        mask = dilate(&mask, w, h);
    }
    Tissue {
        mask: Mask {
            origin: overview.origin,
            cell_um: overview.pixel_um,
            width: overview.width,
            height: overview.height,
            cells: mask,
        },
        threshold,
        kept,
        dropped,
        area_mm2,
    }
}

/// The threshold that best splits `values`, from 0 to 1, into two classes:
/// the most variance between them, over a histogram of 256 bins (Otsu,
/// 1979). 0 for values all alike.
pub fn otsu(values: &[f64]) -> f64 {
    const BINS: usize = 256;
    let mut histogram = [0.0_f64; BINS];
    for v in values {
        histogram[((v.clamp(0.0, 1.0) * BINS as f64) as usize).min(BINS - 1)] += 1.0;
    }
    let total: f64 = histogram.iter().sum();
    let sum: f64 = histogram
        .iter()
        .enumerate()
        .map(|(i, n)| i as f64 * n)
        .sum();
    let (mut below, mut below_sum, mut best, mut split) = (0.0, 0.0, 0.0, 0);
    for (k, n) in histogram.iter().enumerate() {
        below += n;
        below_sum += k as f64 * n;
        let above = total - below;
        if below == 0.0 || above == 0.0 {
            continue;
        }
        let gap = below_sum / below - (sum - below_sum) / above;
        let between = below * above * gap * gap;
        if between > best {
            (best, split) = (between, k + 1);
        }
    }
    split as f64 / BINS as f64
}

/// Each pixel's 8 neighbours and itself, within the image.
fn around(i: usize, w: usize, h: usize) -> impl Iterator<Item = usize> {
    let (x, y) = ((i % w) as i64, (i / w) as i64);
    (-1..=1)
        .flat_map(move |dy| (-1..=1).map(move |dx| (x + dx, y + dy)))
        .filter(move |(x, y)| (0..w as i64).contains(x) && (0..h as i64).contains(y))
        .map(move |(x, y)| y as usize * w + x as usize)
}

/// A 3 × 3 square's dilation: in if any neighbour is.
fn dilate(mask: &[bool], w: usize, h: usize) -> Vec<bool> {
    (0..mask.len())
        .map(|i| around(i, w, h).any(|n| mask[n]))
        .collect()
}

/// A 3 × 3 square's erosion: in if every neighbour within the image is.
fn erode(mask: &[bool], w: usize, h: usize) -> Vec<bool> {
    (0..mask.len())
        .map(|i| around(i, w, h).all(|n| mask[n]))
        .collect()
}

/// `mask` without its pieces of fewer than `least` pixels: what's kept, and
/// how many pieces were kept and dropped.
fn keep_large(mask: &[bool], w: usize, h: usize, least: f64) -> (Vec<bool>, u32, u32) {
    let mut kept = vec![false; mask.len()];
    let mut seen = vec![false; mask.len()];
    let (mut pieces, mut dropped) = (0, 0);
    for start in 0..mask.len() {
        if !mask[start] || seen[start] {
            continue;
        }
        let (mut piece, mut stack) = (Vec::new(), vec![start]);
        seen[start] = true;
        while let Some(i) = stack.pop() {
            piece.push(i);
            for n in around(i, w, h) {
                if mask[n] && !seen[n] {
                    seen[n] = true;
                    stack.push(n);
                }
            }
        }
        if piece.len() as f64 >= least {
            pieces += 1;
            piece.iter().for_each(|&i| kept[i] = true);
        } else {
            dropped += 1;
        }
    }
    (kept, pieces, dropped)
}

/// The latest overview, and the tissue found in it, which its job keeps.
pub type LastOverview = Arc<Mutex<Option<(Overview, Tissue)>>>;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Params {
    region: Region,
    field: Field,
    #[serde(default)]
    options: OverviewOptions,
}

/// Makes overview jobs: `region`, `field` (the camera's frames now, as the
/// `scan` Thing gives it, with no overlap), and optionally `options`. Each
/// takes an overview, finds its tissue, and keeps both in `last`.
pub fn overview_jobs(rig: FocusRig, last: LastOverview) -> Factory {
    Arc::new(move |value| {
        let params = Params::deserialize(value).map_err(|e| e.to_string())?;
        params.region.check().map_err(|e| e.to_string())?;
        let o = &params.options;
        if !((1.0..=1_000.0).contains(&o.pixel_um) && (0.0..=10_000.0).contains(&o.settle_ms)) {
            return Err(format!(
                "pixels of {} µm and {} ms of settling: from 1 to 1,000 µm, and from 0 to \
                 10,000 ms, are needed",
                o.pixel_um, o.settle_ms
            ));
        }
        let spec = JobSpec {
            kind: OVERVIEW.to_owned(),
            title: "an overview, and its tissue".to_owned(),
            account: None,
            params: value.clone(),
            resource: Resource::Hardware,
            pausable: false,
            gpu_mib: 0,
        };
        let (rig, last) = (rig.clone(), Arc::clone(&last));
        Ok(Job {
            spec,
            run: Box::new(move |ctx| Box::pin(run(rig, params, ctx, last))),
        })
    })
}

async fn run(rig: FocusRig, params: Params, ctx: JobCtx, last: LastOverview) -> Result<(), Stop> {
    let taken = overview(&rig, &params.region, &params.field, &params.options, &ctx).await;
    let taken = taken.map_err(|e| match e {
        AutofocusError::Cancelled => Stop::Cancelled,
        e => Stop::failed(e),
    })?;
    let tissue = detect(&taken, &params.options.tissue);
    ctx.add_result(format!(
        "{:.1} mm² of tissue in {} pieces, at saturation over {:.3}; {} frames",
        tissue.area_mm2, tissue.kept, tissue.threshold, taken.frames
    ));
    *last.lock().unwrap_or_else(PoisonError::into_inner) = Some((taken, tissue));
    ctx.progress(Some(1.0), "overview taken");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An overview of `w` × `h` pixels of 50 µm, grey glass but where `pink`
    /// says.
    fn overview(w: u32, h: u32, pink: impl Fn(u32, u32) -> bool) -> Overview {
        let rgb = (0..w * h)
            .map(|k| match pink(k % w, k / w) {
                true => [0.75, 0.45, 0.6],
                false => [0.85, 0.85, 0.85],
            })
            .collect();
        Overview {
            origin: Point2::new(1_000.0, 2_000.0),
            pixel_um: 50.0,
            width: w,
            height: h,
            rgb,
            frames: 1,
        }
    }

    #[test]
    fn otsu_splits_two_classes_between_them() {
        let values: Vec<f64> = (0..1_000)
            .map(|i| {
                if i % 3 == 0 {
                    0.4 + 0.01 * (i % 7) as f64
                } else {
                    0.01 * (i % 5) as f64
                }
            })
            .collect();
        let t = otsu(&values);
        assert!(t > 0.04 && t <= 0.4, "{t}");
        assert_eq!(otsu(&[0.2; 50]), 0.0);
    }

    #[test]
    fn tissue_is_found_cleaned_kept_by_area_and_grown() {
        // A 20 × 12 block, a hole of one pixel in it; a speck of 4 × 4, 0.04
        // mm², under the least area; and a lone pixel, which the opening
        // removes.
        let pink = |x: u32, y: u32| {
            let block = (5..25).contains(&x) && (4..16).contains(&y) && (x, y) != (12, 9);
            let speck = (32..36).contains(&x) && (20..24).contains(&y);
            block || speck || (x, y) == (2, 25)
        };
        let tissue = detect(&overview(40, 30, pink), &TissueOptions::default());
        assert!(
            tissue.threshold >= 0.05 && tissue.threshold < 0.4,
            "{}",
            tissue.threshold
        );
        assert_eq!((tissue.kept, tissue.dropped), (1, 1));
        assert!(
            (tissue.area_mm2 - 20.0 * 12.0 * 0.0025).abs() < 1e-9,
            "{}",
            tissue.area_mm2
        );
        let at = |x: usize, y: usize| tissue.mask.cells[y * 40 + x];
        // The hole filled; the margin of two pixels; the speck and the lone
        // pixel gone.
        assert!(at(12, 9) && at(3, 2) && at(26, 17) && !at(2, 2) && !at(27, 17));
        assert!(!at(32, 20) && !at(2, 25));
    }

    #[test]
    fn saturation_finds_the_same_tissue_in_dim_light() {
        let pink = |x: u32, y: u32| (5..25).contains(&x) && (4..16).contains(&y);
        let bright = overview(40, 30, pink);
        let mut dim = bright.clone();
        dim.rgb.iter_mut().flatten().for_each(|v| *v *= 0.1);
        let options = TissueOptions::default();
        assert_eq!(detect(&dim, &options).mask, detect(&bright, &options).mask);
    }

    #[test]
    fn grey_glass_alone_is_no_tissue() {
        let mut glass = overview(30, 20, |_, _| false);
        // Noise of a hundredth in each channel.
        for (k, c) in glass.rgb.iter_mut().enumerate() {
            c[k % 3] += 0.01 * ((k % 5) as f32 - 2.0) / 2.0;
        }
        let tissue = detect(&glass, &TissueOptions::default());
        assert!(!tissue.mask.cells.contains(&true));
        assert_eq!((tissue.kept, tissue.threshold), (0, 0.05));
    }
}
