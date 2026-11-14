//! The calibration procedures on the simulator (P32), with the truth
//! injected: a camera turned by 30° and mirrored, objectives magnifying 3 %
//! more than the catalogue says, and 5 µm of backlash. Each recovered value
//! is held to the spec's tolerance.
//!
//! The simulator starts over the tissue, and its camera streams a 768 ×
//! 576 region of the quarter sensor: one test at a time, so that a debug
//! build keeps up on two cores.

use std::sync::{Arc, RwLock};
use std::time::Duration;

use oscope_acquire::{Acquisition, FrameBus};
use oscope_core::{FramePool, HeapAllocator, Ms, PixelRect, Point2, Um, Xyz};
use oscope_hal::{Camera, ObjectiveCatalogue, ObjectiveMount, Stage};
use oscope_sim::{
    HeTissue, Mounting, Optics, Rate, Settling, SimCamera, SimCameraConfig, SimClock, SimMount,
    SimStage, SimStageConfig, Slide, World,
};
use oscope_workflows::calibration::{
    CalibrationStore, Geometry, GeometryOptions, Instrument, Motion, Quiet, Rig, StageOptions,
    Values, check_objective, measure_geometry, measure_stage,
};
use tokio::sync::Mutex;

/// The truth injected.
const ROTATION: f64 = 30.0;
const SCALE: f64 = 1.03;
const BACKLASH: f64 = 5.0;

/// One simulator at a time.
static TURN: Mutex<()> = Mutex::const_new(());

/// The simulator's parts that a test reaches for.
struct Bench {
    stage: Arc<SimStage>,
    camera: Arc<SimCamera>,
    mount: Arc<SimMount>,
    rig: Rig,
    _acquisition: Acquisition,
    _folder: tempfile::TempDir,
}

/// What the simulator is: its camera's mounting and mode, its objectives'
/// scale, and its stage.
struct Truth {
    mounting: Mounting,
    scale: f64,
    mode: &'static str,
    stage: SimStageConfig,
}

/// The truth of most tests: turned, mirrored, scaled, with backlash, binned
/// as `mode` says.
fn truth(mode: &'static str) -> Truth {
    Truth {
        mounting: Mounting {
            rotation: ROTATION.to_radians(),
            mirrored: true,
            offset: Point2::new(0.0, 0.0),
        },
        scale: SCALE,
        mode,
        stage: SimStageConfig {
            backlash: Um(BACKLASH),
            ..SimStageConfig::default()
        },
    }
}

/// The simulator of `truth`, ten times as fast as real time, its stage over
/// the middle of the tissue's first island with the section's middle in
/// focus, its camera streaming onto a bus, and a calibration store.
async fn bench(truth: Truth) -> Bench {
    let island = HeTissue::new(1, Slide::COVERSLIP).islands()[0];
    let clock = Arc::new(SimClock::new(Rate::Scaled(10.0)));
    let stage = Arc::new(SimStage::new(
        SimStageConfig {
            require_homing: false,
            start: Xyz::new(Um(island.x), Um(island.y), Um(1_002.0)),
            ..truth.stage
        },
        Arc::clone(&clock),
    ));
    let running = Arc::clone(&stage);
    tokio::spawn(async move { running.run().await });
    let tissue = Arc::new(HeTissue::new(1, Slide::COVERSLIP));
    let world = World::with_slide(Slide::new(tissue, Point2::new(0.0, 0.0)), Um(1_000.0));
    let catalogue = ObjectiveCatalogue::mitutoyo();
    let optics = Optics::new(&catalogue, catalogue.get("10x").unwrap(), Um(9.6));
    let config = SimCameraConfig {
        mounting: truth.mounting,
        ..SimCameraConfig::quarter()
    };
    let camera = Arc::new(SimCamera::new(
        config,
        Arc::clone(&clock),
        Arc::clone(&stage),
        Arc::new(RwLock::new(world)),
        optics,
    ));
    let mount = Arc::new(SimMount::scaled(
        catalogue,
        Arc::clone(&camera),
        truth.scale,
    ));
    mount.fit("10x".to_owned()).await.unwrap();
    let mut settings = camera.settings().await.unwrap();
    settings.mode = truth.mode.to_owned();
    settings.roi = Some(PixelRect::new(300, 168, 768, 576));
    let settings = camera.configure(settings).await.unwrap();
    let bus = Arc::new(FrameBus::new());
    let acquisition = Acquisition::new(Arc::clone(&bus), Duration::from_millis(50));
    let (_, w, h) = camera.capabilities().frame_shape(&settings).unwrap();
    let pool = FramePool::new(settings.format.frame_bytes(w, h), 8, &HeapAllocator);
    camera.start(pool, acquisition.sink()).await.unwrap();
    let folder = tempfile::tempdir().unwrap();
    let rig = Rig {
        stage: Arc::clone(&stage) as Arc<dyn Stage>,
        camera: Arc::clone(&camera) as Arc<dyn Camera>,
        objectives: Arc::clone(&mount) as Arc<dyn ObjectiveMount>,
        bus,
        store: Arc::new(CalibrationStore::open(folder.path()).unwrap()),
    };
    Bench {
        stage,
        camera,
        mount,
        rig,
        _acquisition: acquisition,
        _folder: folder,
    }
}

/// The pixel size at the sample through `objective` at binning 1, truly:
/// the quarter sensor's 9.6 µm over the magnification, 3 % more than the
/// catalogue's.
fn true_pixel(bench: &Bench, objective: &str) -> f64 {
    let catalogue = bench.mount.catalogue();
    9.6 / (catalogue.magnification(catalogue.get(objective).unwrap()) * SCALE)
}

async fn geometry(bench: &Bench) -> (Geometry, Instrument) {
    let options = GeometryOptions::default();
    measure_geometry(&bench.rig, &options, &Quiet)
        .await
        .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_geometry_and_the_stage_are_recovered_within_the_spec_s_tolerances() {
    let _turn = TURN.lock().await;
    let bench = bench(truth("full")).await;
    let (geometry, instrument) = geometry(&bench).await;
    let truth = true_pixel(&bench, "10x");
    assert_eq!((geometry.objective.as_str(), geometry.binning), ("10x", 1));
    assert!(
        (geometry.pixel_um / truth - 1.0).abs() < 0.002,
        "{} against {truth}",
        geometry.pixel_um
    );
    let turn = (geometry.rotation_deg - ROTATION + 540.0).rem_euclid(360.0) - 180.0;
    assert!(
        turn.abs() < 0.1,
        "{}° against {ROTATION}°",
        geometry.rotation_deg
    );
    assert!(geometry.mirrored);
    assert!((geometry.aspect - 1.0).abs() < 0.002 && geometry.skew.abs() < 0.002);
    assert!((geometry.nominal_pixel_um - 0.96).abs() < 1e-12);
    assert!(geometry.residual_px < 0.5 && geometry.moves == 8);
    assert_eq!((instrument.sensor_pixel_um, instrument.binning), (9.6, 1));
    // The stage, from the geometry made active: the backlash where it is,
    // then homing, and back.
    let store = &bench.rig.store;
    store
        .save(Values::Geometry(geometry), instrument, None, true)
        .unwrap();
    let from = bench.stage.position().await.unwrap().position;
    let options = StageOptions::default();
    let (stage, _) = measure_stage(&bench.rig, &options, &Quiet).await.unwrap();
    for (axis, found) in [("x", stage.backlash_um.x), ("y", stage.backlash_um.y)] {
        assert!(
            (found - BACKLASH).abs() < 0.2,
            "{axis}: {found} µm against {BACKLASH}"
        );
    }
    let capabilities = bench.stage.capabilities();
    let limits = [
        stage.soft_limits.x,
        stage.soft_limits.y,
        stage.soft_limits.z,
    ];
    assert_eq!(limits, [capabilities.x, capabilities.y, capabilities.z]);
    // Homed at the home corner, z at its top, less half the backlash.
    let homed = stage.homed_at.unwrap();
    assert!(homed.x.0.abs() < 5.0 && homed.y.0.abs() < 5.0, "{homed:?}");
    let back = bench.stage.position().await.unwrap().position;
    let off = (back.x.0 - from.x.0).hypot(back.y.0 - from.y.0);
    assert!(
        off < BACKLASH && (back.z.0 - from.z.0).abs() < BACKLASH,
        "{from:?} {back:?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_wrong_objective_is_flagged_and_its_geometry_refused() {
    let _turn = TURN.lock().await;
    let bench = bench(truth("full")).await;
    let motion = Motion::default();
    // The 10× fitted and in the light path: plausible.
    let right = check_objective(&bench.rig, &motion, &Quiet).await.unwrap();
    assert!(right.plausible && !right.calibrated, "{right:?}");
    assert!((right.measured_pixel_um / true_pixel(&bench, "10x") - 1.0).abs() < 0.01);
    // The 20× fitted, with the 10× still in the light path.
    bench.mount.fit("20x".to_owned()).await.unwrap();
    let catalogue = bench.mount.catalogue().clone();
    let mut optics = Optics::new(&catalogue, catalogue.get("10x").unwrap(), Um(9.6));
    optics.magnification *= SCALE;
    bench.camera.set_optics(optics);
    let wrong = check_objective(&bench.rig, &motion, &Quiet).await.unwrap();
    assert!(!wrong.plausible && wrong.nearest == "10x", "{wrong:?}");
    assert!((wrong.ratio - 2.0 / SCALE).abs() < 0.02, "{wrong:?}");
    let refused = measure_geometry(&bench.rig, &GeometryOptions::default(), &Quiet).await;
    let why = refused.unwrap_err().to_string();
    assert!(why.contains("is the 20x in the light path"), "{why}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_binned_camera_s_geometry_is_its_own() {
    let _turn = TURN.lock().await;
    // Binned 2 × 2, a pixel is two of the sensor's a side.
    let bench = bench(truth("bin2")).await;
    let (geometry, instrument) = geometry(&bench).await;
    assert_eq!((geometry.binning, instrument.binning), (2, 2));
    let truth = 2.0 * true_pixel(&bench, "10x");
    assert!(
        (geometry.pixel_um / truth - 1.0).abs() < 0.002,
        "{geometry:?}"
    );
    assert!((geometry.nominal_pixel_um - 1.92).abs() < 1e-12);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn frames_wait_for_the_stage_to_settle() {
    let _turn = TURN.lock().await;
    // A stage that overshoots each move by 20 µm, settling to 1/e in 50 ms:
    // longer than a frame's interval, so frames that don't wait see several
    // µm of it. Waiting 400 ms, they see 0.007 µm.
    let settling = Truth {
        mounting: Mounting::default(),
        scale: 1.0,
        mode: "full",
        stage: SimStageConfig {
            backlash: Um(0.0),
            settling: Settling {
                overshoot: Um(20.0),
                time: Ms(50.0),
            },
            ..SimStageConfig::default()
        },
    };
    let bench = bench(settling).await;
    let motion = Motion {
        settle_ms: 400.0,
        ..Motion::default()
    };
    let options = GeometryOptions {
        motion,
        ..GeometryOptions::default()
    };
    let (geometry, _) = measure_geometry(&bench.rig, &options, &Quiet)
        .await
        .unwrap();
    assert!(
        (geometry.pixel_um / 0.96 - 1.0).abs() < 0.002,
        "{geometry:?}"
    );
    assert!(geometry.residual_px < 0.5, "{geometry:?}");
}
