//! The depth-from-focus job (P42) over a z-stack of the simulator's
//! reflective chip at 50×, stored as a raw volume as a z-scan stores it:
//! heights within half a step of the truth where sure, its three results
//! with their provenance, and its mesh; a flat acquisition refused, and a
//! cancel.

use std::sync::Arc;

use oscope_compute::{Compute, ComputeConfig};
use oscope_core::{BayerColour, BayerPattern, PixelFormat, Point2, Timestamp, Um, Xyz};
use oscope_hal::ObjectiveCatalogue;
use oscope_sim::{Field, Light, Optics, Slide, Surface, World};
use oscope_store::{
    FrameRecord, ImageInfo, ImageReader, Input, NewProject, PixelSize, SampleType, Samples, Store,
};
use oscope_volume::MeshFormat;
use oscope_workflows::depth::{DepthError, DepthOptions, depth_from_focus};
use serde_json::json;

/// The stage z at which the slide's surface is sharp.
const FOCUS: f64 = 5_000.0;
/// The chip's corner on the slide.
const CHIP: Point2 = Point2::new(30_000.0, 8_000.0);
/// The stack: 13 planes 0.5 µm apart, from 4 µm under the dome's top.
const PLANES: u32 = 13;
const STEP: f64 = 0.5;

struct Stacked {
    store: Store,
    project: oscope_core::ProjectId,
    id: oscope_core::AcquisitionId,
    /// The true stage z of each cell in focus.
    truth: Vec<f32>,
    _folder: tempfile::TempDir,
}

/// The z-stack over a die's dome, its frames a colour camera's raw mosaic,
/// stored as a z-scan's volume, and the truth at its cells.
fn stacked() -> Stacked {
    let folder = tempfile::tempdir().unwrap();
    let data = folder.path().join("data");
    let store = Store::open(&data, &data.join("index.sqlite")).unwrap();
    let new = NewProject {
        name: "depth".to_owned(),
        ..NewProject::default()
    };
    let project = store.create(new).unwrap().id;
    let surface = Surface::new(1, CHIP);
    let world = World::with_slide(
        Slide::new(Arc::new(surface.clone()), Point2::new(0.0, 0.0)),
        Um(FOCUS),
    );
    let catalogue = ObjectiveCatalogue::mitutoyo();
    let optics = Optics::new(&catalogue, catalogue.get("50x").unwrap(), Um(9.6));
    let p = optics.pixel_size().0;
    let die = surface.die_centre(Point2::new(CHIP.x + 2_500.0, CHIP.y + 2_500.0));
    let dome = Point2::new(die.x + 50.0, die.y - 50.0);
    let top = FOCUS + surface.height(dome).unwrap().0;
    let (w, h) = (320_u32, 240_u32);
    let mut info = ImageInfo::new(u64::from(w), u64::from(h), 1, SampleType::U16);
    (info.planes, info.volume) = (PLANES, true);
    info.channel_names = vec!["raw".to_owned()];
    info.pixel_size = Some(PixelSize {
        x: p,
        y: p,
        z: STEP,
    });
    let mut draft = store.acquisition(project, "a dome", &info).unwrap();
    let pattern = BayerPattern::Rggb;
    let mut frames = Vec::new();
    for k in 0..PLANES {
        let z = top - 4.0 + f64::from(k) * STEP;
        let field = Field {
            centre: dome,
            z: Um(z),
            width: w,
            height: h,
        };
        let rgb = world.render(&field, &optics, Light::Reflected);
        let mosaic: Vec<f32> = (0..h)
            .flat_map(|y| (0..w).map(move |x| (x, y)))
            .map(|(x, y)| {
                let c = match pattern.colour_at(x, y) {
                    BayerColour::Red => 0,
                    BayerColour::Green => 1,
                    BayerColour::Blue => 2,
                };
                rgb.data()[(y * w + x) as usize * 3 + c]
            })
            .collect();
        let writer = draft.image().unwrap();
        let samples = Samples::from_unit(SampleType::U16, &mosaic);
        writer
            .write(k, (0, 0), (u64::from(w), u64::from(h)), &samples)
            .unwrap();
        frames.push(FrameRecord {
            index: u64::from(k),
            position: Some(Xyz::new(Um(dome.x), Um(dome.y), Um(z))),
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
    // The truth at each cell, two pixels across.
    let origin = Point2::new(
        dome.x - f64::from(w) * p / 2.0,
        dome.y - f64::from(h) * p / 2.0,
    );
    let truth = surface
        .height_map(origin, Um(2.0 * p), w / 2, h / 2)
        .iter()
        .map(|z| (FOCUS + f64::from(*z)) as f32)
        .collect();
    Stacked {
        store,
        project,
        id,
        truth,
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

#[test]
fn a_dome_s_heights_are_within_half_a_step_and_its_results_are_recorded() {
    let s = stacked();
    let options = DepthOptions {
        mesh: Some(MeshFormat::Ply),
        ..DepthOptions::default()
    };
    let made = depth_from_focus(&s.store, s.project, s.id, &options, &compute(), &|_, _| {
        true
    })
    .unwrap();
    // The heights, against the truth where sure: the dome is smooth across
    // the measure's window.
    let folder = |id: String| s.store.folder(s.project).join("results").join(id);
    let read = |id: String| {
        let reader = ImageReader::open(&folder(id).join("image.ome.zarr")).unwrap();
        let info = reader.info().clone();
        let Samples::F32(v) = reader
            .read(0, 0, (0, 0), (info.width, info.height))
            .unwrap()
        else {
            panic!("floats");
        };
        (info, v)
    };
    let (info, heights) = read(made.heights.id.to_string());
    let (_, confidence) = read(made.confidence.id.to_string());
    assert_eq!((info.width, info.height, info.channels), (160, 120, 1));
    assert!(
        (info.pixel_size.unwrap().x - 0.384).abs() < 1e-9,
        "{info:?}"
    );
    let sure: Vec<usize> = (0..heights.len())
        .filter(|&i| confidence[i] >= 0.5)
        .collect();
    let squares: f64 = sure
        .iter()
        .map(|&i| f64::from(heights[i] - s.truth[i]).powi(2))
        .sum();
    let rms = (squares / sure.len() as f64).sqrt();
    eprintln!(
        "{} of {} cells sure, {rms:.3} µm rms",
        sure.len(),
        heights.len()
    );
    assert!(
        sure.len() * 10 >= heights.len() * 8,
        "{} of {} sure",
        sure.len(),
        heights.len()
    );
    assert!(rms < STEP / 2.0, "rms {rms:.3} µm");
    assert!((made.sure - sure.len() as f64 / heights.len() as f64).abs() < 1e-9);

    // Three results, each naming the acquisition; the image in colour; the
    // mesh beside the heights.
    let contents = s.store.contents(s.project).unwrap();
    assert_eq!(contents.results.len(), 3);
    for result in &contents.results {
        assert_eq!(result.provenance.inputs, [Input::Acquisition(s.id)]);
        assert_eq!(result.provenance.recipe["planes"]["step_um"], json!(STEP));
    }
    let image =
        ImageReader::open(&folder(made.image.id.to_string()).join("image.ome.zarr")).unwrap();
    assert_eq!(
        (
            image.info().channels,
            image.info().sample,
            image.info().width
        ),
        (3, SampleType::U16, 160)
    );
    let ply = folder(made.heights.id.to_string()).join("exports/surface.ply");
    assert_eq!(made.mesh, std::slice::from_ref(&ply));
    assert!(ply.is_file());
}

#[test]
fn a_flat_acquisition_is_refused_and_a_cancel_stops_it() {
    let s = stacked();
    // A cancel, at the first plane: no results.
    let stopped = depth_from_focus(
        &s.store,
        s.project,
        s.id,
        &DepthOptions::default(),
        &compute(),
        &|_, _| false,
    );
    assert!(matches!(stopped, Err(DepthError::Cancelled)), "{stopped:?}");
    assert!(s.store.contents(s.project).unwrap().results.is_empty());
    // A flat image: not a volume.
    let info = ImageInfo::new(64, 48, 1, SampleType::U16);
    let draft = s.store.acquisition(s.project, "flat", &info).unwrap();
    let writer = draft.image().unwrap();
    writer
        .write(0, (0, 0), (64, 48), &Samples::U16(vec![9; 64 * 48]))
        .unwrap();
    let flat = draft.commit().unwrap().id;
    let refused = depth_from_focus(
        &s.store,
        s.project,
        flat,
        &DepthOptions::default(),
        &compute(),
        &|_, _| true,
    );
    assert!(matches!(refused, Err(DepthError::Store(_))), "{refused:?}");
}
