//! Image algorithms, each with a CPU and a CUDA path: Bayer and colour,
//! filters, FFT and registration, focus measures, morphology, segmentation and
//! stain separation.
//!
//! Layer L1 of ADR-0005. It may not use unsafe code, so the CUDA kernels of
//! its algorithms live in `oscope-compute`, which may.

pub mod color;
pub mod registration;
