//! A Rust extension's scan workflow (P36; ADR-0025), through the C ABI:
//! described as a `workflow`, its schema and its plan called with JSON, and
//! parameters it can't take refused, saying why.

use oscope_ext::Library;
use oscope_ext_sdk::{PlannedTile, ScanField, ScanWorkflow, export, workflow};
use serde_json::{Value, json};

/// A row of `tiles` tiles along x from `x_um`, a step apart.
struct Row;

impl ScanWorkflow for Row {
    fn schema(&self) -> Value {
        json!({"type": "object", "properties": {"x_um": {"type": "number"}, "tiles": {"type": "integer"}}})
    }

    fn plan(&self, params: Value, field: ScanField) -> Result<Vec<PlannedTile>, String> {
        let tiles = params["tiles"]
            .as_u64()
            .ok_or("`tiles`: a count of tiles")?;
        let x = params["x_um"].as_f64().unwrap_or(0.0);
        Ok((0..tiles)
            .map(|k| PlannedTile {
                x_um: x + k as f64 * field.step_um.width,
                y_um: 2_000.0,
                z_um: (k == 0).then_some(5_000.0),
                name: Some(format!("tile {k}")),
            })
            .collect())
    }
}

export!(nodes: [], drivers: [workflow("test.row", Row)]);

#[test]
fn a_rust_workflow_plans_its_tiles_through_the_abi() {
    let library = Library::from_entry(oscope_extension).unwrap();
    let described = &library.drivers()[0];
    assert_eq!(
        (described.name.as_str(), described.kind.as_str()),
        ("test.row", "workflow")
    );
    let schema = library.driver_call(0, "schema", &Value::Null).unwrap();
    assert_eq!(schema["properties"]["tiles"]["type"], "integer");

    // OScope's field, as its plans serialise it: more than the workflow reads.
    let field = json!({
        "corners_um": [{"x": -1.0, "y": -1.0}],
        "cell_um": {"width": 1_313.0, "height": 875.0},
        "overlap": 0.1,
        "step_um": {"width": 1_181.7, "height": 787.5},
    });
    let request = json!({"params": {"x_um": 30_000.0, "tiles": 3}, "field": field});
    let tiles = library.driver_call(0, "plan", &request).unwrap();
    let tiles: Vec<PlannedTile> = serde_json::from_value(tiles).unwrap();
    let xs: Vec<f64> = tiles.iter().map(|t| t.x_um).collect();
    assert_eq!(xs, [30_000.0, 31_181.7, 32_363.4]);
    assert_eq!((tiles[0].z_um, tiles[1].z_um), (Some(5_000.0), None));
    assert_eq!(tiles[2].name.as_deref(), Some("tile 2"));

    // What it can't take, it refuses, saying why.
    let wrong = json!({"params": {"tiles": "three"}, "field": field});
    let refused = library.driver_call(0, "plan", &wrong).unwrap_err();
    assert!(refused.contains("`tiles`: a count of tiles"), "{refused}");
    let nothing = library.driver_call(0, "capture", &Value::Null).unwrap_err();
    assert!(nothing.contains("no method `capture`"), "{nothing}");
}
