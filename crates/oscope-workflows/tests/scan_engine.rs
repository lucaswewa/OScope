//! The scan engine (P36; ADR-0025) on the simulator: a scan recording every
//! frame with where the stage was, ingested with its plan; a stack's planes
//! from the lowest; autofocus at every n-th tile; a cancel keeping the
//! frames, in scratch; a step that fails tried again, and the scan failing
//! after three tries; every scan ending where it started; and plans that
//! can't be refused.

use std::path::Path;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, PoisonError};
use std::time::{Duration, Instant};

use oscope_acquire::{Acquisition, FrameBus};
use oscope_core::{FramePool, HeapAllocator, JobId, PixelRect, Point2, ProjectId, Rect, Um, Xyz};
use oscope_hal::{
    Axis, Camera, HalFuture, Illumination, ObjectiveMount, PositionSample, PositionTrack, Stage,
    StageCapabilities, StageStatus, UmPerS,
};
use oscope_jobs::{JobService, JobState, JobsConfig};
use oscope_sim::{HeTissue, SimStage, Simulator, SimulatorConfig, Slide};
use oscope_store::{NewProject, Scratch, ScratchPolicy, Store};
use oscope_workflows::autofocus::{FocusRig, focus_defaults};
use oscope_workflows::scan::{
    Field, FitOptions, FocusMap, FocusPoint, LastScan, Motion, Plan, Position, Region, SCAN,
    ScanRig, Stack, Surface, ZPolicy, positions, scan_jobs, snake, tiled_stack,
};
use serde_json::{Value, json};
use tokio::sync::Mutex;

/// One simulator at a time.
static TURN: Mutex<()> = Mutex::const_new(());

/// A 256 × 192 region of the quarter sensor's middle: quick to render.
const SMALL: PixelRect = PixelRect::new(556, 360, 256, 192);

/// A stage that fails its next moves, as many as it is told.
struct Flaky {
    stage: Arc<SimStage>,
    failures: AtomicU32,
}

impl Stage for Flaky {
    fn capabilities(&self) -> &StageCapabilities {
        self.stage.capabilities()
    }
    fn status(&self) -> HalFuture<'_, StageStatus> {
        self.stage.status()
    }
    fn position(&self) -> HalFuture<'_, PositionSample> {
        self.stage.position()
    }
    fn track(&self) -> Arc<PositionTrack> {
        self.stage.track()
    }
    fn move_to(&self, target: Xyz, speed: Option<UmPerS>) -> HalFuture<'_, PositionSample> {
        let failing = self
            .failures
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_sub(1));
        match failing {
            Ok(_) => Box::pin(async {
                Err(oscope_hal::Error::Unsupported {
                    what: "a move, this once".to_owned(),
                })
            }),
            Err(_) => self.stage.move_to(target, speed),
        }
    }
    fn jog(&self, axis: Axis, towards_max: bool, speed: Option<UmPerS>) -> HalFuture<'_, ()> {
        self.stage.jog(axis, towards_max, speed)
    }
    fn stop(&self) -> HalFuture<'_, ()> {
        self.stage.stop()
    }
    fn clear_alarm(&self) -> HalFuture<'_, ()> {
        self.stage.clear_alarm()
    }
}

struct Bench {
    sim: Simulator,
    rig: FocusRig,
    stage: Arc<Flaky>,
    jobs: JobService,
    store: Arc<Store>,
    scratch: Arc<Scratch>,
    project: ProjectId,
    last: LastScan,
    /// The first island's middle, where the bench starts.
    island: Point2,
    /// A frame's field, through the region.
    field: Field,
    _acquisition: Acquisition,
    _folder: tempfile::TempDir,
}

/// The H&E slide tilted 1 µm a mm along x and −0.5 along y, the quarter
/// sensor through a small region, ten times real time; the stage at the
/// first island's focus.
async fn bench() -> Bench {
    let config = SimulatorConfig {
        rate: 10.0,
        sensor: "quarter".to_owned(),
        focus: 1_000.0,
        tilt: [1.0, -0.5],
        ..SimulatorConfig::default()
    };
    let sim = Simulator::new(&config, Path::new(".")).await.unwrap();
    let running = Arc::clone(&sim.stage);
    tokio::spawn(async move { running.run().await });
    sim.stage.home().await.unwrap();
    let island = HeTissue::new(1, Slide::COVERSLIP).islands()[0];
    let z = sim
        .camera
        .world()
        .read()
        .unwrap()
        .in_focus(island)
        .unwrap()
        .0;
    let start = Xyz::new(Um(island.x), Um(island.y), Um(z));
    sim.stage.move_to(start, None).await.unwrap();
    let bus = Arc::new(FrameBus::new());
    let acquisition = Acquisition::new(Arc::clone(&bus), Duration::from_millis(50));
    let mut settings = sim.camera.settings().await.unwrap();
    settings.roi = Some(SMALL);
    let settings = sim.camera.configure(settings).await.unwrap();
    let (_, w, h) = sim.camera.capabilities().frame_shape(&settings).unwrap();
    let pool = FramePool::new(settings.format.frame_bytes(w, h), 8, &HeapAllocator);
    sim.camera.start(pool, acquisition.sink()).await.unwrap();
    let stage = Arc::new(Flaky {
        stage: Arc::clone(&sim.stage),
        failures: AtomicU32::new(0),
    });
    let focus = FocusRig {
        stage: Arc::clone(&stage) as Arc<dyn Stage>,
        camera: Arc::clone(&sim.camera) as Arc<dyn Camera>,
        objectives: Arc::clone(&sim.mount) as Arc<dyn ObjectiveMount>,
        bus,
        lights: Arc::clone(&sim.lights) as Arc<dyn Illumination>,
        compute: None,
    };
    let pixel = sim.camera.capabilities().pixel_size.0 / 10.0;
    let folder = tempfile::tempdir().unwrap();
    let data = folder.path().join("data");
    let store = Arc::new(Store::open(&data, &data.join("index.sqlite")).unwrap());
    let scratch =
        Arc::new(Scratch::open(&folder.path().join("scratch"), ScratchPolicy::default()).unwrap());
    let project = store
        .create(NewProject {
            name: "scans".to_owned(),
            ..NewProject::default()
        })
        .unwrap()
        .id;
    let jobs = JobService::open(&folder.path().join("jobs.sqlite"), JobsConfig::default()).unwrap();
    let last = LastScan::default();
    let rig = ScanRig {
        focus: focus.clone(),
        store: Arc::clone(&store),
        scratch: Arc::clone(&scratch),
        tiles: None,
    };
    jobs.register(SCAN, scan_jobs(rig, Arc::clone(&last)));
    Bench {
        sim,
        rig: focus,
        stage,
        jobs,
        store,
        scratch,
        project,
        last,
        island,
        // The region's width is kept a multiple of 12, for the mosaic.
        field: Field::nominal(pixel, 252, 192, 0.1).unwrap(),
        _acquisition: acquisition,
        _folder: folder,
    }
}

impl Bench {
    fn truth(&self, at: Point2) -> f64 {
        let world = self.sim.camera.world();
        let world = world.read().unwrap_or_else(PoisonError::into_inner);
        world.in_focus(at).unwrap().0
    }

    /// A square of `side` µm at the island's middle.
    fn square(&self, side: f64) -> Region {
        let (x, y) = (self.island.x - side / 2.0, self.island.y - side / 2.0);
        Region::Rect(Rect::new(x, y, side, side))
    }

    /// The slide's plane, from the truth at three points about the island.
    fn map(&self) -> ZPolicy {
        let points: Vec<FocusPoint> = [(-500.0, -500.0), (500.0, -500.0), (0.0, 500.0)]
            .iter()
            .map(|&(dx, dy)| {
                let at = Point2::new(self.island.x + dx, self.island.y + dy);
                FocusPoint {
                    x_um: at.x,
                    y_um: at.y,
                    z_um: self.truth(at) - 2.0,
                }
            })
            .collect();
        let map = FocusMap::fit(&points, Surface::Plane, &FitOptions::default()).unwrap();
        ZPolicy::Map { map }
    }

    fn submit(&self, plan: &Plan) -> JobId {
        let params = json!({"plan": plan, "project": self.project, "name": "a scan"});
        self.jobs.submit_kind(SCAN, params, None).unwrap()
    }

    /// Waits for job `id` to end: its state.
    async fn ended(&self, id: JobId) -> JobState {
        let deadline = Instant::now() + Duration::from_secs(600);
        loop {
            let state = self.jobs.get(id).unwrap().state;
            if state.is_finished() {
                return state;
            }
            assert!(Instant::now() < deadline, "the scan didn't end");
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    async fn here(&self) -> Xyz {
        self.sim.stage.position().await.unwrap().position
    }

    async fn stop(self) {
        self.sim.camera.stop().await.unwrap();
    }
}

fn near(a: Xyz, b: Xyz, xy: f64, z: f64) -> bool {
    (a.x.0 - b.x.0).abs() < xy && (a.y.0 - b.y.0).abs() < xy && (a.z.0 - b.z.0).abs() < z
}

#[tokio::test(flavor = "multi_thread")]
async fn a_scan_records_each_frame_where_the_stage_was_and_ends_where_it_started() {
    let _turn = TURN.lock().await;
    let bench = bench().await;
    let start = bench.here().await;
    // A snake of 3 × 3 tiles, by the slide's plane, three planes 2 µm apart
    // at each.
    let stack = Stack {
        below_um: 2.0,
        above_um: 2.0,
        step_um: 2.0,
    };
    let plan = tiled_stack(&bench.square(500.0), &bench.field, bench.map(), stack).unwrap();
    assert_eq!(plan.tiles.len(), 9);
    let id = bench.submit(&plan);
    assert_eq!(
        bench.ended(id).await,
        JobState::Completed,
        "{:?}",
        bench.jobs.get(id).unwrap().error
    );
    let info = bench.jobs.get(id).unwrap();
    assert_eq!(
        (
            info.progress.counters["tiles"],
            info.progress.counters["frames"]
        ),
        (9, 27)
    );
    assert!(
        near(bench.here().await, start, 2.5, 0.3),
        "{:?}",
        bench.here().await
    );

    // One acquisition: each frame where its tile's plane is, in order, and
    // the plan.
    let contents = bench.store.contents(bench.project).unwrap();
    let [acquisition] = &contents.acquisitions[..] else {
        panic!("{} acquisitions", contents.acquisitions.len());
    };
    assert!(
        info.results
            .iter()
            .any(|r| r.ends_with(&acquisition.id.to_string()))
    );
    assert_eq!(
        (acquisition.name.as_str(), acquisition.image.planes),
        ("a scan", 27)
    );
    assert_eq!(acquisition.plan["tiles"].as_array().unwrap().len(), 9);
    let mut lags = Vec::new();
    for (k, frame) in acquisition.frames.iter().enumerate() {
        let (tile, plane) = (&plan.tiles[k / 3], (k % 3) as f64);
        let at = frame.position.unwrap();
        let expected = Xyz::new(
            Um(tile.x_um),
            Um(tile.y_um),
            Um(tile.z_um.unwrap() - 2.0 + 2.0 * plane),
        );
        assert!(
            near(at, expected, 2.5, 1.5),
            "frame {k}: {at:?}, not {expected:?}"
        );
        assert!(frame.timestamp.is_some() && frame.trigger.is_none());
        lags.push(expected.z.0 - at.z.0);
    }
    // Every plane reached moving up: z's backlash shifts each alike.
    let (low, high) = lags
        .iter()
        .fold((f64::MAX, f64::MIN), |(l, h), v| (l.min(*v), h.max(*v)));
    assert!(high - low < 0.5, "lags from {low} to {high} µm");
    let progress = bench.last.lock().unwrap().clone().unwrap();
    assert_eq!(
        (progress.state.as_str(), progress.done.len()),
        ("completed", 9)
    );
    // Nothing left in scratch.
    assert!(bench.scratch.recordings().unwrap().is_empty());
    bench.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn autofocus_at_every_second_tile_finds_the_tilted_slide() {
    let _turn = TURN.lock().await;
    let bench = bench().await;
    let quarter = focus_defaults(&bench.rig).await.unwrap().depth_of_field_um / 4.0;
    // Four tiles 1 mm apart in x: the slide 1 µm higher at each.
    let at = |dx: f64| Position {
        x_um: bench.island.x + dx,
        y_um: bench.island.y,
        z_um: None,
        name: None,
    };
    // And clear glass at the coverslip's corner, fifth: refused as an
    // empty field, the last z found kept.
    let glass = Position {
        x_um: 22_100.0,
        y_um: 600.0,
        ..at(0.0)
    };
    let list = [at(-1_500.0), at(-500.0), at(500.0), at(1_500.0), glass];
    let plan = positions(&list, &bench.field, ZPolicy::Autofocus { every: 2 }, None).unwrap();
    let id = bench.submit(&plan);
    assert_eq!(
        bench.ended(id).await,
        JobState::Completed,
        "{:?}",
        bench.jobs.get(id).unwrap().error
    );
    let progress = bench.last.lock().unwrap().clone().unwrap();
    for done in progress.done.iter().take(4).step_by(2) {
        let truth = bench.truth(Point2::new(done.x_um, done.y_um));
        assert!(
            (done.z_um - truth).abs() < quarter,
            "{done:?}: the truth {truth}"
        );
    }
    // The tiles between keep the last z found, and so does the glass.
    assert_eq!(progress.done[1].z_um, progress.done[0].z_um);
    assert_eq!(progress.done[4].z_um, progress.done[3].z_um);
    assert_eq!(progress.refused, 1);
    bench.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_cancel_keeps_the_frames_in_scratch_and_the_stage_returns() {
    let _turn = TURN.lock().await;
    let bench = bench().await;
    let start = bench.here().await;
    let plan = snake(&bench.square(1_200.0), &bench.field, bench.map()).unwrap();
    assert!(plan.tiles.len() >= 20, "{}", plan.tiles.len());
    let id = bench.submit(&plan);
    let deadline = Instant::now() + Duration::from_secs(300);
    while bench
        .jobs
        .get(id)
        .unwrap()
        .progress
        .counters
        .get("frames")
        .copied()
        .unwrap_or(0)
        < 3
    {
        assert!(Instant::now() < deadline, "no frames");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    bench.jobs.cancel(id).unwrap();
    assert_eq!(bench.ended(id).await, JobState::Cancelled);
    assert!(
        near(bench.here().await, start, 2.5, 0.3),
        "{:?}",
        bench.here().await
    );
    let recordings = bench.scratch.recordings().unwrap();
    let [recording] = &recordings[..] else {
        panic!("{} recordings", recordings.len());
    };
    assert!(recording.frames >= 3 && (recording.frames as usize) < plan.tiles.len());
    assert_eq!(recording.error.as_deref(), Some("cancelled"));
    assert!(
        bench
            .store
            .contents(bench.project)
            .unwrap()
            .acquisitions
            .is_empty()
    );
    let info = bench.jobs.get(id).unwrap();
    assert!(
        info.results
            .iter()
            .any(|r| r.contains(&recording.id.to_string()))
    );
    let state = bench.last.lock().unwrap().clone().unwrap().state;
    assert_eq!(state, "cancelled");
    bench.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_step_that_fails_is_tried_again_twice_before_the_scan_fails() {
    let _turn = TURN.lock().await;
    let bench = bench().await;
    let start = bench.here().await;
    let plan = snake(&bench.square(150.0), &bench.field, bench.map()).unwrap();
    // Two failures: the third try moves, and the scan completes.
    bench.stage.failures.store(2, Ordering::SeqCst);
    let id = bench.submit(&plan);
    assert_eq!(
        bench.ended(id).await,
        JobState::Completed,
        "{:?}",
        bench.jobs.get(id).unwrap().error
    );
    assert_eq!(bench.stage.failures.load(Ordering::SeqCst), 0);
    let here = bench.here().await;
    assert!(near(here, start, 2.5, 0.3), "{here:?}, not {start:?}");
    // Three: the scan fails, saying why, its frames none, and the stage
    // back where it started: within the half of its backlash that a scan
    // with no frames hasn't measured.
    bench.stage.failures.store(3, Ordering::SeqCst);
    let id = bench.submit(&plan);
    assert_eq!(bench.ended(id).await, JobState::Failed);
    let error = bench.jobs.get(id).unwrap().error.unwrap_or_default();
    assert!(
        error.contains("failed 3 times") && error.contains("a move, this once"),
        "{error}"
    );
    let here = bench.here().await;
    assert!(near(here, start, 2.5, 1.5), "{here:?}, not {start:?}");
    let recordings = bench.scratch.recordings().unwrap();
    assert_eq!(recordings.len(), 1);
    assert_eq!(recordings[0].frames, 0);
    bench.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn plans_that_cant_be_run_are_refused_at_submission() {
    let _turn = TURN.lock().await;
    let bench = bench().await;
    let good = snake(&bench.square(150.0), &bench.field, bench.map()).unwrap();
    let refused = |plan: &Plan, extra: Value| {
        let mut params = json!({"plan": plan, "project": bench.project});
        params
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        bench
            .jobs
            .submit_kind(SCAN, params, None)
            .unwrap_err()
            .to_string()
    };
    let continuous = Plan {
        motion: Motion::Continuous { speed_um_s: 500.0 },
        ..good.clone()
    };
    assert!(refused(&continuous, json!({})).contains("M3"));
    let empty = Plan {
        tiles: Vec::new(),
        ..good.clone()
    };
    assert!(refused(&empty, json!({})).contains("at least one"));
    let mut unknown = good.clone();
    unknown.tiles[0].z_um = None;
    assert!(refused(&unknown, json!({})).contains("autofocus"));
    let settle = json!({"options": {"settle_ms": -1.0}});
    assert!(refused(&good, settle).contains("settling"));
    bench.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_map_measured_before_the_first_tile_puts_each_at_its_focus() {
    let _turn = TURN.lock().await;
    let bench = bench().await;
    let quarter = focus_defaults(&bench.rig).await.unwrap().depth_of_field_um / 4.0;
    // A snake over 600 µm of the tilted slide, its z unknown till the map,
    // measured by autofocus at the tiles nearest a 2 × 2 grid over them:
    // three of its 3 × 4 tiles, as two middles are nearest the same.
    let measure = ZPolicy::Measure {
        columns: 2,
        rows: 2,
        surface: Surface::Plane,
    };
    let plan = snake(&bench.square(600.0), &bench.field, measure).unwrap();
    assert!(plan.tiles.iter().all(|t| t.z_um.is_none()));
    let id = bench.submit(&plan);
    assert_eq!(
        bench.ended(id).await,
        JobState::Completed,
        "{:?}",
        bench.jobs.get(id).unwrap().error
    );
    let info = bench.jobs.get(id).unwrap();
    assert!(
        info.results
            .iter()
            .any(|r| r.starts_with("a focus map of 3 points, 0 rejected, none to spare")),
        "{:?}",
        info.results
    );
    let progress = bench.last.lock().unwrap().clone().unwrap();
    let ZPolicy::Map { map } = &progress.plan.z else {
        panic!("{:?}", progress.plan.z);
    };
    for tile in &progress.plan.tiles {
        let z = map.z_at(Point2::new(tile.x_um, tile.y_um));
        assert_eq!(tile.z_um, Some(z));
    }
    assert_eq!(progress.done.len(), plan.tiles.len());
    for done in &progress.done {
        let truth = bench.truth(Point2::new(done.x_um, done.y_um));
        assert!(
            (done.z_um - truth).abs() < quarter,
            "{done:?}: the truth {truth}"
        );
    }
    let contents = bench.store.contents(bench.project).unwrap();
    assert_eq!(contents.acquisitions[0].plan["z"]["kind"], "map");
    bench.stop().await;
}
