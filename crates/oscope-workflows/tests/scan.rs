//! Scan planning (P35) at the simulator's scale: every point of a region in
//! a tile's frame, neighbours overlapping as asked, the snake's order,
//! stacks, positions and z from a map, estimates, and a focus map measured
//! by autofocus recovering a tilted slide.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use oscope_acquire::{Acquisition, FrameBus};
use oscope_core::{FramePool, HeapAllocator, PixelRect, Point2, Rect, Um, Xyz};
use oscope_hal::{Camera, Illumination, ObjectiveMount, Stage};
use oscope_sim::{HeTissue, Simulator, SimulatorConfig, Slide};
use oscope_workflows::autofocus::{FocusOptions, FocusRig, focus_defaults};
use oscope_workflows::calibration::Quiet;
use oscope_workflows::scan::{
    Field, FitOptions, FocusMap, FocusPoint, FrameSize, MapSample, Mask, Pattern, Plan, Position,
    Region, Stack, Surface, Tile, Timing, ZPolicy, estimate, measure_map, positions, raster,
    sample_points, snake, tiled_stack, z_scan,
};

/// The 10×'s pixel at the sample on the MIChrome 20's whole sensor.
const PIXEL: f64 = 0.24;
const SENSOR: (u32, u32) = (5_472, 3_648);

/// The calibration's M with the camera turned `degrees` on the stage.
fn turned(degrees: f64) -> [[f64; 2]; 2] {
    let (s, c) = degrees.to_radians().sin_cos();
    [[PIXEL * c, -PIXEL * s], [PIXEL * s, PIXEL * c]]
}

fn field(degrees: f64, overlap: f64) -> Field {
    Field::new(turned(degrees), SENSOR.0, SENSOR.1, overlap).unwrap()
}

fn flat() -> ZPolicy {
    ZPolicy::Fixed { z_um: 5_000.0 }
}

/// The simulator's coverslip, 50 × 24 mm.
fn coverslip() -> Region {
    Region::Rect(Slide::COVERSLIP)
}

/// A C, 20 × 15 mm, open to the right.
fn letter_c() -> Region {
    let corners = [
        (25.0, 3.0),
        (45.0, 3.0),
        (45.0, 8.0),
        (32.0, 8.0),
        (32.0, 13.0),
        (45.0, 13.0),
        (45.0, 18.0),
        (25.0, 18.0),
    ];
    Region::Polygon {
        corners: corners
            .iter()
            .map(|&(x, y)| Point2::new(x * 1e3, y * 1e3))
            .collect(),
    }
}

/// Two islands of 50 µm cells: ellipses of 12 × 8 and 8 × 6 mm, with 3 mm
/// of rows between them that hold none.
fn islands() -> Region {
    let (origin, cell, width, height) = (Point2::new(25e3, 2e3), 50.0, 800, 400);
    let ellipses = [((35e3, 16e3), (6e3, 4e3)), ((55e3, 6e3), (4e3, 3e3))];
    let cells = (0..width * height)
        .map(|k| {
            let x = origin.x + (f64::from(k % width) + 0.5) * cell;
            let y = origin.y + (f64::from(k / width) + 0.5) * cell;
            ellipses
                .iter()
                .any(|((cx, cy), (a, b))| ((x - cx) / a).powi(2) + ((y - cy) / b).powi(2) <= 1.0)
        })
        .collect();
    Region::Mask(Mask {
        origin,
        cell_um: cell,
        width,
        height,
        cells,
    })
}

/// Checks that each point of `region` on a 100 µm grid is in the frame of a
/// tile of `plan`.
fn assert_covered(region: &Region, plan: &Plan) {
    let step = plan.field.step_um;
    let first = &plan.tiles[0];
    let x0 = first.x_um - f64::from(first.column) * step.width;
    let y0 = first.y_um - f64::from(first.row) * step.height;
    let tiles: HashMap<(i64, i64), &Tile> = plan
        .tiles
        .iter()
        .map(|t| ((i64::from(t.row), i64::from(t.column)), t))
        .collect();
    let b = region.bounds();
    let (mut checked, mut missed) = (0, Vec::new());
    for j in 0..=(b.size.height / 100.0) as i64 {
        for i in 0..=(b.size.width / 100.0) as i64 {
            let p = Point2::new(b.origin.x + i as f64 * 100.0, b.origin.y + j as f64 * 100.0);
            if !region.contains(p) {
                continue;
            }
            checked += 1;
            let (c, r) = (
                ((p.x - x0) / step.width).round() as i64,
                ((p.y - y0) / step.height).round() as i64,
            );
            let near = (r - 1..=r + 1).flat_map(|r| (c - 1..=c + 1).map(move |c| (r, c)));
            let held = near.filter_map(|k| tiles.get(&k)).any(|t| {
                let frame = plan.field.frame_at(Point2::new(t.x_um, t.y_um));
                Region::Polygon {
                    corners: frame.to_vec(),
                }
                .contains(p)
            });
            if !held {
                missed.push(p);
            }
        }
    }
    let some = &missed[..missed.len().min(3)];
    assert!(
        missed.is_empty(),
        "{} of {checked} in no frame: {some:?}",
        missed.len()
    );
    assert!(checked > 10_000, "{checked}");
}

#[test]
fn every_point_of_a_region_is_in_a_tiles_frame() {
    let field = field(0.3, 0.1);
    for region in [coverslip(), letter_c(), islands()] {
        let plans = [
            raster(&region, &field, flat()).unwrap(),
            snake(&region, &field, flat()).unwrap(),
        ];
        for plan in &plans {
            assert_covered(&region, plan);
        }
        let grid = (plans[1].rows * plans[1].columns) as usize;
        match region {
            Region::Rect(_) => assert_eq!(plans[1].tiles.len(), grid),
            _ => assert!(plans[1].tiles.len() < grid, "{grid}"),
        }
    }
}

#[test]
fn neighbours_overlap_by_the_share_asked() {
    let (w, h) = (PIXEL * f64::from(SENSOR.0), PIXEL * f64::from(SENSOR.1));
    for overlap in [0.0, 0.1, 0.25] {
        // Square with the stage, the frames are the cells.
        let square = field(0.0, overlap);
        assert!((square.cell_um.width - w).abs() < 1e-9);
        assert!((square.cell_um.height - h).abs() < 1e-9);
        let plan = snake(&coverslip(), &square, flat()).unwrap();
        let at: HashMap<(u32, u32), &Tile> =
            plan.tiles.iter().map(|t| ((t.row, t.column), t)).collect();
        for t in &plan.tiles {
            if let Some(n) = at.get(&(t.row, t.column + 1)) {
                assert!(((w - (n.x_um - t.x_um)) - overlap * w).abs() < 1e-6);
            }
            if let Some(n) = at.get(&(t.row + 1, t.column)) {
                assert!(((h - (n.y_um - t.y_um)) - overlap * h).abs() < 1e-6);
            }
        }
        // Turned, each frame holds its cell, the largest of its proportions
        // that it does, so neighbours' frames overlap at least as their
        // cells do: for a frame wider than high, whose height binds, and
        // one higher than wide, whose width does.
        let shapes = [SENSOR, (SENSOR.1, SENSOR.0)];
        for ((width, height), degrees) in shapes
            .into_iter()
            .flat_map(|s| [0.3, -2.0, 90.4].map(|d| (s, d)))
        {
            let turned_field = Field::new(turned(degrees), width, height, overlap).unwrap();
            let cell = turned_field.cell_um;
            assert!((turned_field.step_um.width - (1.0 - overlap) * cell.width).abs() < 1e-9);
            assert!((turned_field.step_um.height - (1.0 - overlap) * cell.height).abs() < 1e-9);
            let m = turned(degrees);
            let det = m[0][0] * m[1][1] - m[0][1] * m[1][0];
            let n = [
                [m[1][1] / det, -m[0][1] / det],
                [-m[1][0] / det, m[0][0] / det],
            ];
            let (fw, fh) = (f64::from(width) / 2.0, f64::from(height) / 2.0);
            // Whether the cell, scaled, has its corners in the frame.
            let inside = |scale: f64| {
                [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)]
                    .iter()
                    .all(|(sx, sy)| {
                        let (x, y) = (
                            sx * scale * cell.width / 2.0,
                            sy * scale * cell.height / 2.0,
                        );
                        let (u, v) = (n[0][0] * x + n[0][1] * y, n[1][0] * x + n[1][1] * y);
                        u.abs() <= fw + 1e-6 && v.abs() <= fh + 1e-6
                    })
            };
            assert!(
                inside(1.0) && !inside(1.001),
                "{width} × {height}, {degrees}°: {cell:?}"
            );
            assert!(cell.width * cell.height < w * h);
        }
    }
}

#[test]
fn a_snake_runs_each_row_the_other_way_from_the_last() {
    let field = field(0.3, 0.1);
    let timing = Timing {
        speed_um_s: 10_000.0,
        settle_ms: 0.0,
        frame_ms: 0.0,
        autofocus_s: 0.0,
    };
    let frame = FrameSize {
        width: 1,
        height: 1,
        bytes_per_pixel: 1,
    };
    for region in [coverslip(), letter_c(), islands()] {
        let snake = snake(&region, &field, flat()).unwrap();
        let raster = raster(&region, &field, flat()).unwrap();
        let keys = |p: &Plan| {
            let mut k: Vec<(u32, u32)> = p.tiles.iter().map(|t| (t.row, t.column)).collect();
            k.sort_unstable();
            k
        };
        assert_eq!(keys(&snake), keys(&raster));
        assert!(
            snake
                .tiles
                .iter()
                .enumerate()
                .all(|(i, t)| t.index as usize == i)
        );
        let rows: Vec<&[Tile]> = snake.tiles.chunk_by(|a, b| a.row == b.row).collect();
        assert!(rows.windows(2).all(|w| w[0][0].row < w[1][0].row));
        if let Region::Mask(_) = region {
            assert!(rows.windows(2).any(|w| w[1][0].row > w[0][0].row + 1));
        }
        for (k, row) in rows.iter().enumerate() {
            let forward = row.windows(2).all(|w| w[0].column < w[1].column);
            let back = row.windows(2).all(|w| w[0].column > w[1].column);
            assert!(if k % 2 == 0 { forward } else { back }, "row {k}");
        }
        let travel = |p: &Plan| estimate(p, &timing, &frame).unwrap().travel_um;
        assert!(travel(&snake) < 0.6 * travel(&raster));
    }
    // Over the whole rectangle, each move is one step: along a row, or to
    // the next at its end.
    let whole = snake(&coverslip(), &field, flat()).unwrap();
    let step = whole.field.step_um;
    for w in whole.tiles.windows(2) {
        let (dx, dy) = ((w[1].x_um - w[0].x_um).abs(), w[1].y_um - w[0].y_um);
        let along = (dx - step.width).abs() < 1e-6 && dy == 0.0;
        let up = dx < 1e-6 && (dy - step.height).abs() < 1e-6;
        assert!(along || up, "{dx}, {dy}");
    }
}

#[test]
fn stacks_positions_and_maps_set_each_tiles_z() {
    let field = field(0.0, 0.1);
    let stack = Stack {
        below_um: 5.0,
        above_um: 5.0,
        step_um: 2.0,
    };
    assert_eq!(stack.planes(), [-5.0, -3.0, -1.0, 1.0, 3.0, 5.0]);
    let short = Stack {
        above_um: 3.0,
        below_um: 4.0,
        ..stack
    };
    assert_eq!(short.planes(), [-4.0, -2.0, 0.0, 2.0]);

    let at = Point2::new(35_000.0, 14_000.0);
    let scan = z_scan(at, &field, flat(), stack).unwrap();
    assert_eq!(
        (scan.pattern, scan.tiles.len(), scan.planes().len()),
        (Pattern::ZScan, 1, 6)
    );
    assert_eq!(
        (scan.tiles[0].x_um, scan.tiles[0].z_um),
        (at.x, Some(5_000.0))
    );

    let square = Region::Rect(Rect::new(30e3, 10e3, 3e3, 3e3));
    let tiled = tiled_stack(&square, &field, flat(), stack).unwrap();
    let snaked = snake(&square, &field, flat()).unwrap();
    assert_eq!(tiled.pattern, Pattern::TiledStack);
    assert_eq!(tiled.tiles, snaked.tiles);
    assert_eq!(tiled.planes().len(), 6);

    let place = |x: f64, z: Option<f64>, name: Option<&str>| Position {
        x_um: x,
        y_um: 12e3,
        z_um: z,
        name: name.map(str::to_owned),
    };
    let list = [
        place(40e3, Some(5_003.0), Some("first")),
        place(30e3, None, None),
        place(35e3, None, Some("third")),
    ];
    let visits = positions(&list, &field, flat(), None).unwrap();
    let got: Vec<(f64, Option<f64>, Option<&str>)> = visits
        .tiles
        .iter()
        .map(|t| (t.x_um, t.z_um, t.name.as_deref()))
        .collect();
    assert_eq!(
        got,
        [
            (40e3, Some(5_003.0), Some("first")),
            (30e3, Some(5_000.0), None),
            (35e3, Some(5_000.0), Some("third")),
        ]
    );

    // From a map, each tile at its z there; by autofocus, none known yet.
    let points: Vec<FocusPoint> = [(30e3, 10e3), (36e3, 10e3), (30e3, 16e3), (36e3, 16e3)]
        .iter()
        .map(|&(x, y)| FocusPoint {
            x_um: x,
            y_um: y,
            z_um: 5_000.0 + 0.5 * (x - 30e3) / 1e3 - 0.25 * (y - 10e3) / 1e3,
        })
        .collect();
    let map = FocusMap::fit(&points, Surface::Plane, &FitOptions::default()).unwrap();
    let mapped = snake(&square, &field, ZPolicy::Map { map: map.clone() }).unwrap();
    for t in &mapped.tiles {
        let expected = 5_000.0 + 0.5 * (t.x_um - 30e3) / 1e3 - 0.25 * (t.y_um - 10e3) / 1e3;
        assert!((t.z_um.unwrap() - expected).abs() < 1e-9);
    }
    let later = snake(&square, &field, ZPolicy::Autofocus { every: 4 }).unwrap();
    assert!(later.tiles.iter().all(|t| t.z_um.is_none()));

    // What can't be is refused, saying why.
    assert!(Field::new(turned(0.0), SENSOR.0, SENSOR.1, 0.95).is_err());
    assert!(Field::new([[0.24, 0.48], [0.12, 0.24]], SENSOR.0, SENSOR.1, 0.1).is_err());
    assert!(snake(&square, &field, ZPolicy::Autofocus { every: 0 }).is_err());
    let none = Stack {
        step_um: 0.0,
        ..stack
    };
    assert!(tiled_stack(&square, &field, flat(), none).is_err());
    assert!(positions(&[], &field, flat(), None).is_err());
    let huge = Region::Rect(Rect::new(0.0, 0.0, 1e6, 1e6));
    let refused = snake(&huge, &field, flat()).unwrap_err().to_string();
    assert!(refused.contains("at most 100000 tiles"), "{refused}");
}

#[test]
fn estimates_add_the_moves_the_frames_and_the_autofocus() {
    let field = field(0.0, 0.1);
    let (cell, step) = (field.cell_um, field.step_um);
    // Just over a cell each way: a 2 × 2 grid, three planes at each tile.
    let region = Region::Rect(Rect::new(30e3, 10e3, cell.width + 10.0, cell.height + 10.0));
    let stack = Stack {
        below_um: 2.0,
        above_um: 2.0,
        step_um: 2.0,
    };
    let plan = tiled_stack(&region, &field, ZPolicy::Autofocus { every: 3 }, stack).unwrap();
    assert_eq!(plan.tiles.len(), 4);
    let timing = Timing {
        speed_um_s: 10_000.0,
        settle_ms: 50.0,
        frame_ms: 138.0,
        autofocus_s: 12.0,
    };
    let frame = FrameSize {
        width: SENSOR.0,
        height: SENSOR.1,
        bytes_per_pixel: 2,
    };
    let e = estimate(&plan, &timing, &frame).unwrap();
    assert_eq!((e.tiles, e.frames, e.autofocus_runs), (4, 12, 2));
    assert_eq!(e.bytes, 12 * 5_472 * 3_648 * 2);
    let travel = 2.0 * step.width + step.height;
    assert!((e.travel_um - travel).abs() < 1e-6);
    let seconds = travel / 10_000.0 + 12.0 * 0.188 + 2.0 * 12.0;
    assert!((e.seconds - seconds).abs() < 1e-9, "{}", e.seconds);
    let stopped = Timing {
        speed_um_s: 0.0,
        ..timing
    };
    assert!(estimate(&plan, &stopped, &frame).is_err());
    // A move goes straight: 3 mm along x and 4 along y is 5 mm.
    let at = |x: f64, y: f64| Position {
        x_um: x,
        y_um: y,
        z_um: None,
        name: None,
    };
    let two = positions(&[at(30e3, 10e3), at(33e3, 14e3)], &field, flat(), None).unwrap();
    let e = estimate(&two, &timing, &frame).unwrap();
    assert!((e.travel_um - 5e3).abs() < 1e-9, "{}", e.travel_um);
}

/// A 256 × 192 region of the quarter sensor's middle: quick to render.
const SMALL: Option<PixelRect> = Some(PixelRect::new(556, 360, 256, 192));

#[tokio::test(flavor = "multi_thread")]
async fn a_focus_map_measured_by_autofocus_recovers_a_tilted_slide() {
    // The H&E slide in focus at 1,000 µm at its corner, rising 2 µm a mm
    // along x and falling 1 along y: 56 µm higher at the first island.
    let config = SimulatorConfig {
        rate: 10.0,
        sensor: "quarter".to_owned(),
        focus: 1_000.0,
        tilt: [2.0, -1.0],
        ..SimulatorConfig::default()
    };
    let sim = Simulator::new(&config, Path::new(".")).await.unwrap();
    let stage = Arc::clone(&sim.stage);
    tokio::spawn(async move { stage.run().await });
    let world = sim.camera.world();
    let truth = |at: Point2| world.read().unwrap().in_focus(at).unwrap().0;
    let island = HeTissue::new(1, Slide::COVERSLIP).islands()[0];
    let region = Region::Rect(Rect::new(island.x - 2e3, island.y - 2e3, 4e3, 4e3));
    let points = sample_points(&region, 3, 3);
    assert_eq!(points.len(), 9);

    // Homed, then 15 µm above the first point's focus, before the camera
    // streams.
    sim.stage.home().await.unwrap();
    let start = Xyz::new(
        Um(points[0].x),
        Um(points[0].y),
        Um(truth(points[0]) + 15.0),
    );
    sim.stage.move_to(start, None).await.unwrap();
    let bus = Arc::new(FrameBus::new());
    let acquisition = Acquisition::new(Arc::clone(&bus), Duration::from_millis(50));
    let mut settings = sim.camera.settings().await.unwrap();
    settings.roi = SMALL;
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
    let quarter = focus_defaults(&rig).await.unwrap().depth_of_field_um / 4.0;

    // Clear glass at the coverslip's corner, fifth: refused as an empty
    // field, it keeps no z, and the points after it are found.
    let mut visits = points.clone();
    visits.insert(4, Point2::new(22_100.0, 600.0));
    let samples = measure_map(&rig, &visits, &FocusOptions::default(), &Quiet)
        .await
        .unwrap();
    assert!(samples[4].z_um.is_none(), "{:?}", samples[4]);
    assert!(
        samples[4]
            .refused
            .as_deref()
            .unwrap_or("")
            .contains("empty")
    );
    let found: Vec<FocusPoint> = samples.iter().filter_map(MapSample::point).collect();
    assert!(found.len() >= 7 && samples[5].z_um.is_some(), "{samples:?}");
    for p in &found {
        let miss = p.z_um - truth(Point2::new(p.x_um, p.y_um));
        assert!(miss.abs() < quarter, "{p:?}: {miss:+.2} µm");
    }

    // The plane's slopes are the tilt; between the points, both surfaces
    // are within a quarter of the depth of field of the truth.
    let options = FitOptions {
        floor_um: quarter,
        ..FitOptions::default()
    };
    let plane = FocusMap::fit(&found, Surface::Plane, &options).unwrap();
    let spline = FocusMap::fit(&found, Surface::ThinPlate { smoothing: 0.0 }, &options).unwrap();
    let slopes = (plane.coefficients[1], plane.coefficients[2]);
    assert!(
        (slopes.0 - 2.0).abs() < 0.25 && (slopes.1 + 1.0).abs() < 0.25,
        "{slopes:?}"
    );
    assert!(plane.rejected.is_empty() && spline.rejected.is_empty());
    for (dx, dy) in [
        (-1e3, -1e3),
        (1e3, -1e3),
        (1e3, 1e3),
        (-1e3, 1e3),
        (0.0, 0.0),
    ] {
        let at = Point2::new(island.x + dx, island.y + dy);
        for map in [&plane, &spline] {
            let miss = map.z_at(at) - truth(at);
            assert!(
                miss.abs() < quarter,
                "{:?} at {at:?}: {miss:+.2} µm",
                map.surface
            );
        }
    }
    sim.camera.stop().await.unwrap();
    drop(acquisition);
}
