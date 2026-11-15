//! The histological scan's overview and tissue detection (P37) on the
//! simulator: at least 99 % of the simulated tissue covered by fewer tiles
//! than a raster, and an empty slide giving no tissue, so no tiles.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use oscope_acquire::{Acquisition, FrameBus};
use oscope_core::{FramePool, HeapAllocator, Point2, Rect, Um, Xyz};
use oscope_hal::{Camera, Illumination, ObjectiveMount, Stage};
use oscope_sim::{HeTissue, Simulator, SimulatorConfig, Slide};
use oscope_workflows::autofocus::{ExposureOptions, FocusRig, auto_exposure};
use oscope_workflows::calibration::Quiet;
use oscope_workflows::scan::{
    Field, OverviewOptions, Region, ZPolicy, detect, overview, raster, snake,
};
use tokio::sync::Mutex;

/// One simulator at a time.
static TURN: Mutex<()> = Mutex::const_new(());

/// The quarter sensor binned 3 × 3, ten times as fast as real time, the
/// stage over `at`, at the slide's focus; with `sample` loaded, or none.
async fn bench(sample: &str, at: Point2) -> (Simulator, FocusRig, Acquisition) {
    let config = SimulatorConfig {
        rate: 10.0,
        sensor: "quarter".to_owned(),
        sample: sample.to_owned(),
        focus: 1_000.0,
        ..SimulatorConfig::default()
    };
    let sim = Simulator::new(&config, Path::new(".")).await.unwrap();
    let running = Arc::clone(&sim.stage);
    tokio::spawn(async move { running.run().await });
    sim.stage.home().await.unwrap();
    // The section's middle is 2 µm above the slide's surface.
    let start = Xyz::new(Um(at.x), Um(at.y), Um(1_002.0));
    sim.stage.move_to(start, None).await.unwrap();
    let bus = Arc::new(FrameBus::new());
    let acquisition = Acquisition::new(Arc::clone(&bus), Duration::from_millis(50));
    let mut settings = sim.camera.settings().await.unwrap();
    settings.mode = "bin3".into();
    let settings = sim.camera.configure(settings).await.unwrap();
    let (_, w, h) = sim.camera.capabilities().frame_shape(&settings).unwrap();
    let pool = FramePool::new(settings.format.frame_bytes(w, h), 8, &HeapAllocator);
    sim.camera.start(pool, acquisition.sink()).await.unwrap();
    let rig = FocusRig {
        stage: Arc::clone(&sim.stage) as Arc<dyn Stage>,
        camera: Arc::clone(&sim.camera) as Arc<dyn Camera>,
        objectives: Arc::clone(&sim.mount) as Arc<dyn ObjectiveMount>,
        bus,
        lights: Arc::clone(&sim.lights) as Arc<dyn Illumination>,
        compute: None,
    };
    // Binned 3 × 3, a pixel gathers nine times the light: the exposure set
    // for it, as P34 sets it.
    auto_exposure(&rig, &ExposureOptions::default(), &Quiet)
        .await
        .unwrap();
    (sim, rig, acquisition)
}

/// The frames binned 3 × 3: 456 × 304 pixels of 2.88 µm through the 10×,
/// the quarter sensor's whole field, with no overlap.
fn binned() -> Field {
    Field::nominal(2.88, 456, 304, 0.0).unwrap()
}

/// The scan's frames: the quarter sensor's whole field at full resolution,
/// a tenth shared with each neighbour.
fn full() -> Field {
    Field::nominal(0.96, 1_368, 912, 0.1).unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn the_tissue_found_is_covered_by_fewer_tiles_than_a_raster() {
    let _turn = TURN.lock().await;
    let tissue = HeTissue::new(1, Slide::COVERSLIP);
    let island = tissue.islands()[0];
    let (sim, rig, _acquisition) = bench("H&E tissue", island).await;
    // 16 × 12 mm about the first island.
    let region = Region::Rect(Rect::new(island.x - 8e3, island.y - 6e3, 16e3, 12e3));
    let options = OverviewOptions::default();
    let taken = overview(&rig, &region, &binned(), &options, &Quiet)
        .await
        .unwrap();
    assert_eq!((taken.width, taken.height), (320, 240));
    let found = detect(&taken, &options.tissue);
    let summary = (found.kept, found.area_mm2, found.threshold);
    assert!(found.kept >= 1 && found.area_mm2 > 10.0, "{summary:?}");
    // The mask itself, against the truth at each pixel's middle: it holds
    // 99 % of the tissue, where the overview put each frame's cells.
    let (mut pixels, mut held) = (0_u32, 0_u32);
    for (k, &on) in found.mask.cells.iter().enumerate() {
        let (i, j) = ((k % 320) as f64, (k / 320) as f64);
        let at = Point2::new(
            taken.origin.x + 25.0 + 50.0 * i,
            taken.origin.y + 25.0 + 50.0 * j,
        );
        if tissue.is_tissue(at) {
            pixels += 1;
            held += u32::from(on);
        }
    }
    let share = f64::from(held) / f64::from(pixels);
    assert!(
        share >= 0.99,
        "the mask holds {held} of {pixels} pixels of tissue"
    );

    let tiles = snake(
        &Region::Mask(found.mask.clone()),
        &full(),
        ZPolicy::Fixed { z_um: 1_002.0 },
    )
    .unwrap();
    let whole = raster(&region, &full(), ZPolicy::Fixed { z_um: 1_002.0 }).unwrap();
    assert!(
        tiles.tiles.len() < whole.tiles.len(),
        "{} of {}",
        tiles.tiles.len(),
        whole.tiles.len()
    );

    // The truth every 50 µm over the region: each point of tissue in some
    // tile's frame.
    let frames: Vec<Region> = tiles
        .tiles
        .iter()
        .map(|t| Region::Polygon {
            corners: tiles.field.frame_at(Point2::new(t.x_um, t.y_um)).to_vec(),
        })
        .collect();
    let (mut in_tissue, mut covered) = (0_u32, 0_u32);
    let b = region.bounds();
    for j in 0..240 {
        for i in 0..320 {
            let at = Point2::new(
                b.origin.x + 25.0 + 50.0 * f64::from(i),
                b.origin.y + 25.0 + 50.0 * f64::from(j),
            );
            if tissue.is_tissue(at) {
                in_tissue += 1;
                covered += u32::from(frames.iter().any(|f| f.contains(at)));
            }
        }
    }
    let share = f64::from(covered) / f64::from(in_tissue);
    assert!(
        in_tissue > 4_000 && share >= 0.99,
        "{covered} of {in_tissue}: {share:.4}"
    );
    eprintln!(
        "{covered} of {in_tissue} points of tissue covered ({:.2} %), by {} tiles where a raster takes {}; \
         {:.1} mm² found in {} pieces, at saturation over {:.3}",
        100.0 * share,
        tiles.tiles.len(),
        whole.tiles.len(),
        found.area_mm2,
        found.kept,
        found.threshold
    );
    sim.camera.stop().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn an_empty_slide_has_no_tissue_so_no_tiles() {
    let _turn = TURN.lock().await;
    let at = Point2::new(35_000.0, 14_000.0);
    let (sim, rig, _acquisition) = bench("", at).await;
    let region = Region::Rect(Rect::new(at.x - 1_500.0, at.y - 1_500.0, 3e3, 3e3));
    let options = OverviewOptions::default();
    let taken = overview(&rig, &region, &binned(), &options, &Quiet)
        .await
        .unwrap();
    let found = detect(&taken, &options.tissue);
    assert!(!found.mask.cells.contains(&true), "{found:?}");
    assert_eq!((found.kept, found.threshold), (0, options.tissue.floor));
    let refused = snake(
        &Region::Mask(found.mask),
        &full(),
        ZPolicy::Fixed { z_um: 1_002.0 },
    );
    assert!(refused.unwrap_err().to_string().contains("at least one in"));
    sim.camera.stop().await.unwrap();
}
