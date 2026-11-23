//! Autofocus, auto exposure and auto white balance on the simulator (P34):
//! the focus found within a quarter of the depth of field of the truth on
//! the H&E tissue, the reflective chip and a sparse field of fluorescent
//! cells, each in its light's measure, and through a stage's backlash; an
//! empty field refused; continuous and stepped sweeps agreeing; auto
//! exposure converging,
//! and holding at a limit it can't pass; white balance recovering a tinted
//! light, and beside pink tissue too.

use std::sync::{Arc, RwLock};
use std::time::Duration;

use oscope_acquire::{Acquisition, FrameBus};
use oscope_core::{FramePool, HeapAllocator, LightState, Ms, PixelRect, Point2, Um, Xyz};
use oscope_hal::{Camera, Illumination, ObjectiveCatalogue, ObjectiveMount, Stage};
use oscope_sim::{
    CellCulture, Field, HeTissue, Light, Optics, Rate, Sample, SimCamera, SimCameraConfig,
    SimClock, SimLights, SimMount, SimStage, SimStageConfig, Slide, Surface, World,
};
use oscope_workflows::autofocus::{
    ExposureOptions, FocusOptions, FocusRig, Measure, Sweep, WhiteBalanceOptions, auto_exposure,
    autofocus, white_balance,
};
use oscope_workflows::calibration::{Quiet, Region};
use tokio::sync::Mutex;

/// One simulator at a time.
static TURN: Mutex<()> = Mutex::const_new(());

const IDEAL: [[f64; 3]; 3] = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];

/// A 256 × 192 region of the quarter sensor's middle: quick to render.
const SMALL: Option<PixelRect> = Some(PixelRect::new(556, 360, 256, 192));

/// What a bench is set up with.
struct Setup {
    sample: Option<Arc<dyn Sample>>,
    at: Point2,
    light: &'static str,
    /// How far above where the sample is sharpest the stage starts, in µm.
    off: f64,
    response: [[f64; 3]; 3],
    backlash: f64,
    region: Option<PixelRect>,
}

impl Setup {
    /// Over `sample` at `at` in `light`, at focus, through a small region.
    fn new(sample: Option<Arc<dyn Sample>>, at: Point2, light: &'static str) -> Self {
        Self {
            sample,
            at,
            light,
            off: 0.0,
            response: IDEAL,
            backlash: 2.0,
            region: SMALL,
        }
    }
}

struct Bench {
    rig: FocusRig,
    lights: Arc<SimLights>,
    /// The stage's z at which the slide is sharpest where the bench looks.
    truth: f64,
    at: Point2,
    _acquisition: Acquisition,
}

/// The quarter sensor through the 10×, ten times as fast as real time,
/// streaming, as `setup` says.
async fn bench(setup: Setup) -> Bench {
    let clock = Arc::new(SimClock::new(Rate::Scaled(10.0)));
    let world = match setup.sample {
        Some(sample) => World::with_slide(Slide::new(sample, Point2::new(0.0, 0.0)), Um(1_000.0)),
        None => World::empty(),
    };
    let at = setup.at;
    let truth = world.in_focus(at).map_or(1_000.0, |z| z.0);
    let stage = Arc::new(SimStage::new(
        SimStageConfig {
            require_homing: false,
            start: Xyz::new(Um(at.x), Um(at.y), Um(truth + setup.off)),
            backlash: Um(setup.backlash),
            ..SimStageConfig::default()
        },
        Arc::clone(&clock),
    ));
    let running = Arc::clone(&stage);
    tokio::spawn(async move { running.run().await });
    let catalogue = ObjectiveCatalogue::mitutoyo();
    let optics = Optics::new(&catalogue, catalogue.get("10x").unwrap(), Um(9.6));
    let config = SimCameraConfig {
        response: setup.response,
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
        let on = state.channel == setup.light;
        lights.set(LightState { on, ..state }).await.unwrap();
    }
    let bus = Arc::new(FrameBus::new());
    let acquisition = Acquisition::new(Arc::clone(&bus), Duration::from_millis(50));
    let mut settings = camera.settings().await.unwrap();
    settings.roi = setup.region;
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

/// Clear glass beside the coverslip.
const GLASS: Point2 = Point2::new(10_000.0, 12_500.0);

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
        (
            "the H&E tissue",
            tissue.clone(),
            island,
            "transmitted",
            12.0,
            2.0,
            Measure::Brenner,
        ),
        (
            "the reflective chip",
            Some(chip),
            m,
            "reflected",
            -12.0,
            2.0,
            Measure::Tenengrad,
        ),
        (
            "a sparse field of cells",
            Some(cells),
            m,
            "blue 488",
            12.0,
            2.0,
            Measure::Laplacian,
        ),
        // A stage with 6 µm of backlash: approached from below, and the lag
        // taken up, it still lands within a quarter.
        (
            "the tissue, through 6 µm of backlash",
            tissue,
            island,
            "transmitted",
            -12.0,
            6.0,
            Measure::Brenner,
        ),
    ];
    for (name, sample, at, light, off, backlash, measure) in cases {
        let setup = Setup {
            off,
            backlash,
            ..Setup::new(sample, at, light)
        };
        let bench = bench(setup).await;
        let report = autofocus(&bench.rig, &FocusOptions::default(), &Quiet)
            .await
            .unwrap();
        assert!(report.found, "{name}: {report:?}");
        assert_eq!(report.measure, measure, "{name}: the light's measure");
        let (error, quarter) = (
            z(&bench).await - bench.truth,
            report.depth_of_field_um / 4.0,
        );
        assert!(
            error.abs() < quarter,
            "{name}: {error:+.3} µm, beyond {quarter:.3}: {report:?}"
        );
        assert_eq!(report.frames, 22, "{name}: 13 coarse and 9 fine");
        assert!(
            (report.z_um - z(&bench).await).abs() < 1e-9,
            "{name}: where it says"
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_empty_field_is_refused_and_the_stage_goes_back() {
    let _turn = TURN.lock().await;
    let (tissue, _) = tissue();
    let setup = Setup {
        off: 5.0,
        ..Setup::new(tissue, GLASS, "transmitted")
    };
    let bench = bench(setup).await;
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
        // Back where it started, settled as it was then: the second sweep
        // starts where the first left it, so their errors would add.
        let now = z(&bench).await;
        let back =
            format!("{sweep:?}: back where it started, {start:.3}, not {now:.3}: {report:?}");
        assert!((now - start).abs() < 0.1, "{back}");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn continuous_and_stepped_sweeps_agree() {
    let _turn = TURN.lock().await;
    let (tissue, island) = tissue();
    let setup = Setup {
        off: 12.0,
        ..Setup::new(tissue, island, "transmitted")
    };
    let bench = bench(setup).await;
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
    assert!(continuous.speed_um_s.is_some(), "{continuous:?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn auto_exposure_converges_and_holds_at_a_limit() {
    let _turn = TURN.lock().await;
    let (tissue, _) = tissue();
    let bench = bench(Setup::new(tissue, GLASS, "transmitted")).await;
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
    let setup = Setup {
        response: tint,
        ..Setup::new(tissue, GLASS, "transmitted")
    };
    let bench = bench(setup).await;
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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn white_balance_finds_the_light_beside_a_colourful_sample() {
    let _turn = TURN.lock().await;
    // An edge of the tissue's first island: along x from its middle, the
    // first field a quarter of which is clear glass. Pink tissue fills the
    // rest; the lightest part is the glass, the light itself.
    let tissue: Arc<dyn Sample> = Arc::new(HeTissue::new(1, Slide::COVERSLIP));
    let island = HeTissue::new(1, Slide::COVERSLIP).islands()[0];
    let world = World::with_slide(
        Slide::new(Arc::clone(&tissue), Point2::new(0.0, 0.0)),
        Um(1_000.0),
    );
    // A field, coarsely: in 9.6 µm pixels, 137 × 91 are the 1.31 × 0.88 mm
    // the quarter sensor sees through the 10×.
    let catalogue = ObjectiveCatalogue::mitutoyo();
    let coarse = Optics::new(&catalogue, catalogue.get("10x").unwrap(), Um(96.0));
    let render = |centre: Point2, width: u32, height: u32| {
        let field = Field {
            centre,
            z: Um(1_002.0),
            width,
            height,
        };
        world.render(&field, &coarse, Light::Transmitted)
    };
    // What clear glass passes under the coverslip: at its corner, which no
    // island reaches. A field's share of it, and its mean colour.
    let corner = Slide::COVERSLIP.origin;
    let clear = render(Point2::new(corner.x + 100.0, corner.y + 100.0), 1, 1).data()[1];
    let look = |x: f64| {
        let image = render(Point2::new(x, island.y), 137, 91);
        let pixels: Vec<&[f32]> = image.data().chunks(3).collect();
        let glass = pixels
            .iter()
            .filter(|p| p.iter().all(|v| *v >= 0.99 * clear))
            .count();
        let n = pixels.len() as f64;
        let mean = [0, 1, 2].map(|c| pixels.iter().map(|p| f64::from(p[c])).sum::<f64>() / n);
        (glass as f64 / n, mean)
    };
    let (edge, (share, mean)) = (0..200)
        .map(|k| island.x + 100.0 * f64::from(k))
        .map(|x| (x, look(x)))
        .find(|(_, (share, _))| *share >= 0.25)
        .expect("an edge");
    // The field's mean isn't the light's colour: averaged over every cell,
    // white balance would be off.
    assert!(
        share < 0.75 && mean[1] / mean[0] < 0.95,
        "{share} glass, mean {mean:?}"
    );
    let setup = Setup {
        off: 0.0,
        region: None,
        ..Setup::new(Some(tissue), Point2::new(edge, island.y), "transmitted")
    };
    let bench = bench(setup).await;
    let whole = Region {
        x: 0,
        y: 0,
        width: 1368,
        height: 912,
    };
    let options = WhiteBalanceOptions {
        region: Some(whole),
        ..WhiteBalanceOptions::default()
    };
    let report = white_balance(&bench.rig, &options, &Quiet).await.unwrap();
    let [r, _, b] = report.gains;
    assert!(
        (r - 1.0).abs() < 0.01 && (b - 1.0).abs() < 0.01,
        "{report:?}"
    );
}
