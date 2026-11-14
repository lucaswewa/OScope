//! Data on disk (D15, D16, plan §5.12, ADR-0021): projects, with their
//! acquisitions, results and reports, in folders under the data root; their
//! images as OME-Zarr v3 stores; and the SQLite index of them, which can be
//! rebuilt from the folders.
//!
//! Layer L2 of ADR-0005. The scratch frame log comes with P29, and the tile
//! cache with P40.
//!
//! ```text
//! {data root}/projects/<project>/project.json
//!     acquisitions/<id>/acquisition.json, raw.ome.zarr/, thumbnail.jpg
//!     results/<id>/result.json, image.ome.zarr/ (if an image), thumbnail.jpg
//!     reports/<id>/report.json
//! {data root}/index.sqlite
//! ```

mod image;
mod index;
mod records;
mod store;
mod thumbnail;

pub use image::{CHUNK, ImageInfo, ImageReader, ImageWriter, PixelSize, SampleType, Samples};
pub use records::{
    Acquisition, FrameRecord, Input, NodeVersion, Project, Provenance, Record, RecordKind, Report,
    ResultRecord,
};
pub use store::{Change, Contents, Draft, NewProject, Page, Query, Store, Summary};

/// What the store refuses, or fails at.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum StoreError {
    /// No project, or no record, has this ID.
    #[error("no {kind} {id}")]
    NotFound {
        /// What was looked for: `project`, `acquisition`, `result` or
        /// `report`.
        kind: &'static str,
        /// Its ID.
        id: String,
    },
    /// A request the store refuses, saying why.
    #[error("{0}")]
    Refused(String),
    /// A file or folder failed.
    #[error("{path}: {source}")]
    Io {
        /// The file or folder.
        path: std::path::PathBuf,
        /// What failed.
        source: std::io::Error,
    },
    /// The index failed.
    #[error("the index: {0}")]
    Index(String),
    /// A document didn't read or write.
    #[error("{path}: {reason}")]
    Document {
        /// The file.
        path: std::path::PathBuf,
        /// What is wrong with it.
        reason: String,
    },
    /// An OME-Zarr store failed.
    #[error("OME-Zarr: {0}")]
    Zarr(String),
}

/// The store's results.
pub type Result<T, E = StoreError> = std::result::Result<T, E>;

/// An I/O error at `path`.
fn io(path: &std::path::Path) -> impl FnOnce(std::io::Error) -> StoreError + '_ {
    move |source| StoreError::Io {
        path: path.to_owned(),
        source,
    }
}
