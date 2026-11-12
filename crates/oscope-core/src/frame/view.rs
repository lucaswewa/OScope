//! Windows onto pixels: a whole frame, or a rectangle of it, without copying.

use super::PixelFormat;
use crate::error::{Error, Result};
use crate::geometry::PixelRect;

/// A window onto one plane of pixels: a whole frame, or a rectangle of it. It
/// borrows the pixels, and copies none.
#[derive(Clone, Copy, Debug)]
pub struct FrameView<'a> {
    data: &'a [u8],
    format: PixelFormat,
    width: u32,
    height: u32,
    stride: usize,
}

impl<'a> FrameView<'a> {
    /// A view of tightly packed pixels.
    pub fn new(data: &'a [u8], format: PixelFormat, width: u32, height: u32) -> Result<Self> {
        let stride = width as usize * format.pixel_bytes();
        let needed = stride.saturating_mul(height as usize);
        if data.len() < needed {
            return Err(Error::BufferTooSmall {
                needed,
                got: data.len(),
            });
        }
        Ok(Self {
            data,
            format,
            width,
            height,
            stride,
        })
    }

    /// Its pixel format. A rectangle of a mosaic has the pattern of its own
    /// first cell.
    pub fn format(&self) -> PixelFormat {
        self.format
    }

    /// Its width in pixels.
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Its height in pixels.
    pub fn height(&self) -> u32 {
        self.height
    }

    /// The bytes from the start of one row to the start of the next.
    pub fn stride(&self) -> usize {
        self.stride
    }

    /// Its bytes, from its first pixel to its last, rows `stride` apart: one
    /// span to copy, for example to a GPU. Empty for an empty view.
    pub fn data(&self) -> &'a [u8] {
        let row = self.width as usize * self.format.pixel_bytes();
        match self.height {
            0 => &[],
            h => &self.data[..(h as usize - 1) * self.stride + row],
        }
    }

    /// Row `y`'s pixels, or `None` past the last row.
    pub fn row(&self, y: u32) -> Option<&'a [u8]> {
        let start = y as usize * self.stride;
        (y < self.height)
            .then(|| &self.data[start..start + self.width as usize * self.format.pixel_bytes()])
    }

    /// Sample `channel` of pixel `(x, y)`, widened to 16 bits; `None` outside
    /// the view, or for floating-point samples.
    pub fn sample(&self, x: u32, y: u32, channel: usize) -> Option<u16> {
        if x >= self.width || channel >= self.format.channels() {
            return None;
        }
        let row = self.row(y)?;
        let at = (x as usize * self.format.channels() + channel) * self.format.sample_bytes();
        match self.format.sample_bytes() {
            1 => Some(u16::from(row[at])),
            2 => Some(u16::from_le_bytes([row[at], row[at + 1]])),
            _ => None,
        }
    }

    /// The rectangle `rect` of this view, in this view's coordinates. It must
    /// be non-empty, and wholly inside.
    pub fn roi(&self, rect: PixelRect) -> Result<FrameView<'a>> {
        let whole = PixelRect::new(0, 0, self.width, self.height);
        if rect.width == 0 || rect.height == 0 || !whole.contains(&rect) {
            return Err(Error::OutsideFrame {
                rect,
                width: self.width,
                height: self.height,
            });
        }
        let start = rect.y as usize * self.stride + rect.x as usize * self.format.pixel_bytes();
        Ok(Self {
            data: &self.data[start..],
            format: self.format.shifted(rect.x, rect.y),
            width: rect.width,
            height: rect.height,
            stride: self.stride,
        })
    }
}
