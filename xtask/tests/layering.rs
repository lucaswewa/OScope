//! The layering rules of ADR-0005, on the workspace and on made-up crates.

use xtask::{Crate, LAYERS, SAME_LAYER, UNSAFE_ALLOWED, violations, workspace};

fn krate(name: &str, dependencies: &[&str]) -> Crate {
    Crate {
        name: name.into(),
        dependencies: dependencies.iter().map(|d| d.to_string()).collect(),
        workspace_lints: true,
    }
}

#[test]
fn the_workspace_follows_the_layering_rules() {
    let crates = workspace();
    let names: Vec<&str> = crates.iter().map(|c| c.name.as_str()).collect();
    assert!(
        names.contains(&"oscope-core") && names.contains(&"xtask"),
        "{names:?}"
    );
    let found = violations(&crates);
    assert!(
        found.is_empty(),
        "the layering rules are broken:\n{}",
        found.join("\n")
    );
}

#[test]
fn lower_layers_and_listed_edges_within_a_layer_are_allowed() {
    let crates = [
        krate("oscope-server", &["oscope-api", "oscope-core", "tokio"]),
        krate("oscope-api", &["teta-wot", "oscope-acquire"]),
        krate("oscope-stitch", &["oscope-imaging", "oscope-compute"]),
        krate("oscope-core", &["serde"]),
    ];
    assert_eq!(violations(&crates), Vec::<String>::new());
}

#[test]
fn upward_edges_and_unlisted_edges_within_a_layer_are_refused() {
    let found = violations(&[
        krate("oscope-core", &["oscope-hal"]),
        krate("oscope-hal", &["oscope-sim"]),
        krate("oscope-api", &["oscope-server"]),
    ]);
    assert_eq!(found.len(), 3, "{found:#?}");
    assert!(found[0].contains("oscope-core (L0) depends on oscope-hal (L2)"));
    assert!(found[1].contains("oscope-hal depends on oscope-sim, in its own layer (L2)"));
    assert!(found[2].contains("oscope-api (L5) depends on oscope-server (L6)"));
}

#[test]
fn only_oscope_api_may_depend_on_teta_wot_and_none_on_axum() {
    let found = violations(&[
        krate("oscope-api", &["teta-wot", "axum"]),
        krate("oscope-acquire", &["teta-wot"]),
        krate("ex-live-view", &["teta-wot", "axum"]),
    ]);
    assert_eq!(found.len(), 2, "{found:#?}");
    assert!(found[0].starts_with("oscope-api depends on axum"));
    assert!(found[1].starts_with("oscope-acquire depends on teta-wot"));
}

#[test]
fn oscope_crates_must_be_in_the_table_and_may_not_use_other_members() {
    let found = violations(&[
        krate("oscope-extra", &[]),
        krate("oscope-core", &["ex-core-tour"]),
        krate("ex-core-tour", &["oscope-core", "oscope-server"]),
    ]);
    assert_eq!(found.len(), 2, "{found:#?}");
    assert_eq!(found[0], "oscope-extra isn't in the layering table");
    assert!(found[1].contains("oscope-core depends on ex-core-tour, which isn't a layered crate"));
}

#[test]
fn unsafe_code_is_forbidden_outside_the_listed_crates() {
    let own_lints = |name| Crate {
        workspace_lints: false,
        ..krate(name, &[])
    };
    let found = violations(&[own_lints("oscope-core"), own_lints("oscope-shm")]);
    assert_eq!(
        found,
        ["oscope-core doesn't take the workspace lints, which forbid unsafe code"]
    );
}

#[test]
fn the_tables_name_only_layered_crates() {
    let layer = |name: &str| LAYERS.iter().find(|(n, _)| *n == name).map(|(_, l)| *l);
    for (from, to) in SAME_LAYER {
        assert!(
            layer(from).is_some() && layer(from) == layer(to),
            "{from} -> {to}"
        );
    }
    for name in UNSAFE_ALLOWED {
        assert!(layer(name).is_some(), "{name}");
    }
}
