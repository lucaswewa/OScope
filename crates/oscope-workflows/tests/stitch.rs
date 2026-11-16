//! The stitch job (P39): an acquisition of a raster's tiles, cut from a
//! known slide where the plan put them and recorded up to 12 µm off,
//! stitched into a result: each tile within 0.3 pixels of where it was cut,
//! the mosaic the slide itself, its parts held to a few chunks at a time.

use oscope_core::{AcquisitionId, ProjectId, Rect, Timestamp, Um, Xyz};
use oscope_store::{
    FrameRecord, ImageInfo, ImageReader, Input, NewProject, SampleType, Samples, Store,
    halve_samples,
};
use oscope_workflows::scan::{Field, Region, ZPolicy, raster};
use oscope_workflows::stitch::{StitchOptions, result_image, stitch};

/// The slide: µm as pixels, `w` × `h` of texture, noise blurred thrice by
/// 2 pixels, in 8 bits.
fn slide(w: usize, h: usize) -> Vec<u8> {
    let mut state = 11_u64;
    let mut v: Vec<f32> = (0..w * h)
        .map(|_| {
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (state >> 40) as f32 / (1u64 << 24) as f32
        })
        .collect();
    for _ in 0..3 {
        let rows: Vec<f32> = (0..w * h)
            .map(|i| {
                let (x, y) = (i % w, i / w);
                let (lo, hi) = (x.saturating_sub(2), (x + 2).min(w - 1));
                (lo..=hi).map(|j| v[y * w + j]).sum::<f32>() / (hi - lo + 1) as f32
            })
            .collect();
        v = (0..w * h)
            .map(|i| {
                let (x, y) = (i % w, i / w);
                let (lo, hi) = (y.saturating_sub(2), (y + 2).min(h - 1));
                (lo..=hi).map(|j| rows[j * w + x]).sum::<f32>() / (hi - lo + 1) as f32
            })
            .collect();
    }
    // Stretched, as the blur narrows the noise round its middle.
    v.iter()
        .map(|&s| ((s - 0.5) * 4.0 * 255.0 + 127.5).clamp(0.0, 255.0) as u8)
        .collect()
}

fn jitter(state: &mut u64) -> f64 {
    *state = state
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    (*state >> 11) as f64 / (1u64 << 53) as f64 * 2.0 - 1.0
}

/// A raster's acquisition: its tiles cut from a slide where the plan put
/// them, recorded up to about 12 µm off.
struct Acquired {
    store: Store,
    project: ProjectId,
    id: AcquisitionId,
    /// Each tile's top left on the slide, in pixels of 1 µm.
    truth: Vec<[f64; 2]>,
    /// Each tile's recorded place, from the first's.
    stage: Vec<[f64; 2]>,
    slide: Vec<u8>,
    slide_width: usize,
    _folder: tempfile::TempDir,
}

/// Tiles of 320 × 240 pixels of 1 µm, a fifth overlapping, over `w` × `h`
/// µm from (100, 100) µm.
fn acquired((w, h): (f64, f64)) -> Acquired {
    let folder = tempfile::tempdir().unwrap();
    let data = folder.path().join("data");
    let store = Store::open(&data, &data.join("index.sqlite")).unwrap();
    let new = NewProject {
        name: "stitching".to_owned(),
        ..NewProject::default()
    };
    let project = store.create(new).unwrap().id;
    let (sw, sh) = (w as usize + 300, h as usize + 300);
    let image = slide(sw, sh);
    let field = Field::nominal(1.0, 320, 240, 0.2).unwrap();
    let region = Region::Rect(Rect::new(100.0, 100.0, w, h));
    let plan = raster(&region, &field, ZPolicy::Fixed { z_um: 0.0 }).unwrap();
    let count = plan.tiles.len();
    let mut info = ImageInfo::new(320, 240, 1, SampleType::U8);
    info.planes = count as u32;
    let mut draft = store.acquisition(project, "a raster", &info).unwrap();
    // The errors: none for the first tile, and summing to none, so that the
    // solve, whose offsets keep the stage's mean, lands each on its whole
    // pixel.
    let mut state = 5_u64;
    let mut errors: Vec<[f64; 2]> = (0..count)
        .map(|k| match k {
            0 => [0.0; 2],
            _ => [12.0 * jitter(&mut state), 12.0 * jitter(&mut state)],
        })
        .collect();
    for axis in 0..2 {
        let mean = errors.iter().map(|e| e[axis]).sum::<f64>() / (count - 1) as f64;
        errors.iter_mut().skip(1).for_each(|e| e[axis] -= mean);
    }
    let (mut truth, mut frames) = (Vec::new(), Vec::new());
    for (k, tile) in plan.tiles.iter().enumerate() {
        // Each tile where the plan put it, to the whole µm, its top left
        // half a frame up and left of its centre.
        let (x0, y0) = (
            (tile.x_um - 160.0).round() as usize,
            (tile.y_um - 120.0).round() as usize,
        );
        truth.push([x0 as f64, y0 as f64]);
        let samples: Vec<u8> = (0..240)
            .flat_map(|y| image[(y0 + y) * sw + x0..][..320].to_vec())
            .collect();
        let writer = draft.image().unwrap();
        writer
            .write(k as u32, (0, 0), (320, 240), &Samples::U8(samples))
            .unwrap();
        let recorded = Xyz::new(
            Um(x0 as f64 + 160.0 + errors[k][0]),
            Um(y0 as f64 + 120.0 + errors[k][1]),
            Um(0.0),
        );
        frames.push(FrameRecord {
            index: k as u64,
            position: Some(recorded),
            timestamp: Some(Timestamp(k as u64)),
            trigger: None,
        });
    }
    let record = draft.record();
    record.plan = serde_json::to_value(&plan).unwrap();
    record.frames = frames.clone();
    let id = draft.commit().unwrap().id;
    let first = frames[0].position.unwrap();
    let stage = frames
        .iter()
        .map(|f| {
            let p = f.position.unwrap();
            [p.x.0 - first.x.0, p.y.0 - first.y.0]
        })
        .collect();
    Acquired {
        store,
        project,
        id,
        truth,
        stage,
        slide: image,
        slide_width: sw,
        _folder: folder,
    }
}

/// The root mean square of `found` less `truth`, each less its mean.
fn rms(found: &[[f64; 2]], truth: &[[f64; 2]]) -> f64 {
    let n = truth.len() as f64;
    let mean = [0, 1].map(|a| {
        (0..truth.len())
            .map(|k| found[k][a] - truth[k][a])
            .sum::<f64>()
            / n
    });
    let sum: f64 = (0..truth.len())
        .map(|k| {
            (found[k][0] - truth[k][0] - mean[0]).powi(2)
                + (found[k][1] - truth[k][1] - mean[1]).powi(2)
        })
        .sum();
    (sum / n).sqrt()
}

#[test]
fn tiles_go_back_where_they_were_cut_and_the_mosaic_is_the_slide() {
    let raster = acquired((1400.0, 900.0));
    let (store, project, id) = (&raster.store, raster.project, raster.id);
    let options = StitchOptions::default();
    let (result, report) = stitch(store, project, id, &options, &|_, _| true).unwrap();
    assert_eq!(report.tiles, raster.truth.len());
    // Some narrow overlaps fall under the least confidence, and are dropped;
    // none is rejected.
    let counted = report.kept + report.dropped == report.pairs;
    assert!(counted && report.rejected == 0, "{report:?}");
    assert!(report.dropped * 4 < report.pairs, "{report:?}");
    let (before, after) = (
        rms(&raster.stage, &raster.truth),
        rms(&report.offsets, &raster.truth),
    );
    assert!(
        before > 5.0 && after < 0.3,
        "{before:.2} px off before, {after:.3} after"
    );
    // The result: its provenance, its offsets beside it, and the slide
    // itself, where the tiles cover it.
    assert_eq!(result.provenance.inputs, [Input::Acquisition(id)]);
    let (path, json) = result_image(store, project, &result);
    let written: serde_json::Value = serde_json::from_slice(&std::fs::read(json).unwrap()).unwrap();
    assert_eq!(
        written["offsets"].as_array().unwrap().len(),
        raster.truth.len()
    );
    let reader = ImageReader::open(&path).unwrap();
    let (mw, mh) = report.size;
    assert!(
        mw >= 1400 && mh >= 900 && reader.info().levels >= 2,
        "{:?}",
        report.size
    );
    let corner = raster
        .truth
        .iter()
        .fold([f64::MAX; 2], |m, t| [m[0].min(t[0]), m[1].min(t[1])]);
    let Samples::U8(mosaic) = reader.read(0, 0, (0, 0), (mw, mh)).unwrap() else {
        panic!("8-bit samples");
    };
    let mut differ = 0;
    for y in (0..mh as usize).step_by(7) {
        for x in (0..mw as usize).step_by(7) {
            let (sx, sy) = (x + corner[0] as usize, y + corner[1] as usize);
            differ += usize::from(
                mosaic[y * mw as usize + x] != raster.slide[sy * raster.slide_width + sx],
            );
        }
    }
    assert_eq!(differ, 0, "{differ} sampled pixels differ from the slide");
    // Level 1, made as level 0 was written, a quarter at a time: level 0
    // halved, as the pyramid halves it.
    let halved = halve_samples(&Samples::U8(mosaic), mw, mh, 1);
    let (w1, h1) = reader.info().level_size(1);
    assert_eq!(reader.read(1, 0, (0, 0), (w1, h1)).unwrap(), halved);
}

#[test]
fn its_parts_held_at_once_stay_within_its_budget() {
    let raster = acquired((2800.0, 1900.0));
    let options = StitchOptions {
        memory_mb: 10,
        ..StitchOptions::default()
    };
    let (store, project, id) = (&raster.store, raster.project, raster.id);
    let (_, report) = stitch(store, project, id, &options, &|_, _| true).unwrap();
    assert_eq!(report.rejected, 0, "{report:?}");
    let after = rms(&report.offsets, &raster.truth);
    assert!(
        after < 0.5,
        "{after:.3} px off the truth over {} tiles",
        raster.truth.len()
    );
    // Within its budget, over a mosaic twice as large.
    let (budget, mosaic_bytes) = (10 << 20, report.size.0 * report.size.1 * 4);
    assert!(
        mosaic_bytes > 2 * budget,
        "a mosaic of {mosaic_bytes} bytes"
    );
    assert!(
        report.peak_bytes <= budget,
        "{} bytes at once",
        report.peak_bytes
    );
}
