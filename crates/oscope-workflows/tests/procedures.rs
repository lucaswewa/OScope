//! The calibration procedures on the simulator (P32), with the truth
//! injected: a camera turned by 30° and mirrored, objectives magnifying 3 %
//! more than the catalogue says, and 5 µm of backlash. Each recovered value
//! is held to the spec's tolerance.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use oscope_acquire::{Acquisition, FrameBus};
use oscope_core::{FramePool, HeapAllocator, Um, Xyz};
use oscope_core::{Ms, Point2};
use oscope_hal::ObjectiveCatalogue;
use oscope_hal::{Camera, ObjectiveMount, Stage};
use oscope_sim::{
    HeTissue, Optics, Rate, Settling, SimCamera, SimCameraConfig, SimClock, SimMount, SimStage,
    SimStageConfig, Simulator, SimulatorConfig, Slide, World,
};
use oscope_workflows::calibration::{
    CalibrationStore, GeometryOptions, Instrument, Motion, Quiet, Rig, StageOptions, Values,
    check_objective, measure_geometry, measure_stage,
};
use std::sync::RwLock;

/// The truth injected.
const ROTATION: f64 = 30.0;
const SCALE: f64 = 1.03;
const BACKLASH: f64 = 5.0;

/// The simulator over the tissue's first island, in focus, its camera
/// streaming onto a bus, with a calibration store; the clock ten times as
/// fast as real time.
struct Bench {
    sim: Simulator,
    rig: Rig,
    _acquisition: Acquisition,
    _folder: tempfile::TempDir,
}

async fn bench(mode: &str) -> Bench {
    let config = SimulatorConfig {
        rate: 10.0,
        sensor: "quarter".to_owned(),
        rotation: ROTATION,
        mirrored: true,
        scale: SCALE,
        backlash: BACKLASH,
        ..SimulatorConfig::default()
    };
    let sim = Simulator::new(&config, Path::new(".")).await.unwrap();
    let stage = Arc::clone(&sim.stage);
    tokio::spawn(async move { stage.run().await });
    sim.stage.home().await.unwrap();
    // The catalogue's tissue is from seed 1, over the coverslip; the
    // section's middle is 2 µm above the slide's surface.
    let island = HeTissue::new(1, Slide::COVERSLIP).islands()[0];
    let over = Xyz::new(Um(island.x), Um(island.y), Um(config.focus + 2.0));
    sim.stage.move_to(over, None).await.unwrap();
    let bus = Arc::new(FrameBus::new());
    let acquisition = Acquisition::new(Arc::clone(&bus), Duration::from_millis(50));
    let mut settings = sim.camera.settings().await.unwrap();
    settings.mode = mode.to_owned();
    let settings = sim.camera.configure(settings).await.unwrap();
    let (_, w, h) = sim.camera.capabilities().frame_shape(&settings).unwrap();
    let pool = FramePool::new(settings.format.frame_bytes(w, h), 8, &HeapAllocator);
    sim.camera.start(pool, acquisition.sink()).await.unwrap();
    let folder = tempfile::tempdir().unwrap();
    let rig = Rig {
        stage: Arc::clone(&sim.stage) as Arc<dyn Stage>,
        camera: Arc::clone(&sim.camera) as Arc<dyn Camera>,
        objectives: Arc::clone(&sim.mount) as Arc<dyn ObjectiveMount>,
        bus,
        store: Arc::new(CalibrationStore::open(folder.path()).unwrap()),
    };
    Bench {
        sim,
        rig,
        _acquisition: acquisition,
        _folder: folder,
    }
}

/// The pixel size at the sample through `objective`, truly: the quarter
/// sensor's 9.6 µm over the magnification, 3 % more than the catalogue's.
fn true_pixel(sim: &Simulator, objective: &str) -> f64 {
    let catalogue = sim.mount.catalogue();
    9.6 / (catalogue.magnification(catalogue.get(objective).unwrap()) * SCALE)
}

async fn geometry(bench: &Bench) -> (oscope_workflows::calibration::Geometry, Instrument) {
    let options = GeometryOptions::default();
    measure_geometry(&bench.rig, &options, &Quiet)
        .await
        .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_geometry_and_the_stage_are_recovered_within_the_spec_s_tolerances() {
    let bench = bench("full").await;
    let (geometry, instrument) = geometry(&bench).await;
    let truth = true_pixel(&bench.sim, "10x");
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
    // The stage, from the geometry made active.
    let store = &bench.rig.store;
    store
        .save(Values::Geometry(geometry), instrument, None, true)
        .unwrap();
    let options = StageOptions::default();
    let (stage, _) = measure_stage(&bench.rig, &options, &Quiet).await.unwrap();
    for (axis, found) in [("x", stage.backlash_um.x), ("y", stage.backlash_um.y)] {
        assert!(
            (found - BACKLASH).abs() < 0.2,
            "{axis}: {found} µm against {BACKLASH}"
        );
    }
    let capabilities = bench.sim.stage.capabilities();
    let limits = [
        stage.soft_limits.x,
        stage.soft_limits.y,
        stage.soft_limits.z,
    ];
    assert_eq!(limits, [capabilities.x, capabilities.y, capabilities.z]);
    assert!(stage.homed_at.is_some());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_wrong_objective_is_flagged_and_its_geometry_refused() {
    let bench = bench("full").await;
    let motion = Motion::default();
    // The 10× fitted and in the light path: plausible.
    let right = check_objective(&bench.rig, &motion, &Quiet).await.unwrap();
    assert!(right.plausible && !right.calibrated, "{right:?}");
    assert!((right.measured_pixel_um / true_pixel(&bench.sim, "10x") - 1.0).abs() < 0.01);
    // The 20× fitted, with the 10× still in the light path.
    bench.sim.mount.fit("20x".to_owned()).await.unwrap();
    let catalogue = bench.sim.mount.catalogue().clone();
    let mut optics = Optics::new(&catalogue, catalogue.get("10x").unwrap(), Um(9.6));
    optics.magnification *= SCALE;
    bench.sim.camera.set_optics(optics);
    let wrong = check_objective(&bench.rig, &motion, &Quiet).await.unwrap();
    assert!(!wrong.plausible && wrong.nearest == "10x", "{wrong:?}");
    assert!((wrong.ratio - 2.0 / SCALE).abs() < 0.02, "{wrong:?}");
    let refused = measure_geometry(&bench.rig, &GeometryOptions::default(), &Quiet).await;
    let why = refused.unwrap_err().to_string();
    assert!(why.contains("is the 20x in the light path"), "{why}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_binned_camera_s_geometry_is_its_own() {
    // Binned 2 × 2, a pixel is two of the sensor's a side.
    let bench = bench("bin2").await;
    let (geometry, instrument) = geometry(&bench).await;
    assert_eq!((geometry.binning, instrument.binning), (2, 2));
    let truth = 2.0 * true_pixel(&bench.sim, "10x");
    assert!(
        (geometry.pixel_um / truth - 1.0).abs() < 0.002,
        "{geometry:?}"
    );
    assert!((geometry.nominal_pixel_um - 1.92).abs() < 1e-12);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn frames_wait_for_the_stage_to_settle() {
    // A stage that overshoots each move by 20 µm, settling to 1/e in 50 ms:
    // longer than a frame's interval, so frames that don't wait see several
    // µm of it. Waiting 400 ms, they see 0.007 µm.
    let island = HeTissue::new(1, Slide::COVERSLIP).islands()[0];
    let clock = Arc::new(SimClock::new(Rate::Scaled(10.0)));
    let stage = Arc::new(SimStage::new(
        SimStageConfig {
            backlash: Um(0.0),
            settling: Settling {
                overshoot: Um(20.0),
                time: Ms(50.0),
            },
            require_homing: false,
            start: Xyz::new(Um(island.x), Um(island.y), Um(1_002.0)),
            ..SimStageConfig::default()
        },
        Arc::clone(&clock),
    ));
    let running = Arc::clone(&stage);
    tokio::spawn(async move { running.run().await });
    let tissue = Arc::new(HeTissue::new(1, Slide::COVERSLIP));
    let world = World::with_slide(Slide::new(tissue, Point2::new(0.0, 0.0)), Um(1_000.0));
    let catalogue = ObjectiveCatalogue::mitutoyo();
    let optics = Optics::new(&catalogue, catalogue.get("10x").unwrap(), Um(9.6));
    let camera = Arc::new(SimCamera::new(
        SimCameraConfig::quarter(),
        Arc::clone(&clock),
        Arc::clone(&stage),
        Arc::new(RwLock::new(world)),
        optics,
    ));
    let mount = Arc::new(SimMount::new(catalogue, Arc::clone(&camera)));
    mount.fit("10x".to_owned()).await.unwrap();
    let bus = Arc::new(FrameBus::new());
    let acquisition = Acquisition::new(Arc::clone(&bus), Duration::from_millis(50));
    let settings = camera.settings().await.unwrap();
    let (_, w, h) = camera.capabilities().frame_shape(&settings).unwrap();
    let pool = FramePool::new(settings.format.frame_bytes(w, h), 8, &HeapAllocator);
    camera.start(pool, acquisition.sink()).await.unwrap();
    let folder = tempfile::tempdir().unwrap();
    let rig = Rig {
        stage: stage as Arc<dyn Stage>,
        camera: camera as Arc<dyn Camera>,
        objectives: mount as Arc<dyn ObjectiveMount>,
        bus,
        store: Arc::new(CalibrationStore::open(folder.path()).unwrap()),
    };
    let motion = Motion {
        settle_ms: 400.0,
        ..Motion::default()
    };
    let options = GeometryOptions {
        motion,
        ..GeometryOptions::default()
    };
    let (geometry, _) = measure_geometry(&rig, &options, &Quiet).await.unwrap();
    assert!(
        (geometry.pixel_um / 0.96 - 1.0).abs() < 0.002,
        "{geometry:?}"
    );
    assert!(geometry.residual_px < 0.5, "{geometry:?}");
}
