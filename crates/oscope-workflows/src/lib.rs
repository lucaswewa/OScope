//! Workflows: autofocus, exposure, calibration routines, scan planning and the
//! scan engine, and analysis runners.
//!
//! Layer L4 of ADR-0005. So far, calibration (P32, P33); autofocus, auto
//! exposure and auto white balance (P34); scan planning (P35); stitching
//! (P39); and depth from focus over a z-stack (P42).

pub mod autofocus;
pub mod calibration;
pub mod deconvolve;
pub mod depth;
pub mod scan;
pub mod stain;
pub mod stitch;
