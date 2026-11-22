//! A region of a frame, demosaiced and binned without demosaicing the rest.

use oscope_compute::{Image, Shape, cpu};
use oscope_core::{BayerPattern, PixelRect};

use super::{ColorError, Demosaic, demosaic};

/// How far the full-size methods look around a pixel: Malvar–He–Cutler's
/// 5 × 5 filters reach 2 pixels.
const MARGIN: u32 = 2;

/// A rectangle of an image, copied.
pub fn crop(image: &Image, rect: PixelRect) -> Result<Image, ColorError> {
    let whole = PixelRect::new(0, 0, image.width(), image.height());
    if rect.width == 0 || rect.height == 0 || !whole.contains(&rect) {
        return Err(ColorError::Region(rect, whole));
    }
    let c = image.channels();
    let (x0, x1) = (rect.x as usize * c, (rect.x + rect.width) as usize * c);
    let mut data = Vec::with_capacity(rect.width as usize * rect.height as usize * c);
    for y in rect.y..rect.y + rect.height {
        data.extend_from_slice(&image.row(y)[x0..x1]);
    }
    Ok(Image::from_vec(
        Shape::new(rect.width, rect.height, c),
        data,
    )?)
}

/// `rect` of a mosaic's RGB, binned by `bin`: each output pixel the mean of
/// a `bin` × `bin` block, as [`cpu::downsample`] makes it, and 1 for none.
///
/// `rect` is in the RGB image's pixels: the mosaic's for the full-size
/// methods, and half of them for superpixel. Only the rectangle, and the 2
/// pixels around it that the filters need, are demosaiced. The result is
/// the same as demosaicing the whole mosaic, then cropping and binning.
pub fn demosaic_region(
    mosaic: &Image,
    pattern: BayerPattern,
    method: Demosaic,
    rect: PixelRect,
    bin: u32,
) -> Result<Image, ColorError> {
    let (width, height) = (mosaic.width(), mosaic.height());
    let rgb = match method {
        Demosaic::Superpixel => {
            let whole = PixelRect::new(0, 0, width / 2, height / 2);
            if rect.width == 0 || rect.height == 0 || !whole.contains(&rect) {
                return Err(ColorError::Region(rect, whole));
            }
            // Whole cells: an even shift keeps the pattern.
            let cells = PixelRect::new(2 * rect.x, 2 * rect.y, 2 * rect.width, 2 * rect.height);
            demosaic(&crop(mosaic, cells)?, pattern, method)?
        }
        _ => {
            let whole = PixelRect::new(0, 0, width, height);
            if rect.width == 0 || rect.height == 0 || !whole.contains(&rect) {
                return Err(ColorError::Region(rect, whole));
            }
            let (x0, y0) = (rect.x.saturating_sub(MARGIN), rect.y.saturating_sub(MARGIN));
            let x1 = (rect.x + rect.width + MARGIN).min(width);
            let y1 = (rect.y + rect.height + MARGIN).min(height);
            let around = crop(mosaic, PixelRect::new(x0, y0, x1 - x0, y1 - y0))?;
            let rgb = demosaic(&around, pattern.shifted(x0, y0), method)?;
            crop(
                &rgb,
                PixelRect::new(rect.x - x0, rect.y - y0, rect.width, rect.height),
            )?
        }
    };
    Ok(cpu::downsample(&rgb, bin)?)
}
