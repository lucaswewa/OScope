//! The calibration procedures on the simulator (P32), with the truth
//! injected: a camera turned by 30° and mirrored, objectives magnifying 3 %
//! more than the catalogue says, and 5 µm of backlash. Each recovered value
//! is held to the spec's tolerance.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use oscope_acquire::{Acquisition, FrameBus};
use oscope_core::{FramePool, HeapAllocator, Um, Xyz};
use oscope_hal::{Camera, ObjectiveMount, Stage};
use oscope_sim::{HeTissue, Optics, Simulator, SimulatorConfig, Slide};
use oscope_workflows::calibration::{
    CalibrationStore, GeometryOptions, Instrument, Motion, Quiet, Rig, StageOptions, Values,
    check_objective, measure_geometry, measure_stage,
};

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

async fn bench() -> Bench {
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
    let settings = sim.camera.settings().await.unwrap();
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
    let bench = bench().await;
    let (geometry, instrument) = geometry(&bench).await;
    let truth = true_pixel(&bench.sim, "10x");
    eprintln!("{geometry:?}");
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
    eprintln!("{stage:?}");
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
    let bench = bench().await;
    let motion = Motion::default();
    // The 10× fitted and in the light path: plausible.
    let right = check_objective(&bench.rig, &motion, &Quiet).await.unwrap();
    eprintln!("{right:?}");
    assert!(right.plausible && !right.calibrated, "{right:?}");
    assert!((right.measured_pixel_um / true_pixel(&bench.sim, "10x") - 1.0).abs() < 0.01);
    // The 20× fitted, with the 10× still in the light path.
    bench.sim.mount.fit("20x".to_owned()).await.unwrap();
    let catalogue = bench.sim.mount.catalogue().clone();
    let mut optics = Optics::new(&catalogue, catalogue.get("10x").unwrap(), Um(9.6));
    optics.magnification *= SCALE;
    bench.sim.camera.set_optics(optics);
    let wrong = check_objective(&bench.rig, &motion, &Quiet).await.unwrap();
    eprintln!("{wrong:?}");
    assert!(!wrong.plausible && wrong.nearest == "10x", "{wrong:?}");
    assert!((wrong.ratio - 2.0 / SCALE).abs() < 0.02, "{wrong:?}");
    let refused = measure_geometry(&bench.rig, &GeometryOptions::default(), &Quiet).await;
    let why = refused.unwrap_err().to_string();
    assert!(why.contains("is the 20x in the light path"), "{why}");
}
