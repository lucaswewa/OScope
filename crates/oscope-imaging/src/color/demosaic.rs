//! Demosaicing: from a Bayer mosaic, where each pixel has one colour, to RGB.

use oscope_compute::{Image, Shape};
use oscope_core::{BayerColour, BayerPattern};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use super::ColorError;

/// How to fill in the two colours each pixel of a mosaic lacks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Demosaic {
    /// Each 2 × 2 cell becomes one pixel: its red, the mean of its two
    /// greens, and its blue. Half the width and height, and nothing is
    /// interpolated.
    Superpixel,
    /// Each missing colour is the mean of the nearest pixels of that colour.
    Bilinear,
    /// Bilinear, corrected by the gradient of the pixel's own colour, with
    /// Malvar, He and Cutler's 5 × 5 filters (ICASSP 2004). Sharper than
    /// bilinear at edges.
    MalvarHeCutler,
}

impl Demosaic {
    /// Every method, simplest first.
    pub const ALL: [Self; 3] = [Self::Superpixel, Self::Bilinear, Self::MalvarHeCutler];
}

/// RGB, from 0 to 1 like the mosaic, from a one-channel mosaic whose
/// top-left pixel is `pattern`'s first.
///
/// - **Superpixel** gives half the width and height, rounded down.
/// - **The others** give the mosaic's size. At the edges they mirror the
///   mosaic about its outermost pixels, which keeps every pixel's colour.
///   Malvar–He–Cutler can overshoot at sharp edges; its output is clamped
///   to 0–1.
pub fn demosaic(
    mosaic: &Image,
    pattern: BayerPattern,
    method: Demosaic,
) -> Result<Image, ColorError> {
    if mosaic.channels() != 1 {
        return Err(ColorError::Channels(mosaic.channels()));
    }
    Ok(match method {
        Demosaic::Superpixel => superpixel(mosaic, pattern),
        Demosaic::Bilinear => full(mosaic, pattern, bilinear),
        Demosaic::MalvarHeCutler => full(mosaic, pattern, malvar_he_cutler),
    })
}

fn superpixel(mosaic: &Image, pattern: BayerPattern) -> Image {
    let shape = Shape::new(mosaic.width() / 2, mosaic.height() / 2, 3);
    let mut out = Image::zeros(shape);
    if shape.is_empty() {
        return out;
    }
    // Where each colour sits in a cell.
    let cell = [(0, 0), (1, 0), (0, 1), (1, 1)];
    let at = |colour| {
        cell.into_iter()
            .filter(move |&(x, y)| pattern.colour_at(x, y) == colour)
    };
    let (red, blue) = (at(BayerColour::Red).next(), at(BayerColour::Blue).next());
    let mut greens = at(BayerColour::Green);
    let [red, blue, green_1, green_2] = [red, blue, greens.next(), greens.next()]
        .map(|place| place.expect("a Bayer cell has one red, one blue and two greens"));
    out.data_mut()
        .par_chunks_mut(shape.row_len())
        .enumerate()
        .for_each(|(y, row)| {
            let rows = [mosaic.row(2 * y as u32), mosaic.row(2 * y as u32 + 1)];
            for (x, pixel) in row.as_chunks_mut::<3>().0.iter_mut().enumerate() {
                let get = |(dx, dy): (u32, u32)| rows[dy as usize][2 * x + dx as usize];
                *pixel = [get(red), (get(green_1) + get(green_2)) / 2.0, get(blue)];
            }
        });
    out
}

/// What colour a pixel has, and for green, which colour shares its row.
#[derive(Clone, Copy)]
enum Site {
    Red,
    Blue,
    GreenInRedRow,
    GreenInBlueRow,
}

/// The 5 × 5 pixels around one, mirrored at the mosaic's edges.
struct Window<'a> {
    rows: &'a [&'a [f32]; 5],
    x: usize,
    width: usize,
    inside: bool,
}

impl Window<'_> {
    /// The pixel `dx` across and `dy` down from the centre, each from -2 to 2.
    #[inline(always)]
    fn at(&self, dx: isize, dy: isize) -> f32 {
        let row = self.rows[(dy + 2) as usize];
        let x = self.x as isize + dx;
        match self.inside {
            true => row[x as usize],
            false => row[mirror(x, self.width)],
        }
    }
}

/// `i` mirrored into `0..n` about the edge pixels (… 2 1 0 1 2 …), which
/// keeps a mosaic's colours in place. A side of 1 or 2 pixels has no room
/// to mirror by 2, and keeps `i`'s parity instead.
fn mirror(i: isize, n: usize) -> usize {
    let n = n as isize;
    let mirrored = if i < 0 {
        -i
    } else if i >= n {
        2 * (n - 1) - i
    } else {
        i
    };
    match (0..n).contains(&mirrored) {
        true => mirrored as usize,
        false => i.rem_euclid(2).min(n - 1) as usize,
    }
}

/// A full-size RGB image, each pixel from `pixel` on its window.
fn full<F>(mosaic: &Image, pattern: BayerPattern, pixel: F) -> Image
where
    F: Fn(&Window<'_>, Site) -> [f32; 3] + Sync,
{
    let (width, height) = (mosaic.width() as usize, mosaic.height() as usize);
    let mut out = Image::zeros(Shape::new(mosaic.width(), mosaic.height(), 3));
    if width == 0 || height == 0 {
        return out;
    }
    out.data_mut()
        .par_chunks_mut(width * 3)
        .enumerate()
        .for_each(|(y, row)| {
            let rows =
                [-2, -1, 0, 1, 2].map(|dy| mosaic.row(mirror(y as isize + dy, height) as u32));
            let colours = [0, 1].map(|x| pattern.colour_at(x, y as u32));
            let red_row = colours.contains(&BayerColour::Red);
            for (x, rgb) in row.as_chunks_mut::<3>().0.iter_mut().enumerate() {
                let site = match colours[x % 2] {
                    BayerColour::Red => Site::Red,
                    BayerColour::Blue => Site::Blue,
                    BayerColour::Green if red_row => Site::GreenInRedRow,
                    BayerColour::Green => Site::GreenInBlueRow,
                };
                let inside = x >= 2 && x + 2 < width;
                *rgb = pixel(
                    &Window {
                        rows: &rows,
                        x,
                        width,
                        inside,
                    },
                    site,
                );
            }
        });
    out
}

/// Bilinear: the mean of the nearest pixels of each missing colour.
fn bilinear(w: &Window<'_>, site: Site) -> [f32; 3] {
    let c = w.at(0, 0);
    let cross = (w.at(-1, 0) + w.at(1, 0) + w.at(0, -1) + w.at(0, 1)) / 4.0;
    let diagonal = (w.at(-1, -1) + w.at(1, -1) + w.at(-1, 1) + w.at(1, 1)) / 4.0;
    let across = (w.at(-1, 0) + w.at(1, 0)) / 2.0;
    let down = (w.at(0, -1) + w.at(0, 1)) / 2.0;
    match site {
        Site::Red => [c, cross, diagonal],
        Site::Blue => [diagonal, cross, c],
        Site::GreenInRedRow => [across, c, down],
        Site::GreenInBlueRow => [down, c, across],
    }
}

/// Malvar, He and Cutler's filters (2004, figure 2), in eighths. Each adds
/// to the bilinear estimate a share of the centre colour's own gradient.
fn malvar_he_cutler(w: &Window<'_>, site: Site) -> [f32; 3] {
    let c = w.at(0, 0);
    let across = w.at(-1, 0) + w.at(1, 0);
    let down = w.at(0, -1) + w.at(0, 1);
    let diagonal = w.at(-1, -1) + w.at(1, -1) + w.at(-1, 1) + w.at(1, 1);
    let far_across = w.at(-2, 0) + w.at(2, 0);
    let far_down = w.at(0, -2) + w.at(0, 2);
    // Green at red or blue.
    let green = (4.0 * c + 2.0 * (across + down) - far_across - far_down) / 8.0;
    // Red at blue, or blue at red.
    let opposite = (6.0 * c + 2.0 * diagonal - 1.5 * (far_across + far_down)) / 8.0;
    // At green: the colour beside it in its row, and the one above and below.
    let in_row = (5.0 * c + 4.0 * across - diagonal - far_across + 0.5 * far_down) / 8.0;
    let in_column = (5.0 * c + 4.0 * down - diagonal - far_down + 0.5 * far_across) / 8.0;
    let rgb = match site {
        Site::Red => [c, green, opposite],
        Site::Blue => [opposite, green, c],
        Site::GreenInRedRow => [in_row, c, in_column],
        Site::GreenInBlueRow => [in_column, c, in_row],
    };
    rgb.map(|v| v.clamp(0.0, 1.0))
}
