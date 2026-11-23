//! The simulator's lights and objective mount set what its camera sees.

mod common;

use std::sync::Arc;
use std::time::Duration;

use common::{rig, set, start};
use oscope_core::{LightState, Ms, Point2, Timestamp};
use oscope_hal::{Camera, Illumination, ObjectiveCatalogue, ObjectiveMount};
use oscope_sim::{Rate, SimCameraConfig, SimLights, SimMount};

#[tokio::test]
async fn the_lights_and_the_mount_set_what_the_camera_sees() {
    // The test target, at a square's corner, without noise.
    let mut config = SimCameraConfig::small(192, 144);
    config.sensor.noise = false;
    let rig = rig(config, true, Point2::new(31_050.0, 11_050.0), Rate::Stepped);
    let (clock, camera) = (rig.clock, Arc::new(rig.camera));
    let lights = SimLights::new(Arc::clone(&camera));
    let mount = SimMount::new(ObjectiveCatalogue::mitutoyo(), Arc::clone(&camera));
    set(&camera, |s| s.exposure = Ms(5.0)).await;
    let frames = start(&camera).await;
    let interval = Duration::from_secs_f64(1.0 / 7.24);
    let mut k = 0;
    let mut frame = || {
        clock.advance_to(Timestamp(0) + interval * k + Duration::from_millis(66));
        k += 1;
        let (buffer, meta) = frames.recv_timeout(Duration::from_secs(60)).unwrap();
        let counts: Vec<u16> = buffer.bytes()[..(meta.width * meta.height * 2) as usize]
            .chunks(2)
            .map(|b| u16::from_le_bytes([b[0], b[1]]) >> 4)
            .collect();
        counts
    };
    let mean = |c: &[u16]| c.iter().map(|v| f64::from(*v)).sum::<f64>() / c.len() as f64;
    let light = |channel: &str, on: bool| LightState {
        channel: channel.to_owned(),
        on,
        intensity: 1.0,
    };

    // Transmitted light, at first; then reflected, where the target's
    // glass reflects only 4%.
    let transmitted = mean(&frame());
    lights.set(light("transmitted", false)).await.unwrap();
    lights.set(light("reflected", true)).await.unwrap();
    let reflected = mean(&frame());
    assert!(
        transmitted > 4.0 * reflected,
        "{transmitted} against {reflected}"
    );
    // Every light off: only black.
    lights.set(light("reflected", false)).await.unwrap();
    assert!(frame().iter().all(|c| *c == 125));

    // Through the 10× and then the 50×, the same corner looks different.
    lights.set(light("transmitted", true)).await.unwrap();
    mount.fit("10x".to_owned()).await.unwrap();
    let ten = frame();
    mount.fit("50x".to_owned()).await.unwrap();
    let fifty = frame();
    assert_ne!(ten, fifty);
    camera.stop().await.unwrap();
}

#[tokio::test]
async fn a_configuration_the_simulator_cannot_be_is_refused_before_it_is_made() {
    use oscope_sim::{Simulator, SimulatorConfig};
    assert_eq!(SimulatorConfig::default().check(), Ok(()));
    type Change = fn(&mut SimulatorConfig);
    let cases: [(Change, &str); 9] = [
        (|c| c.rate = 0.0, "a clock rate of 0"),
        (|c| c.scale = 0.0, "a scale of 0"),
        (|c| c.backlash = -1.0, "a backlash of -1"),
        (|c| c.rotation = f64::NAN, "a rotation of NaN"),
        (|c| c.latency = -1.0, "a stage latency of -1"),
        (|c| c.lost_steps = [2.0, 5.0], "a chance of 2"),
        (|c| c.sensor = "eighth".into(), "a sensor `eighth`"),
        (|c| c.objective = "7x".into(), "7x"),
        (|c| c.sample = "no such".into(), "a sample `no such`"),
    ];
    for (change, why) in cases {
        let mut config = SimulatorConfig::default();
        change(&mut config);
        let refused = config.check().unwrap_err();
        assert!(refused.contains(why), "{why}: {refused}");
        // Making it is refused the same way, before anything is made.
        let made = Simulator::new(&config, std::path::Path::new(".")).await;
        assert_eq!(made.err(), Some(refused));
    }
}

#[tokio::test]
async fn the_camera_renders_on_the_gpu_unless_told_not_to() {
    use oscope_sim::{Simulator, SimulatorConfig};
    let quarter = |gpu: bool| SimulatorConfig {
        sensor: "quarter".to_owned(),
        gpu,
        ..SimulatorConfig::default()
    };
    let simulator = Simulator::new(&quarter(false), std::path::Path::new("."))
        .await
        .unwrap();
    assert!(!simulator.camera.on_gpu());
    assert_eq!(
        simulator.on_cpu.as_deref(),
        Some("the configuration says `gpu = false`")
    );
    // By default: on the GPU, with `OSCOPE_CUDA=1` and a build with CUDA;
    // without CUDA, on the CPU, saying so.
    let simulator = Simulator::new(&quarter(true), std::path::Path::new("."))
        .await
        .unwrap();
    if !cfg!(feature = "cuda") {
        assert!(!simulator.camera.on_gpu());
        assert_eq!(simulator.on_cpu.as_deref(), Some("the build has no CUDA"));
    } else if std::env::var("OSCOPE_CUDA").as_deref() == Ok("1") {
        assert!(simulator.camera.on_gpu(), "{:?}", simulator.on_cpu);
        assert_eq!(simulator.on_cpu, None);
    }
}

#[test]
fn the_gpu_s_tiles_are_sized_by_the_sensor_and_the_free_memory() {
    use oscope_sim::devices::{GPU_SLOTS, SLOT_BYTES, gpu_slots};
    let free = 8 << 30;
    // The full sensor: three frames of 368 tiles, at most 1024; the quarter
    // sensor, three of 35.
    assert_eq!(gpu_slots(5472, 3648, free), Some(GPU_SLOTS));
    assert_eq!(gpu_slots(1368, 912, free), Some(105));
    // A quarter of what's free, and none below a frame's worth.
    assert_eq!(gpu_slots(5472, 3648, 4 * 500 * SLOT_BYTES), Some(500));
    assert_eq!(gpu_slots(5472, 3648, 1 << 30), None);
}
