//! Volumes: stacks, depth from focus, extended depth of field, deconvolution,
//! and bricks for the client.
//!
//! Layer L1 of ADR-0005.
//!
//! - **[`depth`]:** depth from focus, each pixel's height from where its
//!   focus measure peaks through a stack, with a confidence; and the
//!   all-in-focus image (P42).
//! - **[`mesh`]:** a height map as a mesh, in PLY, STL or OBJ (P42).
//! - **[`psf`]:** Born and Wolf's widefield point spread function (P43).
//! - **[`deconvolve`]:** Richardson–Lucy deconvolution, with total
//!   variation if asked, on the CPU or the GPU, and the NRMSE (P43).

pub mod deconvolve;
pub mod depth;
pub mod mesh;
pub mod psf;

pub use deconvolve::{DeconvolveOptions, Padded, Volume, deconvolve, nrmse, richardson_lucy};
pub use depth::{AllInFocus, Depth, FocusOptions, Peaks, Planes, all_in_focus, depth_from_focus};
pub use mesh::{Mesh, MeshFormat, MeshOptions, write_mesh};
pub use psf::{Optics, psf};

use std::path::PathBuf;

use oscope_compute::ComputeError;

/// What a volume's work can't do.
#[derive(Debug, thiserror::Error)]
pub enum VolumeError {
    /// What was asked for can't be done.
    #[error("{0}")]
    Refused(String),
    /// The compute layer failed.
    #[error(transparent)]
    Compute(#[from] ComputeError),
    /// A file couldn't be written.
    #[error("{path}: {source}")]
    Io {
        /// The file.
        path: PathBuf,
        /// Why.
        source: std::io::Error,
    },
}

fn refused(why: impl Into<String>) -> VolumeError {
    VolumeError::Refused(why.into())
}
