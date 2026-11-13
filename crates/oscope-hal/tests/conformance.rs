//! The conformance suite against the fakes and the manual implementations,
//! and a scripted failure of each fake failing it.

use std::sync::Arc;

use oscope_core::{Clock, Ms, SystemClock, Um};
use oscope_hal::conformance::{self, Failure};
use oscope_hal::fake::{FakeCamera, FakeIllumination, FakeObjectiveMount, FakeStage, Script};
use oscope_hal::{
    Error, ManualIllumination, ManualObjectiveMount, ObjectiveCatalogue, ObjectiveMount, Shutter,
    TriggerCapabilities,
};

fn clock() -> Arc<dyn Clock> {
    Arc::new(SystemClock::new())
}

#[tokio::test]
async fn the_fakes_conform() {
    conformance::camera(&FakeCamera::new(clock()))
        .await
        .unwrap();
    conformance::stage(&FakeStage::new(clock()), Um(0.0))
        .await
        .unwrap();
    conformance::illumination(&FakeIllumination::two_lights())
        .await
        .unwrap();
    let mount = FakeObjectiveMount::new(ObjectiveCatalogue::mitutoyo());
    conformance::objective_mount(&mount).await.unwrap();
}

#[tokio::test]
async fn plainer_devices_conform() {
    // A global shutter, and no ROI, trigger or temperature.
    let mut caps = FakeCamera::default_capabilities();
    caps.shutter = Shutter::Global;
    for mode in &mut caps.modes {
        mode.line_time = None;
    }
    caps.roi = None;
    caps.trigger = TriggerCapabilities::default();
    caps.temperature = false;
    conformance::camera(&FakeCamera::with_capabilities(caps, clock()))
        .await
        .unwrap();

    let mut caps = FakeStage::default_capabilities();
    caps.homing = false;
    let stage = FakeStage::with_capabilities(caps, clock());
    conformance::stage(&stage, Um(0.0)).await.unwrap();
}

#[tokio::test]
async fn the_manual_implementations_conform() {
    conformance::illumination(&ManualIllumination::brightfield())
        .await
        .unwrap();
    let mount = ManualObjectiveMount::new(ObjectiveCatalogue::mitutoyo());
    mount.fit("10x".into()).await.unwrap();
    conformance::objective_mount(&mount).await.unwrap();
    // The suite puts the objective back.
    assert_eq!(mount.fitted().await.unwrap().unwrap().name, "10x");
}

/// Runs a suite on a fresh fake whose `operation` will fail once, and gives
/// the check that caught it.
async fn caught(operation: &'static str, suite: &str) -> Failure {
    let scripted = || Error::Device {
        message: "scripted".into(),
    };
    let arm = |script: &Script| script.fail_next(operation, scripted());
    let result = match suite {
        "camera" => {
            let camera = FakeCamera::new(clock());
            arm(camera.script());
            conformance::camera(&camera).await
        }
        "stage" => {
            let stage = FakeStage::new(clock());
            arm(stage.script());
            conformance::stage(&stage, Um(0.0)).await
        }
        "lights" => {
            let lights = FakeIllumination::two_lights();
            arm(lights.script());
            conformance::illumination(&lights).await
        }
        _ => {
            let mount = FakeObjectiveMount::new(ObjectiveCatalogue::mitutoyo());
            arm(mount.script());
            conformance::objective_mount(&mount).await
        }
    };
    let failure = result.unwrap_err();
    assert!(failure.detail.contains("scripted"), "{failure}");
    failure
}

#[tokio::test]
async fn a_scripted_failure_fails_the_suite() {
    for (suite, operation, check) in [
        ("camera", "settings", "read the settings"),
        ("camera", "configure", "configure every mode and format"),
        ("camera", "start", "start streaming"),
        ("camera", "stop", "stop streaming"),
        ("camera", "trigger", "one frame for each software trigger"),
        ("camera", "temperature", "a temperature, if it has one"),
        ("stage", "home", "home"),
        ("stage", "move_to", "move to a target"),
        ("stage", "position", "read the position"),
        ("stage", "jog", "jog"),
        ("stage", "stop", "stop a jog"),
        ("lights", "state", "read the state"),
        ("lights", "set", "set each light"),
        ("mount", "fit", "fit each objective"),
        ("mount", "fitted", "read the objective fitted"),
    ] {
        let failure = caught(operation, suite).await;
        assert_eq!(failure.check, check, "{suite}'s {operation}: {failure}");
    }
}

#[tokio::test]
async fn a_stage_off_its_target_fails_the_suite() {
    // A tolerance of -1 µm can't be met, so the first position checked,
    // after homing, fails.
    let failure = conformance::stage(&FakeStage::new(clock()), Um(-1.0))
        .await
        .unwrap_err();
    assert_eq!(failure.check, "home");
    let mut caps = FakeStage::default_capabilities();
    caps.report_interval = Ms(0.0);
    let stage = FakeStage::with_capabilities(caps, clock());
    let failure = conformance::stage(&stage, Um(0.0)).await.unwrap_err();
    assert_eq!(failure.check, "capabilities that make sense");
}
