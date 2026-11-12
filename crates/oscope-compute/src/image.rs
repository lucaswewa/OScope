//! Images of 32-bit floats, the kernels' working format.

use serde::{Deserialize, Serialize};

use crate::ComputeError;

/// An image's size: pixels across and down, and samples per pixel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Shape {
    /// Pixels across.
    pub width: u32,
    /// Pixels down.
    pub height: u32,
    /// Samples per pixel: 1 for grey or a mosaic, 3 for RGB.
    pub channels: usize,
}

impl Shape {
    /// A shape.
    pub fn new(width: u32, height: u32, channels: usize) -> Self {
        Self {
            width,
            height,
            channels,
        }
    }

    /// Samples in a row.
    pub fn row_len(self) -> usize {
        self.width as usize * self.channels
    }

    /// Samples in the image.
    pub fn len(self) -> usize {
        self.row_len() * self.height as usize
    }

    /// Whether the image has no samples.
    pub fn is_empty(self) -> bool {
        self.len() == 0
    }
}

/// An image of 32-bit floats, its samples interleaved, row by row.
#[derive(Debug, Clone, PartialEq)]
pub struct Image {
    shape: Shape,
    data: Vec<f32>,
}

impl Image {
    /// An image of zeros.
    pub fn zeros(shape: Shape) -> Self {
        Self {
            shape,
            data: vec![0.0; shape.len()],
        }
    }

    /// An image of `data`, which must hold exactly `shape`'s samples.
    pub fn from_vec(shape: Shape, data: Vec<f32>) -> Result<Self, ComputeError> {
        if data.len() != shape.len() {
            return Err(ComputeError::Size {
                expected: shape.len(),
                got: data.len(),
            });
        }
        Ok(Self { shape, data })
    }

    /// Its shape.
    pub fn shape(&self) -> Shape {
        self.shape
    }

    /// Pixels across.
    pub fn width(&self) -> u32 {
        self.shape.width
    }

    /// Pixels down.
    pub fn height(&self) -> u32 {
        self.shape.height
    }

    /// Samples per pixel.
    pub fn channels(&self) -> usize {
        self.shape.channels
    }

    /// The samples.
    pub fn data(&self) -> &[f32] {
        &self.data
    }

    /// The samples, to change.
    pub fn data_mut(&mut self) -> &mut [f32] {
        &mut self.data
    }

    /// Row `y`'s samples.
    pub fn row(&self, y: u32) -> &[f32] {
        let len = self.shape.row_len();
        let start = y as usize * len;
        &self.data[start..start + len]
    }

    /// One sample, or `None` outside the image.
    pub fn get(&self, x: u32, y: u32, channel: usize) -> Option<f32> {
        let inside = x < self.shape.width && y < self.shape.height && channel < self.shape.channels;
        inside.then(|| self.row(y)[x as usize * self.shape.channels + channel])
    }
}
