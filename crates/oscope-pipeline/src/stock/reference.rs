//! References: images such as flat fields and dark frames, from the
//! server's references folder.

use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::PathBuf;
use std::sync::Arc;

use oscope_imaging::color::read_image;
use schemars::JsonSchema;
use serde::Deserialize;

use super::WHOLE;
use crate::node::{Descriptor, Inputs, Node, NodeError, Step, StepCtx, try_node};
use crate::value::{PortType, Value, Values, outputs};

/// `io.reference`'s parameters.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Name {
    /// The reference's name: its file's, without `.tiff` or `.png`.
    /// Letters, digits, `-` and `_`.
    name: String,
}

/// `io.reference`: a reference image, read from the references folder when
/// the plan is made: `{name}.tiff`, `{name}.tif` or `{name}.png`
/// (`oscope_imaging::color::read_image`), in linear light from 0 to 1. Its
/// content is part of its memo key, so a new file is never mistaken for the
/// old.
pub(super) fn reference(folder: PathBuf) -> Arc<dyn Node> {
    let descriptor = Descriptor::new(
        "io.reference",
        1,
        "input",
        "A reference image, such as a flat field",
    )
    .output("image", PortType::Image)
    .params::<Name>()
    .capabilities(WHOLE);
    try_node(descriptor, move |Name { name }| {
        let named = |c: char| c.is_ascii_alphanumeric() || c == '-' || c == '_';
        if name.is_empty() || !name.chars().all(named) {
            return Err(format!(
                "`{name}` isn't a reference's name: letters, digits, `-` and `_`"
            ));
        }
        let files =
            ["tiff", "tif", "png"].map(|extension| folder.join(format!("{name}.{extension}")));
        let file = files.into_iter().find(|file| file.is_file());
        let file =
            file.ok_or_else(|| format!("there is no reference `{name}` in {}", folder.display()))?;
        let pages = read_image(&file).map_err(|error| error.to_string())?;
        let image = pages.into_iter().next().ok_or("the file holds no image")?;
        let mut hasher = DefaultHasher::new();
        image.shape().hash(&mut hasher);
        image
            .data()
            .iter()
            .for_each(|v| hasher.write_u32(v.to_bits()));
        Ok(Reference {
            image: Value::image(image),
            key: hasher.finish(),
        })
    })
}

/// A reference, read.
struct Reference {
    image: Value,
    key: u64,
}

impl Step for Reference {
    fn run(&self, _: &StepCtx<'_>, _: &Inputs<'_>) -> Result<Values, NodeError> {
        Ok(outputs([("image", self.image.clone())]))
    }

    fn key(&self) -> Option<u64> {
        Some(self.key)
    }
}
