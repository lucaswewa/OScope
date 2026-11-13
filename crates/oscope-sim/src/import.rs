//! Your own images and stacks as samples (D23): placed on the slide at
//! their scale, rendered at any pixel.

use std::path::Path;

use oscope_compute::{Image, Shape};
use oscope_core::{Point2, Rect, Um};
use oscope_imaging::color::{ColorError, read_image};

use crate::fluorescence::Dye;
use crate::sample::{Layer, LayerKind, Sample, render_pixels};

/// The lowest transmittance an imported image's black stands for: an
/// optical density of 4.
const DARKEST: f32 = 1e-4;

/// What an imported image's values are.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImportLight {
    /// A brightfield image: the light each pixel passes. Its pages become
    /// absorbing layers.
    Transmitted,
    /// A reflected-light image: the light each pixel reflects.
    Reflected,
    /// A fluorescence image or stack: each pixel's brightness, 0 to 1, is
    /// its amount of `Dye`, in the units of `LayerKind::Fluorescent`, times
    /// [`Imported::FLUORESCENCE`].
    Fluorescent(Dye),
}

/// An image, or a stack of them, imported as a sample.
///
/// - **Its place:** its top-left corner at a point on the slide, each pixel
///   `um_per_pixel` across.
/// - **Its depth:** a stack's pages are layers [`Imported::z_step`] apart,
///   the first at the slide's surface; a single image is one layer there.
///
/// It renders each pixel as the exact mean of the image's pixels under it,
/// whatever the scale: so a coarse field of it is a fine one averaged
/// down. It has no ground truth beyond itself.
#[derive(Clone, Debug)]
pub struct Imported {
    name: String,
    pages: Vec<Image>,
    um_per_pixel: f64,
    z_step: f64,
    corner: Point2,
    layers: Vec<Layer>,
}

impl Imported {
    /// The amount of dye a fluorescence image's full brightness stands for.
    pub const FLUORESCENCE: f32 = 0.2;

    /// `pages` of linear RGB, `um_per_pixel` across and `z_step` apart, as
    /// `light`'s layers, with the first page's corner at `corner` on the
    /// slide. Every page must have the first's size.
    pub fn new(
        name: &str,
        pages: Vec<Image>,
        um_per_pixel: Um,
        z_step: Um,
        corner: Point2,
        light: ImportLight,
    ) -> Result<Self, String> {
        let first = pages.first().ok_or("no pages")?.shape();
        if pages
            .iter()
            .any(|page| page.shape() != first || page.channels() != 3)
        {
            return Err("pages of different sizes".to_owned());
        }
        if !(um_per_pixel.0 > 0.0 && z_step.0 >= 0.0) {
            return Err(format!(
                "a scale of {} µm a pixel and a step of {} µm",
                um_per_pixel.0, z_step.0
            ));
        }
        let kind = match light {
            ImportLight::Transmitted => LayerKind::Absorbing,
            ImportLight::Reflected => LayerKind::Reflective,
            ImportLight::Fluorescent(dye) => LayerKind::Fluorescent(dye),
        };
        // What each layer renders: optical densities, reflectances or
        // amounts of dye.
        let pages: Vec<Image> = pages
            .into_iter()
            .map(|mut page| {
                for v in page.data_mut() {
                    *v = match light {
                        ImportLight::Transmitted => -v.clamp(DARKEST, 1.0).log10(),
                        ImportLight::Reflected => v.clamp(0.0, 1.0),
                        ImportLight::Fluorescent(_) => v.max(0.0) * Self::FLUORESCENCE,
                    };
                }
                page
            })
            .collect();
        let layers = (0..pages.len())
            .map(|k| Layer {
                name: format!("{name}, page {}", k + 1),
                z: Um(k as f64 * z_step.0),
                thickness: Um(z_step.0.max(0.1)),
                kind,
            })
            .collect();
        Ok(Self {
            name: name.to_owned(),
            pages,
            um_per_pixel: um_per_pixel.0,
            z_step: z_step.0,
            corner,
            layers,
        })
    }

    /// The image or stack in the file at `path` (PNG, JPEG or TIFF; see
    /// `oscope_imaging::color::read_image`), as [`Imported::new`] places it.
    pub fn read(
        path: &Path,
        um_per_pixel: Um,
        z_step: Um,
        corner: Point2,
        light: ImportLight,
    ) -> Result<Self, String> {
        let pages = read_image(path).map_err(|e: ColorError| e.to_string())?;
        let name = path.file_name().map_or_else(
            || "imported".to_owned(),
            |n| n.to_string_lossy().into_owned(),
        );
        Self::new(&name, pages, um_per_pixel, z_step, corner, light)
            .map_err(|e| format!("{}: {e}", path.display()))
    }

    /// The same, moved so that its middle is at `centre` on the slide.
    pub fn centred_on(mut self, centre: Point2) -> Self {
        let (w, h) = self.pixels();
        let s = self.um_per_pixel;
        self.corner = Point2::new(
            centre.x - f64::from(w) * s / 2.0,
            centre.y - f64::from(h) * s / 2.0,
        );
        self
    }

    /// The spacing of a stack's pages.
    pub fn z_step(&self) -> Um {
        Um(self.z_step)
    }

    /// The size of a page, in pixels.
    pub fn pixels(&self) -> (u32, u32) {
        (self.pages[0].width(), self.pages[0].height())
    }
}

/// The image pixels an interval `[from, to)` of the output covers, in
/// image pixels, each with the share of the interval it covers.
fn shares(from: f64, to: f64, size: u32) -> Vec<(usize, f64)> {
    let length = to - from;
    let first = from.floor().max(0.0) as i64;
    let last = (to.ceil() as i64).min(i64::from(size));
    (first..last)
        .filter_map(|i| {
            let covered = (to.min(i as f64 + 1.0) - from.max(i as f64)).max(0.0);
            (covered > 0.0).then_some((i as usize, covered / length))
        })
        .collect()
}

impl Sample for Imported {
    fn name(&self) -> &str {
        &self.name
    }

    fn layers(&self) -> &[Layer] {
        &self.layers
    }

    fn bounds(&self) -> Rect {
        let (w, h) = self.pixels();
        let s = self.um_per_pixel;
        Rect::new(
            self.corner.x,
            self.corner.y,
            f64::from(w) * s,
            f64::from(h) * s,
        )
    }

    fn render(&self, index: usize, origin: Point2, pixel: Um, width: u32, height: u32) -> Image {
        let Some(page) = self.pages.get(index) else {
            return Image::zeros(Shape::new(width, height, 3));
        };
        let (pw, ph) = self.pixels();
        let (p, s) = (pixel.0, self.um_per_pixel);
        let data = page.data();
        render_pixels(origin, pixel, width, height, |x0, y0| {
            // The output pixel's square, in image pixels.
            let (u0, v0) = ((x0 - self.corner.x) / s, (y0 - self.corner.y) / s);
            let (columns, rows) = (shares(u0, u0 + p / s, pw), shares(v0, v0 + p / s, ph));
            let mut rgb = [0.0_f64; 3];
            for (y, wy) in &rows {
                for (x, wx) in &columns {
                    let at = (y * pw as usize + x) * 3;
                    for (c, out) in rgb.iter_mut().enumerate() {
                        *out += wy * wx * f64::from(data[at + c]);
                    }
                }
            }
            rgb.map(|v| v as f32)
        })
    }
}
