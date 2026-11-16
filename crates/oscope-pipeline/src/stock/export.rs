//! Export: an image written as a static Deep Zoom folder (ADR-0028), as the
//! store's DZI export writes a record's.

use std::path::PathBuf;
use std::sync::Arc;

use oscope_imaging::color::{Curve, Display, DisplayTable};
use oscope_imaging::zoom::{InMemory, write_dzi};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::json;

use crate::node::{Capabilities, Descriptor, Inputs, Node, NodeError, StepCtx, try_node};
use crate::value::{PortType, Value, Values, outputs};

/// On the CPU, on the whole image, and in batch runs alone: it writes
/// files, which no live frame should.
const BATCH: Capabilities = Capabilities {
    cpu: true,
    cuda: false,
    live: false,
    tileable: false,
    halo: 0,
};

fn one() -> f32 {
    1.0
}

fn image() -> String {
    "image".to_owned()
}

/// `export.dzi`'s parameters.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Dzi {
    /// The folder to write in, on the server, as a full path.
    folder: String,
    /// The image's name, `<name>.dzi` and `<name>_files/`: letters, digits,
    /// `-` and `_`.
    #[serde(default = "image")]
    name: String,
    /// The level shown as black.
    #[serde(default)]
    black: f32,
    /// The level shown as white: above black.
    #[serde(default = "one")]
    white: f32,
    /// The gamma, such as 2.2; sRGB's curve unless given.
    #[serde(default)]
    gamma: Option<f32>,
}

/// `export.dzi`: the image as a static Deep Zoom folder in `folder`,
/// `<name>.dzi` and its tiles in `<name>_files/`, shown through its levels
/// and curve. It says where, and how many tiles. A folder that holds
/// `<name>_files` already is refused, not written over.
pub(super) fn dzi() -> Arc<dyn Node> {
    let descriptor = Descriptor::new("export.dzi", 1, "export", "A static Deep Zoom folder")
        .input("image", PortType::Image)
        .output("written", PortType::Json)
        .params::<Dzi>()
        .capabilities(BATCH);
    try_node(descriptor, |p: Dzi| {
        let folder = PathBuf::from(&p.folder);
        if !folder.is_absolute() {
            return Err(format!("the folder ({}) must be a full path", p.folder));
        }
        let plain = |c: char| c.is_ascii_alphanumeric() || c == '-' || c == '_';
        if p.name.is_empty() || !p.name.chars().all(plain) {
            return Err(format!(
                "the name ({}) must be letters, digits, - and _",
                p.name
            ));
        }
        if p.white.partial_cmp(&p.black) != Some(std::cmp::Ordering::Greater) {
            return Err(format!(
                "white ({}) must be above black ({})",
                p.white, p.black
            ));
        }
        let curve = match p.gamma {
            Some(gamma) if gamma > 0.0 => Curve::Gamma { gamma },
            Some(gamma) => return Err(format!("the gamma ({gamma}) must be above 0")),
            None => Curve::Srgb,
        };
        let display = DisplayTable::new(&Display {
            black: p.black,
            white: p.white,
            curve,
        })
        .map_err(|e| e.to_string())?;
        let name = p.name;
        Ok(
            move |_: &StepCtx<'_>, inputs: &Inputs<'_>| -> Result<Values, NodeError> {
                let image = inputs.image("image")?;
                let files = folder.join(format!("{name}_files"));
                if files.exists() {
                    return Err(format!("{} is there already", files.display()).into());
                }
                let failed = |e: oscope_imaging::zoom::ZoomError| NodeError::Failed(e.to_string());
                let pyramid = InMemory::new(image.clone()).map_err(failed)?;
                let tiles = write_dzi(&pyramid, &display, &folder, &name).map_err(failed)?;
                let written = json!({ "folder": folder, "name": name, "tiles": tiles });
                Ok(outputs([("written", Value::json(written))]))
            },
        )
    })
}
