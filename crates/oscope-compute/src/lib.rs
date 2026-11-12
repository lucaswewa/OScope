//! Compute backends: the CPU, and CUDA when a GPU is present. Device buffers,
//! pinned host memory, streams, and the kernel build.
//!
//! Layer L1 of ADR-0005.
//! Unsafe code is allowed here, for CUDA (plan §4.3).
