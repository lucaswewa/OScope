//! Stain separation as a job (P45): over a stored RGB image and a raw one,
//! its results are the library's amounts and images, with their summary;
//! the white and the vectors estimated; a cancel; refusals.

use oscope_core::{BayerColour, BayerPattern, PixelFormat, ProjectId};
use oscope_imaging::stain::{
    DAB, DabCounts, DabThresholds, HEMATOXYLIN, StainChoice, Stains, TISSUE, Vector, dab_classes,
    macenko,
};
use oscope_store::{
    ImageInfo, ImageReader, Input, NewProject, PixelSize, ResultRecord, SampleType, Samples, Store,
    StoreError,
};
use oscope_workflows::stain::{StainOptions, separate, stain_jobs};
use serde_json::json;

const WHITE: [f32; 3] = [0.9, 0.85, 0.8];

fn unit(v: Vector) -> Vector {
    let n = v.iter().map(|x| x * x).sum::<f64>().sqrt();
    v.map(|x| x / n)
}

/// An H-DAB picture's pixel at `(x, y)` of `w` × `h`, from 0 to 1: glass in
/// its top-left eighth, a faint stain under tissue's density along its
/// bottom, and hematoxylin across it and DAB down it.
fn hdab(x: u64, y: u64, (w, h): (u64, u64)) -> [f32; 3] {
    if x < w / 8 && y < h / 8 {
        return WHITE;
    }
    let (a, b) = match y >= h - h / 16 {
        true => (0.01, 0.0),
        false => (0.1 + 0.9 * x as f64 / w as f64, 0.9 * y as f64 / h as f64),
    };
    let (hv, dv) = (unit(HEMATOXYLIN), unit(DAB));
    [0, 1, 2].map(|c| WHITE[c] * 10f64.powf(-(a * hv[c] + b * dv[c])) as f32)
}

struct Made {
    store: Store,
    project: ProjectId,
    input: Input,
    _folder: tempfile::TempDir,
}

/// A store with one acquisition, `w` × `h`, of the H-DAB picture: in RGB,
/// or as a raw mosaic of `pattern` whose cells are its pixels, 16-bit.
fn made((w, h): (u64, u64), pattern: Option<BayerPattern>) -> Made {
    let folder = tempfile::tempdir().unwrap();
    let data = folder.path().join("data");
    let store = Store::open(&data, &data.join("index.sqlite")).unwrap();
    let new = NewProject {
        name: "stains".to_owned(),
        ..NewProject::default()
    };
    let project = store.create(new).unwrap().id;
    let (info, values) = match pattern {
        None => {
            let values = (0..h).flat_map(|y| (0..w).flat_map(move |x| hdab(x, y, (w, h))));
            (
                ImageInfo::new(w, h, 3, SampleType::U16),
                values.collect::<Vec<f32>>(),
            )
        }
        Some(p) => {
            let mut info = ImageInfo::new(2 * w, 2 * h, 1, SampleType::U16);
            info.channel_names = vec!["raw".to_owned()];
            let values = (0..2 * h).flat_map(|y| {
                (0..2 * w).map(move |x| {
                    let rgb = hdab(x / 2, y / 2, (w, h));
                    match p.colour_at(x as u32, y as u32) {
                        BayerColour::Red => rgb[0],
                        BayerColour::Green => rgb[1],
                        BayerColour::Blue => rgb[2],
                    }
                })
            });
            (info, values.collect())
        }
    };
    let mut info = info;
    info.pixel_size = Some(PixelSize {
        x: 0.5,
        y: 0.5,
        z: 1.0,
    });
    let mut draft = store.acquisition(project, "a slide", &info).unwrap();
    let writer = draft.image().unwrap();
    let samples = Samples::from_unit(SampleType::U16, &values);
    writer
        .write(0, (0, 0), (info.width, info.height), &samples)
        .unwrap();
    if let Some(pattern) = pattern {
        let format = PixelFormat::Bayer16 {
            pattern,
            significant_bits: 16,
        };
        draft.record().camera = json!({ "format": format });
    }
    let id = draft.commit().unwrap().id;
    Made {
        store,
        project,
        input: Input::Acquisition(id),
        _folder: folder,
    }
}

/// Every pixel of `record`'s image.
fn pixels(store: &Store, project: ProjectId, record: &ResultRecord) -> (ImageInfo, Samples) {
    let path = store
        .folder(project)
        .join(format!("results/{}/image.ome.zarr", record.id));
    let reader = ImageReader::open(&path).unwrap();
    let info = reader.info().clone();
    let samples = reader
        .read(0, 0, (0, 0), (info.width, info.height))
        .unwrap();
    (info, samples)
}

/// The picture's pixels as stored, 16-bit, from 0 to 1.
fn stored((w, h): (u64, u64)) -> Vec<f32> {
    let values: Vec<f32> = (0..h)
        .flat_map(|y| (0..w).flat_map(move |x| hdab(x, y, (w, h))))
        .collect();
    Samples::from_unit(SampleType::U16, &values).to_unit()
}

#[test]
fn an_rgb_slide_s_stains_are_the_library_s() {
    let size = (700, 600);
    let m = made(size, None);
    let options = StainOptions {
        stains: StainChoice::Hdab,
        white: Some(WHITE),
        ..StainOptions::default()
    };
    let told = std::sync::Mutex::new(Vec::new());
    let progress = |share: f64, _: String| {
        told.lock().unwrap().push(share);
        true
    };
    let made = separate(&m.store, m.project, m.input, &options, &progress).unwrap();
    let rgb = stored(size);
    let library = Stains::hdab().deconvolve(&rgb, WHITE);
    let (info, amounts) = pixels(&m.store, m.project, &made.amounts);
    assert_eq!((info.width, info.height, info.channels), (700, 600, 3));
    assert_eq!(info.channel_names, ["hematoxylin", "DAB", "residual"]);
    assert_eq!(amounts, Samples::F32(library.clone()));
    // Each stain's image: the stain alone, of the white, in 8 bits.
    let (_, first) = pixels(&m.store, m.project, &made.images[0]);
    let alone = Stains::hdab().alone(0, &library, WHITE);
    let want: Vec<u8> = alone
        .iter()
        .enumerate()
        .map(|(i, v)| (255.0 * v / WHITE[i % 3]).round().clamp(0.0, 255.0) as u8)
        .collect();
    assert_eq!(first, Samples::U8(want));
    let (_, second) = pixels(&m.store, m.project, &made.images[1]);
    let alone = Stains::hdab().alone(1, &library, WHITE);
    let want: Vec<u8> = alone
        .iter()
        .enumerate()
        .map(|(i, v)| (255.0 * v / WHITE[i % 3]).round().clamp(0.0, 255.0) as u8)
        .collect();
    assert_eq!(second, Samples::U8(want));
    // The summary: DAB's bins over the slide as the library counts them.
    let mut counts = DabCounts::default();
    for class in dab_classes(&Stains::hdab(), &rgb, WHITE, &DabThresholds::default()) {
        counts.add(class);
    }
    let summary = &made.summary;
    assert_eq!(summary.dab.as_ref().unwrap().counts, counts);
    assert_eq!(summary.tissue, counts.tissue());
    assert!(
        counts.glass > 0 && counts.weak > 0 && counts.strong > 0,
        "{counts:?}"
    );
    assert!(summary.residual_rms < 1e-3, "{summary:?}");
    // Each stain's mean over the tissue, whose summed density is 0.05 or
    // more.
    let tissue: Vec<usize> = (0..rgb.len() / 3)
        .filter(|&i| {
            let od: f32 = (0..3).map(|c| -(rgb[3 * i + c] / WHITE[c]).log10()).sum();
            od >= TISSUE
        })
        .collect();
    for k in 0..2 {
        let mean = tissue
            .iter()
            .map(|&i| f64::from(library[3 * i + k]))
            .sum::<f64>()
            / tissue.len() as f64;
        assert!(
            (summary.means[k] - mean).abs() < 1e-9,
            "{k}: {summary:?} {mean}"
        );
    }
    let file = m
        .store
        .folder(m.project)
        .join(format!("results/{}/summary.json", made.amounts.id));
    let written: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(file).unwrap()).unwrap();
    assert_eq!(written["dab"]["h_score"], json!(counts.h_score()));
    assert_eq!(made.amounts.provenance.inputs, vec![m.input]);
    // H&E's stains aren't binned as DAB.
    let he = StainOptions {
        white: Some(WHITE),
        ..StainOptions::default()
    };
    let other = separate(&m.store, m.project, m.input, &he, &|_, _| true).unwrap();
    assert!(other.summary.dab.is_none() && other.summary.names[1] == "eosin");
    // Progress rose to the end, a tile at a time.
    let told = told.into_inner().unwrap();
    assert!(
        told.windows(2).all(|w| w[0] <= w[1]) && told.len() == 4,
        "{told:?}"
    );
}

#[test]
fn a_raw_slide_is_separated_on_its_cells_and_the_white_found() {
    let size = (300, 260);
    let m = made(size, Some(BayerPattern::Grbg));
    let options = StainOptions {
        stains: StainChoice::Hdab,
        ..StainOptions::default()
    };
    let made = separate(&m.store, m.project, m.input, &options, &|_, _| true).unwrap();
    // The white is the glass's.
    let glass = Samples::from_unit(SampleType::U16, &WHITE).to_unit();
    assert_eq!(made.summary.white.to_vec(), glass);
    let (info, amounts) = pixels(&m.store, m.project, &made.amounts);
    assert_eq!((info.width, info.height), size);
    assert_eq!(info.pixel_size.map(|p| p.x), Some(1.0));
    let library = Stains::hdab().deconvolve(&stored(size), made.summary.white);
    let Samples::F32(amounts) = amounts else {
        panic!("floats")
    };
    let worst = amounts
        .iter()
        .zip(&library)
        .map(|(a, b)| (a - b).abs())
        .fold(0.0, f32::max);
    assert!(worst < 1e-5, "{worst}");
}

#[test]
fn the_vectors_are_estimated_if_asked() {
    let m = made((400, 300), None);
    let options = StainOptions {
        stains: StainChoice::Hdab,
        estimate: true,
        white: Some(WHITE),
        ..StainOptions::default()
    };
    let made = separate(&m.store, m.project, m.input, &options, &|_, _| true).unwrap();
    assert!(made.summary.estimated);
    assert_eq!(made.amounts.name, "a slide (Macenko): stain amounts");
    // Macenko's on the job's sample: one tile, every 8th pixel each way.
    let rgb = stored((400, 300));
    let sample: Vec<f32> = (0..300)
        .step_by(8)
        .flat_map(|y| (0..400).step_by(8).map(move |x| (y * 400 + x) * 3))
        .flat_map(|i| rgb[i..i + 3].to_vec())
        .collect();
    let [h, d] = macenko(&sample, WHITE, 0.15, 1.0).unwrap();
    let want = Stains::new(h, d, None).unwrap().vectors();
    assert_eq!(made.summary.vectors, want);
    // No pixel of the picture is DAB alone, so its vector leans towards
    // hematoxylin, which is found.
    let degrees = |a: Vector, b: Vector| {
        let (a, b) = (unit(a), unit(b));
        (a[0] * b[0] + a[1] * b[1] + a[2] * b[2])
            .clamp(-1.0, 1.0)
            .acos()
            .to_degrees()
    };
    assert!(degrees(h, HEMATOXYLIN) < 0.1, "{h:?}");
}

#[test]
fn a_cancel_leaves_nothing_and_what_isn_t_stained_is_refused() {
    let m = made((700, 600), None);
    let options = StainOptions {
        white: Some(WHITE),
        ..StainOptions::default()
    };
    let cancelled = separate(&m.store, m.project, m.input, &options, &|share, _| {
        share < 0.5
    });
    assert!(
        matches!(cancelled, Err(StoreError::Cancelled)),
        "{cancelled:?}"
    );
    assert!(m.store.contents(m.project).unwrap().results.is_empty());
    // Cancelled in the first pass, which finds the white.
    let sampling = StainOptions::default();
    let told = std::sync::Mutex::new(Vec::new());
    let early = separate(&m.store, m.project, m.input, &sampling, &|share, what| {
        told.lock().unwrap().push(what);
        share < 0.1
    });
    assert!(matches!(early, Err(StoreError::Cancelled)), "{early:?}");
    let told = told.into_inner().unwrap();
    assert!(told.iter().all(|what| what == "sampled"), "{told:?}");
    assert!(m.store.contents(m.project).unwrap().results.is_empty());
    let same = StainOptions {
        stains: StainChoice::Vectors(vec![HEMATOXYLIN, HEMATOXYLIN]),
        ..StainOptions::default()
    };
    let refused = separate(&m.store, m.project, m.input, &same, &|_, _| true);
    assert!(
        matches!(refused, Err(StoreError::Refused(_))),
        "{refused:?}"
    );
    // A grey image isn't in colour.
    let folder = tempfile::tempdir().unwrap();
    let store = Store::open(folder.path(), &folder.path().join("index.sqlite")).unwrap();
    let project = store
        .create(NewProject {
            name: "grey".to_owned(),
            ..NewProject::default()
        })
        .unwrap()
        .id;
    let info = ImageInfo::new(64, 48, 1, SampleType::U8);
    let draft = store.acquisition(project, "grey", &info).unwrap();
    draft
        .image()
        .unwrap()
        .write(0, (0, 0), (64, 48), &Samples::U8(vec![128; 64 * 48]))
        .unwrap();
    let id = draft.commit().unwrap().id;
    let grey = separate(
        &store,
        project,
        Input::Acquisition(id),
        &options,
        &|_, _| true,
    );
    assert!(matches!(grey, Err(StoreError::Refused(_))), "{grey:?}");
    // The job's parameters.
    let jobs = stain_jobs(std::sync::Arc::new(store));
    let input = json!({"kind": "acquisition", "id": id});
    assert!(jobs(&json!({"project": project, "input": input})).is_ok());
    assert!(
        jobs(&json!({"project": project, "input": input, "options": {"stains": "dye"}})).is_err()
    );
}
