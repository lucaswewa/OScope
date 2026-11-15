//! Workflows: autofocus, exposure, calibration routines, scan planning and the
//! scan engine, and analysis runners.
//!
//! Layer L4 of ADR-0005. So far, calibration (P32, P33), and autofocus,
//! auto exposure and auto white balance (P34).

pub mod autofocus;
pub mod calibration;
