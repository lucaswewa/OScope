//! Units and IDs: conversions, text, and JSON.

use std::collections::BTreeSet;
use std::time::Duration;

use oscope_core::{Error, FrameId, JobId, Ms, ProjectId, Um};
use schemars::schema_for;

#[test]
fn lengths_convert_between_mm_um_and_nm() {
    assert_eq!(Um::from_mm(25.0), Um(25_000.0));
    assert_eq!(Um::from_nm(240.0), Um(0.24));
    assert_eq!(Um(1_500.0).mm(), 1.5);
    assert_eq!(Um(0.6).nm(), 600.0);
    assert_eq!(Um::from_mm(25.0) / Um::from_nm(240.0), 25_000.0 / 0.24);
    let mut position = Um(10.0);
    position += Um(2.5);
    position -= -Um(1.0);
    assert_eq!(position * 2.0, Um(27.0));
    assert_eq!(0.5 * position, Um(6.75));
    assert_eq!((Um::ZERO - position).abs(), position);
}

#[test]
fn durations_convert_to_and_from_std() {
    assert_eq!(Ms::from(Duration::from_micros(2_500)), Ms(2.5));
    assert_eq!(Ms(2.5).to_duration(), Some(Duration::from_micros(2_500)));
    assert_eq!(Ms::from_secs(0.069).secs(), 0.069);
    assert_eq!(Ms(-1.0).to_duration(), None);
    assert_eq!(Ms(f64::NAN).to_duration(), None);
}

#[test]
fn units_print_and_serialise_as_numbers_in_their_unit() {
    assert_eq!(Um(12.5).to_string(), "12.5 µm");
    assert_eq!(format!("{:.2}", Ms(1.0 / 3.0)), "0.33 ms");
    assert_eq!(serde_json::to_string(&Um(0.24)).unwrap(), "0.24");
    assert_eq!(serde_json::from_str::<Ms>("30").unwrap(), Ms(30.0));
    let schema = serde_json::to_value(schema_for!(Um)).unwrap();
    assert_eq!(schema["type"], "number");
    assert!(
        schema["description"]
            .as_str()
            .unwrap()
            .contains("micrometres")
    );
}

#[test]
fn ids_are_unique_and_ordered_by_creation() {
    let ids: Vec<JobId> = (0..10_000).map(|_| JobId::generate()).collect();
    assert!(
        ids.windows(2).all(|pair| pair[0] < pair[1]),
        "in creation order"
    );
    assert_eq!(ids.iter().collect::<BTreeSet<_>>().len(), ids.len());
}

#[test]
fn ids_round_trip_through_text_and_json() {
    let id = ProjectId::generate();
    let text = id.to_string();
    assert_eq!(text.len(), 36);
    assert_eq!(text.parse::<ProjectId>().unwrap(), id);
    assert_eq!(
        serde_json::to_value(id).unwrap(),
        serde_json::Value::String(text.clone())
    );
    assert_eq!(
        serde_json::from_value::<ProjectId>(text.into()).unwrap(),
        id
    );
    let schema = serde_json::to_value(schema_for!(ProjectId)).unwrap();
    assert_eq!(
        (&schema["type"], &schema["format"]),
        (&"string".into(), &"uuid".into())
    );
}

#[test]
fn text_that_isnt_an_id_names_the_kind() {
    let error = "frame-12".parse::<JobId>().unwrap_err();
    assert!(matches!(&error, Error::InvalidId { kind: "JobId", text } if text == "frame-12"));
    assert_eq!(error.to_string(), "\"frame-12\" isn't a JobId");
}

#[test]
fn frame_ids_count_up() {
    assert_eq!(FrameId::default().next().next(), FrameId(2));
    assert_eq!(FrameId(41).to_string(), "frame 41");
    assert_eq!(serde_json::to_string(&FrameId(7)).unwrap(), "7");
}
