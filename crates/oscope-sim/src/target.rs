//! A test target with exact answers, to check the renderer against.

use oscope_compute::{Image, Shape};
use oscope_core::{Point2, Rect, Um};

use crate::noise::stripes;
use crate::sample::{Layer, LayerKind, Sample, render_pixels};

/// A 4 × 4 mm target of two layers:
///
/// - **`squares`,** absorbing, 0.1 µm thick at the slide's surface: squares
///   [`TestTarget::SQUARE`] on a side every [`TestTarget::PITCH`], of the
///   same optical density in every channel;
/// - **`bars`,** reflective, [`TestTarget::BAR_HEIGHT`] up: vertical bars
///   [`TestTarget::BAR`] wide every [`TestTarget::BAR_PITCH`].
///
/// Each pixel is the exact mean over its square, so a coarse rendering
/// equals a fine one averaged down.
#[derive(Clone, Debug)]
pub struct TestTarget {
    bounds: Rect,
    od: f64,
    reflectance: f64,
    layers: Vec<Layer>,
}

impl TestTarget {
    /// The side of a square.
    pub const SQUARE: Um = Um(50.0);
    /// The squares' pitch.
    pub const PITCH: Um = Um(100.0);
    /// A bar's width.
    pub const BAR: Um = Um(20.0);
    /// The bars' pitch.
    pub const BAR_PITCH: Um = Um(40.0);
    /// The height of the bars' layer above the slide.
    pub const BAR_HEIGHT: Um = Um(20.0);

    /// A target whose corner is at `corner` on the slide, with squares of
    /// optical density `od`, and bars that reflect `reflectance` of the
    /// light.
    pub fn new(corner: Point2, od: f64, reflectance: f64) -> Self {
        let layer = |name: &str, z: f64, kind| Layer {
            name: name.to_owned(),
            z: Um(z),
            thickness: Um(0.1),
            kind,
        };
        Self {
            bounds: Rect::new(corner.x, corner.y, 4_000.0, 4_000.0),
            od,
            reflectance,
            layers: vec![
                layer("squares", 0.05, LayerKind::Absorbing),
                layer("bars", Self::BAR_HEIGHT.0, LayerKind::Reflective),
            ],
        }
    }
}

impl Sample for TestTarget {
    fn name(&self) -> &str {
        "test target"
    }

    fn layers(&self) -> &[Layer] {
        &self.layers
    }

    fn bounds(&self) -> Rect {
        self.bounds
    }

    fn render(&self, index: usize, origin: Point2, pixel: Um, width: u32, height: u32) -> Image {
        let (b, p) = (self.bounds, pixel.0);
        // The mean of a pattern over the pixel's extent along one axis, in
        // the target's own coordinates.
        let along = |start: f64, corner: f64, period: f64, width: f64, extent: f64| {
            stripes(start - corner, start - corner + p, period, width, extent)
        };
        match index {
            0 => render_pixels(origin, pixel, width, height, |x0, y0| {
                let (pitch, side) = (Self::PITCH.0, Self::SQUARE.0);
                let fx = along(x0, b.origin.x, pitch, side, b.size.width);
                let fy = along(y0, b.origin.y, pitch, side, b.size.height);
                [(self.od * fx * fy) as f32; 3]
            }),
            1 => render_pixels(origin, pixel, width, height, |x0, y0| {
                let fx = along(x0, b.origin.x, Self::BAR_PITCH.0, Self::BAR.0, b.size.width);
                let fy = along(y0, b.origin.y, b.size.height, b.size.height, b.size.height);
                [(self.reflectance * fx * fy) as f32; 3]
            }),
            _ => Image::zeros(Shape::new(width, height, 3)),
        }
    }
}
