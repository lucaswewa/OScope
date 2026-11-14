//! Panels (ADR-0036): the Python sample's panel validates and loads; a
//! panel that isn't one, a form the pipeline's checks refuse or that isn't
//! an object's, a submission to a node the extension doesn't provide, and a
//! panel whose document isn't among the assets are refused.

use std::fs;
use std::path::Path;

use oscope_ext::{Contribution, Extensions, Manifest, Submit, View};
use serde_json::json;

const ID: &str = "org.example.panels";

/// A Python extension providing `example.node` and the panel
/// `example.panel`, whose document is `panel`.
fn package(folder: &Path, panel: &serde_json::Value) -> std::path::PathBuf {
    let source = folder.join("source");
    fs::create_dir_all(source.join("panels")).unwrap();
    fs::create_dir_all(source.join("example")).unwrap();
    let manifest = format!(
        "id = \"{ID}\"\nname = \"Panels\"\nversion = \"1.0.0\"\nlicense = \"MIT\"\n\
         publisher = \"Example Lab\"\noscope-api = \"^1.0\"\nkind = \"python\"\n\
         assets = [\"panels/example.panel.json\"]\n\n\
         [provides]\nnodes = [\"example.node\"]\npanels = [\"example.panel\"]\n\n\
         [python]\nrequires-python = \">=3.12\"\nentry = \"example\"\n"
    );
    fs::write(source.join("manifest.toml"), manifest).unwrap();
    fs::write(source.join("example/__init__.py"), "").unwrap();
    fs::write(source.join("panels/example.panel.json"), panel.to_string()).unwrap();
    let out = folder.join("panels.omext");
    oscope_ext::pack(&source, &out).unwrap();
    out
}

fn panel() -> serde_json::Value {
    json!({
        "title": "A panel",
        "form": { "type": "object", "properties": { "k": { "type": "number", "minimum": 0 } } },
        "submit": { "kind": "node", "node": "example.node" },
        "view": "image",
    })
}

#[test]
fn the_python_sample_s_panel_validates_and_loads() {
    let folder = tempfile::tempdir().unwrap();
    let sample = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/ext-python-node");
    let package = folder.path().join("sample.omext");
    oscope_ext::pack(&sample, &package).unwrap();
    let root = folder.path().join("extensions");
    Extensions::open(&root).unwrap().install(&package).unwrap();
    let extensions = Extensions::open(&root).unwrap();
    let (extension, panel) = &extensions.panels()["example.denoise_tv"];
    assert_eq!(extension, "org.oscope.examples.tv-denoise");
    assert_eq!(panel.title, "Denoise the live view");
    assert_eq!(
        panel.submit,
        Submit::Node {
            node: "example.denoise_tv".to_owned()
        }
    );
    assert_eq!(panel.view, View::Image);
    assert_eq!(panel.form["properties"]["iterations"]["maximum"], 500);
    let registry = extensions.registry();
    assert_eq!(
        registry[&Contribution::Panels]["example.denoise_tv"],
        "org.oscope.examples.tv-denoise"
    );
}

#[test]
fn a_panel_that_doesn_t_hold_is_refused_at_install() {
    let cases = [
        (
            json!({ "form": { "type": "object" }, "submit": { "kind": "node", "node": "example.node" }, "view": "image" }),
            "missing field `title`",
        ),
        (
            json!({ "title": "t", "form": { "type": "object", "properties": { "k": { "pattern": "a" } } }, "submit": { "kind": "node", "node": "example.node" }, "view": "json" }),
            "its form: ",
        ),
        (
            json!({ "title": "t", "form": { "type": "string" }, "submit": { "kind": "node", "node": "example.node" }, "view": "text" }),
            "its form isn't an object's schema",
        ),
        (
            json!({ "title": "t", "form": { "type": "object" }, "submit": { "kind": "node", "node": "example.other" }, "view": "image" }),
            "it submits to `example.other`, which the extension doesn't provide",
        ),
        (
            json!({ "title": "t", "form": { "type": "object" }, "submit": { "kind": "recipe" }, "view": "image" }),
            "unknown variant `recipe`",
        ),
    ];
    for (document, said) in cases {
        let folder = tempfile::tempdir().unwrap();
        let root = folder.path().join("extensions");
        let error = Extensions::open(&root)
            .unwrap()
            .install(&package(folder.path(), &document));
        let error = error.unwrap_err().to_string();
        assert!(
            error.starts_with("the package: panel `example.panel`: "),
            "{error}"
        );
        assert!(error.contains(said), "{said}: {error}");
        assert!(!root.join(ID).exists(), "nothing left");
    }
    // A good panel, and one submitting to a Thing's action, install.
    for submit in [
        json!({ "kind": "node", "node": "example.node" }),
        json!({ "kind": "action", "thing": "stage", "action": "home" }),
    ] {
        let folder = tempfile::tempdir().unwrap();
        let mut document = panel();
        document["submit"] = submit;
        let extensions = Extensions::open(folder.path().join("extensions")).unwrap();
        extensions
            .install(&package(folder.path(), &document))
            .unwrap();
    }
}

#[test]
fn a_panel_must_have_its_document_among_the_assets() {
    let manifest = "id = \"org.example.panels\"\nname = \"P\"\nversion = \"1.0.0\"\nlicense = \"MIT\"\n\
                    publisher = \"E\"\noscope-api = \"^1.0\"\nkind = \"python\"\nassets = [\"panels/other.json\"]\n\n\
                    [provides]\npanels = [\"example.panel\"]\n\n\
                    [python]\nrequires-python = \">=3.12\"\nentry = \"example\"\n";
    let error = Manifest::parse(manifest).unwrap_err().to_string();
    let said =
        "provides.panels \"example.panel\": \"panels/example.panel.json\" isn't among its assets";
    assert!(error.ends_with(said), "{error}");
}
