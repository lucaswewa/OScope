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
