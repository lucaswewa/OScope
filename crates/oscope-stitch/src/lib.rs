//! Stitching: placing tiles, the registration graph and its global solve,
//! blending, and multiscale pyramids.
//!
//! Layer L1 of ADR-0005. So far, the live mosaic's [`canvas`] (P38).

pub mod canvas;

pub use canvas::{Affine, Blend, Canvas, CanvasError, CanvasGeometry, Changes, Rgb, TILE, TileKey};
