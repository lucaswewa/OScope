//! Autofocus, auto exposure and auto white balance on the simulator (P34):
//! the focus found within a quarter of the depth of field of the truth on
//! the H&E tissue, the reflective chip and a sparse field of fluorescent
//! cells; an empty field refused; continuous and stepped sweeps agreeing;
//! auto exposure converging, and holding at a limit it can't pass; white
//! balance recovering a tinted light's gains.

use std::sync::{Arc, RwLock};
use std::time::Duration;

use oscope_acquire::{Acquisition, FrameBus};
use oscope_core::{FramePool, HeapAllocator, LightState, Ms, PixelRect, Point2, Um, Xyz};
use oscope_hal::{Camera, Illumination, ObjectiveCatalogue, ObjectiveMount, Stage};
use oscope_sim::{
    CellCulture, HeTissue, Optics, Rate, Sample, SimCamera, SimCameraConfig, SimClock, SimLights,
    SimMount, SimStage, SimStageConfig, Slide, Surface, World,
};
use oscope_workflows::autofocus::{
    ExposureOptions, FocusOptions, FocusRig, Sweep, WhiteBalanceOptions, auto_exposure, autofocus,
    white_balance,
};
use oscope_workflows::calibration::Quiet;
use tokio::sync::Mutex;

/// One simulator at a time.
static TURN: Mutex<()> = Mutex::const_new(());

const IDEAL: [[f64; 3]; 3] = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];

struct Bench {
    rig: FocusRig,
    lights: Arc<SimLights>,
    /// The stage's z at which the slide is sharpest where the bench looks.
    truth: f64,
    at: Point2,
    _acquisition: Acquisition,
}

/// The quarter sensor through the 10×, ten times as fast as real time,
/// streaming a 256 × 192 region of its middle; over `sample` at `at`, `off`
/// µm above where it is sharpest, in the light `light`, with the sensor's
/// colour `response`.
async fn bench(
    sample: Option<Arc<dyn Sample>>,
    at: Point2,
    light: &str,
    off: f64,
    response: [[f64; 3]; 3],
) -> Bench {
    let clock = Arc::new(SimClock::new(Rate::Scaled(10.0)));
    let world = match sample {
        Some(sample) => World::with_slide(Slide::new(sample, Point2::new(0.0, 0.0)), Um(1_000.0)),
        None => World::empty(),
    };
    let truth = world.in_focus(at).map_or(1_000.0, |z| z.0);
    let stage = Arc::new(SimStage::new(
        SimStageConfig {
            require_homing: false,
            start: Xyz::new(Um(at.x), Um(at.y), Um(truth + off)),
            ..SimStageConfig::default()
        },
        Arc::clone(&clock),
    ));
    let running = Arc::clone(&stage);
    tokio::spawn(async move { running.run().await });
    let catalogue = ObjectiveCatalogue::mitutoyo();
    let optics = Optics::new(&catalogue, catalogue.get("10x").unwrap(), Um(9.6));
    let config = SimCameraConfig {
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
    for state in lights.state().await.unwrap() {
        let on = state.channel == light;
        lights.set(LightState { on, ..state }).await.unwrap();
    }
    let bus = Arc::new(FrameBus::new());
    let acquisition = Acquisition::new(Arc::clone(&bus), Duration::from_millis(50));
    let mut settings = camera.settings().await.unwrap();
    settings.roi = Some(PixelRect::new(556, 360, 256, 192));
    let settings = camera.configure(settings).await.unwrap();
    let (_, w, h) = camera.capabilities().frame_shape(&settings).unwrap();
    let pool = FramePool::new(settings.format.frame_bytes(w, h), 8, &HeapAllocator);
    camera.start(pool, acquisition.sink()).await.unwrap();
    let rig = FocusRig {
        stage: stage as Arc<dyn Stage>,
        camera: camera as Arc<dyn Camera>,
        objectives: mount as Arc<dyn ObjectiveMount>,
        bus,
        lights: Arc::clone(&lights) as Arc<dyn Illumination>,
        compute: None,
    };
    Bench {
        rig,
        lights,
        truth,
        at,
        _acquisition: acquisition,
    }
}

fn tissue() -> (Option<Arc<dyn Sample>>, Point2) {
    let tissue = HeTissue::new(1, Slide::COVERSLIP);
    let island = tissue.islands()[0];
    (Some(Arc::new(tissue)), island)
}

fn middle() -> Point2 {
    let cover = Slide::COVERSLIP;
    Point2::new(
        cover.origin.x + cover.size.width / 2.0,
        cover.origin.y + cover.size.height / 2.0,
    )
}

async fn z(bench: &Bench) -> f64 {
    bench.rig.stage.position().await.unwrap().position.z.0
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn autofocus_comes_within_a_quarter_of_the_depth_of_field_on_each_sample() {
    let _turn = TURN.lock().await;
    let half = Surface::SIDE.0 / 2.0;
    let m = middle();
    let chip: Arc<dyn Sample> = Arc::new(Surface::new(1, Point2::new(m.x - half, m.y - half)));
    let cells: Arc<dyn Sample> = Arc::new(CellCulture::new(1, Slide::COVERSLIP));
    let (tissue, island) = tissue();
    let cases = [
        ("the H&E tissue", tissue, island, "transmitted", 12.0),
        ("the reflective chip", Some(chip), m, "reflected", -12.0),
        ("a sparse field of cells", Some(cells), m, "blue 488", 12.0),
    ];
    for (name, sample, at, light, off) in cases {
        let bench = bench(sample, at, light, off, IDEAL).await;
        let report = autofocus(&bench.rig, &FocusOptions::default(), &Quiet)
            .await
            .unwrap();
        assert!(report.found, "{name}: {report:?}");
        let (error, quarter) = (
            z(&bench).await - bench.truth,
            report.depth_of_field_um / 4.0,
        );
        assert!(
            error.abs() < quarter,
            "{name}: {error:+.3} µm, beyond {quarter:.3}: {report:?}"
        );
        assert_eq!(report.frames, 22, "{name}: 13 coarse and 9 fine");
        let _ = bench.at;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_empty_field_is_refused_and_the_stage_goes_back() {
    let _turn = TURN.lock().await;
    let (tissue, _) = tissue();
    // Clear glass beside the coverslip.
    let bench = bench(
        tissue,
        Point2::new(10_000.0, 12_500.0),
        "transmitted",
        5.0,
        IDEAL,
    )
    .await;
    let start = z(&bench).await;
    for sweep in [Sweep::Stepped, Sweep::Continuous] {
        let options = FocusOptions {
            sweep,
            ..FocusOptions::default()
        };
        let report = autofocus(&bench.rig, &options, &Quiet).await.unwrap();
        assert!(!report.found, "{sweep:?}: {report:?}");
        assert!(
            report.refused.as_deref().unwrap().contains("empty"),
            "{report:?}"
        );
        assert!(
            (z(&bench).await - start).abs() < 0.5,
            "{sweep:?}: back where it started"
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn continuous_and_stepped_sweeps_agree() {
    let _turn = TURN.lock().await;
    let (tissue, island) = tissue();
    let bench = bench(tissue, island, "transmitted", 12.0, IDEAL).await;
    let start = z(&bench).await;
    let stepped = autofocus(&bench.rig, &FocusOptions::default(), &Quiet)
        .await
        .unwrap();
    let at_stepped = z(&bench).await;
    // Back to the start, from below, and again continuously.
    let to = |z: f64| Xyz::new(Um(bench.at.x), Um(bench.at.y), Um(z));
    bench
        .rig
        .stage
        .move_to(to(start - 10.0), None)
        .await
        .unwrap();
    bench.rig.stage.move_to(to(start), None).await.unwrap();
    let options = FocusOptions {
        sweep: Sweep::Continuous,
        ..FocusOptions::default()
    };
    let continuous = autofocus(&bench.rig, &options, &Quiet).await.unwrap();
    let at_continuous = z(&bench).await;
    let quarter = stepped.depth_of_field_um / 4.0;
    assert!(
        stepped.found && continuous.found,
        "{stepped:?} {continuous:?}"
    );
    assert!(
        (at_continuous - at_stepped).abs() < quarter,
        "stepped {at_stepped:.3}, continuous {at_continuous:.3}: {continuous:?}"
    );
    assert!(
        (at_continuous - bench.truth).abs() < quarter,
        "{continuous:?}"
    );
    assert!(
        continuous.speed_um_s.is_some() && continuous.coarse.len() >= 3,
        "{continuous:?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn auto_exposure_converges_and_holds_at_a_limit() {
    let _turn = TURN.lock().await;
    let (tissue, _) = tissue();
    let bench = bench(
        tissue,
        Point2::new(10_000.0, 12_500.0),
        "transmitted",
        0.0,
        IDEAL,
    )
    .await;
    let options = ExposureOptions::default();
    // From too dark, and from saturated.
    for start in [0.05, 400.0] {
        let mut settings = bench.rig.camera.settings().await.unwrap();
        settings.exposure = Ms(start);
        bench.rig.camera.configure(settings).await.unwrap();
        let report = auto_exposure(&bench.rig, &options, &Quiet).await.unwrap();
        assert!(report.converged, "from {start} ms: {report:?}");
        assert!(
            (report.level / options.target - 1.0).abs() <= options.tolerance,
            "{report:?}"
        );
        let set = bench.rig.camera.settings().await.unwrap().exposure.0;
        assert_eq!(
            set, report.exposure_ms,
            "the camera keeps the exposure found"
        );
        assert!(report.steps.len() >= 2, "{report:?}");
    }
    // A light too dim to reach the target at the longest exposure.
    let mut dim = bench.lights.state().await.unwrap()[0].clone();
    dim.intensity = 1e-7;
    bench.lights.set(dim).await.unwrap();
    let report = auto_exposure(&bench.rig, &options, &Quiet).await.unwrap();
    assert!(!report.converged, "{report:?}");
    assert!(
        report.note.as_deref().unwrap().contains("longest exposure"),
        "{report:?}"
    );
    let longest = bench.rig.camera.capabilities().exposure_max.0;
    assert_eq!(report.exposure_ms, longest, "{report:?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn white_balance_recovers_a_tinted_light_s_gains() {
    let _turn = TURN.lock().await;
    let (tissue, _) = tissue();
    // A warm light through the sensor: red 1, green 0.85, blue 0.6.
    let tint = [[0.9, 0.1, 0.0], [0.05, 0.75, 0.05], [0.0, 0.1, 0.5]];
    let bench = bench(
        tissue,
        Point2::new(10_000.0, 12_500.0),
        "transmitted",
        0.0,
        tint,
    )
    .await;
    let report = white_balance(&bench.rig, &WhiteBalanceOptions::default(), &Quiet)
        .await
        .unwrap();
    let [r, g, b] = report.gains;
    assert_eq!(g, 1.0);
    assert!(
        (r - 0.85).abs() < 0.01 && (b - 0.85 / 0.6).abs() < 0.02,
        "{report:?}"
    );
    // A light switched off leaves nothing to balance on.
    let mut off = bench.lights.state().await.unwrap()[0].clone();
    off.on = false;
    bench.lights.set(off).await.unwrap();
    let refused = white_balance(&bench.rig, &WhiteBalanceOptions::default(), &Quiet).await;
    assert!(refused.unwrap_err().to_string().contains("too dark"));
}
