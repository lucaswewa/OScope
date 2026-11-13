//! The simulated camera on a small sensor: timing and metadata, noise,
//! saturation, binning, triggers, the image's motion through the mounting,
//! blur, the rolling shutter's shear, dropped frames, and the conformance
//! suite.

use std::sync::mpsc::{Receiver, channel};
use std::sync::{Arc, RwLock};
use std::time::Duration;

use oscope_core::{
    Clock, FrameBuffer, FrameMeta, FramePool, HeapAllocator, Ms, PixelRect, Point2, Timestamp, Um,
    Xyz,
};
use oscope_hal::{
    Axis, Camera, CameraSettings, FrameSink, ObjectiveCatalogue, Stage, Trigger, UmPerS,
    conformance,
};
use oscope_sim::camera::{BAYER8, BAYER16};
use oscope_sim::{
    Light, Mounting, Optics, Rate, Settling, SimCamera, SimCameraConfig, SimClock, SimStage,
    SimStageConfig, Slide, TestTarget, World,
};

/// The test target's squares lie from (30, 10) mm on the slide, which sits at
/// the stage's origin: a square's right edge is at x = 31,050 µm, and its
/// rows from y = 11,000 to 11,050 µm.
const EDGE: f64 = 31_050.0;
const FOCUS: f64 = 1_000.0;

struct Rig {
    clock: Arc<SimClock>,
    stage: Arc<SimStage>,
    camera: SimCamera,
}

fn optics() -> Optics {
    let catalogue = ObjectiveCatalogue::mitutoyo();
    Optics::new(&catalogue, catalogue.get("10x").unwrap(), Um(2.4))
}

/// A camera of `config` over an exact stage at `at`, looking at the test
/// target, or at nothing, on `rate`.
fn rig(config: SimCameraConfig, target: bool, at: Point2, rate: Rate) -> Rig {
    let clock = Arc::new(SimClock::new(rate));
    let stage = Arc::new(SimStage::new(
        SimStageConfig {
            acceleration: None,
            backlash: Um(0.0),
            settling: Settling {
                overshoot: Um(0.0),
                time: Ms(15.0),
            },
            require_homing: false,
            start: Xyz::new(Um(at.x), Um(at.y), Um(FOCUS)),
            ..SimStageConfig::default()
        },
        Arc::clone(&clock),
    ));
    let world = if target {
        let target = TestTarget::new(Point2::new(30_000.0, 10_000.0), 0.5, 0.0);
        World::with_slide(
            Slide::new(Arc::new(target), Point2::new(0.0, 0.0)),
            Um(FOCUS),
        )
    } else {
        World::empty()
    };
    let camera = SimCamera::new(
        config,
        Arc::clone(&clock),
        Arc::clone(&stage),
        Arc::new(RwLock::new(world)),
        optics(),
    );
    Rig {
        clock,
        stage,
        camera,
    }
}

fn quiet(width: u32, height: u32) -> SimCameraConfig {
    let mut config = SimCameraConfig::small(width, height);
    config.sensor.noise = false;
    config.sensor.dark_current = 0.0;
    config
}

async fn set(camera: &SimCamera, change: impl FnOnce(&mut CameraSettings)) -> CameraSettings {
    let mut settings = camera.settings().await.unwrap();
    change(&mut settings);
    camera.configure(settings).await.unwrap()
}

/// Starts streaming into a channel.
async fn start(camera: &SimCamera) -> Receiver<(FrameBuffer, FrameMeta)> {
    let settings = camera.settings().await.unwrap();
    let (_, w, h) = camera.capabilities().frame_shape(&settings).unwrap();
    let pool = FramePool::new(settings.format.frame_bytes(w, h), 8, &HeapAllocator);
    let (sender, frames) = channel();
    let sink: FrameSink = Arc::new(move |buffer, meta| {
        let _ = sender.send((buffer, meta));
    });
    camera.start(pool, sink).await.unwrap();
    frames
}

/// The frame that ends by `at`, once the clock is there: its 12-bit counts.
fn frame_by(
    rig: &Rig,
    frames: &Receiver<(FrameBuffer, FrameMeta)>,
    at: Timestamp,
) -> (Vec<u16>, FrameMeta) {
    rig.clock.advance_to(at);
    let (buffer, meta) = frames
        .recv_timeout(Duration::from_secs(60))
        .expect("a frame");
    let counts = if meta.format == BAYER8 {
        buffer.bytes()[..(meta.width * meta.height) as usize]
            .iter()
            .map(|b| u16::from(*b) << 4)
            .collect()
    } else {
        buffer.bytes()[..(meta.width * meta.height * 2) as usize]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|b| u16::from_le_bytes(*b) >> 4)
            .collect()
    };
    (counts, meta)
}

/// A frame's span: its rows' line time over all but one row, and the
/// exposure.
fn span(meta: &FrameMeta) -> Duration {
    meta.line_time.unwrap().to_duration().unwrap() * (meta.height - 1)
        + meta.exposure.to_duration().unwrap()
}

fn mean(values: &[u16]) -> f64 {
    values.iter().map(|v| f64::from(*v)).sum::<f64>() / values.len() as f64
}

#[tokio::test]
async fn free_running_frames_come_at_the_mode_s_interval() {
    let rig = rig(
        quiet(192, 144),
        false,
        Point2::new(40_000.0, 10_000.0),
        Rate::Stepped,
    );
    set(&rig.camera, |s| s.exposure = Ms(1.0)).await;
    let frames = start(&rig.camera).await;
    // Full resolution in 16 bits: 7.24 fps; 60 ms of readout over 144 rows.
    let interval = Duration::from_secs_f64(1.0 / 7.24);
    let line = Ms(60.0 / 144.0);
    for k in 0..3_u32 {
        let begins = Timestamp(0) + interval * k;
        let ends = begins + line.to_duration().unwrap() * 143 + Duration::from_millis(1);
        let (_, meta) = frame_by(&rig, &frames, ends);
        assert_eq!((meta.id.0, meta.exposure_start), (u64::from(k), begins));
        assert!(meta.host_time >= ends);
        assert_eq!((meta.width, meta.height, meta.binning), (192, 144, 1));
        assert_eq!(
            (meta.format, meta.mode.as_str(), meta.line_time),
            (BAYER16, "full", Some(line))
        );
        assert_eq!(
            (meta.black_level, meta.roi),
            (2_000.0, PixelRect::new(0, 0, 192, 144))
        );
        assert_eq!(span(&meta), ends.saturating_since(begins));
    }
    rig.camera.stop().await.unwrap();
}

#[tokio::test]
async fn the_noise_matches_the_model() {
    // An empty world passes all the light: at half intensity for 10 ms,
    // 4,000 electrons a pixel.
    let rig = rig(
        SimCameraConfig::small(192, 144),
        false,
        Point2::new(0.0, 0.0),
        Rate::Stepped,
    );
    rig.camera.set_light(0.5, Light::Transmitted);
    set(&rig.camera, |s| s.exposure = Ms(10.0)).await;
    let frames = start(&rig.camera).await;
    let (counts, meta) = frame_by(&rig, &frames, Timestamp(70_000_000));
    rig.camera.stop().await.unwrap();
    assert_eq!(meta.exposure, Ms(10.0));
    let s = SensorModel::default();
    let cpe = s.counts_per_electron(0.0);
    let m = mean(&counts);
    let variance = counts
        .iter()
        .map(|v| (f64::from(*v) - m).powi(2))
        .sum::<f64>()
        / (counts.len() - 1) as f64;
    let expected = f64::from(s.black) + 4_000.0 * cpe;
    let expected_variance = (4_000.0 + 2.5 * 2.5) * cpe * cpe + 1.0 / 12.0;
    assert!((m - expected).abs() < 0.5, "{m} against {expected}");
    assert!(
        (variance / expected_variance - 1.0).abs() < 0.05,
        "{variance} against {expected_variance}"
    );
}

use oscope_sim::SensorModel;

#[tokio::test]
async fn bright_light_saturates_every_pixel() {
    let rig = rig(
        SimCameraConfig::small(192, 144),
        false,
        Point2::new(0.0, 0.0),
        Rate::Stepped,
    );
    set(&rig.camera, |s| s.exposure = Ms(100.0)).await;
    let frames = start(&rig.camera).await;
    let (counts, _) = frame_by(&rig, &frames, Timestamp(170_000_000));
    rig.camera.stop().await.unwrap();
    assert!(counts.iter().all(|c| *c == 4_095));
}

#[tokio::test]
async fn binned_values_are_sums() {
    // 1,600 electrons a pixel in 10 ms at a fifth of the light.
    let mut levels = Vec::new();
    for mode in ["full", "bin2", "bin3"] {
        let rig = rig(quiet(192, 144), false, Point2::new(0.0, 0.0), Rate::Stepped);
        rig.camera.set_light(0.2, Light::Transmitted);
        set(&rig.camera, |s| {
            s.mode = mode.into();
            s.exposure = Ms(10.0);
        })
        .await;
        let frames = start(&rig.camera).await;
        let (counts, meta) = frame_by(&rig, &frames, Timestamp(200_000_000));
        rig.camera.stop().await.unwrap();
        assert_eq!(meta.binning, [1, 2, 3][levels.len()]);
        levels.push(mean(&counts) - 125.0);
    }
    assert!((levels[1] / levels[0] - 4.0).abs() < 0.01, "{levels:?}");
    assert!((levels[2] / levels[0] - 9.0).abs() < 0.02, "{levels:?}");
}

#[tokio::test]
async fn triggers_start_exposures() {
    // A software trigger at 0.5 s.
    let rig = rig(
        quiet(192, 144),
        false,
        Point2::new(31_000.0, 11_025.0),
        Rate::Stepped,
    );
    set(&rig.camera, |s| {
        s.trigger = Trigger::Software;
        s.exposure = Ms(1.0);
    })
    .await;
    let frames = start(&rig.camera).await;
    rig.clock.advance_to(Timestamp(500_000_000));
    rig.camera.trigger().await.unwrap();
    let (_, meta) = frame_by(&rig, &frames, Timestamp(600_000_000));
    assert_eq!(meta.exposure_start, Timestamp(500_000_000));
    rig.camera.stop().await.unwrap();

    // The stage's pulses, at x = 31.1 and 31.2 mm on a 1 mm/s move from 31 mm:
    // 0.1 s and 0.2 s after it starts.
    set(&rig.camera, |s| {
        s.trigger = Trigger::External {
            edge: oscope_hal::Edge::Rising,
        }
    })
    .await;
    let frames = start(&rig.camera).await;
    let at = |mm: f64| Um::from_mm(mm);
    rig.stage
        .arm_trigger(Axis::X, vec![at(31.1), at(31.2)])
        .await
        .unwrap();
    let stage = Arc::clone(&rig.stage);
    let moving = tokio::spawn(async move {
        let to = Xyz::new(Um::from_mm(31.5), Um(11_025.0), Um(FOCUS));
        stage.move_to(to, Some(UmPerS(1_000.0))).await
    });
    tokio::task::yield_now().await;
    let began = rig.clock.now();
    // Past the second pulse's frame: 0.2 s, then 60 ms of readout.
    while rig.clock.now() < began + Duration::from_millis(270) {
        rig.clock.advance(Duration::from_millis(1));
        tokio::task::yield_now().await;
    }
    let starts: Vec<Timestamp> = (0..2)
        .map(|_| {
            frames
                .recv_timeout(Duration::from_secs(60))
                .unwrap()
                .1
                .exposure_start
        })
        .collect();
    while !moving.is_finished() {
        rig.clock.advance(Duration::from_millis(1));
        tokio::task::yield_now().await;
    }
    rig.camera.stop().await.unwrap();
    assert_eq!(
        starts,
        [
            began + Duration::from_millis(100),
            began + Duration::from_millis(200)
        ]
    );
}

/// The shift that best maps `a` onto `b`, both `w` × `h`, to a tenth of a
/// pixel: the least mean squared difference over whole shifts, then a
/// parabola through its neighbours.
fn shift(a: &[u16], b: &[u16], w: i64, h: i64, reach: i64) -> (f64, f64) {
    let cost = |dx: i64, dy: i64| {
        let (mut sum, mut n) = (0.0, 0.0);
        for y in 0.max(-dy)..h.min(h - dy) {
            for x in 0.max(-dx)..w.min(w - dx) {
                let d = f64::from(a[(y * w + x) as usize])
                    - f64::from(b[((y + dy) * w + x + dx) as usize]);
                sum += d * d;
                n += 1.0;
            }
        }
        sum / n
    };
    let mut best = (0, 0, f64::MAX);
    for dy in -reach..=reach {
        for dx in -reach..=reach {
            let c = cost(dx, dy);
            if c < best.2 {
                best = (dx, dy, c);
            }
        }
    }
    let (dx, dy, c) = best;
    let vertex = |m: f64, p: f64| {
        let curve = m - 2.0 * c + p;
        if curve > 0.0 {
            0.5 * (m - p) / curve
        } else {
            0.0
        }
    };
    (
        dx as f64 + vertex(cost(dx - 1, dy), cost(dx + 1, dy)),
        dy as f64 + vertex(cost(dx, dy - 1), cost(dx, dy + 1)),
    )
}

#[tokio::test]
async fn a_jog_moves_the_image_through_the_mounting() {
    // Turned by 30° and mirrored; binned 3 × 3, so a pixel is 0.72 µm, and
    // frames are 1 / 33.87 s apart. A jog at 0.5 mm/s in x moves the stage
    // 14.8 µm between frames.
    let mut config = quiet(384, 288);
    config.mounting = Mounting {
        rotation: 30f64.to_radians(),
        mirrored: true,
        offset: Point2::new(0.0, 0.0),
    };
    let rig = rig(config, true, Point2::new(31_000.0, 11_000.0), Rate::Stepped);
    set(&rig.camera, |s| {
        s.mode = "bin3".into();
        s.exposure = Ms(1.0);
    })
    .await;
    rig.stage
        .jog(Axis::X, true, Some(UmPerS(500.0)))
        .await
        .unwrap();
    let frames = start(&rig.camera).await;
    let interval = Duration::from_secs_f64(1.0 / 33.87);
    let first_end =
        Timestamp(0) + Ms(15.0 * 95.0 / 96.0).to_duration().unwrap() + Duration::from_millis(1);
    let (a, meta) = frame_by(&rig, &frames, first_end);
    let (b, _) = frame_by(&rig, &frames, first_end + interval);
    rig.camera.stop().await.unwrap();
    let (w, h) = (i64::from(meta.width), i64::from(meta.height));
    let measured = shift(&a, &b, w, h, 30);
    // The stage moves Δ; the image moves by −(mirror · turn⁻¹) Δ ÷ pixel:
    // here along (cos 30°, sin 30°), at a = 0.5 mm/s ÷ 0.72 µm pixels a
    // second. The rolling shutter adds to it: content dy rows lower was
    // exposed dy line times L later, so dy = sin 30° · a · (Δt + dy L),
    // and dx = cos 30° · a · (Δt + dy L).
    let (sin, cos) = 30f64.to_radians().sin_cos();
    let (a, dt, line) = (500.0 / 0.72, interval.as_secs_f64(), 15e-3 / 96.0);
    let dy = sin * a * dt / (1.0 - sin * a * line);
    let expected = (cos * a * (dt + dy * line), dy);
    assert!(
        (measured.0 - expected.0).abs() < 0.3 && (measured.1 - expected.1).abs() < 0.3,
        "{measured:?} against {expected:?}"
    );
}

/// The columns where a row's level first crosses `fraction` of the way from
/// its darkest to its brightest, going right from the darkest.
fn crossing(row: &[u16], fraction: f64) -> f64 {
    let (lo, hi) = (*row.iter().min().unwrap(), *row.iter().max().unwrap());
    let start = row.iter().position(|v| *v == lo).unwrap();
    let level = f64::from(lo) + fraction * f64::from(hi - lo);
    let i = (start..row.len())
        .find(|i| f64::from(row[*i]) >= level)
        .unwrap();
    let (a, b) = (f64::from(row[i - 1]), f64::from(row[i]));
    (i - 1) as f64 + (level - a) / (b - a)
}

#[tokio::test]
async fn the_blur_is_the_speed_times_the_exposure() {
    // A jog at 1 mm/s through a 20 ms exposure: 20 µm, 83 pixels at 10×. A
    // short readout, so the first rows aren't sheared. The square's edge is
    // in the middle of the first row at the exposure's middle.
    let mut config = quiet(384, 96);
    config.readout = [Ms(1.0); 3];
    let rig = rig(
        config,
        true,
        Point2::new(EDGE - 10.0, 11_025.0),
        Rate::Stepped,
    );
    set(&rig.camera, |s| s.exposure = Ms(20.0)).await;
    rig.stage
        .jog(Axis::X, true, Some(UmPerS(1_000.0)))
        .await
        .unwrap();
    let frames = start(&rig.camera).await;
    let (counts, meta) = frame_by(&rig, &frames, Timestamp(25_000_000));
    rig.camera.stop().await.unwrap();
    let row = &counts[..meta.width as usize];
    let width = crossing(row, 0.9) - crossing(row, 0.1);
    // A box blur of length L rises from 10% to 90% over 0.8 L.
    let expected = 0.8 * 20.0 / 0.24;
    assert!(
        (width / expected - 1.0).abs() < 0.05,
        "{width} against {expected}"
    );
}

#[tokio::test]
async fn the_rolling_shutter_shears_a_jog() {
    // 60 ms of readout over 144 rows: each row 0.417 ms after the last. At
    // 0.2 mm/s the stage moves 0.083 µm a row, 0.347 pixels at 10×, so the
    // edge moves left by that much each row.
    let rig = rig(
        quiet(192, 144),
        true,
        Point2::new(EDGE - 10.7, 11_025.0),
        Rate::Stepped,
    );
    set(&rig.camera, |s| s.exposure = Ms(1.0)).await;
    rig.stage
        .jog(Axis::X, true, Some(UmPerS(200.0)))
        .await
        .unwrap();
    let frames = start(&rig.camera).await;
    let (counts, meta) = frame_by(&rig, &frames, Timestamp(70_000_000));
    rig.camera.stop().await.unwrap();
    let w = meta.width as usize;
    let rows: Vec<(f64, f64)> = (0..meta.height as usize)
        .step_by(8)
        .map(|y| (y as f64, crossing(&counts[y * w..(y + 1) * w], 0.5)))
        .collect();
    let n = rows.len() as f64;
    let (sx, sy) = rows
        .iter()
        .fold((0.0, 0.0), |(a, b), (x, y)| (a + x, b + y));
    let (mx, my) = (sx / n, sy / n);
    let slope = rows.iter().map(|(x, y)| (x - mx) * (y - my)).sum::<f64>()
        / rows.iter().map(|(x, _)| (x - mx).powi(2)).sum::<f64>();
    let expected = -200.0 * (60e-3 / 144.0) / 0.24;
    assert!(
        (slope / expected - 1.0).abs() < 0.05,
        "{slope} against {expected}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn slow_rendering_drops_frames_but_not_the_motion() {
    // At 300× the clock outruns the rendering: frames that ended meanwhile
    // are skipped and counted, and each delivered frame starts on the
    // schedule.
    let rig = rig(
        SimCameraConfig::small(768, 576),
        true,
        Point2::new(31_000.0, 11_025.0),
        Rate::Scaled(300.0),
    );
    set(&rig.camera, |s| s.exposure = Ms(1.0)).await;
    rig.stage
        .jog(Axis::X, true, Some(UmPerS(100.0)))
        .await
        .unwrap();
    let jogged = rig.clock.now();
    let frames = start(&rig.camera).await;
    let interval = Duration::from_secs_f64(1.0 / 7.24);
    let mut metas = Vec::new();
    while metas.len() < 4 {
        metas.push(frames.recv_timeout(Duration::from_secs(60)).unwrap().1);
    }
    rig.camera.stop().await.unwrap();
    let mut skipped = 0;
    for pair in metas.windows(2) {
        let gap = pair[1]
            .exposure_start
            .saturating_since(pair[0].exposure_start);
        let steps = gap.as_secs_f64() / interval.as_secs_f64();
        assert!(
            (steps - steps.round()).abs() < 1e-6,
            "off the schedule: {steps}"
        );
        assert_eq!(pair[1].dropped_before, steps.round() as u64 - 1);
        skipped += pair[1].dropped_before;
    }
    assert!(skipped > 0, "nothing was dropped");
    // The stage kept to the clock all the while: exactly 100 µm/s between
    // a moment before the frames and one after.
    let now = rig.clock.now();
    let moved = rig.stage.true_position(now).x.0 - rig.stage.true_position(jogged).x.0;
    let speed = moved / now.saturating_since(jogged).as_secs_f64();
    assert!((speed - 100.0).abs() < 1e-6, "{speed} µm/s");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_camera_conforms() {
    // At a quarter of real time, each frame has four times its interval to
    // render in, so other tests running alongside don't make it drop any.
    let rig = rig(
        SimCameraConfig::small(192, 144),
        true,
        Point2::new(31_000.0, 11_025.0),
        Rate::Scaled(0.25),
    );
    conformance::camera(&rig.camera).await.unwrap();
}
