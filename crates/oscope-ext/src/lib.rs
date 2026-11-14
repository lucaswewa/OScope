//! Extensions: `.omext` packages, the loader for native ones, and the
//! supervisor of Python workers.
//!
//! Layer L3 of ADR-0005.
//! Unsafe code is allowed here, for loading native libraries (plan §4.3).
//!
//! An `.omext` is a zip file with a [`Manifest`] at its root (ADR-0018).
//! [`Extensions`] installs packages into the data root, and enables,
//! disables and uninstalls them. Every change takes effect at the next
//! start, when [`Extensions::open`] loads the enabled extensions, by ID, and
//! records what each provides in its [`Registry`]. [`pack`] makes a package
//! from a folder.
//!
//! A Rust extension's [`Library`] is loaded then too, and its nodes go to
//! the pipeline (ADR-0019). A Python extension's environment is made when it
//! is installed, and its worker runs its nodes, started on first use
//! (ADR-0020): with a [`PythonRuntime`] only.

use std::path::{Path, PathBuf};

mod installed;
mod manifest;
mod native;
mod package;
mod python;
mod worker;

pub use installed::{Extension, Extensions, Loaded, Registry, WorkerState};
pub use manifest::{API_VERSION, Contribution, Kind, Manifest, PLATFORM, PLATFORMS, Python, Rust};
pub use native::{Library, MAX_SAMPLES};
pub use package::{MANIFEST, MAX_ENTRIES, MAX_UNPACKED, Package, pack, sha256};
pub use python::{OSCOPE_FOLDER, PythonRuntime, find_uv};

/// Why a package, a manifest or an extension was refused, or a file failed.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ExtError {
    /// The manifest is wrong.
    #[error("the manifest: {0}")]
    Manifest(String),
    /// The package needs an extension API that this OScope doesn't offer.
    #[error("it needs the extension API {needs}, and this OScope offers {offers}")]
    Incompatible {
        /// The package's `oscope-api` range.
        needs: String,
        /// [`API_VERSION`].
        offers: String,
    },
    /// A Rust package has no library for this platform.
    #[error("it has no library for {platform}, only for {}", builds.join(" and "))]
    NoBuild {
        /// This platform.
        platform: String,
        /// The platforms it has libraries for.
        builds: Vec<String>,
    },
    /// The package can't be unpacked safely, or isn't a package.
    #[error("the package: {0}")]
    Package(String),
    /// What is installed stands in the way: the same version from another
    /// package, or an extension waiting to be uninstalled.
    #[error("{0}")]
    Conflict(String),
    /// A Rust extension's library couldn't be loaded, or isn't one
    /// (ADR-0019).
    #[error("the library {}: {why}", path.display())]
    Library {
        /// The library.
        path: PathBuf,
        /// Why.
        why: String,
    },
    /// A Python extension's environment or worker failed (ADR-0020).
    #[error("Python: {0}")]
    Python(String),
    /// No extension with this ID is installed.
    #[error("no extension `{0}` is installed")]
    Unknown(String),
    /// A file or folder couldn't be read or written.
    #[error("{}: {source}", path.display())]
    Io {
        /// The file or folder.
        path: PathBuf,
        /// Why.
        source: std::io::Error,
    },
}

/// Turns an I/O error at `path` into an [`ExtError`].
pub(crate) fn io(path: &Path) -> impl FnOnce(std::io::Error) -> ExtError + '_ {
    move |source| ExtError::Io {
        path: path.to_owned(),
        source,
    }
}
