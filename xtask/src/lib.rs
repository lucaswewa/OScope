//! Checks on the workspace itself: the layering rules of ADR-0005, which
//! OScope crate may depend on which, read from `cargo metadata`.

use std::path::Path;
use std::process::Command;

/// Each OScope crate and its layer (plan §5.2), the M3 crates included. A
/// crate may depend on crates in lower layers, and on crates in its own layer
/// only through [`SAME_LAYER`].
pub const LAYERS: &[(&str, u8)] = &[
    ("oscope-core", 0),
    ("oscope-compute", 1),
    ("oscope-shm", 1),
    ("oscope-imaging", 1),
    ("oscope-stitch", 1),
    ("oscope-volume", 1),
    ("oscope-hal", 2),
    ("oscope-sim", 2),
    ("oscope-grbl", 2),
    ("oscope-tucsen", 2),
    ("oscope-store", 2),
    ("oscope-jobs", 3),
    ("oscope-pipeline", 3),
    ("oscope-ext", 3),
    ("oscope-ext-sdk", 3),
    ("oscope-report", 3),
    ("oscope-acquire", 4),
    ("oscope-workflows", 4),
    ("oscope-api", 5),
    ("oscope-server", 6),
];

/// The edges allowed within a layer, as (from, to).
pub const SAME_LAYER: &[(&str, &str)] = &[
    ("oscope-imaging", "oscope-compute"),
    ("oscope-stitch", "oscope-compute"),
    ("oscope-stitch", "oscope-imaging"),
    ("oscope-volume", "oscope-compute"),
    ("oscope-volume", "oscope-imaging"),
    ("oscope-sim", "oscope-hal"),
    ("oscope-grbl", "oscope-hal"),
    ("oscope-tucsen", "oscope-hal"),
    ("oscope-pipeline", "oscope-jobs"),
    ("oscope-ext", "oscope-jobs"),
    ("oscope-ext", "oscope-pipeline"),
    ("oscope-ext", "oscope-ext-sdk"),
    // Its test reads a Rust generator's report as the model (P47).
    ("oscope-ext", "oscope-report"),
    ("oscope-workflows", "oscope-acquire"),
];

/// Outside crates that only some OScope crates may depend on.
pub const RESTRICTED: &[(&str, &[&str])] = &[
    ("teta-wot", &["oscope-api"]),
    // oscope-api uses teta-wot's re-export (ADR-0004).
    ("axum", &[]),
];

/// The crates allowed unsafe code (plan §4.3). Every other OScope crate must
/// take the workspace's lints, which forbid it.
pub const UNSAFE_ALLOWED: &[&str] = &[
    "oscope-compute",
    "oscope-shm",
    "oscope-ext",
    "oscope-ext-sdk",
    "oscope-tucsen",
];

/// A workspace member, as far as the rules need it.
#[derive(Clone, Debug)]
pub struct Crate {
    /// The package name.
    pub name: String,
    /// The packages it depends on, of every kind (normal, dev and build).
    pub dependencies: Vec<String>,
    /// Whether its manifest says `[lints] workspace = true`.
    pub workspace_lints: bool,
}

fn layer(name: &str) -> Option<u8> {
    LAYERS.iter().find(|(n, _)| *n == name).map(|(_, l)| *l)
}

/// The workspace's members, from `cargo metadata`.
pub fn workspace() -> Vec<Crate> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let output = Command::new(cargo)
        .args(["metadata", "--format-version", "1", "--no-deps", "--locked"])
        .current_dir(root)
        .output()
        .expect("cargo metadata runs");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "cargo metadata failed: {stderr}");
    let metadata: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let text = |v: &serde_json::Value| v.as_str().unwrap().to_owned();
    let packages = metadata["packages"].as_array().unwrap();
    packages
        .iter()
        .map(|p| Crate {
            name: text(&p["name"]),
            dependencies: p["dependencies"]
                .as_array()
                .unwrap()
                .iter()
                .map(|d| text(&d["name"]))
                .collect(),
            workspace_lints: takes_workspace_lints(
                &std::fs::read_to_string(text(&p["manifest_path"])).unwrap(),
            ),
        })
        .collect()
}

/// Whether a manifest's `[lints]` table is `workspace = true`.
pub fn takes_workspace_lints(manifest: &str) -> bool {
    let mut lines = manifest.lines().map(|l| l.replace(' ', ""));
    lines.any(|l| l == "[lints]")
        && lines
            .take_while(|l| !l.starts_with('['))
            .any(|l| l == "workspace=true")
}

/// Every broken rule, one sentence each; empty when the rules hold.
pub fn violations(crates: &[Crate]) -> Vec<String> {
    let member = |name: &str| crates.iter().any(|c| c.name == name);
    let mut found = Vec::new();
    for krate in crates {
        let from = krate.name.as_str();
        let Some(level) = layer(from) else {
            if from.starts_with("oscope-") {
                found.push(format!("{from} isn't in the layering table"));
            }
            continue;
        };
        if !krate.workspace_lints && !UNSAFE_ALLOWED.contains(&from) {
            found.push(format!(
                "{from} doesn't take the workspace lints, which forbid unsafe code"
            ));
        }
        for to in krate.dependencies.iter().map(String::as_str) {
            match layer(to) {
                Some(l) if l > level => found.push(format!(
                    "{from} (L{level}) depends on {to} (L{l}), in a higher layer"
                )),
                Some(l) if l == level && !SAME_LAYER.contains(&(from, to)) => found.push(format!(
                    "{from} depends on {to}, in its own layer (L{l}), and SAME_LAYER doesn't list the edge"
                )),
                Some(_) => {}
                None if member(to) => {
                    found.push(format!("{from} depends on {to}, which isn't a layered crate"))
                }
                None => {
                    if let Some((_, allowed)) = RESTRICTED.iter().find(|(r, _)| *r == to)
                        && !allowed.contains(&from)
                    {
                        let who = match allowed.join(", ") {
                            none if none.is_empty() => "no OScope crate".to_owned(),
                            some => format!("only {some}"),
                        };
                        found.push(format!("{from} depends on {to}, which {who} may depend on"));
                    }
                }
            }
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::takes_workspace_lints;

    #[test]
    fn workspace_lints_are_found_only_in_their_own_table() {
        assert!(takes_workspace_lints(
            "[package]\nname = \"a\"\n\n[lints]\nworkspace = true\n"
        ));
        assert!(takes_workspace_lints("[lints]\r\nworkspace=true\r\n"));
        assert!(takes_workspace_lints(
            "[lints]\n# Forbids unsafe code.\nworkspace = true\n"
        ));
        assert!(!takes_workspace_lints(
            "[lints]\n\n[lints.rust]\nworkspace = true\n"
        ));
        assert!(!takes_workspace_lints("[lints]\nworkspace = false\n"));
        assert!(!takes_workspace_lints(
            "[lints.rust]\nunsafe_code = \"allow\"\n"
        ));
        assert!(!takes_workspace_lints("[package]\nname = \"a\"\n"));
    }
}
