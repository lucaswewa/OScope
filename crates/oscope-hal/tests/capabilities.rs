//! The camera's and the stage's capabilities and their checks, their JSON,
//! and the traits' default operations.

use std::sync::Arc;

use oscope_core::{BayerPattern, FramePool, Ms, PixelFormat, PixelRect, Timestamp, Um, Xyz};
use oscope_hal::{
    Axis, Camera, CameraCapabilities, CameraMode, CameraSettings, Edge, Error, FrameSink,
    HalFuture, ModeRate, PositionSample, PositionTrack, RoiRules, Shutter, Stage,
    StageCapabilities, StageStatus, Travel, Trigger, TriggerCapabilities, UmPerS,
};
use serde_json::json;

const BAYER8: PixelFormat = PixelFormat::Bayer8 {
    pattern: BayerPattern::Rggb,
};
const BAYER16: PixelFormat = PixelFormat::Bayer16 {
    pattern: BayerPattern::Rggb,
    significant_bits: 16,
};

/// The MIChrome 20 as measured in P02b (`docs/hardware.md`).
fn michrome20() -> CameraCapabilities {
    let mode = |name: &str, binning: u32, fps8: f64, fps16: f64| CameraMode {
        name: name.to_owned(),
        width: 5472 / binning,
        height: 3648 / binning,
        binning,
        rates: vec![
            ModeRate {
                format: BAYER8,
                fps: fps8,
            },
            ModeRate {
                format: BAYER16,
                fps: fps16,
            },
        ],
        line_time: None,
    };
    CameraCapabilities {
        model: "Tucsen MIChrome 20".to_owned(),
        sensor_width: 5472,
        sensor_height: 3648,
        pixel_size: Um(2.4),
        shutter: Shutter::Rolling,
        modes: vec![
            mode("full", 1, 14.49, 7.24),
            mode("bin2", 2, 53.14, 26.57),
            mode("bin3", 3, 67.26, 33.87),
        ],
        exposure_min: Ms(0.0184),
        exposure_max: Ms(19_340.1),
        gain_min: 0.0,
        gain_max: 240.0,
        roi: Some(RoiRules {
            x_step: 2,
            y_step: 1,
            min_size: 16,
            raises_rate: false,
        }),
        trigger: TriggerCapabilities::default(),
        sensor_timestamps: false,
        temperature: false,
    }
}

fn settings() -> CameraSettings {
    CameraSettings {
        mode: "full".to_owned(),
        format: BAYER16,
        exposure: Ms(1.0),
        gain: 0.0,
        roi: None,
        trigger: Trigger::FreeRunning,
    }
}

#[test]
fn the_michrome_20_s_settings_are_checked_as_the_camera_would() {
    let camera = michrome20();
    assert_eq!(camera.check(&settings()).unwrap(), settings());
    assert_eq!(camera.mode("full").unwrap().fps(BAYER16), Some(7.24));

    // The camera rounds x and the width down to even numbers (P02b).
    let mut odd = settings();
    odd.roi = Some(PixelRect::new(333, 333, 1001, 1001));
    let applied = camera.check(&odd).unwrap();
    assert_eq!(applied.roi, Some(PixelRect::new(332, 333, 1000, 1001)));

    let refused = |change: fn(&mut CameraSettings)| {
        let mut s = settings();
        change(&mut s);
        camera.check(&s).unwrap_err().to_string()
    };
    assert_eq!(
        refused(|s| s.mode = "bin4".into()),
        "there is no camera mode named \"bin4\""
    );
    assert!(
        refused(|s| s.format = PixelFormat::Rgb8).starts_with("the device doesn't support Rgb8")
    );
    assert_eq!(
        refused(|s| s.exposure = Ms(0.01)),
        "the exposure in ms must be from 0.0184 to 19340.1, and was 0.01"
    );
    assert!(refused(|s| s.gain = 241.0).starts_with("the gain must be"));
    assert!(
        refused(|s| s.roi = Some(PixelRect::new(5000, 0, 600, 100))).contains("outside the sensor")
    );
    assert!(
        refused(|s| s.roi = Some(PixelRect::new(0, 0, 8, 100))).starts_with("the ROI's least side")
    );
    assert_eq!(
        refused(|s| s.trigger = Trigger::Software),
        "the device doesn't support the trigger Software"
    );
    assert!(refused(|s| s.trigger = Trigger::External { edge: Edge::Rising }).contains("External"));

    // A camera without ROIs refuses one; a camera with a trigger input
    // accepts an external trigger.
    let mut plain = michrome20();
    plain.roi = None;
    plain.trigger.external = true;
    assert_eq!(plain.round_roi(PixelRect::new(0, 0, 64, 64)), None);
    assert!(plain.check(&odd).is_err());
    let mut triggered = settings();
    triggered.trigger = Trigger::External {
        edge: Edge::Falling,
    };
    assert!(plain.check(&triggered).is_ok());
}

/// A stage of 100 × 75 × 20 mm, as P11's simulated one might be.
fn stage_capabilities() -> StageCapabilities {
    let travel = |mm: f64| Travel {
        min: Um(0.0),
        max: Um::from_mm(mm),
    };
    StageCapabilities {
        model: "test".to_owned(),
        x: travel(100.0),
        y: travel(75.0),
        z: travel(20.0),
        max_speed: UmPerS(50_000.0),
        default_speed: UmPerS(10_000.0),
        homing: true,
        encoders: true,
        trigger_output: true,
        report_interval: Ms(10.0),
    }
}

#[test]
fn a_stage_checks_targets_against_its_soft_limits_and_speeds() {
    let stage = stage_capabilities();
    let at = |x: f64, y: f64, z: f64| Xyz::new(Um(x), Um(y), Um(z));
    stage.check_target(at(0.0, 75_000.0, 10_000.0)).unwrap();
    assert_eq!(
        stage
            .check_target(at(-1.0, 0.0, 0.0))
            .unwrap_err()
            .to_string(),
        "the x target in µm must be from 0 to 100000, and was -1"
    );
    assert!(stage.check_target(at(0.0, 0.0, 20_001.0)).is_err());
    assert_eq!(stage.travel(Axis::Y).max, Um(75_000.0));

    assert_eq!(stage.check_speed(None).unwrap(), UmPerS(10_000.0));
    assert_eq!(
        stage.check_speed(Some(UmPerS(50_000.0))).unwrap(),
        UmPerS(50_000.0)
    );
    for refused in [0.0, -5.0, 50_001.0, f64::NAN] {
        assert!(
            stage.check_speed(Some(UmPerS(refused))).is_err(),
            "{refused}"
        );
    }
}

#[test]
fn statuses_and_triggers_read_well_as_json() {
    let alarm = StageStatus::Alarm {
        reason: "hard limit".into(),
    };
    assert_eq!(
        serde_json::to_value(&alarm).unwrap(),
        json!({"state": "alarm", "reason": "hard limit"})
    );
    assert_eq!(
        serde_json::to_value(StageStatus::Idle).unwrap(),
        json!({"state": "idle"})
    );
    assert!(StageStatus::Homing.is_moving() && !alarm.is_moving());
    assert_eq!(
        serde_json::to_value(Trigger::External { edge: Edge::Rising }).unwrap(),
        json!({"kind": "external", "edge": "rising"})
    );
    let caps = serde_json::to_value(michrome20()).unwrap();
    assert_eq!(caps["shutter"], "rolling");
    assert_eq!(caps["roi"]["raises_rate"], false);
}

/// A camera that offers only what the trait requires.
struct Bare(CameraCapabilities);

impl Camera for Bare {
    fn capabilities(&self) -> &CameraCapabilities {
        &self.0
    }
    fn settings(&self) -> HalFuture<'_, CameraSettings> {
        Box::pin(async { Ok(settings()) })
    }
    fn configure(&self, s: CameraSettings) -> HalFuture<'_, CameraSettings> {
        Box::pin(async move { self.0.check(&s) })
    }
    fn start(&self, _: FramePool, _: FrameSink) -> HalFuture<'_, ()> {
        Box::pin(async { Ok(()) })
    }
    fn stop(&self) -> HalFuture<'_, ()> {
        Box::pin(async { Ok(()) })
    }
    fn streaming(&self) -> bool {
        false
    }
}

/// A stage that is wherever it was last sent.
struct Teleport {
    capabilities: StageCapabilities,
    track: Arc<PositionTrack>,
}

impl Stage for Teleport {
    fn capabilities(&self) -> &StageCapabilities {
        &self.capabilities
    }
    fn status(&self) -> HalFuture<'_, StageStatus> {
        Box::pin(async { Ok(StageStatus::Idle) })
    }
    fn position(&self) -> HalFuture<'_, PositionSample> {
        Box::pin(async { self.track.latest().ok_or(Error::Disconnected) })
    }
    fn track(&self) -> Arc<PositionTrack> {
        Arc::clone(&self.track)
    }
    fn move_to(&self, target: Xyz, speed: Option<UmPerS>) -> HalFuture<'_, PositionSample> {
        Box::pin(async move {
            self.capabilities.check_target(target)?;
            self.capabilities.check_speed(speed)?;
            let time = Timestamp(self.track.latest().map_or(0, |s| s.time.0) + 1);
            let sample = PositionSample {
                time,
                position: target,
            };
            self.track.push(sample);
            Ok(sample)
        })
    }
    fn jog(&self, _: Axis, _: bool, _: Option<UmPerS>) -> HalFuture<'_, ()> {
        Box::pin(async { Ok(()) })
    }
    fn stop(&self) -> HalFuture<'_, ()> {
        Box::pin(async { Ok(()) })
    }
    fn clear_alarm(&self) -> HalFuture<'_, ()> {
        Box::pin(async { Ok(()) })
    }
}

#[tokio::test]
async fn the_traits_default_operations() {
    let camera: Arc<dyn Camera> = Arc::new(Bare(michrome20()));
    let error = camera.trigger().await.unwrap_err();
    assert_eq!(
        error.to_string(),
        "the device doesn't support a software trigger"
    );
    assert!(matches!(
        camera.temperature().await,
        Err(Error::Unsupported { .. })
    ));
    assert!(camera.configure(settings()).await.is_ok());

    let stage: Arc<dyn Stage> = Arc::new(Teleport {
        capabilities: stage_capabilities(),
        track: Arc::new(PositionTrack::new(16)),
    });
    assert!(matches!(stage.home().await, Err(Error::Unsupported { .. })));
    let start = Xyz::new(Um(1_000.0), Um(2_000.0), Um(3_000.0));
    stage.move_to(start, None).await.unwrap();
    // A relative move goes from where the stage is.
    let delta = Xyz::new(Um(500.0), Um(-2_000.0), Um(0.0));
    let end = stage.move_by(delta, None).await.unwrap();
    assert_eq!(end.position, Xyz::new(Um(1_500.0), Um(0.0), Um(3_000.0)));
    assert!(
        stage.move_by(delta, None).await.is_err(),
        "y would be -2000 µm"
    );
}
