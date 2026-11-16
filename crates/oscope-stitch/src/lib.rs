//! Stitching: placing tiles, the registration graph and its global solve,
//! blending, and multiscale pyramids.
//!
//! Layer L1 of ADR-0005. The live mosaic's [`canvas`] (P38); a scan's
//! [`tiles`], their [`solve`] and their [`blend`] (P39).

pub mod blend;
pub mod canvas;
pub mod solve;
pub mod tiles;

pub use blend::{Flat, Part, blend, flat_field};
pub use canvas::{Affine, Blend, Canvas, CanvasError, CanvasGeometry, Changes, Rgb, TILE, TileKey};
pub use solve::{Link, Solution, SolveOptions, solve};
pub use tiles::{Crops, Pair, PixelBox, Placed, Tiles};
