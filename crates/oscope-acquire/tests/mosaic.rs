//! The live mosaic (ADR-0026): free roaming's gates, a scan's frames drawn
//! ungated, and frames drawn while the stage moves, its reports 40 ms late,
//! lying where the simulator's slide does.

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use oscope_acquire::mosaic::{Gates, LiveMosaic, LookSource, MosaicSetup, Verdict};
use oscope_acquire::preview::{Look, Preview};
use oscope_acquire::{Acquisition, FrameBus};
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

/// A 128 × 96 grey frame through the 10× at `binning`, at `at` µm,
/// exposed for `exposure` ms from `start` ms: a pattern of `contrast`.
fn frame(
    at: Option<(f64, f64)>,
    (start, exposure): (u64, f64),
    contrast: f64,
    binning: u32,
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
    meta.objective = Some("10x".to_owned());
    meta.position = at.map(|(x, y)| Xyz::new(Um(x), Um(y), Um(0.0)));
    Frame::new(buffer, meta).unwrap()
}

fn at(ms: u64, x: f64) -> PositionSample {
    PositionSample {
        time: Timestamp(ms * 1_000_000),
        position: Xyz::new(Um(x), Um(500.0), Um(0.0)),
    }
}

#[tokio::test]
async fn roaming_draws_new_places_and_sharper_views_unblurred() {
    let folder = tempfile::tempdir().unwrap();
    let setup = MosaicSetup {
        geometry: CanvasGeometry::covering((0.0, 0.0), (2000.0, 2000.0), 1.0).unwrap(),
        blend: Blend::Newest,
        objective: Some("10x".to_owned()),
        binning: 1,
        matrix: [[1.0, 0.0], [0.0, 1.0]],
        sensor: (128, 96),
        gates: Gates::default(),
        budget: 64 * TILE_BYTES,
        folder: folder.path().to_owned(),
    };
    let mosaic = LiveMosaic::start(setup, preview(), look(), None).unwrap();
    // Still until 200 ms, then 1 µm a ms.
    let track = PositionTrack::new(16);
    for sample in [at(0, 500.0), at(200, 500.0), at(300, 600.0)] {
        track.push(sample);
    }
    let roam = |at, exposure, contrast| mosaic.roam(&frame(at, exposure, contrast, 1), &track);
    // A new place; the same place, no sharper, then, with nothing moved,
    // the same unmeasured, and moved, sharper; another.
    assert_eq!(roam(Some((500.0, 500.0)), (10, 1.0), 0.8), Verdict::Drawn);
    assert_eq!(roam(Some((510.0, 500.0)), (20, 1.0), 0.5), Verdict::Same);
    assert_eq!(roam(Some((510.0, 500.0)), (25, 1.0), 1.0), Verdict::Same);
    assert_eq!(roam(Some((505.0, 505.0)), (30, 1.0), 1.0), Verdict::Drawn);
    assert_eq!(roam(Some((800.0, 500.0)), (40, 1.0), 0.5), Verdict::Drawn);
    // Moving: 5 µm in a 5 ms exposure, then 0.4 µm in 0.4 ms.
    assert_eq!(
        roam(Some((550.0, 500.0)), (240, 5.0), 1.0),
        Verdict::Blurred
    );
    assert_eq!(roam(Some((1100.0, 500.0)), (240, 0.4), 1.0), Verdict::Drawn);
    // No position, or none the track reaches; another binning; off the
    // canvas.
    assert_eq!(roam(None, (40, 1.0), 1.0), Verdict::Unplaced);
    assert_eq!(
        roam(Some((900.0, 900.0)), (400, 1.0), 1.0),
        Verdict::Unplaced
    );
    let binned = frame(Some((1500.0, 1500.0)), (40, 1.0), 1.0, 2);
    assert_eq!(mosaic.roam(&binned, &track), Verdict::Other);
    assert_eq!(roam(Some((-5000.0, 0.0)), (40, 1.0), 1.0), Verdict::Outside);
    // A scan's frames are drawn ungated: the blurry view where the sharp
    // one is, and one waiting in turn with the scan's position.
    let blurry = frame(Some((500.0, 500.0)), (50, 1.0), 0.2, 1);
    assert_eq!(mosaic.offer(&blurry, None), Verdict::Drawn);
    let placed = Xyz::new(Um(1500.0), Um(1500.0), Um(0.0));
    mosaic.scan_frame(frame(None, (60, 1.0), 1.0, 1), placed);
    let deadline = Instant::now() + Duration::from_secs(30);
    while mosaic.counts().drawn < 6 {
        assert!(Instant::now() < deadline, "{:?}", mosaic.counts());
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let counts = mosaic.counts();
    let tally = [counts.drawn, counts.scans, counts.same, counts.blurred];
    assert_eq!(tally, [6, 2, 2, 1], "{counts:?}");
    assert_eq!(counts.unplaced, 2);
    assert_eq!([counts.other, counts.outside, counts.failed], [1, 1, 0]);
    // Stopped, it takes no more.
    mosaic.stop();
    mosaic.scan_frame(frame(Some((300.0, 300.0)), (70, 1.0), 1.0, 1), placed);
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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn frames_drawn_as_the_stage_moves_lie_where_the_slide_does() {
    // The H&E slide through the quarter sensor at twice real time, the
    // stage's reports 40 ms late.
    let config = SimulatorConfig {
        rate: 2.0,
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
    // Pixels of 2 µm over 5 × 2 mm about the path, through the nominal M.
    let pixel = capabilities.pixel_size.0 / 10.0;
    let corner = (island.x - 2000.0, island.y - 1000.0);
    let folder = tempfile::tempdir().unwrap();
    let setup = MosaicSetup {
        geometry: CanvasGeometry::covering(corner, (5000.0, 2000.0), 2.0).unwrap(),
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
    // 1 × 0.25 mm about the path's middle, which only frames on the way
    // saw: a frame is 1.3 mm wide, and the first was 1.25 mm before it. It
    // is in the frames' upper half, where the rolling shutter exposed rows
    // some 17 ms before the middle row's.
    let middle = (island.x + 250.0, island.y - 250.0);
    let (u, v) = ((middle.0 - corner.0) / 2.0, (middle.1 - corner.1) / 2.0);
    let (drawn, seen) = region(&mosaic, (u as u32 - 256, v as u32 - 64), (512, 128));
    assert!(seen > 512 * 128 * 99 / 100, "{seen} pixels drawn");
    let optics = Optics {
        na: 0.25,
        magnification: 1.0,
        sensor_pixel: Um(2.0),
        wavelength: Um(0.55),
        medium: 1.0,
    };
    let field = Field {
        centre: Point2::new(middle.0, middle.1),
        z,
        width: 512,
        height: 128,
    };
    let truth = world
        .read()
        .unwrap()
        .render(&field, &optics, Light::Transmitted);
    let grey: Vec<f32> = truth
        .data()
        .chunks_exact(truth.channels())
        .map(|c| c.iter().sum::<f32>() / c.len() as f32)
        .collect();
    let truth = Image::from_vec(Shape::new(512, 128, 1), grey).unwrap();
    let found = register(&truth, &drawn, &RegisterOptions::default()).unwrap();
    let off_um = 2.0 * found.dx.hypot(found.dy);
    assert!(found.confidence > 0.2, "{found:?}");
    assert!(off_um < 2.0, "{off_um:.2} µm off the truth: {found:?}");
}
