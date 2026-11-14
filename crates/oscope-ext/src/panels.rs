//! Panels: forms that extensions describe, and the app draws (D11,
//! ADR-0036). Extensions ship no code for them: each is a JSON document in
//! the package, `panels/<name>.json`, for a name the manifest provides.

use std::path::Path;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{Contribution, Manifest};

/// A panel, as an extension describes it: a form, what submitting it does,
/// and how to show what comes back.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Panel {
    /// Its title.
    pub title: String,
    /// What it is for.
    #[serde(default)]
    pub description: String,
    /// The form: a JSON Schema of an object, in the part the pipeline checks
    /// (ADR-0017).
    pub form: serde_json::Value,
    /// What submitting the form does.
    pub submit: Submit,
    /// How to show what comes back.
    pub view: View,
}

/// What submitting a panel's form does.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "kind", deny_unknown_fields)]
pub enum Submit {
    /// Runs one of the extension's nodes on the live view's newest frame,
    /// the form's values its parameters.
    Node {
        /// The node.
        node: String,
    },
    /// Invokes a Thing's action, the form's values its input.
    Action {
        /// The Thing, such as `stage`.
        thing: String,
        /// Its action.
        action: String,
    },
}

/// How to show what a panel's submission gives.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum View {
    /// An image.
    Image,
    /// A table.
    Table,
    /// JSON, as it is.
    Json,
    /// Text.
    Text,
}

/// The file in a package that holds the panel `name`.
pub fn asset(name: &str) -> String {
    format!("panels/{name}.json")
}

/// The panels `manifest` provides, read from the package unpacked in
/// `folder`, and checked: a document that is a [`Panel`], a form the
/// pipeline's schema checks accept, of an object, and a node to submit to
/// that the extension provides.
pub(crate) fn read(manifest: &Manifest, folder: &Path) -> Result<Vec<(String, Panel)>, String> {
    let names = manifest
        .provides
        .get(&Contribution::Panels)
        .into_iter()
        .flatten();
    let mut panels = Vec::new();
    for name in names {
        let path = asset(name)
            .split('/')
            .fold(folder.to_owned(), |path, part| path.join(part));
        let wrong = |why: String| format!("panel `{name}`: {why}");
        let text = std::fs::read(&path).map_err(|e| wrong(format!("{}: {e}", path.display())))?;
        let panel: Panel = serde_json::from_slice(&text).map_err(|e| wrong(e.to_string()))?;
        oscope_pipeline::schema::check(&panel.form).map_err(|e| wrong(format!("its form: {e}")))?;
        if panel.form.get("type").and_then(|t| t.as_str()) != Some("object") {
            return Err(wrong("its form isn't an object's schema".to_owned()));
        }
        if let Submit::Node { node } = &panel.submit {
            let nodes = manifest
                .provides
                .get(&Contribution::Nodes)
                .into_iter()
                .flatten();
            if !nodes.into_iter().any(|provided| provided == node) {
                return Err(wrong(format!(
                    "it submits to `{node}`, which the extension doesn't provide"
                )));
            }
        }
        panels.push((name.clone(), panel));
    }
    Ok(panels)
}
