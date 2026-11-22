//! The colour target (P33): OScope's [`ColourTarget`], its patches
//! absorbing as their colours say between opaque borders, which
//! calibration's colour step measures.

use oscope_compute::Image;
use oscope_core::{Point2, Rect, Um};
use oscope_imaging::color::ColourTarget;

use crate::noise::stripes;
use crate::sample::{Layer, LayerKind, Sample, render_pixels};

/// The colour target, centred at a point of the slide.
#[derive(Clone, Debug)]
pub struct ColourTargetSample {
    target: ColourTarget,
    bounds: Rect,
    /// Each patch's optical densities.
    densities: Vec<[f64; 3]>,
    layers: Vec<Layer>,
}

impl ColourTargetSample {
    /// OScope's target, centred at `centre` on the slide.
    pub fn new(centre: Point2) -> Self {
        let target = ColourTarget::oscope();
        let (w, h) = target.size_um();
        let densities = target
            .patches()
            .iter()
            .map(|p| p.rgb.map(|t| -t.log10()))
            .collect();
        Self {
            target,
            bounds: Rect::new(centre.x - w / 2.0, centre.y - h / 2.0, w, h),
            densities,
            layers: vec![Layer {
                name: "patches".to_owned(),
                z: Um(0.05),
                thickness: Um(0.1),
                kind: LayerKind::Absorbing,
            }],
        }
    }
}

impl Sample for ColourTargetSample {
    fn name(&self) -> &str {
        "colour target"
    }

    fn layers(&self) -> &[Layer] {
        &self.layers
    }

    fn bounds(&self) -> Rect {
        self.bounds
    }

    /// Each pixel is its cell's patch where the patch covers it, and the
    /// border elsewhere; clear outside the target.
    fn render(&self, index: usize, origin: Point2, pixel: Um, width: u32, height: u32) -> Image {
        let (b, p, t) = (self.bounds, pixel.0, &self.target);
        let (pitch, side) = (t.pitch_um, t.side_um);
        let inset = (pitch - side) / 2.0;
        let extent = |n: u32| f64::from(n - 1) * pitch + side;
        render_pixels(origin, pixel, width, height, |x0, y0| {
            let (cx, cy) = (x0 + p / 2.0 - b.origin.x, y0 + p / 2.0 - b.origin.y);
            let inside = (0.0..b.size.width).contains(&cx) && (0.0..b.size.height).contains(&cy);
            if index != 0 || !inside {
                return [0.0; 3];
            }
            let (column, row) = ((cx / pitch) as u32, (cy / pitch) as u32);
            let patch = &self.densities[(row * t.columns + column) as usize];
            let (sx, sy) = (x0 - b.origin.x - inset, y0 - b.origin.y - inset);
            let fx = stripes(sx, sx + p, pitch, side, extent(t.columns));
            let fy = stripes(sy, sy + p, pitch, side, extent(t.rows));
            let covered = fx * fy;
            patch.map(|od| (covered * od + (1.0 - covered) * t.border_od) as f32)
        })
    }
}
