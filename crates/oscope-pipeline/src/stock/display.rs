//! Display: levels and a tone curve, lookup tables, and false colour. Each
//! gives display values, from 0 to 1.

use std::sync::Arc;

use oscope_compute::{Image, Shape};
use oscope_imaging::color::Curve;
use rayon::prelude::*;
use schemars::JsonSchema;
use serde::Deserialize;

use super::PIXELWISE;
use crate::node::{Descriptor, Inputs, Node, NodeError, StepCtx, try_node};
use crate::value::{PortType, Value, Values, outputs};

fn one() -> f32 {
    1.0
}

/// `display.levels`'s parameters.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Levels {
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

/// `display.levels`: each sample from `black`–`white` to 0–1, through the
/// tone curve, read from a table of the 65,536 16-bit levels. It is
/// `oscope_imaging::color::to_display` before its last step to 8 bits.
pub(super) fn levels() -> Arc<dyn Node> {
    let descriptor = Descriptor::new("display.levels", 1, "display", "Levels and a tone curve")
        .input("image", PortType::Image)
        .output("image", PortType::Image)
        .params::<Levels>()
        .capabilities(PIXELWISE);
    try_node(
        descriptor,
        |Levels {
             black,
             white,
             gamma,
         }| {
            if white.partial_cmp(&black) != Some(std::cmp::Ordering::Greater) {
                return Err(format!("white ({white}) must be above black ({black})"));
            }
            let curve = match gamma {
                Some(gamma) if gamma > 0.0 => Curve::Gamma { gamma },
                Some(gamma) => return Err(format!("the gamma ({gamma}) must be above 0")),
                None => Curve::Srgb,
            };
            let level = |code: u16| (f32::from(code) / 65_535.0 - black) / (white - black);
            let table: Vec<f32> = (0..=u16::MAX)
                .map(|code| curve.apply(level(code).clamp(0.0, 1.0)))
                .collect();
            Ok(map_samples(move |v| {
                table[(v.clamp(0.0, 1.0) * 65_535.0).round() as usize]
            }))
        },
    )
}

/// `display.lut`'s parameters.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Lut {
    /// The table: the values for inputs from 0 to 1, evenly spaced.
    #[schemars(length(min = 2, max = 65536))]
    table: Vec<f32>,
}

/// `display.lut`: each sample through a lookup table, interpolated
/// linearly between its entries; inputs outside 0–1 take its ends.
pub(super) fn lut() -> Arc<dyn Node> {
    let descriptor = Descriptor::new("display.lut", 1, "display", "A lookup table")
        .input("image", PortType::Image)
        .output("image", PortType::Image)
        .params::<Lut>()
        .capabilities(PIXELWISE);
    try_node(descriptor, |Lut { table }| {
        if table.len() < 2 {
            return Err("the table needs two entries at least".into());
        }
        Ok(map_samples(move |v| look_up(&table, v.clamp(0.0, 1.0))))
    })
}

/// `values` at `t` from 0 to 1, evenly spaced over it, interpolated.
fn look_up(values: &[f32], t: f32) -> f32 {
    let at = t * (values.len() - 1) as f32;
    let i = (at.floor() as usize).min(values.len() - 2);
    let f = at - i as f32;
    values[i] + f * (values[i + 1] - values[i])
}

/// A step changing each sample by `f`.
fn map_samples(
    f: impl Fn(f32) -> f32 + Send + Sync + 'static,
) -> impl Fn(&StepCtx<'_>, &Inputs<'_>) -> Result<Values, NodeError> + Send + Sync {
    move |_: &StepCtx<'_>, inputs: &Inputs<'_>| {
        let image = inputs.image("image")?;
        let mut out = Image::zeros(image.shape());
        let samples = out.data_mut().par_iter_mut().zip(image.data().par_iter());
        samples.for_each(|(out, v)| *out = f(*v));
        Ok(outputs([("image", Value::image(out))]))
    }
}

/// A colour map: intensities from 0 to 1 to colours.
#[derive(Clone, Copy, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum ColourMap {
    /// Black to white.
    Grey,
    /// Black, red, yellow, white.
    #[default]
    Fire,
    /// Black, blue, cyan, white.
    Ice,
    /// Blue, cyan, green, yellow, red.
    Spectrum,
}

impl ColourMap {
    /// Its colours, at intensities from 0 to 1, in display values.
    fn stops(self) -> &'static [(f32, [f32; 3])] {
        match self {
            Self::Grey => &[(0.0, [0.0; 3]), (1.0, [1.0; 3])],
            Self::Fire => &[
                (0.0, [0.0, 0.0, 0.0]),
                (0.35, [0.8, 0.0, 0.0]),
                (0.7, [1.0, 0.8, 0.0]),
                (1.0, [1.0, 1.0, 1.0]),
            ],
            Self::Ice => &[
                (0.0, [0.0, 0.0, 0.0]),
                (0.35, [0.0, 0.2, 0.8]),
                (0.7, [0.0, 0.9, 1.0]),
                (1.0, [1.0, 1.0, 1.0]),
            ],
            Self::Spectrum => &[
                (0.0, [0.0, 0.0, 1.0]),
                (0.25, [0.0, 1.0, 1.0]),
                (0.5, [0.0, 1.0, 0.0]),
                (0.75, [1.0, 1.0, 0.0]),
                (1.0, [1.0, 0.0, 0.0]),
            ],
        }
    }

    /// The colour at intensity `t`, from 0 to 1.
    fn colour(self, t: f32) -> [f32; 3] {
        let stops = self.stops();
        let i = stops
            .iter()
            .rposition(|(at, _)| *at <= t)
            .unwrap_or(0)
            .min(stops.len() - 2);
        let ((a, low), (b, high)) = (stops[i], stops[i + 1]);
        let f = ((t - a) / (b - a)).clamp(0.0, 1.0);
        [0, 1, 2].map(|c| low[c] + f * (high[c] - low[c]))
    }
}

/// `display.false_colour`'s parameters.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct FalseColour {
    /// The colour map: `fire` unless given.
    #[serde(default)]
    map: ColourMap,
    /// The intensity shown as the map's first colour.
    #[serde(default)]
    black: f32,
    /// The intensity shown as its last: above black.
    #[serde(default = "one")]
    white: f32,
}

/// `display.false_colour`: each pixel's intensity, the mean of its
/// channels, from `black`–`white` through a colour map: an RGB image in
/// display values.
pub(super) fn false_colour() -> Arc<dyn Node> {
    let descriptor = Descriptor::new(
        "display.false_colour",
        1,
        "display",
        "Intensity through a colour map",
    )
    .input("image", PortType::Image)
    .output("image", PortType::Image)
    .params::<FalseColour>()
    .capabilities(PIXELWISE);
    try_node(descriptor, |FalseColour { map, black, white }| {
        if white.partial_cmp(&black) != Some(std::cmp::Ordering::Greater) {
            return Err(format!("white ({white}) must be above black ({black})"));
        }
        Ok(
            move |_: &StepCtx<'_>, inputs: &Inputs<'_>| -> Result<Values, NodeError> {
                let image = inputs.image("image")?;
                let channels = image.channels().max(1);
                let shape = Shape::new(image.width(), image.height(), 3);
                let mut rgb = Image::zeros(shape);
                let pixels = rgb
                    .data_mut()
                    .par_chunks_mut(3)
                    .zip(image.data().par_chunks(channels));
                pixels.for_each(|(out, pixel)| {
                    let intensity = pixel.iter().sum::<f32>() / channels as f32;
                    out.copy_from_slice(
                        &map.colour(((intensity - black) / (white - black)).clamp(0.0, 1.0)),
                    );
                });
                Ok(outputs([("image", Value::image(rgb))]))
            },
        )
    })
}
