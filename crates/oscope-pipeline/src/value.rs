//! What flows along a recipe's edges: values of the port types.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

use oscope_compute::Image;
#[cfg(feature = "cuda")]
use oscope_compute::cuda::DeviceImage;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A port's type, and its values'.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PortType {
    /// An image of 32-bit floats, in memory or on the GPU.
    Image,
    /// Images of one shape: a z-stack, a time series or a volume.
    Stack,
    /// A binary mask, a byte a pixel.
    Mask,
    /// A table: named columns, and rows.
    Table,
    /// Shapes, as GeoJSON.
    Geometry,
    /// A number, or any JSON.
    Json,
    /// A fragment of a report.
    Report,
}

impl PortType {
    /// Whether its values are rasters, which tiled runs cut into tiles.
    pub fn is_raster(self) -> bool {
        matches!(self, Self::Image | Self::Mask)
    }
}

impl fmt::Display for PortType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = serde_json::to_value(self).unwrap_or_default();
        write!(f, "{}", name.as_str().unwrap_or("?"))
    }
}

/// A binary mask: a byte a pixel, 0 or 1, row by row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mask {
    width: u32,
    height: u32,
    data: Vec<u8>,
}

impl Mask {
    /// A mask of zeros.
    pub fn zeros(width: u32, height: u32) -> Mask {
        let data = vec![0; width as usize * height as usize];
        Mask {
            width,
            height,
            data,
        }
    }

    /// Pixels across.
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Pixels down.
    pub fn height(&self) -> u32 {
        self.height
    }

    /// The pixels.
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    /// The pixels, to change.
    pub fn data_mut(&mut self) -> &mut [u8] {
        &mut self.data
    }
}

/// A table: named columns, and rows of JSON values.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Table {
    /// The columns' names.
    pub columns: Vec<String>,
    /// The rows, a value for each column.
    pub rows: Vec<Vec<serde_json::Value>>,
}

/// A value on an edge. Cloning it shares it.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum Value {
    /// An image in memory.
    Image(Arc<Image>),
    /// An image on the GPU.
    #[cfg(feature = "cuda")]
    DeviceImage(Arc<DeviceImage>),
    /// Images of one shape.
    Stack(Arc<Vec<Image>>),
    /// A mask.
    Mask(Arc<Mask>),
    /// A table.
    Table(Arc<Table>),
    /// Shapes, as GeoJSON.
    Geometry(Arc<serde_json::Value>),
    /// A number, or any JSON.
    Json(Arc<serde_json::Value>),
    /// A fragment of a report.
    Report(Arc<serde_json::Value>),
}

/// Values by name: a step's outputs by port, or a recipe's inputs and
/// outputs.
pub type Values = BTreeMap<String, Value>;

/// A step's outputs, from their ports' names and values.
pub fn outputs<const N: usize>(values: [(&str, Value); N]) -> Values {
    values
        .into_iter()
        .map(|(port, value)| (port.to_owned(), value))
        .collect()
}

impl Value {
    /// An image in memory.
    pub fn image(image: Image) -> Value {
        Value::Image(Arc::new(image))
    }

    /// A stack of images of one shape.
    pub fn stack(images: Vec<Image>) -> Value {
        Value::Stack(Arc::new(images))
    }

    /// A mask.
    pub fn mask(mask: Mask) -> Value {
        Value::Mask(Arc::new(mask))
    }

    /// A table.
    pub fn table(table: Table) -> Value {
        Value::Table(Arc::new(table))
    }

    /// JSON.
    pub fn json(value: serde_json::Value) -> Value {
        Value::Json(Arc::new(value))
    }

    /// Its type.
    pub fn port_type(&self) -> PortType {
        match self {
            Value::Image(_) => PortType::Image,
            #[cfg(feature = "cuda")]
            Value::DeviceImage(_) => PortType::Image,
            Value::Stack(_) => PortType::Stack,
            Value::Mask(_) => PortType::Mask,
            Value::Table(_) => PortType::Table,
            Value::Geometry(_) => PortType::Geometry,
            Value::Json(_) => PortType::Json,
            Value::Report(_) => PortType::Report,
        }
    }

    /// The image, if it is one in memory.
    pub fn as_image(&self) -> Option<&Image> {
        match self {
            Value::Image(image) => Some(image),
            _ => None,
        }
    }

    /// The stack's images, if it is one.
    pub fn as_stack(&self) -> Option<&[Image]> {
        match self {
            Value::Stack(images) => Some(images),
            _ => None,
        }
    }

    /// The mask, if it is one.
    pub fn as_mask(&self) -> Option<&Mask> {
        match self {
            Value::Mask(mask) => Some(mask),
            _ => None,
        }
    }

    /// The table, if it is one.
    pub fn as_table(&self) -> Option<&Table> {
        match self {
            Value::Table(table) => Some(table),
            _ => None,
        }
    }

    /// The JSON of JSON, geometry and report values.
    pub fn as_json(&self) -> Option<&serde_json::Value> {
        match self {
            Value::Json(value) | Value::Geometry(value) | Value::Report(value) => Some(value),
            _ => None,
        }
    }

    /// Whether it is on the GPU.
    pub fn on_gpu(&self) -> bool {
        #[cfg(feature = "cuda")]
        if let Value::DeviceImage(_) = self {
            return true;
        }
        false
    }

    /// About the memory it holds, in bytes: what the memo cache counts.
    pub(crate) fn bytes(&self) -> usize {
        let json = |value: &serde_json::Value| value.to_string().len();
        match self {
            Value::Image(image) => image.data().len() * 4,
            #[cfg(feature = "cuda")]
            Value::DeviceImage(image) => image.shape().len() * 4,
            Value::Stack(images) => images.iter().map(|image| image.data().len() * 4).sum(),
            Value::Mask(mask) => mask.data().len(),
            Value::Table(table) => serde_json::to_string(&**table).map_or(0, |text| text.len()),
            Value::Geometry(value) | Value::Json(value) | Value::Report(value) => json(value),
        }
    }
}
