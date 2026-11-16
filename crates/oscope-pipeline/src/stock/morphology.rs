//! Morphology on masks (P23): erosion, dilation, opening and closing; the
//! distance transform; and connected-component labelling.

use std::sync::Arc;

use oscope_compute::{Image, Shape};
use rayon::prelude::*;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::json;

use super::{PIXELWISE, WHOLE};
use crate::node::{Descriptor, Inputs, Node, NodeError, Step, StepCtx, try_node};
use crate::value::{Mask, PortType, Table, Value, Values, outputs};

/// A structuring element's shape.
#[derive(Clone, Copy, Default, PartialEq, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum ElementShape {
    /// A square `2r + 1` across.
    #[default]
    Square,
    /// The pixels within `r` of the centre.
    Disk,
}

/// The morphology nodes' parameters.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Element {
    /// The structuring element's radius, in pixels.
    #[schemars(range(min = 1, max = 32))]
    radius: u32,
    /// Its shape: `square` unless given.
    #[serde(default)]
    shape: ElementShape,
}

/// A structuring element's offsets.
fn offsets(radius: u32, shape: ElementShape) -> Vec<(i64, i64)> {
    let r = i64::from(radius);
    let within = |dx: i64, dy: i64| shape == ElementShape::Square || dx * dx + dy * dy <= r * r;
    (-r..=r)
        .flat_map(|dy| (-r..=r).map(move |dx| (dx, dy)))
        .filter(|&(dx, dy)| within(dx, dy))
        .collect()
}

/// Erosion, `true`, or dilation. A pixel is set by dilation if any pixel
/// under the element is, and kept by erosion only if all are: beyond the
/// edges, dilation sees nothing set and erosion everything, so the edges
/// neither grow nor wear.
fn apply(mask: &Mask, element: &[(i64, i64)], erode: bool) -> Mask {
    let (width, height) = (i64::from(mask.width()), i64::from(mask.height()));
    let mut out = Mask::zeros(mask.width(), mask.height());
    if width == 0 || height == 0 {
        return out;
    }
    let data = mask.data();
    out.data_mut()
        .par_chunks_mut(width as usize)
        .enumerate()
        .for_each(|(y, row)| {
            for (x, bit) in row.iter_mut().enumerate() {
                let set = |&(dx, dy): &(i64, i64)| {
                    let (xx, yy) = (x as i64 + dx, y as i64 + dy);
                    match (0..width).contains(&xx) && (0..height).contains(&yy) {
                        true => data[(yy * width + xx) as usize] != 0,
                        false => erode,
                    }
                };
                *bit = u8::from(match erode {
                    true => element.iter().all(set),
                    false => element.iter().any(set),
                });
            }
        });
    out
}

/// The four operations, as erosions (`true`) and dilations in order.
const OPERATIONS: [(&str, &str, &[bool]); 4] = [
    ("morphology.erode", "Erosion", &[true]),
    ("morphology.dilate", "Dilation", &[false]),
    (
        "morphology.open",
        "Opening: erosion, then dilation",
        &[true, false],
    ),
    (
        "morphology.close",
        "Closing: dilation, then erosion",
        &[false, true],
    ),
];

/// A morphology step: its element, and its erosions and dilations in order.
struct Morphology {
    element: Vec<(i64, i64)>,
    passes: &'static [bool],
    radius: u32,
}

impl Step for Morphology {
    fn run(&self, _: &StepCtx<'_>, inputs: &Inputs<'_>) -> Result<Values, NodeError> {
        let mut mask = inputs.mask("mask")?.clone();
        for &erode in self.passes {
            mask = apply(&mask, &self.element, erode);
        }
        Ok(outputs([("mask", Value::mask(mask))]))
    }

    fn halo(&self) -> Option<u32> {
        Some(self.radius * self.passes.len() as u32)
    }
}

/// `morphology.erode`, `dilate`, `open` and `close`: on a mask, with a
/// square or disk structuring element.
pub(super) fn operations() -> Vec<Arc<dyn Node>> {
    OPERATIONS
        .iter()
        .map(|&(id, description, passes)| {
            let descriptor = Descriptor::new(id, 1, "morphology", description)
                .input("mask", PortType::Mask)
                .output("mask", PortType::Mask)
                .params::<Element>()
                .capabilities(PIXELWISE);
            try_node(descriptor, move |Element { radius, shape }| {
                if !(1..=32).contains(&radius) {
                    return Err(format!("a radius of {radius}: from 1 to 32"));
                }
                let element = offsets(radius, shape);
                Ok(Morphology {
                    element,
                    passes,
                    radius,
                })
            })
        })
        .collect()
}

/// The Euclidean distance from each set pixel to the nearest pixel not set,
/// in pixels, exactly (Felzenszwalb and Huttenlocher, 2012; P46 moved it
/// into `oscope-imaging`).
pub(crate) fn distances(mask: &Mask) -> Image {
    let shape = Shape::new(mask.width(), mask.height(), 1);
    let d = oscope_imaging::cells::distances(mask.data(), mask.width() as usize);
    Image::from_vec(shape, d).expect("a distance for each pixel")
}

/// `morphology.distance`: each set pixel's Euclidean distance to the
/// nearest pixel not set, exactly, in pixels.
pub(super) fn distance() -> Arc<dyn Node> {
    let descriptor = Descriptor::new(
        "morphology.distance",
        1,
        "morphology",
        "The distance transform",
    )
    .input("mask", PortType::Mask)
    .output("image", PortType::Image)
    .params::<super::Nothing>()
    .capabilities(WHOLE);
    try_node(descriptor, |_: super::Nothing| {
        Ok(
            |_: &StepCtx<'_>, inputs: &Inputs<'_>| -> Result<Values, NodeError> {
                Ok(outputs([(
                    "image",
                    Value::image(distances(inputs.mask("mask")?)),
                )]))
            },
        )
    })
}

fn eight() -> u8 {
    8
}

/// `morphology.label`'s parameters.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Connectivity {
    /// The neighbours that join pixels: 4 (sides) or 8 (sides and
    /// corners), 8 unless given.
    #[serde(default = "eight")]
    connectivity: u8,
}

/// Each set pixel's component, from 1 in the order the components first
/// appear row by row, and 0 elsewhere (Rosenfeld and Pfaltz, 1966; P46
/// moved it into `oscope-imaging`).
pub(crate) fn components(mask: &Mask, eight: bool) -> (Vec<u32>, u32) {
    oscope_imaging::cells::components(mask.data(), mask.width() as usize, eight)
}

/// `morphology.label`: the mask's connected components, as an image of
/// their numbers, from 1 in the order they first appear row by row, 0
/// elsewhere; and a table with a row a component: its number, area,
/// centroid and bounding box.
pub(super) fn label() -> Arc<dyn Node> {
    let descriptor = Descriptor::new("morphology.label", 1, "morphology", "Connected components")
        .input("mask", PortType::Mask)
        .output("labels", PortType::Image)
        .output("components", PortType::Table)
        .params::<Connectivity>()
        .capabilities(WHOLE);
    try_node(descriptor, |Connectivity { connectivity }| {
        if !matches!(connectivity, 4 | 8) {
            return Err(format!("a connectivity of {connectivity}: 4 or 8"));
        }
        Ok(
            move |_: &StepCtx<'_>, inputs: &Inputs<'_>| -> Result<Values, NodeError> {
                let mask = inputs.mask("mask")?;
                let (labels, count) = components(mask, connectivity == 8);
                let width = mask.width() as usize;
                // Area, the sums of x and y, and the bounds: for each component.
                let mut tally =
                    vec![(0_u64, 0_u64, 0_u64, u32::MAX, u32::MAX, 0_u32, 0_u32); count as usize];
                for (i, &label) in labels.iter().enumerate().filter(|(_, label)| **label != 0) {
                    let (x, y) = ((i % width) as u32, (i / width) as u32);
                    let t = &mut tally[label as usize - 1];
                    *t = (
                        t.0 + 1,
                        t.1 + u64::from(x),
                        t.2 + u64::from(y),
                        t.3.min(x),
                        t.4.min(y),
                        t.5.max(x),
                        t.6.max(y),
                    );
                }
                let columns = ["label", "area", "x", "y", "left", "top", "width", "height"];
                let rows = tally.iter().enumerate().map(
                    |(k, &(area, sx, sy, left, top, right, bottom))| {
                        let n = area as f64;
                        vec![
                            json!(k + 1),
                            json!(area),
                            json!(sx as f64 / n),
                            json!(sy as f64 / n),
                            json!(left),
                            json!(top),
                            json!(right - left + 1),
                            json!(bottom - top + 1),
                        ]
                    },
                );
                let table = Table {
                    columns: columns.map(String::from).to_vec(),
                    rows: rows.collect(),
                };
                let shape = Shape::new(mask.width(), mask.height(), 1);
                let image = Image::from_vec(shape, labels.iter().map(|&l| l as f32).collect())?;
                Ok(outputs([
                    ("labels", Value::image(image)),
                    ("components", Value::table(table)),
                ]))
            },
        )
    })
}
