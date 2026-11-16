//! Workflows: autofocus, exposure, calibration routines, scan planning and the
//! scan engine, and analysis runners.
//!
//! Layer L4 of ADR-0005. So far, calibration (P32, P33); autofocus, auto
//! exposure and auto white balance (P34); and scan planning (P35).

pub mod autofocus;
pub mod calibration;
pub mod scan;
pub mod stitch;
