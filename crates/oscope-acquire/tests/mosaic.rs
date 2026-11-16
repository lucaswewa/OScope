//! The live mosaic (ADR-0026): free roaming's gates, a scan's frames drawn
//! ungated, and frames drawn while the stage moves, its reports 40 ms late,
//! lying where the simulator's slide does.

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use oscope_acquire::mosaic::{Gates, LiveMosaic, LookSource, MosaicSetup, Verdict};
use oscope_acquire::preview::{Look, Preview};
use oscope_acquire::{Acquisition, FrameBus, LatestReceiver};
use oscope_compute::{Compute, ComputeConfig, Image, Shape};
use oscope_core::{
    Frame, FrameId, FrameMeta, FramePool, HeapAllocator, Ms, PixelFormat, PixelRect, Point2,
    Timestamp, Um, Xyz,
};
use oscope_hal::{Camera, PositionSample, PositionTrack, Stage, UmPerS};
use oscope_imaging::registration::{RegisterOptions, register};
use oscope_sim::optics::Optics;
use oscope_sim::world::{Field, Light};
use oscope_sim::{HeTissue, Simulator, SimulatorConfig, Slide};
use oscope_stitch::canvas::TILE_BYTES;
use oscope_stitch::{Blend, CanvasGeometry, TILE, TileKey};

fn preview() -> Preview {
    let config = ComputeConfig {
        live_threads: Some(2),
        batch_threads: Some(1),
        cuda_device: None,
    };
    Preview::new(Arc::new(Compute::new(&config).unwrap()))
}

fn look() -> LookSource {
    Arc::new(|| (Look::default(), false))
}

/// The 10× at no binning.
const TEN: (u32, &str) = (1, "10x");

/// A 128 × 96 grey frame through `lens`'s objective at its binning, at
/// `at` µm, exposed for `exposure` ms from `start` ms: a pattern of
/// `contrast`.
fn frame(
    at: Option<(f64, f64)>,
    (start, exposure): (u64, f64),
    contrast: f64,
    (binning, objective): (u32, &str),
) -> Frame {
    let (w, h) = (128, 96);
    let pool = FramePool::new(PixelFormat::Mono8.frame_bytes(w, h), 1, &HeapAllocator);
    let mut buffer = pool.take().unwrap();
    for (i, v) in buffer.bytes_mut().iter_mut().enumerate() {
        let (x, y) = (f64::from(i as u32 % w), f64::from(i as u32 / w));
        *v = (127.0 + 120.0 * contrast * (0.9 * x).sin() * (0.7 * y).sin()).round() as u8;
    }
    let mut meta = FrameMeta::new(FrameId(start), PixelFormat::Mono8, w, h, Timestamp(0));
    meta.roi = PixelRect {
        x: 0,
        y: 0,
        width: w * binning,
        height: h * binning,
    };
    meta.binning = binning;
    meta.exposure_start = Timestamp(start * 1_000_000);
    meta.exposure = Ms(exposure);
    meta.objective = Some(objective.to_owned());
    meta.position = at.map(|(x, y)| Xyz::new(Um(x), Um(y), Um(0.0)));
    Frame::new(buffer, meta).unwrap()
}

fn at(ms: u64, x: f64) -> PositionSample {
    PositionSample {
        time: Timestamp(ms * 1_000_000),
        position: Xyz::new(Um(x), Um(500.0), Um(0.0)),
    }
}

/// Frames of 1 µm pixels on a canvas of 0.5 µm ones.
fn setup(folder: &Path) -> MosaicSetup {
    MosaicSetup {
        geometry: CanvasGeometry::covering((0.0, 0.0), (2000.0, 2000.0), 0.5).unwrap(),
        blend: Blend::Newest,
        objective: Some("10x".to_owned()),
        binning: 1,
        matrix: [[1.0, 0.0], [0.0, 1.0]],
        sensor: (128, 96),
        gates: Gates::default(),
        budget: 64 * TILE_BYTES,
        folder: folder.to_owned(),
    }
}

#[tokio::test]
async fn roaming_draws_new_places_and_sharper_views_unblurred() {
    let folder = tempfile::tempdir().unwrap();
    // Gates and a matrix that can't be.
    let mut wrong = setup(folder.path());
    wrong.gates.blur_px = 0.0;
    assert!(LiveMosaic::start(wrong, preview(), look(), None).is_err());
    let mut wrong = setup(folder.path());
    wrong.matrix = [[1.0, 2.0], [2.0, 4.0]];
    assert!(LiveMosaic::start(wrong, preview(), look(), None).is_err());
    let mosaic = LiveMosaic::start(setup(folder.path()), preview(), look(), None).unwrap();
    // Still until 200 ms, then 1 µm a ms.
    let track = PositionTrack::new(16);
    for sample in [at(0, 500.0), at(200, 500.0), at(300, 600.0)] {
        track.push(sample);
    }
    let roam = |at, exposure, contrast| mosaic.roam(&frame(at, exposure, contrast, TEN), &track);
    // A new place. The same place, within a quarter of 96 µm: no sharper;
    // with nothing moved, the same unmeasured; 5 % sharper, not enough; and
    // sharper by half. Another place.
    assert_eq!(roam(Some((500.0, 500.0)), (10, 1.0), 0.8), Verdict::Drawn);
    assert_eq!(roam(Some((518.0, 500.0)), (20, 1.0), 0.5), Verdict::Same);
    assert_eq!(roam(Some((518.0, 500.0)), (25, 1.0), 1.0), Verdict::Same);
    assert_eq!(roam(Some((505.0, 500.0)), (27, 1.0), 0.82), Verdict::Same);
    assert_eq!(roam(Some((505.0, 505.0)), (30, 1.0), 1.0), Verdict::Drawn);
    assert_eq!(roam(Some((800.0, 500.0)), (40, 1.0), 0.5), Verdict::Drawn);
    // Moving: 5 µm in a 5 ms exposure, then 0.4 µm in 0.4 ms.
    let moving = roam(Some((550.0, 500.0)), (240, 5.0), 1.0);
    assert_eq!(moving, Verdict::Blurred);
    assert_eq!(roam(Some((1100.0, 500.0)), (240, 0.4), 1.0), Verdict::Drawn);
    // No position, or none the track reaches; another binning, another
    // objective; off the canvas.
    assert_eq!(roam(None, (40, 1.0), 1.0), Verdict::Unplaced);
    let late = roam(Some((900.0, 900.0)), (400, 1.0), 1.0);
    assert_eq!(late, Verdict::Unplaced);
    for lens in [(2, "10x"), (1, "20x")] {
        let other = frame(Some((1500.0, 1500.0)), (40, 1.0), 1.0, lens);
        assert_eq!(mosaic.roam(&other, &track), Verdict::Other);
    }
    assert_eq!(roam(Some((-5000.0, 0.0)), (40, 1.0), 1.0), Verdict::Outside);
    // Cleared, it forgets where it drew.
    mosaic.clear().unwrap();
    assert_eq!(roam(Some((500.0, 500.0)), (45, 1.0), 0.5), Verdict::Drawn);
    // A scan's frames are drawn ungated: the blurry view where one is, and
    // those waiting in turn with the scan's position, up to 8.
    let blurry = frame(Some((500.0, 500.0)), (50, 1.0), 0.2, TEN);
    assert_eq!(mosaic.offer(&blurry, None), Verdict::Drawn);
    let placed = Xyz::new(Um(1500.0), Um(1500.0), Um(0.0));
    let drawn = async |frames: u64| {
        let deadline = Instant::now() + Duration::from_secs(30);
        while mosaic.counts().scans < frames {
            assert!(Instant::now() < deadline, "{:?}", mosaic.counts());
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    };
    assert!(mosaic.scan_frame(frame(None, (60, 1.0), 1.0, TEN), placed));
    drawn(2).await;
    for k in 0..12 {
        let taken = mosaic.scan_frame(frame(None, (61 + k, 1.0), 1.0, TEN), placed);
        assert_eq!(taken, k < 8, "frame {k}");
    }
    drawn(10).await;
    let counts = mosaic.counts();
    let tally = [counts.drawn, counts.scans, counts.dropped, counts.same];
    assert_eq!(tally, [15, 10, 4, 3], "{counts:?}");
    let refused = [
        counts.blurred,
        counts.unplaced,
        counts.other,
        counts.outside,
    ];
    assert_eq!((refused, counts.failed), ([1, 2, 2, 1], 0), "{counts:?}");
    // Stopped, it takes no more.
    mosaic.stop();
    let late = frame(Some((300.0, 300.0)), (90, 1.0), 1.0, TEN);
    assert!(!mosaic.scan_frame(late, placed));
    assert_eq!(mosaic.counts(), counts);
}

/// Level 0's pixels `x`–`x + w` by `y`–`y + h`, grey from 0 to 1, and how
/// many were drawn.
fn region(mosaic: &LiveMosaic, (x, y): (u32, u32), (w, h): (u32, u32)) -> (Image, usize) {
    let (mut grey, mut drawn) = (Vec::new(), 0);
    let mut tiles = std::collections::HashMap::new();
    for v in y..y + h {
        for u in x..x + w {
            let key = TileKey {
                level: 0,
                col: u / TILE,
                row: v / TILE,
            };
            let tile = tiles
                .entry(key)
                .or_insert_with(|| mosaic.tile(key).unwrap().unwrap_or_default());
            let at = (((v % TILE) * TILE + u % TILE) * 4) as usize;
            let pixel = tile.get(at..at + 4).unwrap_or(&[0; 4]);
            drawn += usize::from(pixel[3] == 255);
            let sum: u32 = pixel[..3].iter().map(|&c| u32::from(c)).sum();
            grey.push(sum as f32 / 765.0);
        }
    }
    (Image::from_vec(Shape::new(w, h, 1), grey).unwrap(), drawn)
}

/// Waits for a frame taken with the stage within 5 µm of `at`: as it
/// reports, its true position, a backlash's half from where it was sent.
async fn frame_at(frames: &mut LatestReceiver, at: Xyz) {
    loop {
        let frame = frames.recv().await.unwrap();
        let here = frame
            .meta()
            .position
            .unwrap_or(Xyz::new(Um(f64::MAX), Um(0.0), Um(0.0)));
        if (here.x.0 - at.x.0).hypot(here.y.0 - at.y.0) < 5.0 {
            return;
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn frames_drawn_as_the_stage_moves_lie_where_the_slide_does() {
    // The H&E slide through the quarter sensor at real time, the stage's
    // reports 40 ms late.
    let config = SimulatorConfig {
        sensor: "quarter".to_owned(),
        latency: 40.0,
        ..SimulatorConfig::default()
    };
    let sim = Simulator::new(&config, Path::new(".")).await.unwrap();
    let running = Arc::clone(&sim.stage);
    tokio::spawn(async move { running.run().await });
    sim.stage.home().await.unwrap();
    let island = HeTissue::new(1, Slide::COVERSLIP).islands()[0];
    let world = sim.camera.world();
    let z = world.read().unwrap().in_focus(island).unwrap();
    let start = Xyz::new(Um(island.x - 1000.0), Um(island.y), z);
    sim.stage.move_to(start, None).await.unwrap();
    // 2 ms frames, a gain making up the light, each at its exposure's
    // midpoint by the track.
    let bus = Arc::new(FrameBus::new());
    let acquisition = Acquisition::new(Arc::clone(&bus), Duration::from_millis(200));
    acquisition.set_track(Some(sim.stage.track()));
    acquisition.set_objective(Some("10x".to_owned()));
    let mut settings = sim.camera.settings().await.unwrap();
    (settings.exposure, settings.gain) = (Ms(2.0), 175.0);
    let settings = sim.camera.configure(settings).await.unwrap();
    let capabilities = sim.camera.capabilities();
    let (_, w, h) = capabilities.frame_shape(&settings).unwrap();
    let pool = FramePool::new(settings.format.frame_bytes(w, h), 8, &HeapAllocator);
    sim.camera.start(pool, acquisition.sink()).await.unwrap();
    // The path rendered once, a frame every 0.625 mm with the stage still,
    // so that the frames on the way come as fast as the camera takes them:
    // a new place takes the simulator seconds to render on two cores.
    let mut frames = bus.subscribe_latest("the path");
    for k in 0..=4 {
        let at = Xyz::new(Um(start.x.0 + 625.0 * f64::from(k)), start.y, z);
        sim.stage.move_to(at, None).await.unwrap();
        frame_at(&mut frames, at).await;
    }
    sim.stage.move_to(start, None).await.unwrap();
    frame_at(&mut frames, start).await;
    // Pixels of 4 µm over 5 × 2 mm about the path, through the nominal M.
    let pixel = capabilities.pixel_size.0 / 10.0;
    let corner = (island.x - 2000.0, island.y - 1000.0);
    let folder = tempfile::tempdir().unwrap();
    let setup = MosaicSetup {
        geometry: CanvasGeometry::covering(corner, (5000.0, 2000.0), 4.0).unwrap(),
        blend: Blend::Newest,
        objective: Some("10x".to_owned()),
        binning: 1,
        matrix: [[pixel, 0.0], [0.0, pixel]],
        sensor: (capabilities.sensor_width, capabilities.sensor_height),
        gates: Gates::default(),
        budget: 256 * TILE_BYTES,
        folder: folder.path().to_owned(),
    };
    let roam = Some((bus.subscribe_latest("mosaic"), sim.stage.track()));
    let mosaic = LiveMosaic::start(setup, preview(), look(), roam).unwrap();
    // 2.5 mm along x at 0.4 mm/s, drawn on the way.
    let end = Xyz::new(Um(island.x + 1500.0), Um(island.y), z);
    sim.stage.move_to(end, Some(UmPerS(400.0))).await.unwrap();
    mosaic.stop();
    sim.camera.stop().await.unwrap();
    let counts = mosaic.counts();
    assert!(counts.drawn >= 4 && counts.failed == 0, "{counts:?}");
    // 1 × 0.25 mm about the path's middle, some 2.25 and 0.75 mm from the
    // canvas's corner, which only frames on the way saw: a frame is 1.3 mm
    // wide, and the first was 1.25 mm before it. It is in the frames' upper
    // half, where the rolling shutter exposed rows some 17 ms before the
    // middle row's.
    let near = (562_u32, 187_u32);
    let (drawn, seen) = region(&mosaic, (near.0 - 128, near.1 - 32), (256, 64));
    assert!(seen > 256 * 64 * 99 / 100, "{seen} pixels drawn");
    // The region's own middle, on the stage: whole pixels from the corner.
    let middle = Point2::new(
        corner.0 + 4.0 * f64::from(near.0),
        corner.1 + 4.0 * f64::from(near.1),
    );
    // The truth, rendered at 1 µm and averaged in fours, so that each 4 µm
    // pixel is the mean of its square, as the canvas's is.
    let optics = Optics {
        na: 0.25,
        magnification: 1.0,
        sensor_pixel: Um(1.0),
        wavelength: Um(0.55),
        medium: 1.0,
    };
    let field = Field {
        centre: middle,
        z,
        width: 1024,
        height: 256,
    };
    let fine = world
        .read()
        .unwrap()
        .render(&field, &optics, Light::Transmitted);
    let truth = oscope_compute::cpu::downsample(&fine, 4).unwrap();
    let grey: Vec<f32> = truth
        .data()
        .chunks_exact(truth.channels())
        .map(|c| c.iter().sum::<f32>() / c.len() as f32)
        .collect();
    let truth = Image::from_vec(Shape::new(256, 64, 1), grey).unwrap();
    let found = register(&truth, &drawn, &RegisterOptions::default()).unwrap();
    let off_um = 4.0 * found.dx.hypot(found.dy);
    assert!(found.confidence > 0.2, "{found:?}");
    assert!(off_um < 2.0, "{off_um:.2} µm off the truth: {found:?}");
}
