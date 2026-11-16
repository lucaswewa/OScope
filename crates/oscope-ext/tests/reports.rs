//! A Rust extension's report generator (P47), through the C ABI: described
//! as a `report`, its schema and its report called with JSON, and
//! parameters it can't take refused, saying why.

use oscope_ext::Library;
use oscope_ext_sdk::{ReportGenerator, export, report};
use serde_json::{Value, json};

/// A table of `count` squares.
struct Squares;

impl ReportGenerator for Squares {
    fn schema(&self) -> Value {
        json!({"type": "object", "properties": {"count": {"type": "integer"}}})
    }

    fn generate(&self, params: Value) -> Result<Value, String> {
        let count = params["count"]
            .as_u64()
            .ok_or("`count`: how many squares")?;
        let rows: Vec<Value> = (0..count)
            .map(|k| json!([k.to_string(), (k * k).to_string()]))
            .collect();
        let table = json!({"kind": "table", "columns": ["k", "k²"], "rows": rows});
        Ok(
            json!({"title": "Squares", "date": "2026-01-01", "sections": [{"heading": "Squares", "blocks": [table]}]}),
        )
    }
}

export!(nodes: [], drivers: [report("test.squares", Squares)]);

#[test]
fn a_rust_report_generator_makes_its_report_through_the_abi() {
    let library = Library::from_entry(oscope_extension).unwrap();
    let described = &library.drivers()[0];
    assert_eq!(
        (described.name.as_str(), described.kind.as_str()),
        ("test.squares", "report")
    );
    let schema = library.driver_call(0, "schema", &Value::Null).unwrap();
    assert_eq!(schema["properties"]["count"]["type"], "integer");
    let made = library
        .driver_call(0, "generate", &json!({"count": 3}))
        .unwrap();
    // OScope's model, as it reads it.
    let made: oscope_report::Report = serde_json::from_value(made).unwrap();
    assert_eq!(made.sections[0].blocks.len(), 1);
    let oscope_report::Block::Table { rows, .. } = &made.sections[0].blocks[0] else {
        panic!("a table")
    };
    assert_eq!(rows[2], ["2", "4"]);
    let refused = library.driver_call(0, "generate", &json!({}));
    assert!(refused.unwrap_err().contains("`count`"));
}
