//! The manual implementations: lights the user switches, and the mount whose
//! objective the user swaps, each used through its trait as P16 will.

use std::sync::Arc;

use oscope_core::LightState;
use oscope_hal::{
    Error, Illumination, LightChannel, LightKind, ManualIllumination, ManualObjectiveMount,
    ObjectiveCatalogue, ObjectiveMount,
};
use serde_json::json;

fn light(channel: &str, on: bool, intensity: f64) -> LightState {
    LightState {
        channel: channel.to_owned(),
        on,
        intensity,
    }
}

#[tokio::test]
async fn manual_lights_record_what_the_user_set() {
    let lights: Arc<dyn Illumination> = Arc::new(ManualIllumination::brightfield());
    assert_eq!(
        serde_json::to_value(lights.capabilities()).unwrap(),
        json!({"model": "manual", "channels": [
            {"name": "transmitted", "kind": "transmitted", "dimmable": true}
        ]})
    );
    assert_eq!(
        lights.state().await.unwrap(),
        [light("transmitted", false, 1.0)]
    );

    let set = light("transmitted", true, 0.6);
    assert_eq!(lights.set(set.clone()).await.unwrap(), set);
    assert_eq!(lights.state().await.unwrap(), std::slice::from_ref(&set));

    // What the lights refuse leaves them as they were.
    let error = lights
        .set(light("transmitted", true, 1.5))
        .await
        .unwrap_err();
    assert_eq!(
        error.to_string(),
        "the intensity must be from 0 to 1, and was 1.5"
    );
    let error = lights.set(light("ring", true, 0.5)).await.unwrap_err();
    assert!(matches!(
        error,
        Error::Unknown {
            kind: "light channel",
            ..
        }
    ));
    assert!(
        lights
            .set(light("transmitted", false, f64::NAN))
            .await
            .is_err()
    );
    assert_eq!(lights.state().await.unwrap(), [set]);
}

#[tokio::test]
async fn a_light_that_can_t_dim_is_at_full_intensity() {
    let lights = ManualIllumination::new(vec![
        LightChannel {
            name: "transmitted".into(),
            kind: LightKind::Transmitted,
            dimmable: true,
        },
        LightChannel {
            name: "ring".into(),
            kind: LightKind::Reflected,
            dimmable: false,
        },
    ]);
    let applied = lights.set(light("ring", true, 0.3)).await.unwrap();
    assert_eq!(applied, light("ring", true, 1.0));
    assert_eq!(
        lights.state().await.unwrap(),
        [light("transmitted", false, 1.0), light("ring", true, 1.0)]
    );
}

#[tokio::test]
async fn the_manual_mount_records_which_objective_was_fitted() {
    let mount: Arc<dyn ObjectiveMount> =
        Arc::new(ManualObjectiveMount::new(ObjectiveCatalogue::mitutoyo()));
    assert_eq!(
        serde_json::to_value(mount.capabilities()).unwrap(),
        json!({"model": "manual", "motorised": false})
    );
    assert_eq!(mount.catalogue().objectives.len(), 4);
    assert_eq!(mount.fitted().await.unwrap(), None);

    let twenty = mount.fit("20x".into()).await.unwrap();
    assert_eq!((twenty.name.as_str(), twenty.na), ("20x", 0.42));
    assert_eq!(mount.fitted().await.unwrap(), Some(twenty.clone()));

    // An objective not in the catalogue changes nothing.
    let error = mount.fit("7x".into()).await.unwrap_err();
    assert!(matches!(
        error,
        Error::Unknown {
            kind: "objective",
            ..
        }
    ));
    assert_eq!(mount.fitted().await.unwrap(), Some(twenty));
}
