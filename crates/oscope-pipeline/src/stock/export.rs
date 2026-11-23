//! Export: an image written as a static Deep Zoom folder (ADR-0028), as the
//! store's DZI export writes a record's; and a height map as a mesh (P42).

use std::path::PathBuf;
use std::sync::Arc;

use oscope_imaging::color::{Curve, Display, DisplayTable};
use oscope_imaging::zoom::{InMemory, write_dzi};
use oscope_volume::{Mesh, MeshFormat, MeshOptions, VolumeError, write_mesh};
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

fn surface() -> String {
    "surface".to_owned()
}

/// `export.mesh`'s parameters.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct MeshParams {
    /// The folder to write in, on the server, as a full path.
    folder: String,
    /// The mesh's name, `<name>.<format>`: letters, digits, `-` and `_`.
    #[serde(default = "surface")]
    name: String,
    /// `ply`, `stl` or `obj`.
    format: MeshFormat,
    /// A pixel's side, in µm.
    pixel_um: f64,
    /// A vertex at every this many pixels, along each axis.
    #[serde(default = "stride")]
    #[schemars(range(min = 1))]
    stride: u32,
    /// The confidence a square's four corners need for it to be kept.
    #[serde(default)]
    min_confidence: f32,
}

fn stride() -> u32 {
    1
}

/// `export.mesh`: the heights as a mesh in `folder`, in µm, its squares
/// under the confidence floor left out where a `confidence` is given, and
/// coloured, or textured for OBJ, by the `texture` if given. It says what it
/// wrote. A file there already is refused, not written over.
pub(super) fn mesh() -> Arc<dyn Node> {
    let descriptor = Descriptor::new("export.mesh", 1, "export", "A height map as a mesh")
        .input("heights", PortType::Image)
        .optional_input("confidence", PortType::Image)
        .optional_input("texture", PortType::Image)
        .output("written", PortType::Json)
        .params::<MeshParams>()
        .capabilities(BATCH);
    try_node(descriptor, |p: MeshParams| {
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
        let options = MeshOptions {
            pixel_um: p.pixel_um,
            stride: p.stride,
            min_confidence: p.min_confidence,
        };
        Ok(
            move |_: &StepCtx<'_>, inputs: &Inputs<'_>| -> Result<Values, NodeError> {
                let failed = |e: VolumeError| NodeError::Failed(e.to_string());
                let heights = inputs.image("heights")?;
                let confidence = inputs.get("confidence").and_then(Value::as_image);
                let texture = inputs.get("texture").and_then(Value::as_image);
                let mesh = Mesh::new(heights, confidence, texture, &options).map_err(failed)?;
                let files =
                    write_mesh(&mesh, p.format, texture, &folder, &p.name).map_err(failed)?;
                let written = json!({
                    "files": files,
                    "vertices": mesh.vertices.len(),
                    "triangles": mesh.triangles.len(),
                });
                Ok(outputs([("written", Value::json(written))]))
            },
        )
    })
}
