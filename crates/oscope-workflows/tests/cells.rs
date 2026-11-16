//! Cell counting as a job (P46): stained nuclei on a slide wider than a
//! tile, some on its tiles' seams, counted once each, as the library counts
//! the whole; its table, outlines, layer and summary; DAB's classes; grey;
//! a cancel; refusals.

use oscope_core::ProjectId;
use oscope_imaging::cells::{Filters, Threshold, measure, segment};
use oscope_imaging::stain::{DAB, HEMATOXYLIN, Stains, Vector};
use oscope_store::{
    ImageInfo, Input, NewProject, PixelSize, RecordKind, SampleType, Samples, Store, StoreError,
};
use oscope_workflows::cells::{Channel, CountOptions, count, count_jobs};
use serde_json::{Value, json};

const WHITE: [f32; 3] = [0.9, 0.85, 0.8];

fn unit(v: Vector) -> Vector {
    let n = v.iter().map(|x| x * x).sum::<f64>().sqrt();
    v.map(|x| x / n)
}

/// Nuclei, their centres in pixels: a grid, one on each of the tiles'
/// seams at 256 and 512, and a touching pair.
fn nuclei() -> Vec<(f64, f64, f64)> {
    let mut centres: Vec<(f64, f64, f64)> = (0..6)
        .flat_map(|j| {
            (0..7).map(move |i| {
                (
                    40.0 + 95.0 * f64::from(i),
                    40.0 + 95.0 * f64::from(j),
                    f64::from((i + j) % 3),
                )
            })
        })
        .filter(|&(x, y, _)| x < 680.0 && y < 580.0)
        .collect();
    centres.extend([
        (256.0, 300.0, 0.0),
        (512.0, 256.0, 1.0),
        (130.0, 520.0, 2.0),
        (142.0, 520.0, 2.0),
    ]);
    centres
}

/// The picture: nuclei of hematoxylin 0.8 and radius 6 pixels, DAB at the
/// third value's level, on a faint ground that is still tissue.
fn picture(x: u64, y: u64) -> [f32; 3] {
    // Glass in the bottom-right corner, clear of the nuclei.
    if x >= 640 && y >= 540 {
        return WHITE;
    }
    let (x, y) = (x as f64 + 0.5, y as f64 + 0.5);
    let on = nuclei()
        .into_iter()
        .find(|&(cx, cy, _)| (x - cx).hypot(y - cy) <= 6.0);
    let (h, d) = match on {
        Some((_, _, level)) => (0.8, [0.0, 0.25, 0.7][level as usize]),
        None => (0.04, 0.0),
    };
    let (hv, dv) = (unit(HEMATOXYLIN), unit(DAB));
    [0, 1, 2].map(|c| WHITE[c] * 10f64.powf(-(h * hv[c] + d * dv[c])) as f32)
}

struct Made {
    store: Store,
    project: ProjectId,
    input: Input,
    _folder: tempfile::TempDir,
}

/// A store with one acquisition, 700 × 600, of the picture, or of a grey
/// one if `grey`, pixels of 0.5 µm.
fn made(grey: bool) -> Made {
    let folder = tempfile::tempdir().unwrap();
    let data = folder.path().join("data");
    let store = Store::open(&data, &data.join("index.sqlite")).unwrap();
    let new = NewProject {
        name: "cells".to_owned(),
        ..NewProject::default()
    };
    let project = store.create(new).unwrap().id;
    let (w, h) = (700_u64, 600_u64);
    let channels = if grey { 1 } else { 3 };
    let mut info = ImageInfo::new(w, h, channels, SampleType::U16);
    info.pixel_size = Some(PixelSize {
        x: 0.5,
        y: 0.5,
        z: 1.0,
    });
    let values: Vec<f32> = (0..h)
        .flat_map(|y| (0..w).map(move |x| picture(x, y)))
        .flat_map(|p| match grey {
            // In grey, the glass is white: no density, but counted on.
            true => vec![if p == WHITE { 1.0 } else { 1.0 - p[1] }],
            false => p.to_vec(),
        })
        .collect();
    let draft = store.acquisition(project, "a slide", &info).unwrap();
    let samples = Samples::from_unit(SampleType::U16, &values);
    draft
        .image()
        .unwrap()
        .write(0, (0, 0), (w, h), &samples)
        .unwrap();
    let id = draft.commit().unwrap().id;
    Made {
        store,
        project,
        input: Input::Acquisition(id),
        _folder: folder,
    }
}

#[test]
fn nuclei_are_counted_once_each_whatever_the_tiles() {
    let m = made(false);
    let options = CountOptions {
        white: Some(WHITE),
        min_area: 5.0,
        ..CountOptions::default()
    };
    let told = std::sync::Mutex::new(Vec::new());
    let progress = |share: f64, _: String| {
        told.lock().unwrap().push(share);
        true
    };
    let counted = count(&m.store, m.project, m.input, &options, &progress).unwrap();
    let truth = nuclei();
    assert_eq!(counted.summary.count, truth.len() as u64);
    // The library over the whole picture, at Otsu's level of the job's sample.
    let level = counted.summary.level.unwrap();
    let rgb: Vec<f32> = (0..600)
        .flat_map(|y| (0..700).flat_map(move |x| picture(x, y)))
        .collect();
    let stored = Samples::from_unit(SampleType::U16, &rgb).to_unit();
    let hematoxylin: Vec<f32> = Stains::he()
        .deconvolve(&stored, WHITE)
        .iter()
        .step_by(3)
        .copied()
        .collect();
    let (labels, n) = segment(&hematoxylin, 700, Threshold::Level(level), 2, 1.5);
    let filters = Filters {
        min_area: 5.0,
        ..Filters::default()
    };
    let mut whole: Vec<[f64; 2]> = measure(&labels, 700, n, [0.5, 0.5], &[])
        .into_iter()
        .filter(|o| filters.keep(o))
        .map(|o| o.centroid)
        .collect();
    whole.sort_by(|a, b| a[1].total_cmp(&b[1]).then(a[0].total_cmp(&b[0])));
    // The job's table: a row each, the same centroids, the touching pair
    // apart, and each nucleus once.
    let folder = m
        .store
        .folder(m.project)
        .join(format!("results/{}", counted.result.id));
    let csv = std::fs::read_to_string(folder.join("cells.csv")).unwrap();
    let mut rows: Vec<[f64; 2]> = csv
        .split("\r\n")
        .skip(1)
        .filter(|l| !l.is_empty())
        .map(|l| {
            let f: Vec<f64> = l.split(',').map(|v| v.parse().unwrap()).collect();
            [f[1], f[2]]
        })
        .collect();
    assert!(csv.starts_with("id,x_um,y_um,area_um2,perimeter_um,circularity,mean\r\n"));
    rows.sort_by(|a, b| a[1].total_cmp(&b[1]).then(a[0].total_cmp(&b[0])));
    assert_eq!(rows.len(), whole.len());
    for (a, b) in rows.iter().zip(&whole) {
        assert!(
            (a[0] - b[0]).abs() < 1e-9 && (a[1] - b[1]).abs() < 1e-9,
            "{a:?} {b:?}"
        );
    }
    for &(x, y, _) in &truth {
        let near = rows
            .iter()
            .filter(|r| (r[0] - x * 0.5).hypot(r[1] - y * 0.5) < 1.0)
            .count();
        assert_eq!(near, 1, "the nucleus at ({x}, {y})");
    }
    // The outlines and the layer; the summary.
    let geojson: Value =
        serde_json::from_str(&std::fs::read_to_string(folder.join("cells.geojson")).unwrap())
            .unwrap();
    assert_eq!(
        geojson["features"].as_array().map(Vec::len),
        Some(truth.len())
    );
    let layer = m
        .store
        .annotations(m.project, RecordKind::Acquisition, &m.input_id())
        .unwrap();
    assert_eq!(
        (counted.annotated, layer.layers["cells"].features.len()),
        (truth.len(), truth.len())
    );
    let summary: Value =
        serde_json::from_str(&std::fs::read_to_string(folder.join("summary.json")).unwrap())
            .unwrap();
    assert_eq!(summary["count"], json!(truth.len()));
    let area = (700.0 * 600.0 - 60.0 * 60.0) * 0.25e-6;
    assert!(
        (summary["tissue_mm2"].as_f64().unwrap() - area).abs() < 0.01 * area,
        "{summary}"
    );
    assert_eq!(counted.result.provenance.inputs, vec![m.input]);
    // The white found, the glass's, its brightest: the same nuclei.
    let found = CountOptions {
        min_area: 5.0,
        ..CountOptions::default()
    };
    let again = count(&m.store, m.project, m.input, &found, &|_, _| true).unwrap();
    let glass = Samples::from_unit(SampleType::U16, &WHITE).to_unit();
    assert_eq!(again.summary.white.to_vec(), glass);
    assert_eq!(again.summary.count, truth.len() as u64);
    let told = told.into_inner().unwrap();
    assert!(
        told.windows(2).all(|w| w[0] <= w[1]) && told.len() > 6,
        "{told:?}"
    );
}

impl Made {
    fn input_id(&self) -> String {
        match self.input {
            Input::Acquisition(a) => a.to_string(),
            Input::Result(r) => r.to_string(),
        }
    }
}

#[test]
fn dab_classes_grey_and_refusals() {
    let m = made(false);
    let options = CountOptions {
        white: Some(WHITE),
        min_area: 5.0,
        dab: true,
        ..CountOptions::default()
    };
    let counted = count(&m.store, m.project, m.input, &options, &|_, _| true).unwrap();
    let truth = nuclei();
    let at = |level: f64| truth.iter().filter(|n| n.2 == level).count() as u64;
    let classes = counted.summary.dab.unwrap();
    assert_eq!(
        (classes.negative, classes.weak, classes.strong),
        (at(0.0), at(1.0), at(2.0))
    );
    // Grey: bright nuclei, found the same.
    let g = made(true);
    let grey = CountOptions {
        channel: Channel::Grey,
        min_area: 5.0,
        ..CountOptions::default()
    };
    let counted = count(&g.store, g.project, g.input, &grey, &|_, _| true).unwrap();
    assert_eq!(counted.summary.count, truth.len() as u64);
    // In grey, the whole image is the tissue counted on.
    let s = &counted.summary;
    assert!(
        (s.tissue_mm2 - 0.35 * 0.3).abs() < 1e-9
            && (s.per_mm2 - s.count as f64 / s.tissue_mm2).abs() < 1e-9,
        "{s:?}"
    );
    // Hematoxylin isn't found in grey; a cancel leaves nothing.
    let refused = count(
        &g.store,
        g.project,
        g.input,
        &CountOptions::default(),
        &|_, _| true,
    );
    assert!(
        matches!(refused, Err(StoreError::Refused(_))),
        "{refused:?}"
    );
    let before = m.store.contents(m.project).unwrap().results.len();
    // Cancelled in the first pass, which finds Otsu's level: the second
    // never starts.
    let told = std::sync::Mutex::new(Vec::new());
    let early = count(&m.store, m.project, m.input, &options, &|share, what| {
        told.lock().unwrap().push(what);
        share < 0.1
    });
    assert!(matches!(early, Err(StoreError::Cancelled)), "{early:?}");
    assert!(told.into_inner().unwrap().iter().all(|w| w == "sampled"));
    let cancelled = count(&m.store, m.project, m.input, &options, &|share, _| {
        share < 0.5
    });
    assert!(
        matches!(cancelled, Err(StoreError::Cancelled)),
        "{cancelled:?}"
    );
    assert_eq!(m.store.contents(m.project).unwrap().results.len(), before);
    // The job's parameters.
    let jobs = count_jobs(std::sync::Arc::new(made(false).store));
    let input = json!({"kind": "acquisition", "id": m.input_id()});
    assert!(jobs(&json!({"project": m.project, "input": input})).is_ok());
    assert!(
        jobs(&json!({"project": m.project, "input": input, "options": {"channel": "red"}}))
            .is_err()
    );
}
