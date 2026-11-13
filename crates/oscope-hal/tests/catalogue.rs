//! The objective catalogue: Mitutoyo's defaults, the TOML file, other tube
//! lenses, and catalogues that break the rules.

use oscope_core::Um;
use oscope_hal::{Error, ObjectiveCatalogue};

/// The MIChrome 20's pixel.
const PIXEL: Um = Um(2.4);

#[test]
fn the_defaults_are_mitutoyo_s_published_values() {
    let catalogue = ObjectiveCatalogue::mitutoyo();
    let found: Vec<_> = catalogue
        .objectives
        .iter()
        .map(|o| (o.name.as_str(), o.na, o.working_distance.mm()))
        .collect();
    assert_eq!(
        found,
        [
            ("5x", 0.14, 34.0),
            ("10x", 0.28, 33.5),
            ("20x", 0.42, 20.0),
            ("50x", 0.55, 13.0)
        ]
    );
    assert_eq!(catalogue.tube_lens, Um::from_mm(200.0));
    // Plan §5.9: a pixel at the sample is 0.48, 0.24, 0.12 and 0.048 µm.
    for (name, size) in [("5x", 0.48), ("10x", 0.24), ("20x", 0.12), ("50x", 0.048)] {
        let objective = catalogue.get(name).unwrap();
        let pixel = catalogue.pixel_size(objective, PIXEL);
        assert!((pixel.0 - size).abs() < 1e-12, "{name}: {pixel}");
    }
}

#[test]
fn the_shipped_file_is_the_default_catalogue() {
    let file = include_str!("../../../configs/objectives.toml");
    assert_eq!(
        ObjectiveCatalogue::from_toml(file).unwrap(),
        ObjectiveCatalogue::mitutoyo()
    );
}

#[test]
fn magnification_follows_the_tube_lens() {
    // Mitutoyo's 20× behind a 180 mm tube lens magnifies 18×.
    let mut catalogue = ObjectiveCatalogue::mitutoyo();
    catalogue.tube_lens = Um::from_mm(180.0);
    let twenty = catalogue.get("20x").unwrap().clone();
    assert!((catalogue.magnification(&twenty) - 18.0).abs() < 1e-12);

    // An objective designed for 180 mm, behind the 200 mm tube lens.
    let text = r#"
        tube_lens = 200000
        [[objective]]
        name = "40x"
        magnification = 40
        na = 0.65
        working_distance = 600
        design_tube_lens = 180000
    "#;
    let catalogue = ObjectiveCatalogue::from_toml(text).unwrap();
    let forty = catalogue.get("40x").unwrap();
    assert_eq!(forty.model, "");
    assert!((catalogue.magnification(forty) - 40.0 * 200.0 / 180.0).abs() < 1e-12);
    assert!((catalogue.pixel_size(forty, PIXEL).0 - 0.054).abs() < 1e-12);
}

/// A change that breaks one of the catalogue's rules.
type Breaks = fn(&mut ObjectiveCatalogue);

#[test]
fn each_rule_rejects_a_catalogue_that_breaks_it() {
    let broken: [(&str, Breaks); 9] = [
        ("it has no objectives", |c| c.objectives.clear()),
        ("its tube lens is 0 µm", |c| c.tube_lens = Um(0.0)),
        ("an objective has no name", |c| {
            c.objectives[0].name = " ".into()
        }),
        ("two objectives are named \"5x\"", |c| {
            c.objectives[1].name = "5x".into()
        }),
        ("\"5x\"'s magnification is NaN", |c| {
            c.objectives[0].magnification = f64::NAN
        }),
        ("\"10x\"'s numerical aperture is 0", |c| {
            c.objectives[1].na = 0.0
        }),
        ("\"50x\"'s numerical aperture is 1.7", |c| {
            c.objectives[3].na = 1.7
        }),
        ("\"20x\"'s working distance is -1 µm", |c| {
            c.objectives[2].working_distance = Um(-1.0)
        }),
        ("\"5x\"'s design tube lens is inf µm", |c| {
            c.objectives[0].design_tube_lens = Um(f64::INFINITY)
        }),
    ];
    for (reason, breaks) in broken {
        let mut catalogue = ObjectiveCatalogue::mitutoyo();
        breaks(&mut catalogue);
        let error = catalogue.validate().unwrap_err();
        assert!(
            matches!(&error, Error::InvalidCatalogue { reason: r } if r == reason),
            "{reason}: got {error}"
        );
    }
    // An oil immersion objective's 1.4 is allowed.
    let mut catalogue = ObjectiveCatalogue::mitutoyo();
    catalogue.objectives[3].na = 1.4;
    catalogue.validate().unwrap();
}

#[test]
fn a_file_that_isn_t_a_catalogue_is_rejected() {
    let base = include_str!("../../../configs/objectives.toml");
    for text in [
        "tube_lens = ",                                                // not TOML
        &base.replace("na = 0.14\n", ""),                              // a field missing
        &base.replace("na = 0.14", "na = 0.14\nfocal_length = 40"),    // an unknown field
        &base.replace("tube_lens = 200000", "tube_lens = \"200 mm\""), // the wrong type
    ] {
        let error = ObjectiveCatalogue::from_toml(text).unwrap_err();
        assert!(
            matches!(error, Error::CatalogueFile(_)),
            "{text:?}: {error}"
        );
        assert_eq!(
            error.to_string(),
            "the objective catalogue couldn't be read"
        );
    }
    let empty = "tube_lens = 200000\nobjective = []";
    assert!(matches!(
        ObjectiveCatalogue::from_toml(empty),
        Err(Error::InvalidCatalogue { .. })
    ));
}

#[test]
fn an_unknown_objective_is_named_in_the_error() {
    let error = ObjectiveCatalogue::mitutoyo().get("7x").unwrap_err();
    assert_eq!(error.to_string(), "there is no objective named \"7x\"");
}
