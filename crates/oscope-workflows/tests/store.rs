//! The calibration store (P32; ADR-0023): versions per key, one active set
//! per key, export and import, refusals, and the store read back from its
//! files; dark and flat sets with their frames (P33).

use oscope_compute::{Image, Shape};
use oscope_core::{Um, Xyz};
use oscope_hal::Travel;
use oscope_workflows::calibration::{
    CalibrationStore, Dark, Geometry, Instrument, PerAxis, Region, STAGE_KEY, SoftLimits,
    StageValues, StoreError, Values, dark_key, exposure_octave, geometry_key,
};
use serde_json::json;
use uuid::Uuid;

fn instrument() -> Instrument {
    Instrument {
        camera: "a camera".to_owned(),
        sensor_pixel_um: 2.4,
        binning: 1,
        stage: "a stage".to_owned(),
        objective: None,
        tube_lens_um: 200_000.0,
    }
}

/// A geometry of `objective` at `binning`: a pixel of `pixel` µm, turned
/// 90°.
fn geometry(objective: &str, binning: u32, pixel: f64) -> Values {
    Values::Geometry(Geometry {
        objective: objective.to_owned(),
        binning,
        nominal_pixel_um: 0.24,
        matrix: [[0.0, -pixel], [pixel, 0.0]],
        pixel_um: 0.0,
        rotation_deg: 0.0,
        mirrored: true,
        aspect: 0.0,
        skew: 0.0,
        residual_px: 0.05,
        residuals: vec![[0.04, -0.03], [-0.06, 0.05]],
        step_um: 123.0,
        moves: 8,
    })
}

fn stage(backlash: f64) -> Values {
    let travel = |max: f64| Travel {
        min: Um(0.0),
        max: Um(max),
    };
    Values::Stage(StageValues {
        backlash_um: PerAxis {
            x: backlash,
            y: backlash,
        },
        soft_limits: SoftLimits {
            x: travel(100_000.0),
            y: travel(75_000.0),
            z: travel(20_000.0),
        },
        homed_at: Some(Xyz::new(Um(1_000.0), Um(1_000.0), Um(19_000.0))),
    })
}

#[test]
fn versions_count_up_per_key_and_one_set_of_each_is_active() {
    let dir = tempfile::tempdir().unwrap();
    let store = CalibrationStore::open(dir.path()).unwrap();
    let save = |values, activate| {
        store
            .save(values, instrument(), Some("ann".to_owned()), activate)
            .unwrap()
    };
    let first = save(geometry("10x", 1, 0.24), true);
    let second = save(geometry("10x", 1, 0.241), true);
    let binned = save(geometry("10x", 2, 0.48), true);
    let other = save(geometry("20x", 1, 0.12), false);
    let stage_one = save(stage(2.0), true);
    assert_eq!(
        [
            first.version,
            second.version,
            binned.version,
            other.version,
            stage_one.version
        ],
        [1, 2, 1, 1, 1]
    );
    assert_eq!(first.account.as_deref(), Some("ann"));
    // A geometry's decomposition is made from its matrix.
    let Values::Geometry(g) = &second.values else {
        panic!()
    };
    assert!((g.pixel_um - 0.241).abs() < 1e-12 && (g.rotation_deg - 90.0).abs() < 1e-9);
    assert!(!g.mirrored && (g.aspect - 1.0).abs() < 1e-12 && g.skew.abs() < 1e-12);
    // The latest of a key, if made active; the 20× has none.
    let active = |key: &str| store.active(key).map(|s| s.id);
    assert_eq!(active(&geometry_key("10x", 1)), Some(second.id));
    assert_eq!(active(&geometry_key("10x", 2)), Some(binned.id));
    assert_eq!(active(&geometry_key("20x", 1)), None);
    assert_eq!(active(STAGE_KEY), Some(stage_one.id));
    // An earlier version made active again.
    store.activate(first.id).unwrap();
    assert_eq!(active(&geometry_key("10x", 1)), Some(first.id));
    let listed = store.list();
    assert_eq!(listed.len(), 5);
    assert_eq!(listed[0].id, stage_one.id, "newest first");
    let flags: Vec<(Uuid, bool)> = listed.iter().map(|s| (s.id, s.active)).collect();
    assert!(flags.contains(&(first.id, true)) && flags.contains(&(second.id, false)));
    assert!(matches!(
        store.activate(Uuid::nil()),
        Err(StoreError::NotFound(_))
    ));
    assert!(matches!(
        store.get(Uuid::nil()),
        Err(StoreError::NotFound(_))
    ));
}

#[test]
fn the_store_reads_back_its_files_and_skips_what_isnt_a_set() {
    let dir = tempfile::tempdir().unwrap();
    let (kept, active) = {
        let store = CalibrationStore::open(dir.path()).unwrap();
        let kept = store
            .save(geometry("10x", 1, 0.24), instrument(), None, true)
            .unwrap();
        let later = store
            .save(geometry("10x", 1, 0.25), instrument(), None, false)
            .unwrap();
        (vec![kept.clone(), later], kept.id)
    };
    std::fs::write(dir.path().join("notes.json"), "{\"not\": \"a set\"}").unwrap();
    std::fs::write(dir.path().join("readme.txt"), "not JSON").unwrap();
    let store = CalibrationStore::open(dir.path()).unwrap();
    for set in &kept {
        assert_eq!(&store.get(set.id).unwrap(), set);
    }
    assert_eq!(store.list().len(), 2);
    assert_eq!(
        store.active(&geometry_key("10x", 1)).map(|s| s.id),
        Some(active)
    );
    // The next version follows those read back.
    let next = store
        .save(geometry("10x", 1, 0.26), instrument(), None, false)
        .unwrap();
    assert_eq!(next.version, 3);
    // A set made active by hand is read back active too.
    store.activate(kept[1].id).unwrap();
    let again = CalibrationStore::open(dir.path()).unwrap();
    let key = geometry_key("10x", 1);
    assert_eq!(again.active(&key).map(|s| s.id), Some(kept[1].id));
}

#[test]
fn a_set_exports_and_imports_as_the_next_version_not_active() {
    let (here, there) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let (from, to) = (
        CalibrationStore::open(here.path()).unwrap(),
        CalibrationStore::open(there.path()).unwrap(),
    );
    let made = from
        .save(stage(4.5), instrument(), Some("bo".to_owned()), true)
        .unwrap();
    let exported = from.export(made.id).unwrap();
    let local = to.save(stage(2.0), instrument(), None, true).unwrap();
    let imported = to.import(exported.clone()).unwrap();
    assert_ne!(imported.id, made.id);
    assert_eq!(imported.imported_from, Some(made.id));
    assert_eq!((imported.version, imported.created), (2, made.created));
    assert_eq!(
        (imported.account, imported.values),
        (made.account, made.values)
    );
    assert_eq!(
        to.active(STAGE_KEY).map(|s| s.id),
        Some(local.id),
        "an import isn't active"
    );
    // Refused: not a set, another format, values that aren't sane.
    let refused = |value: serde_json::Value| match to.import(value) {
        Err(StoreError::Invalid(why)) => why,
        other => panic!("{other:?}"),
    };
    assert!(refused(json!({"format": 1})).contains("not a calibration set"));
    let mut later = exported.clone();
    later["format"] = json!(2);
    assert!(refused(later).contains("format 2"));
    let mut broken = serde_json::to_value(
        from.save(geometry("5x", 1, 0.48), instrument(), None, false)
            .unwrap(),
    )
    .unwrap();
    broken["values"]["matrix"] = json!([[1.0, 2.0], [2.0, 4.0]]);
    assert!(refused(broken).contains("can't be inverted"));
    let mut limits = exported;
    limits["values"]["soft_limits"]["x"] = json!({"min": 5.0, "max": 1.0});
    assert!(refused(limits).contains("soft limits"));
    assert_eq!(to.list().len(), 2, "nothing refused was kept");
}

/// A dark frame taken at `taken_ms`, gain 1, in the mode `full`.
fn dark(taken_ms: f64) -> Values {
    Values::Dark(Dark {
        mode: "full".to_owned(),
        gain: 1.0,
        exposure_ms: exposure_octave(taken_ms),
        taken_ms,
        region: Region {
            x: 0,
            y: 0,
            width: 4,
            height: 2,
        },
        frames: 16,
        mean: 0.03,
        spread: 0.001,
    })
}

#[test]
fn a_dark_frame_keeps_its_image_and_stays_on_its_instrument() {
    let folder = tempfile::tempdir().unwrap();
    let store = CalibrationStore::open(folder.path()).unwrap();
    let data = (0..8).map(|i| i as f32 / 100.0).collect();
    let image = Image::from_vec(Shape::new(4, 2, 1), data).unwrap();
    // Saved with its image, and only so.
    let without = store.save(dark(30.0), instrument(), None, true);
    assert!(
        matches!(without, Err(StoreError::Invalid(_))),
        "{without:?}"
    );
    let with = store.save_with_image(stage(2.0), instrument(), None, true, &image);
    assert!(matches!(with, Err(StoreError::Invalid(_))), "{with:?}");
    let saved = store
        .save_with_image(dark(30.0), instrument(), None, true, &image)
        .unwrap();
    assert_eq!(store.image(saved.id).unwrap(), image);
    // One dark for an octave of exposure: 30 ms's serves 20 ms, not 40.
    assert_eq!(saved.values.key(), dark_key("full", 1.0, 20.0));
    assert_ne!(saved.values.key(), dark_key("full", 1.0, 40.0));
    // Read back, active, with its image.
    let again = CalibrationStore::open(folder.path()).unwrap();
    let active = again.active(&dark_key("full", 1.0, 30.0));
    assert_eq!(active.map(|s| s.id), Some(saved.id));
    assert_eq!(again.image(saved.id).unwrap(), image);
    // Its frame is this instrument's: neither exported nor imported.
    let exported = again.export(saved.id);
    assert!(
        matches!(exported, Err(StoreError::Invalid(_))),
        "{exported:?}"
    );
    let imported = again.import(serde_json::to_value(&saved).unwrap());
    assert!(
        matches!(imported, Err(StoreError::Invalid(_))),
        "{imported:?}"
    );
    assert_eq!(again.list().len(), 1);
}
