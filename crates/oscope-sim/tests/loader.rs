//! The sample catalogue and the slide loader: what a configuration lists,
//! which samples carry ground truth, loading and unloading, and what the
//! camera sees once the stage is empty.

mod common;

use std::sync::PoisonError;
use std::time::Duration;

use common::{FOCUS, rig, set, start};
use oscope_core::{Ms, Point2, Timestamp, Um};
use oscope_hal::Camera;
use oscope_sim::{
    Dye, Light, Rate, SampleCatalogue, SampleKind, SensorModel, SimCameraConfig, SlideLoader,
};

const CONFIGURATION: &str = r#"
[[samples]]
name = "H&E tissue"
kind = "tissue"
seed = 7

[[samples]]
name = "my section"
kind = "image"
path = "section.png"
um_per_pixel = 0.25

[[samples]]
name = "my stack"
kind = "stack"
path = "stack.tif"
um_per_pixel = 0.1
z_step = 0.3
light = "fluorescent"
dye = "far-red"
"#;

#[test]
fn the_catalogue_lists_its_samples_and_their_truth() {
    // The built-in samples, each with ground truth.
    let builtin = SampleCatalogue::builtin();
    assert_eq!(builtin.samples.len(), 5);
    assert!(builtin.samples.iter().all(|s| s.has_ground_truth()));
    let kinds: Vec<&str> = builtin.samples.iter().map(|s| s.kind.as_str()).collect();
    assert_eq!(
        kinds,
        ["tissue", "test-target", "surface", "cells", "structures"]
    );
    // A configuration adds imports, which have none, and replaces a
    // built-in of the same name.
    let catalogue = SampleCatalogue::from_toml(CONFIGURATION).unwrap();
    let names: Vec<&str> = catalogue.samples.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "test target",
            "reflective surface",
            "fluorescent cells",
            "sub-diffraction structures",
            "H&E tissue",
            "my section",
            "my stack"
        ]
    );
    let tissue = catalogue.get("H&E tissue").unwrap();
    assert_eq!(
        (tissue.kind, tissue.seed, tissue.has_ground_truth()),
        (SampleKind::Tissue, 7, true)
    );
    let section = catalogue.get("my section").unwrap();
    assert!(!section.has_ground_truth());
    assert_eq!(catalogue.get("my stack").unwrap().dye, Dye::FarRed);
    // What a configuration can't say.
    for wrong in [
        "[[samples]]\nname = \"x\"\nkind = \"photo\"",
        "[[samples]]\nname = \"x\"\nkind = \"tissue\"\ncolour = \"red\"",
        "[[samples]]\nkind = \"tissue\"",
    ] {
        assert!(SampleCatalogue::from_toml(wrong).is_err(), "{wrong}");
    }
}

#[test]
fn loading_puts_a_sample_on_the_stage_and_unloading_takes_it_off() {
    let folder = tempfile::tempdir().unwrap();
    let world = std::sync::Arc::new(std::sync::RwLock::new(oscope_sim::World::empty()));
    let catalogue = SampleCatalogue::from_toml(CONFIGURATION).unwrap();
    let loader = SlideLoader::new(
        world.clone(),
        catalogue,
        folder.path(),
        Point2::new(5_000.0, 5_000.0),
    );
    let on_stage = || {
        let world = world.read().unwrap_or_else(PoisonError::into_inner);
        world
            .slide
            .as_ref()
            .map(|s| (s.sample.name().to_owned(), s.origin))
    };
    assert_eq!((loader.loaded(), on_stage()), (None, None));
    loader.load("fluorescent cells").unwrap();
    assert_eq!(loader.loaded().as_deref(), Some("fluorescent cells"));
    assert_eq!(
        on_stage(),
        Some((
            "fluorescent cell culture".to_owned(),
            Point2::new(5_000.0, 5_000.0)
        ))
    );
    // A sample that isn't there, or can't be built, leaves the stage as it
    // was: "my section"'s file is missing.
    assert!(loader.load("no such sample").is_err());
    let error = loader.load("my section").unwrap_err();
    assert!(error.contains("section.png"), "{error}");
    assert_eq!(loader.loaded().as_deref(), Some("fluorescent cells"));
    assert_eq!(on_stage().unwrap().0, "fluorescent cell culture");
    // Another load replaces it; unloading empties the stage.
    loader.load("reflective surface").unwrap();
    assert_eq!(on_stage().unwrap().0, "reflective surface");
    assert_eq!(loader.unload().as_deref(), Some("reflective surface"));
    assert_eq!((loader.loaded(), on_stage()), (None, None));
    assert_eq!(loader.unload(), None);
}

#[tokio::test]
async fn unloading_leaves_only_background_and_noise() {
    // A camera over the tissue, in transmitted light at 0.3 of full
    // intensity for 10 ms: 2,400 electrons a pixel where nothing is in the
    // way. Unloaded, every pixel sees that, and only the sensor's noise.
    let rig = rig(
        SimCameraConfig::small(192, 144),
        false,
        Point2::new(45_000.0, 12_500.0),
        Rate::Stepped,
    );
    rig.camera
        .world()
        .write()
        .unwrap_or_else(PoisonError::into_inner)
        .focus = Um(FOCUS);
    let loader = SlideLoader::new(
        rig.camera.world(),
        SampleCatalogue::builtin(),
        std::path::Path::new("."),
        Point2::new(0.0, 0.0),
    );
    rig.camera.set_light(0.3, Light::Transmitted);
    set(&rig.camera, |s| s.exposure = Ms(10.0)).await;
    let frames = start(&rig.camera).await;
    // Each frame ends 60 ms of readout and 10 ms after it starts, and the
    // next starts 1 / 7.24 s later.
    let interval = Duration::from_secs_f64(1.0 / 7.24);
    let frame = |k: u32| {
        rig.clock
            .advance_to(Timestamp(0) + interval * k + Duration::from_millis(71));
        let (buffer, meta) = frames.recv_timeout(Duration::from_secs(60)).unwrap();
        let counts: Vec<f64> = buffer.bytes()[..(meta.width * meta.height * 2) as usize]
            .chunks(2)
            .map(|b| f64::from(u16::from_le_bytes([b[0], b[1]]) >> 4))
            .collect();
        counts
    };
    loader.load("H&E tissue").unwrap();
    let loaded = frame(0);
    loader.unload();
    let empty = frame(1);
    rig.camera.stop().await.unwrap();

    let stats = |counts: &[f64]| {
        let m = counts.iter().sum::<f64>() / counts.len() as f64;
        let v = counts.iter().map(|c| (c - m).powi(2)).sum::<f64>() / (counts.len() - 1) as f64;
        (m, v)
    };
    let s = SensorModel::default();
    let cpe = s.counts_per_electron(0.0);
    let electrons = 0.3 * s.electrons_per_ms * 10.0;
    let (mean, variance) = stats(&empty);
    let expected = f64::from(s.black) + electrons * cpe;
    let expected_variance = (electrons + s.read_noise * s.read_noise) * cpe * cpe + 1.0 / 12.0;
    assert!((mean - expected).abs() < 0.5, "{mean} against {expected}");
    assert!(
        (variance / expected_variance - 1.0).abs() < 0.05,
        "{variance} against {expected_variance}"
    );
    // The tissue, or the glass, was there before.
    assert!(stats(&loaded).0 < expected - 50.0, "{}", stats(&loaded).0);
}
