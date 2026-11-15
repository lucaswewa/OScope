//! The photometric calibrations on the simulator (P33): dark subtraction
//! removes the offset; the flat field evens out the optics' vignetting,
//! and refuses a field that isn't blank; the colour matrix recovers the
//! colour target's colours through a sensor's crosstalk, and is refused
//! where they aren't the target's.

use std::sync::{Arc, RwLock};
use std::time::Duration;

use oscope_acquire::{Acquisition, FrameBus};
use oscope_compute::cpu;
use oscope_core::{FramePool, HeapAllocator, Point2, Um, Xyz};
use oscope_hal::{Camera, Illumination, ObjectiveCatalogue, ObjectiveMount, Stage};
use oscope_imaging::color::ColourTarget;
use oscope_sim::{
    ColourTargetSample, HeTissue, Optics, Rate, SimCamera, SimCameraConfig, SimClock, SimLights,
    SimMount, SimStage, SimStageConfig, Slide, World,
};
use oscope_workflows::calibration::{
    CalibrationStore, Frames, Geometry, Instrument, Quiet, Rig, Values, measure_colour,
    measure_dark, measure_flat,
};
use tokio::sync::Mutex;

/// One simulator at a time.
static TURN: Mutex<()> = Mutex::const_new(());

/// A sensor whose colours cross: red reads some green, and so on, in a
/// warm light.
const CROSSED: [[f64; 3]; 3] = [[0.85, 0.2, 0.05], [0.1, 0.8, 0.1], [0.02, 0.18, 0.6]];

struct Bench {
    camera: Arc<SimCamera>,
    lights: Arc<SimLights>,
    rig: Rig,
    _acquisition: Acquisition,
    _folder: tempfile::TempDir,
}

/// What the slide holds.
enum Slide_ {
    Empty,
    Tissue,
    Target(Point2),
}

/// The quarter sensor through the 10×, ten times as fast as real time,
/// over `slide` at `at`, with `vignetting` and `response`; streaming, with
/// a calibration store.
async fn bench(slide: Slide_, at: Point2, vignetting: f64, response: [[f64; 3]; 3]) -> Bench {
    let clock = Arc::new(SimClock::new(Rate::Scaled(10.0)));
    let stage = Arc::new(SimStage::new(
        SimStageConfig {
            require_homing: false,
            start: Xyz::new(Um(at.x), Um(at.y), Um(1_002.0)),
            ..SimStageConfig::default()
        },
        Arc::clone(&clock),
    ));
    let running = Arc::clone(&stage);
    tokio::spawn(async move { running.run().await });
    let world = match slide {
        Slide_::Empty => World::empty(),
        Slide_::Tissue => {
            let tissue = Arc::new(HeTissue::new(1, Slide::COVERSLIP));
            World::with_slide(Slide::new(tissue, Point2::new(0.0, 0.0)), Um(1_000.0))
        }
        Slide_::Target(centre) => {
            let target = Arc::new(ColourTargetSample::new(centre));
            World::with_slide(Slide::new(target, Point2::new(0.0, 0.0)), Um(1_000.0))
        }
    };
    let catalogue = ObjectiveCatalogue::mitutoyo();
    let optics = Optics::new(&catalogue, catalogue.get("10x").unwrap(), Um(9.6));
    let config = SimCameraConfig {
        vignetting,
        response,
        ..SimCameraConfig::quarter()
    };
    let camera = Arc::new(SimCamera::new(
        config,
        Arc::clone(&clock),
        Arc::clone(&stage),
        Arc::new(RwLock::new(world)),
        optics,
    ));
    let mount = Arc::new(SimMount::new(catalogue, Arc::clone(&camera)));
    mount.fit("10x".to_owned()).await.unwrap();
    let lights = Arc::new(SimLights::new(Arc::clone(&camera)));
    let bus = Arc::new(FrameBus::new());
    let acquisition = Acquisition::new(Arc::clone(&bus), Duration::from_millis(50));
    let settings = camera.settings().await.unwrap();
    let (_, w, h) = camera.capabilities().frame_shape(&settings).unwrap();
    let pool = FramePool::new(settings.format.frame_bytes(w, h), 8, &HeapAllocator);
    camera.start(pool, acquisition.sink()).await.unwrap();
    let folder = tempfile::tempdir().unwrap();
    let rig = Rig {
        stage: stage as Arc<dyn Stage>,
        camera: Arc::clone(&camera) as Arc<dyn Camera>,
        objectives: mount as Arc<dyn ObjectiveMount>,
        bus,
        store: Arc::new(CalibrationStore::open(folder.path()).unwrap()),
        lights: Arc::clone(&lights) as Arc<dyn Illumination>,
    };
    Bench {
        camera,
        lights,
        rig,
        _acquisition: acquisition,
        _folder: folder,
    }
}

/// The next frame as linear values, and its black level in them.
async fn frame(bench: &Bench) -> (Vec<f32>, f32, u32) {
    let frame = bench
        .rig
        .bus
        .next_frame_after(oscope_core::Timestamp(0))
        .await
        .unwrap();
    let meta = frame.meta().clone();
    let image = cpu::to_f32(&frame.view()).unwrap();
    let black = (meta.black_level / 65_535.0) as f32;
    (image.data().to_vec(), black, meta.width)
}

fn spread(values: &[f64]) -> f64 {
    let n = values.len() as f64;
    let mean = values.iter().sum::<f64>() / n;
    (values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / n).sqrt() / mean
}

const IDEAL: [[f64; 3]; 3] = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn dark_subtraction_removes_the_offset_and_the_lights_come_back() {
    let _turn = TURN.lock().await;
    let bench = bench(Slide_::Empty, Point2::new(40_000.0, 10_000.0), 0.0, IDEAL).await;
    let (dark, _, image) = measure_dark(&bench.rig, &Frames::default(), &Quiet)
        .await
        .unwrap();
    // The black level, 125 of 4095, aligned high in 16 bits.
    let black = 125.0 * 16.0 / 65_535.0;
    assert!((dark.mean - black).abs() < 0.0005, "{dark:?}");
    assert_eq!(dark.frames, 16);
    // The lights are back on: a frame is bright.
    let on = bench.lights.state().await.unwrap();
    assert!(on[0].on, "{on:?}");
    // A dark frame taken afresh, less the dark: its offset is gone.
    let off = oscope_core::LightState {
        on: false,
        ..on[0].clone()
    };
    bench.lights.set(off).await.unwrap();
    let after = bench.rig.stage.position().await.unwrap().time + Duration::from_millis(100);
    let fresh = bench.rig.bus.next_frame_after(after).await.unwrap();
    let fresh = cpu::to_f32(&fresh.view()).unwrap();
    let raw = fresh.data().iter().map(|v| f64::from(*v)).sum::<f64>() / fresh.data().len() as f64;
    let less = fresh
        .data()
        .iter()
        .zip(image.data())
        .map(|(v, d)| f64::from(*v - d))
        .sum::<f64>()
        / fresh.data().len() as f64;
    assert!(raw > 0.02, "a dark frame holds the offset: {raw}");
    assert!(less.abs() < 0.0002, "{less} left of {raw}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_flat_field_evens_out_vignetting_and_gives_the_white_balance() {
    let _turn = TURN.lock().await;
    let tint = [[0.9, 0.1, 0.0], [0.05, 0.75, 0.05], [0.0, 0.1, 0.5]];
    let bench = bench(Slide_::Empty, Point2::new(40_000.0, 10_000.0), 0.3, tint).await;
    let (flat, _, image) = measure_flat(&bench.rig, &Frames::default(), &Quiet)
        .await
        .unwrap();
    // The light the tint gives: red 1, green 0.85, blue 0.6.
    let gains = flat.gains;
    assert!(
        (gains[0] - 0.85).abs() < 0.01 && (gains[2] - 0.85 / 0.6).abs() < 0.02,
        "{flat:?}"
    );
    assert!(
        flat.uneven > 0.05,
        "vignetting makes the field uneven: {flat:?}"
    );
    assert!(
        !flat.dark,
        "no dark frame was taken: the black level instead"
    );
    // A blank frame's greens, before and after the flat field.
    let (raw, black, width) = frame(&bench).await;
    let (mut before, mut after) = (Vec::new(), Vec::new());
    for (i, (v, f)) in raw.iter().zip(image.data()).enumerate() {
        let (x, y) = (i as u32 % width, i as u32 / width);
        if (x + y) % 2 == 1 {
            before.push(f64::from(v - black));
            after.push(f64::from((v - black) / f));
        }
    }
    let (was, is) = (spread(&before), spread(&after));
    assert!(was > 0.05 && is < was / 5.0 && is < 0.02, "{was} to {is}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_flat_field_is_refused_where_the_slide_isnt_blank() {
    let _turn = TURN.lock().await;
    let island = HeTissue::new(1, Slide::COVERSLIP).islands()[0];
    let bench = bench(Slide_::Tissue, island, 0.0, IDEAL).await;
    let refused = measure_flat(&bench.rig, &Frames::default(), &Quiet).await;
    let why = refused.unwrap_err().to_string();
    assert!(why.contains("isn't blank"), "{why}");
}

fn instrument() -> Instrument {
    Instrument {
        camera: "simulated".to_owned(),
        sensor_pixel_um: 9.6,
        binning: 1,
        stage: "simulated".to_owned(),
        objective: None,
        tube_lens_um: 200_000.0,
    }
}

/// The bench's true geometry, active: unturned, 0.96 µm a pixel.
fn true_geometry(bench: &Bench) {
    let geometry = Geometry {
        objective: "10x".to_owned(),
        binning: 1,
        nominal_pixel_um: 0.96,
        matrix: [[0.96, 0.0], [0.0, 0.96]],
        pixel_um: 0.0,
        rotation_deg: 0.0,
        mirrored: false,
        aspect: 0.0,
        skew: 0.0,
        residual_px: 0.0,
        step_um: 0.0,
        moves: 0,
    };
    let values = Values::Geometry(geometry);
    bench
        .rig
        .store
        .save(values, instrument(), None, true)
        .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_colour_matrix_recovers_the_target_s_colours() {
    let _turn = TURN.lock().await;
    let centre = Point2::new(47_000.0, 12_500.0);
    let bench = bench(Slide_::Target(centre), centre, 0.2, CROSSED).await;
    true_geometry(&bench);
    let store = &bench.rig.store;
    // The flat field, from clear glass beside the target.
    bench
        .rig
        .stage
        .move_to(
            Xyz::new(Um(centre.x + 2_000.0), Um(centre.y), Um(1_002.0)),
            None,
        )
        .await
        .unwrap();
    let (flat, flat_instrument, image) = measure_flat(&bench.rig, &Frames::default(), &Quiet)
        .await
        .unwrap();
    store
        .save_with_image(Values::Flat(flat), flat_instrument, None, true, &image)
        .unwrap();
    bench
        .rig
        .stage
        .move_to(Xyz::new(Um(centre.x), Um(centre.y), Um(1_002.0)), None)
        .await
        .unwrap();
    let options = Frames {
        frames: 4,
        ..Frames::default()
    };
    let (colour, _) = measure_colour(&bench.rig, &options, &Quiet).await.unwrap();
    assert_eq!(
        colour.patches,
        ColourTarget::oscope().patches().len() as u32
    );
    assert!(
        colour.delta_e_before > 5.0,
        "the crosstalk shows: {colour:?}"
    );
    assert!(
        colour.delta_e_mean < 2.0 && colour.delta_e_max < 4.0,
        "{colour:?}"
    );
    let _ = &bench.camera;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_colour_matrix_that_fits_poorly_is_refused() {
    let _turn = TURN.lock().await;
    // Over the tissue, the patches' places hold its colours, not the target's.
    let island = HeTissue::new(1, Slide::COVERSLIP).islands()[0];
    let bench = bench(Slide_::Tissue, island, 0.0, IDEAL).await;
    true_geometry(&bench);
    let options = Frames {
        frames: 2,
        ..Frames::default()
    };
    let refused = measure_colour(&bench.rig, &options, &Quiet).await;
    let why = refused.unwrap_err().to_string();
    assert!(why.contains("fit to a mean ΔE*ab"), "{why}");
}
