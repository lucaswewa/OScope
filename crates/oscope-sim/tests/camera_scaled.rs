//! The simulated camera on a scaled clock, where the host's speed matters:
//! dropping frames when rendering falls behind, and the HAL's conformance
//! suite. They run here, away from the stepped tests, and one at a time,
//! so that no other rendering slows theirs.

mod common;

use std::time::Duration;

use common::{rig, set, start};
use oscope_core::{Clock, Ms, Point2};
use oscope_hal::{Axis, Camera, Stage, UmPerS, conformance};
use oscope_sim::{Rate, SimCameraConfig};
use tokio::sync::Mutex;

/// Each test holds it while it runs.
static ONE_AT_A_TIME: Mutex<()> = Mutex::const_new(());

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn slow_rendering_drops_frames_but_not_the_motion() {
    let _turn = ONE_AT_A_TIME.lock().await;
    // At 300× the clock outruns the rendering: frames that ended meanwhile
    // are skipped and counted, and each delivered frame starts on the
    // schedule.
    let rig = rig(
        SimCameraConfig::small(384, 288),
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
    // The stage kept to the clock all the while: 1 mm in its first 10 s,
    // whatever the host's speed, long before the jog reaches x's limit.
    let until = jogged + Duration::from_secs(10);
    while rig.clock.now() < until {
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
    let moved = rig.stage.true_position(until).x.0 - rig.stage.true_position(jogged).x.0;
    assert!((moved - 1_000.0).abs() < 1e-6, "{moved} µm in 10 s");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_camera_conforms() {
    let _turn = ONE_AT_A_TIME.lock().await;
    // At a tenth of real time, each frame has ten times its interval to
    // render in: enough on a loaded CI runner, in a debug build.
    let rig = rig(
        SimCameraConfig::small(192, 144),
        true,
        Point2::new(31_000.0, 11_025.0),
        Rate::Scaled(0.1),
    );
    conformance::camera(&rig.camera).await.unwrap();
}
