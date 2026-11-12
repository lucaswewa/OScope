//! Versioned documents: written with their version, upgraded when old,
//! refused when newer.

use oscope_core::{Error, Versioned, from_document, to_document};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

/// Version 1 called the exposure `exposure_us`, in microseconds. Version 2
/// made it `exposure_ms`, and version 3 added `gain`.
#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Preset {
    name: String,
    exposure_ms: f64,
    gain: f64,
}

impl Versioned for Preset {
    const SCHEMA: &'static str = "oscope.test-preset";
    const VERSION: u32 = 3;

    fn upgrade(
        from: u32,
        mut fields: Map<String, Value>,
    ) -> oscope_core::Result<Map<String, Value>> {
        match from {
            1 => {
                let us = fields
                    .remove("exposure_us")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);
                fields.insert("exposure_ms".into(), json!(us / 1e3));
            }
            2 => {
                fields.insert("gain".into(), json!(1.0));
            }
            _ => unreachable!("only versions 1 and 2 are older"),
        }
        Ok(fields)
    }
}

fn preset() -> Preset {
    Preset {
        name: "brightfield 10x".into(),
        exposure_ms: 2.5,
        gain: 1.0,
    }
}

#[test]
fn a_document_carries_its_schema_and_version_and_reads_back() {
    let document = to_document(&preset()).unwrap();
    assert_eq!(
        document,
        json!({"schema": "oscope.test-preset", "version": 3, "name": "brightfield 10x", "exposure_ms": 2.5, "gain": 1.0})
    );
    assert_eq!(from_document::<Preset>(document).unwrap(), preset());
}

#[test]
fn an_old_document_is_upgraded_a_version_at_a_time() {
    let v1 = json!({"schema": "oscope.test-preset", "version": 1, "name": "brightfield 10x", "exposure_us": 2500});
    assert_eq!(from_document::<Preset>(v1).unwrap(), preset());
    let v2 = json!({"schema": "oscope.test-preset", "version": 2, "name": "brightfield 10x", "exposure_ms": 2.5});
    assert_eq!(from_document::<Preset>(v2).unwrap(), preset());
}

#[test]
fn a_newer_or_foreign_document_is_refused() {
    let newer = json!({"schema": "oscope.test-preset", "version": 4, "name": "x", "exposure_ms": 1, "gain": 1});
    let error = from_document::<Preset>(newer).unwrap_err();
    assert!(matches!(
        error,
        Error::NewerVersion {
            found: 4,
            current: 3,
            ..
        }
    ));
    assert_eq!(
        error.to_string(),
        "the oscope.test-preset document is version 4, newer than this build's 3"
    );
    for (document, reason) in [
        (
            json!({"schema": "oscope.recipe", "version": 3}),
            "its schema is \"oscope.recipe\"",
        ),
        (json!({"version": 3}), "it names no schema"),
        (
            json!({"schema": "oscope.test-preset", "version": 0}),
            "it has no version",
        ),
        (
            json!({"schema": "oscope.test-preset", "version": "3"}),
            "it has no version",
        ),
        (json!([1, 2, 3]), "it isn't a JSON object"),
    ] {
        let error = from_document::<Preset>(document).unwrap_err();
        assert!(
            error.to_string().contains(reason),
            "{error} lacks {reason:?}"
        );
    }
    let mismatch = json!({"schema": "oscope.test-preset", "version": 3, "name": "x", "exposure_ms": "fast", "gain": 1});
    let error = from_document::<Preset>(mismatch).unwrap_err();
    assert!(matches!(&error, Error::Mismatch { .. }));
    assert!(
        std::error::Error::source(&error).is_some(),
        "serde's error is the source"
    );
}

#[test]
fn by_default_an_older_version_cant_be_upgraded() {
    #[derive(Debug, Serialize, Deserialize)]
    struct Fresh {
        version_note: String,
    }
    impl Versioned for Fresh {
        const SCHEMA: &'static str = "oscope.test-fresh";
        const VERSION: u32 = 2;
    }
    let old = json!({"schema": "oscope.test-fresh", "version": 1, "version_note": "x"});
    assert!(matches!(
        from_document::<Fresh>(old),
        Err(Error::CantUpgrade { found: 1, .. })
    ));
}

#[test]
fn a_type_with_its_own_version_field_cant_be_written() {
    #[derive(Debug, Serialize, Deserialize)]
    struct Clash {
        version: u32,
    }
    impl Versioned for Clash {
        const SCHEMA: &'static str = "oscope.test-clash";
        const VERSION: u32 = 1;
    }
    let error = to_document(&Clash { version: 9 }).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("a field of its own named \"version\""),
        "{error}"
    );
}
