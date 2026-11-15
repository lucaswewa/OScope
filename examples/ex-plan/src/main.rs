//! `ex-plan`: a focus map measured by autofocus over the simulator's H&E
//! slide, tilted, against the simulator's truth; then scan plans over the
//! slide, each printed with its estimates and drawn on a page. See the
//! README.

use std::path::Path;
use std::process::ExitCode;
use std::sync::{Arc, PoisonError};
use std::time::{Duration, Instant};

use anyhow::ensure;
use ex_common::{Example, get};
use oscope_acquire::{Acquisition, FrameBus};
use oscope_api::Api;
use oscope_core::{FramePool, HeapAllocator, PixelRect, Point2, Rect, Um, Xyz};
use oscope_hal::{Camera, Illumination, ObjectiveCatalogue, ObjectiveMount, Stage};
use oscope_sim::{HeTissue, Light, Optics, SimCameraConfig, Simulator, SimulatorConfig, Slide};
use oscope_workflows::autofocus::{FocusOptions, FocusRig, focus_defaults};
use oscope_workflows::calibration::Quiet;
use oscope_workflows::scan::{
    Field, FitOptions, FocusMap, FocusPoint, FrameSize, MapSample, Mask, Plan, Position, Region,
    Stack, Surface, Timing, ZPolicy, estimate, measure_map, positions, raster, sample_points,
    snake, tiled_stack, z_scan,
};
use serde_json::{Value, json};

/// How the slide tilts: how much higher it is in focus, in µm, a mm along
/// x, and along y.
const TILT: [f64; 2] = [1.0, -0.5];

/// The MIChrome 20's frames a second at full resolution in 16 bits, as
/// measured on the instrument.
const FPS: f64 = 7.24;

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("ex-plan: {error:#}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> anyhow::Result<()> {
    let example = Example::from_args_with("ex-plan", 8239, &["--rate"])?;
    let smoke = example.smoke;
    let rate = example
        .number("--rate")
        .unwrap_or(if smoke { 10 } else { 1 });
    let config = SimulatorConfig {
        rate: rate as f64,
        sensor: if smoke { "quarter" } else { "full" }.to_owned(),
        tilt: TILT,
        ..SimulatorConfig::default()
    };
    let sim = Simulator::new(&config, Path::new("."))
        .await
        .map_err(anyhow::Error::msg)?;
    let stage = Arc::clone(&sim.stage);
    tokio::spawn(async move { stage.run().await });

    // The MIChrome 20's whole sensor through the 10×, neighbours sharing a
    // tenth; and as a calibration finding the camera turned 0.5° gives it.
    let sensor = SimCameraConfig::default();
    let catalogue = ObjectiveCatalogue::mitutoyo();
    let pixel = catalogue
        .pixel_size(catalogue.get("10x")?, sensor.pixel_size)
        .0;
    let (w, h) = (sensor.width, sensor.height);
    let field = Field::nominal(pixel, w, h, 0.1)?;
    let (s, c) = 0.5_f64.to_radians().sin_cos();
    let turned = Field::new([[pixel * c, -pixel * s], [pixel * s, pixel * c]], w, h, 0.1)?;
    let size = |f: &Field| format!("{:.1} × {:.1} µm", f.cell_um.width, f.cell_um.height);
    println!(
        "the 10× on the MIChrome 20: frames of {}, a tenth shared with each neighbour; \
         turned 0.5°, cells of {}",
        size(&field),
        size(&turned)
    );

    // The focus map, over 4 × 4 mm of the first island.
    let islands = HeTissue::new(1, Slide::COVERSLIP).islands();
    let first = islands[0];
    let square = Region::Rect(Rect::new(first.x - 2e3, first.y - 2e3, 4e3, 4e3));
    let map = map(&sim, &square, smoke).await?;

    // The plans.
    let coverslip = Region::Rect(Slide::COVERSLIP);
    let tissue = tissue(&sim)?;
    let every = |every| ZPolicy::Autofocus { every };
    let stack = Stack {
        below_um: 10.0,
        above_um: 10.0,
        step_um: 2.0,
    };
    let middles: Vec<Position> = islands
        .iter()
        .enumerate()
        .map(|(i, p)| Position {
            x_um: p.x,
            y_um: p.y,
            z_um: None,
            name: Some(format!("island {}", i + 1)),
        })
        .collect();
    let by_map = ZPolicy::Map {
        map: map.plane.clone(),
    };
    let plans: Vec<(&str, &Region, Plan)> = vec![
        (
            "raster over the coverslip",
            &coverslip,
            raster(&coverslip, &field, every(10))?,
        ),
        (
            "snake over the coverslip",
            &coverslip,
            snake(&coverslip, &field, every(10))?,
        ),
        (
            "snake over the tissue",
            &tissue,
            snake(&tissue, &field, every(10))?,
        ),
        (
            "snake over 4 × 4 mm, by the map",
            &square,
            snake(&square, &field, by_map)?,
        ),
        (
            "tiled z-stack over 4 × 4 mm",
            &square,
            tiled_stack(&square, &field, every(1), stack)?,
        ),
        (
            "z-scan at the first island",
            &square,
            z_scan(first, &field, every(1), stack)?,
        ),
        (
            "the islands' middles",
            &coverslip,
            positions(&middles, &field, every(1), None)?,
        ),
    ];
    let timing = Timing {
        speed_um_s: sim.stage.capabilities().default_speed.0,
        settle_ms: 50.0,
        frame_ms: 1e3 / FPS,
        autofocus_s: map.seconds,
    };
    let frame = FrameSize {
        width: w,
        height: h,
        bytes_per_pixel: 2,
    };
    println!(
        "\n{:<32} {:>6} {:>9} {:>6} {:>7} {:>9} {:>9} {:>11}",
        "plan", "tiles", "grid", "planes", "frames", "data", "travel", "time"
    );
    let mut drawn = Vec::new();
    for (name, region, plan) in &plans {
        let e = estimate(plan, &timing, &frame)?;
        println!(
            "{name:<32} {:>6} {:>9} {:>6} {:>7} {:>6.1} GB {:>6.1} mm {:>11}",
            e.tiles,
            format!("{} × {}", plan.columns, plan.rows),
            plan.planes().len(),
            e.frames,
            e.bytes as f64 / 1e9,
            e.travel_um / 1e3,
            duration(e.seconds)
        );
        drawn.push(json!({"name": name, "region": outline(region), "plan": plan, "estimate": e}));
    }
    println!(
        "  at {} mm/s, {} ms settling and {:.0} ms a frame, and {:.1} s an autofocus, as on the \
         simulator above",
        timing.speed_um_s / 1e3,
        timing.settle_ms,
        timing.frame_ms,
        timing.autofocus_s
    );
    if smoke {
        let whole = &plans[1].2;
        ensure!(whole.tiles.len() as u32 == whole.rows * whole.columns);
        ensure!(plans.iter().all(|(_, _, p)| !p.tiles.is_empty()));
    }

    // The page: its files, and the plans beside them.
    let folder = example.roots.data.join("page");
    std::fs::create_dir_all(&folder)?;
    for name in ["index.html", "app.js"] {
        let from = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("page")
            .join(name);
        std::fs::copy(from, folder.join(name))?;
    }
    let document = json!({"plans": drawn, "map": map.drawn});
    std::fs::write(folder.join("plans.json"), serde_json::to_vec(&document)?)?;
    let api = Api::builder(&example.roots);
    let count = plans.len();
    example
        .serve(api, &folder, move |address| async move {
            let plans = get(address, "/plans.json").await?;
            ensure!(plans.status == 200, "plans.json: {}", plans.status);
            let shown = plans.json()?["plans"].as_array().map_or(0, Vec::len);
            ensure!(shown == count, "{shown} plans of {count}");
            for path in ["/", "/app.js"] {
                let answer = get(address, path).await?;
                ensure!(answer.status == 200, "{path}: {}", answer.status);
            }
            Ok(())
        })
        .await
}

/// A focus map measured, its plane, and what the page draws of it.
struct Measured {
    plane: FocusMap,
    /// An autofocus's time, on average, in s.
    seconds: f64,
    drawn: Value,
}

/// Measures focus by autofocus at a grid of points over `region`, 3 × 3 (2
/// × 2 for `--smoke`), starting 15 µm above the first's focus; fits a plane
/// and a thin-plate spline; and prints each against the simulator's truth.
async fn map(sim: &Simulator, region: &Region, smoke: bool) -> anyhow::Result<Measured> {
    let side = if smoke { 2 } else { 3 };
    let points = sample_points(region, side, side);
    let world = sim.camera.world();
    let truth = |p: Point2| {
        let world = world.read().unwrap_or_else(PoisonError::into_inner);
        world.in_focus(p).map_or(f64::NAN, |z| z.0)
    };
    sim.stage.home().await?;
    let start = Xyz::new(
        Um(points[0].x),
        Um(points[0].y),
        Um(truth(points[0]) + 15.0),
    );
    sim.stage.move_to(start, None).await?;
    // A region of the sensor's middle: the simulator renders each new depth
    // afresh (P34).
    let roi = match smoke {
        true => PixelRect::new(556, 360, 256, 192),
        false => PixelRect::new(2_224, 1_440, 1_024, 768),
    };
    let bus = Arc::new(FrameBus::new());
    let acquisition = Acquisition::new(Arc::clone(&bus), Duration::from_millis(50));
    let mut settings = sim.camera.settings().await?;
    settings.roi = Some(roi);
    let settings = sim.camera.configure(settings).await?;
    let (_, w, h) = sim.camera.capabilities().frame_shape(&settings)?;
    let pool = FramePool::new(settings.format.frame_bytes(w, h), 8, &HeapAllocator);
    sim.camera.start(pool, acquisition.sink()).await?;
    let rig = FocusRig {
        stage: Arc::clone(&sim.stage) as Arc<dyn Stage>,
        camera: Arc::clone(&sim.camera) as Arc<dyn Camera>,
        objectives: Arc::clone(&sim.mount) as Arc<dyn ObjectiveMount>,
        bus,
        lights: Arc::clone(&sim.lights) as Arc<dyn Illumination>,
        compute: None,
    };
    let quarter = focus_defaults(&rig).await?.depth_of_field_um / 4.0;
    println!(
        "\nthe focus map: autofocus at {} points over 4 × 4 mm of the first island, the slide \
         tilted {} µm a mm along x and {} along y",
        points.len(),
        TILT[0],
        TILT[1]
    );
    let started = Instant::now();
    let samples = measure_map(&rig, &points, &FocusOptions::default(), &Quiet).await?;
    let seconds = started.elapsed().as_secs_f64() / points.len() as f64;
    sim.camera.stop().await?;
    drop(acquisition);

    println!("  point, mm          found µm   truth µm    miss");
    for s in &samples {
        let at = format!("({:.2}, {:.2})", s.at.x / 1e3, s.at.y / 1e3);
        match s.z_um {
            Some(z) => println!(
                "  {at:<18} {z:>9.3} {:>10.3} {:>+7.3}",
                truth(s.at),
                z - truth(s.at)
            ),
            None => println!("  {at:<18} refused: {}", s.refused.as_deref().unwrap_or("")),
        }
    }
    let found: Vec<FocusPoint> = samples.iter().filter_map(MapSample::point).collect();
    let options = FitOptions {
        floor_um: quarter,
        ..FitOptions::default()
    };
    let plane = FocusMap::fit(&found, Surface::Plane, &options)?;
    let spline = FocusMap::fit(&found, Surface::ThinPlate { smoothing: 0.0 }, &options)?;
    let c = &plane.coefficients;
    println!(
        "  a plane: {:.2} µm at the middle, rising {:+.2} µm a mm along x and {:+.2} along y",
        c[0], c[1], c[2]
    );
    // Between the points, against the truth.
    let b = region.bounds();
    let between: Vec<Point2> = (0..4)
        .flat_map(|j| (0..4).map(move |i| (i, j)))
        .map(|(i, j)| {
            Point2::new(
                b.origin.x + (0.2 + 0.2 * f64::from(i)) * b.size.width,
                b.origin.y + (0.2 + 0.2 * f64::from(j)) * b.size.height,
            )
        })
        .collect();
    let worst = |m: &FocusMap| {
        between
            .iter()
            .map(|&p| (m.z_at(p) - truth(p)).abs())
            .fold(0.0, f64::max)
    };
    let (wp, ws) = (worst(&plane), worst(&spline));
    println!(
        "  between the points, against the truth: the plane within {wp:.2} µm, a thin-plate \
         spline within {ws:.2}; a quarter of the depth of field is {quarter:.2} µm"
    );
    println!("  each autofocus took {seconds:.1} s, on average");
    if smoke {
        ensure!(found.len() >= 3, "{samples:?}");
        ensure!(wp < quarter && ws < quarter, "{wp} and {ws} µm off");
    }
    let drawn = json!({"samples": samples, "plane": plane, "spline": spline, "tilt": TILT});
    Ok(Measured {
        plane,
        seconds,
        drawn,
    })
}

/// Where the simulator's tissue is: the coverslip rendered at 100 µm a
/// pixel, in focus in transmitted light, its pixels darker in green than
/// clear glass, as at the coverslip's corner. A stand-in for P37's tissue
/// detection.
fn tissue(sim: &Simulator) -> anyhow::Result<Region> {
    let world = sim.camera.world();
    let world = world.read().unwrap_or_else(PoisonError::into_inner);
    let (cover, cell) = (Slide::COVERSLIP, 100.0);
    let (width, height) = (
        (cover.size.width / cell) as u32,
        (cover.size.height / cell) as u32,
    );
    // A pixel of 100 µm through the 10×: a sensor's of 1 mm.
    let catalogue = ObjectiveCatalogue::mitutoyo();
    let optics = Optics::new(&catalogue, catalogue.get("10x")?, Um(cell * 10.0));
    let centre = cover.centre();
    let view = oscope_sim::Field {
        centre,
        z: world.focus_at(centre),
        width,
        height,
    };
    let image = world.render(&view, &optics, Light::Transmitted);
    let green: Vec<f32> = image.data().iter().skip(1).step_by(3).copied().collect();
    let glass = green[0];
    Ok(Region::Mask(Mask {
        origin: cover.origin,
        cell_um: cell,
        width,
        height,
        cells: green.iter().map(|&g| g < 0.95 * glass).collect(),
    }))
}

/// What the page draws of a region: its corners, or a mask's runs of cells
/// that are in, by row.
fn outline(region: &Region) -> Value {
    match region {
        Region::Rect(r) => {
            let (a, b) = (r.origin, r.end());
            json!({"corners": [[a.x, a.y], [b.x, a.y], [b.x, b.y], [a.x, b.y]]})
        }
        Region::Polygon { corners } => {
            json!({"corners": corners.iter().map(|p| [p.x, p.y]).collect::<Vec<_>>()})
        }
        Region::Mask(m) => {
            let mut runs = Vec::new();
            for (row, cells) in m.cells.chunks(m.width as usize).enumerate() {
                let mut from = None;
                for (i, &in_) in cells.iter().chain([&false]).enumerate() {
                    match (in_, from) {
                        (true, None) => from = Some(i),
                        (false, Some(f)) => {
                            runs.push([row, f, i]);
                            from = None;
                        }
                        _ => {}
                    }
                }
            }
            json!({"origin": [m.origin.x, m.origin.y], "cell_um": m.cell_um, "runs": runs})
        }
    }
}

/// `seconds` as hours, minutes and seconds.
fn duration(seconds: f64) -> String {
    let s = seconds.round() as u64;
    match (s / 3600, s / 60 % 60, s % 60) {
        (0, 0, s) => format!("{s} s"),
        (0, m, s) => format!("{m} min {s} s"),
        (h, m, _) => format!("{h} h {m} min"),
    }
}
