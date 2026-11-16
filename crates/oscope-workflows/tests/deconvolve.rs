//! The deconvolution job (P43) over widefield z-stacks the simulator
//! renders, stored as a camera's raw volumes: the fluorescent cells and the
//! sub-diffraction structures, each deconvolved nearer its truth than the
//! stack; the result's record; refusals and a cancel.

use std::sync::Arc;

use oscope_compute::{Compute, ComputeConfig, Image};
use oscope_core::{BayerColour, BayerPattern, PixelFormat, Point2, Timestamp, Um, Xyz};
use oscope_hal::ObjectiveCatalogue;
use oscope_sim::fluorescence::Excitation;
use oscope_sim::{CellCulture, Field, Light, Optics, Sample, Slide, Structures, World};
use oscope_store::{
    FrameRecord, ImageInfo, ImageReader, Input, NewProject, PixelSize, SampleType, Samples, Store,
};
use oscope_volume::nrmse;
use oscope_workflows::deconvolve::{DeconvolveJobOptions, deconvolve_acquisition};
use oscope_workflows::depth::DepthError;
use serde_json::json;

/// The stage z at which the slide's surface is sharp.
const FOCUS: f64 = 5_000.0;
/// 21 planes 0.5 µm apart, from 5 µm under the sample's focus to 5 over.
const PLANES: u32 = 21;
const STEP: f64 = 0.5;
/// The 50× objective's.
const NA: f64 = 0.55;

/// The simulated quarter sensor's shot noise at pixel `i` of plane `k`.
fn noise(k: u32, i: u64) -> f32 {
    let hash = (u64::from(k) * 1_000_003 + i).wrapping_mul(0x9E37_79B9_7F4A_7C15) >> 40;
    (hash as f32 / (1 << 24) as f32 - 0.5) * 0.006
}

struct Stacked {
    store: Store,
    project: oscope_core::ProjectId,
    id: oscope_core::AcquisitionId,
    /// Each plane's height above the slide, and the field's corner and
    /// cell, two pixels across.
    heights: Vec<f64>,
    origin: Point2,
    cell: f64,
    cells: (u32, u32),
    _folder: tempfile::TempDir,
}

/// A widefield stack of `sample` lit by the blue line at 50×, `w` × `h`
/// pixels over the coverslip's middle, about the height `focus` above the
/// slide, stored as a raw volume as a z-scan stores it.
fn stacked(sample: Arc<dyn Sample>, focus: f64, (w, h): (u32, u32)) -> Stacked {
    let folder = tempfile::tempdir().unwrap();
    let data = folder.path().join("data");
    let store = Store::open(&data, &data.join("index.sqlite")).unwrap();
    let new = NewProject {
        name: "deconvolution".to_owned(),
        ..NewProject::default()
    };
    let project = store.create(new).unwrap().id;
    let world = World::with_slide(Slide::new(sample, Point2::new(0.0, 0.0)), Um(FOCUS));
    let catalogue = ObjectiveCatalogue::mitutoyo();
    let optics = Optics::new(&catalogue, catalogue.get("50x").unwrap(), Um(9.6));
    let p = optics.pixel_size().0;
    let cover = Slide::COVERSLIP;
    let centre = Point2::new(
        cover.origin.x + cover.size.width / 2.0,
        cover.origin.y + cover.size.height / 2.0,
    );
    let mut info = ImageInfo::new(u64::from(w), u64::from(h), 1, SampleType::U16);
    (info.planes, info.volume) = (PLANES, true);
    info.channel_names = vec!["raw".to_owned()];
    info.pixel_size = Some(PixelSize {
        x: p,
        y: p,
        z: STEP,
    });
    let mut draft = store.acquisition(project, "a stack", &info).unwrap();
    let pattern = BayerPattern::Rggb;
    let light = Light::Fluorescence {
        line: Excitation::Blue,
        confocal: false,
    };
    let heights: Vec<f64> = (0..PLANES)
        .map(|k| focus - 5.0 + f64::from(k) * STEP)
        .collect();
    let rendered: Vec<Image> = heights
        .iter()
        .map(|height| {
            let field = Field {
                centre,
                z: Um(FOCUS + height),
                width: w,
                height: h,
            };
            world.render(&field, &optics, light)
        })
        .collect();
    // One exposure for the stack: its brightest voxel at 0.6.
    let top = rendered
        .iter()
        .flat_map(|plane| plane.data().iter().copied())
        .fold(1e-6_f32, f32::max);
    let mut frames = Vec::new();
    for (k, (rgb, height)) in (0..PLANES).zip(rendered.iter().zip(&heights)) {
        let mosaic: Vec<f32> = (0..h)
            .flat_map(|y| (0..w).map(move |x| (x, y)))
            .map(|(x, y)| {
                let c = match pattern.colour_at(x, y) {
                    BayerColour::Red => 0,
                    BayerColour::Green => 1,
                    BayerColour::Blue => 2,
                };
                let i = u64::from(y * w + x);
                0.6 * rgb.data()[i as usize * 3 + c] / top + noise(k, i)
            })
            .collect();
        let writer = draft.image().unwrap();
        let samples = Samples::from_unit(SampleType::U16, &mosaic);
        writer
            .write(k, (0, 0), (u64::from(w), u64::from(h)), &samples)
            .unwrap();
        frames.push(FrameRecord {
            index: u64::from(k),
            position: Some(Xyz::new(Um(centre.x), Um(centre.y), Um(FOCUS + height))),
            timestamp: Some(Timestamp(u64::from(k))),
            trigger: None,
        });
    }
    let format = PixelFormat::Bayer16 {
        pattern,
        significant_bits: 12,
    };
    let record = draft.record();
    record.frames = frames;
    record.camera = json!({ "format": format });
    let id = draft.commit().unwrap().id;
    let origin = Point2::new(
        centre.x - f64::from(w) * p / 2.0,
        centre.y - f64::from(h) * p / 2.0,
    );
    Stacked {
        store,
        project,
        id,
        heights,
        origin,
        cell: 2.0 * p,
        cells: (w / 2, h / 2),
        _folder: folder,
    }
}

fn compute() -> Compute {
    let config = ComputeConfig {
        live_threads: Some(1),
        batch_threads: Some(2),
        cuda_device: None,
    };
    Compute::new(&config).unwrap()
}

/// The stack's cells' means, every plane, and the deconvolved result's
/// voxels.
fn volumes(s: &Stacked, result: &str) -> (Vec<f32>, Vec<f32>) {
    let read = |path: std::path::PathBuf, level: u32| {
        let reader = ImageReader::open(&path).unwrap();
        let info = reader.info().clone();
        let (w, h) = info.level_size(level);
        (0..info.level_planes(level))
            .flat_map(|k| reader.read(level, k, (0, 0), (w, h)).unwrap().to_unit())
            .collect::<Vec<f32>>()
    };
    let folder = s.store.folder(s.project);
    let stack = read(
        folder
            .join("acquisitions")
            .join(s.id.to_string())
            .join("raw.ome.zarr"),
        1,
    );
    let result = read(
        folder.join("results").join(result).join("image.ome.zarr"),
        0,
    );
    (stack, result)
}

fn options() -> DeconvolveJobOptions {
    DeconvolveJobOptions {
        na: NA,
        wavelength_um: 0.519,
        medium: 1.0,
        iterations: 20,
        tv: 0.0,
    }
}

#[test]
fn the_cells_and_the_structures_are_nearer_their_truth_deconvolved() {
    let cover = Slide::COVERSLIP;
    // The cells: their membranes, in the green dye the blue line lights.
    let cells = CellCulture::new(1, cover);
    let s = stacked(Arc::new(cells.clone()), 2.0, (384, 288));
    let made = deconvolve_acquisition(
        &s.store,
        s.project,
        s.id,
        &options(),
        &compute(),
        &|_, _| true,
    )
    .unwrap();
    let (stack, result) = volumes(&s, &made.id.to_string());
    let (cw, ch) = s.cells;
    let truth: Vec<f32> = s
        .heights
        .iter()
        .flat_map(|z| {
            // Through the voxel's depth: five heights across it.
            let mut plane = vec![0.0_f32; (cw * ch) as usize];
            for dz in [-0.2, -0.1, 0.0, 0.1, 0.2] {
                let slice: Image = cells.truth(s.origin, Um(s.cell), cw, ch, Um(*z + dz));
                plane
                    .iter_mut()
                    .zip(slice.data().chunks(3))
                    .for_each(|(v, p)| *v += p[1] / 5.0);
            }
            plane
        })
        .collect();
    let (before, after) = (nrmse(&stack, &truth), nrmse(&result, &truth));
    eprintln!("the cells: NRMSE {before:.3} the stack, {after:.3} deconvolved");
    assert!(after < before, "{after} after {before}");
    // The record: a volume of the cells' voxels, naming the acquisition.
    let contents = s.store.contents(s.project).unwrap();
    let record = &contents.results[0];
    assert_eq!(record.provenance.inputs, [Input::Acquisition(s.id)]);
    assert_eq!(
        record.provenance.recipe["deconvolve"]["iterations"],
        json!(20)
    );
    let image = record.image.as_ref().unwrap();
    assert!(image.volume && image.planes == PLANES && image.sample == SampleType::F32);
    assert!((image.pixel_size.unwrap().x - s.cell).abs() < 1e-9);

    // The sub-diffraction structures: a thin layer of filaments, in the
    // green dye, at their height.
    let structures = Structures::new(1, cover);
    let height = Structures::HEIGHT.0;
    let s = stacked(Arc::new(structures.clone()), height, (256, 192));
    let made = deconvolve_acquisition(
        &s.store,
        s.project,
        s.id,
        &options(),
        &compute(),
        &|_, _| true,
    )
    .unwrap();
    let (stack, result) = volumes(&s, &made.id.to_string());
    let (cw, ch) = s.cells;
    let layer: Vec<f32> = structures
        .truth(s.origin, Um(s.cell), cw, ch)
        .data()
        .chunks(3)
        .map(|p| p[0])
        .collect();
    let truth: Vec<f32> = s
        .heights
        .iter()
        .flat_map(|z| match (z - height).abs() < STEP / 2.0 {
            true => layer.clone(),
            false => vec![0.0; layer.len()],
        })
        .collect();
    let (before, after) = (nrmse(&stack, &truth), nrmse(&result, &truth));
    eprintln!("the structures: NRMSE {before:.3} the stack, {after:.3} deconvolved");
    assert!(after < before, "{after} after {before}");
}

#[test]
fn a_flat_acquisition_is_refused_and_a_cancel_stops_it() {
    let s = stacked(
        Arc::new(Structures::new(1, Slide::COVERSLIP)),
        0.1,
        (64, 48),
    );
    let stopped = deconvolve_acquisition(
        &s.store,
        s.project,
        s.id,
        &options(),
        &compute(),
        &|_, _| false,
    );
    assert!(matches!(stopped, Err(DepthError::Cancelled)), "{stopped:?}");
    assert!(s.store.contents(s.project).unwrap().results.is_empty());
    let info = ImageInfo::new(64, 48, 1, SampleType::U16);
    let draft = s.store.acquisition(s.project, "flat", &info).unwrap();
    let writer = draft.image().unwrap();
    writer
        .write(0, (0, 0), (64, 48), &Samples::U16(vec![9; 64 * 48]))
        .unwrap();
    let flat = draft.commit().unwrap().id;
    let refused = deconvolve_acquisition(
        &s.store,
        s.project,
        flat,
        &options(),
        &compute(),
        &|_, _| true,
    );
    assert!(matches!(refused, Err(DepthError::Store(_))), "{refused:?}");
}
