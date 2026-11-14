//! Data on disk (D15, D16, plan §5.12, ADR-0021, ADR-0022): projects, with
//! their acquisitions, results and reports, in folders under the data root;
//! their images as OME-Zarr v3 stores; the SQLite index of them, which can be
//! rebuilt from the folders; and the scratch space, whose recordings hold
//! frames as the camera gave them until they are ingested.
//!
//! Layer L2 of ADR-0005. The tile cache comes with P40.
//!
//! ```text
//! {data root}/projects/<project>/project.json
//!     acquisitions/<id>/acquisition.json, raw.ome.zarr/, thumbnail.jpg
//!     results/<id>/result.json, image.ome.zarr/ (if an image), thumbnail.jpg
//!     reports/<id>/report.json
//! {data root}/index.sqlite
//! {scratch root}/recordings/<recording>/recording.json, 000000.frames, …
//! ```

mod formats;
mod image;
mod index;
mod ingest;
mod recording;
mod records;
mod scratch;
mod store;
mod thumbnail;

pub use formats::{
    Exporter, Format, Importer, Rendition, export, exporters, import, importers, render,
};
pub use image::{CHUNK, ImageInfo, ImageReader, ImageWriter, PixelSize, SampleType, Samples};
pub use ingest::ingest;
pub use recording::{
    FrameEntry, Recording, RecordingOptions, RecordingReader, RecordingWriter, SEGMENT_BYTES,
};
pub use records::{
    Acquisition, FrameRecord, Input, NodeVersion, Project, Provenance, Record, RecordKind, Report,
    ResultRecord,
};
pub use scratch::{Scratch, ScratchHealth, ScratchPolicy, ScratchState};
pub use store::{Change, Contents, Draft, NewProject, Page, Query, Store, Summary};

/// What the store refuses, or fails at.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum StoreError {
    /// No project, or no record, has this ID.
    #[error("no {kind} {id}")]
    NotFound {
        /// What was looked for: `project`, `acquisition`, `result`,
        /// `report` or `recording`.
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
    /// The work was stopped before it finished.
    #[error("cancelled")]
    Cancelled,
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
