//! Cells (P46): the adaptive threshold, the seeded watershed that splits
//! touching nuclei, and each object's measures and outline.

use std::sync::Arc;

use oscope_compute::{Image, Shape};
use oscope_imaging::cells::{
    Filters, adaptive, distances, fill_holes, measure, outline, watershed,
};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::json;

use super::{PIXELWISE, WHOLE};
use crate::node::{Descriptor, Inputs, Node, NodeError, Step, StepCtx, try_node};
use crate::value::{Mask, PortType, Table, Value, Values, outputs};

/// Each pixel's intensity: the mean of its channels.
fn intensities(image: &Image) -> Vec<f32> {
    let channels = image.channels().max(1);
    let pixels = image.data().chunks(channels);
    pixels
        .map(|p| p.iter().sum::<f32>() / channels as f32)
        .collect()
}

fn ten() -> usize {
    10
}

fn small() -> f32 {
    0.05
}

/// `threshold.adaptive`'s parameters.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Adaptive {
    /// The window's half side, in pixels: 10 unless given.
    #[serde(default = "ten")]
    #[schemars(range(min = 1, max = 512))]
    radius: usize,
    /// How far over its window's mean a pixel must be: 0.05 unless given.
    #[serde(default = "small")]
    offset: f32,
}

impl Step for Adaptive {
    fn run(&self, _: &StepCtx<'_>, inputs: &Inputs<'_>) -> Result<Values, NodeError> {
        let image = inputs.image("image")?;
        let bits = adaptive(
            &intensities(image),
            image.width() as usize,
            self.radius,
            self.offset,
        );
        let mut mask = Mask::zeros(image.width(), image.height());
        mask.data_mut().copy_from_slice(&bits);
        Ok(outputs([("mask", Value::mask(mask))]))
    }

    fn halo(&self) -> Option<u32> {
        Some(self.radius as u32)
    }
}

/// `threshold.adaptive`: a mask of the pixels whose intensity exceeds their
/// window's mean by an offset, its halo the window's.
pub(super) fn threshold_adaptive() -> Arc<dyn Node> {
    let descriptor = Descriptor::new(
        "threshold.adaptive",
        1,
        "threshold",
        "The pixels over their neighbourhood's mean",
    )
    .input("image", PortType::Image)
    .output("mask", PortType::Mask)
    .params::<Adaptive>()
    .capabilities(PIXELWISE);
    try_node(descriptor, |p: Adaptive| Ok(p))
}

fn two() -> usize {
    2
}

fn half_and_one() -> f32 {
    1.5
}

/// `cells.split`'s parameters.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Split {
    /// No pixel within this many of a seed is higher: 2 unless given.
    #[serde(default = "two")]
    #[schemars(range(min = 1, max = 64))]
    seed_radius: usize,
    /// A seed's least distance from the background, in pixels: 1.5 unless
    /// given.
    #[serde(default = "half_and_one")]
    min_distance: f32,
}

/// `cells.split`: a mask's objects, its holes filled, touching ones split
/// by the seeded watershed on its distance transform: an image of their
/// numbers, from 1, and 0 elsewhere.
pub(super) fn split() -> Arc<dyn Node> {
    let descriptor = Descriptor::new(
        "cells.split",
        1,
        "cells",
        "Objects, touching ones split by a seeded watershed",
    )
    .input("mask", PortType::Mask)
    .output("labels", PortType::Image)
    .params::<Split>()
    .capabilities(WHOLE);
    try_node(descriptor, |p: Split| {
        Ok(
            move |_: &StepCtx<'_>, inputs: &Inputs<'_>| -> Result<Values, NodeError> {
                let mask = inputs.mask("mask")?;
                let width = mask.width() as usize;
                let filled = fill_holes(mask.data(), width);
                let distance = distances(&filled, width);
                let (labels, _) =
                    watershed(&filled, &distance, width, p.seed_radius, p.min_distance);
                let shape = Shape::new(mask.width(), mask.height(), 1);
                let labels = labels.iter().map(|&l| l as f32).collect();
                let image = Image::from_vec(shape, labels).map_err(|e| e.to_string())?;
                Ok(outputs([("labels", Value::image(image))]))
            },
        )
    })
}

fn one_um() -> [f64; 2] {
    [1.0, 1.0]
}

/// `cells.measure`'s parameters.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Measure {
    /// A pixel's size across and down, in µm: 1 unless given.
    #[serde(default = "one_um")]
    pixel_um: [f64; 2],
    /// The least area kept, in µm²: 10 unless given.
    min_area: Option<f64>,
    /// The greatest area kept, in µm²: 200 unless given.
    max_area: Option<f64>,
    /// The least circularity kept: 0.4 unless given.
    min_circularity: Option<f64>,
}

/// `cells.measure`: of labels, and an image of their size if given, each
/// object's area, perimeter, circularity, centroid and channels' means, in
/// a table, and its outline, as GeoJSON in µm, those its filters keep.
pub(super) fn measure_node() -> Arc<dyn Node> {
    let descriptor = Descriptor::new(
        "cells.measure",
        1,
        "cells",
        "Each object's measures and outline",
    )
    .input("labels", PortType::Image)
    .optional_input("image", PortType::Image)
    .output("objects", PortType::Table)
    .output("outlines", PortType::Geometry)
    .params::<Measure>()
    .capabilities(WHOLE);
    try_node(descriptor, |p: Measure| {
        let defaults = Filters::default();
        let filters = Filters {
            min_area: p.min_area.unwrap_or(defaults.min_area),
            max_area: p.max_area.unwrap_or(defaults.max_area),
            min_circularity: p.min_circularity.unwrap_or(defaults.min_circularity),
        };
        Ok(
            move |_: &StepCtx<'_>, inputs: &Inputs<'_>| -> Result<Values, NodeError> {
                let image = inputs.image("labels")?;
                let width = image.width() as usize;
                let labels: Vec<u32> = image.data().iter().map(|&v| v as u32).collect();
                let count = labels.iter().copied().max().unwrap_or(0);
                // Each channel of the image, apart.
                let picture = inputs.get("image").and_then(Value::as_image);
                let channels: Vec<Vec<f32>> = match picture {
                    Some(p) if p.width() == image.width() && p.height() == image.height() => {
                        let n = p.channels();
                        (0..n)
                            .map(|c| p.data().iter().skip(c).step_by(n).copied().collect())
                            .collect()
                    }
                    Some(_) => return Err("the image isn't the labels' size".into()),
                    None => Vec::new(),
                };
                let views: Vec<&[f32]> = channels.iter().map(Vec::as_slice).collect();
                let objects = measure(&labels, width, count, p.pixel_um, &views);
                let kept: Vec<_> = objects.iter().filter(|o| filters.keep(o)).collect();
                let mut columns: Vec<String> =
                    ["label", "area", "perimeter", "circularity", "x", "y"]
                        .map(str::to_owned)
                        .to_vec();
                columns.extend((0..views.len()).map(|c| format!("mean {c}")));
                let rows = kept
                    .iter()
                    .map(|o| {
                        let mut row = vec![
                            json!(o.label),
                            json!(o.area),
                            json!(o.perimeter),
                            json!(o.circularity),
                            json!(o.centroid[0]),
                            json!(o.centroid[1]),
                        ];
                        row.extend(o.means.iter().map(|m| json!(m)));
                        row
                    })
                    .collect();
                let features: Vec<serde_json::Value> = kept
                    .iter()
                    .map(|o| {
                        let ring: Vec<[f64; 2]> = outline(&labels, width, o)
                            .iter()
                            .map(|c| [c[0] * p.pixel_um[0], c[1] * p.pixel_um[1]])
                            .collect();
                        json!({
                            "type": "Feature",
                            "geometry": {"type": "Polygon", "coordinates": [ring]},
                            "properties": {"label": o.label, "area": o.area, "circularity": o.circularity},
                        })
                    })
                    .collect();
                let table = Table { columns, rows };
                let geometry = json!({"type": "FeatureCollection", "features": features});
                Ok(outputs([
                    ("objects", Value::table(table)),
                    ("outlines", Value::Geometry(Arc::new(geometry))),
                ]))
            },
        )
    })
}
