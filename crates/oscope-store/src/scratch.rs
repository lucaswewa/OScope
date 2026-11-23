//! The scratch space (plan §5.12, ADR-0022): recordings, and temporary
//! files, on a fast disk whose free space is watched.
//!
//! ```text
//! {scratch root}/recordings/<recording>/…   frame logs, until ingested
//!     tmp/                                  emptied when the space opens
//! ```
//!
//! Its clean-up: `tmp/` is emptied when it opens; a recording a crash cut
//! short is recovered then, and kept; ingest deletes the recording it took
//! in; a rolling recording deletes its own oldest segments. Nothing else
//! deletes a recording: they are raw data. A full disk refuses new ones.

use std::fs;
use std::path::{Path, PathBuf};

use oscope_core::RecordingId;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::recording::{
    DOCUMENT, Recording, RecordingOptions, RecordingWriter, SEGMENT_BYTES, in_progress, recover,
};
use crate::store::read;
use crate::{Result, StoreError, io};

/// When the scratch space counts as low, or full.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScratchPolicy {
    /// What a recording leaves free on the disk, for everything else.
    pub reserve_bytes: u64,
    /// Below this, the space is low.
    pub low_bytes: u64,
}

impl Default for ScratchPolicy {
    /// A reserve of 4 GiB; low under 35 GB, about a minute at the design
    /// rate, 580 MB/s.
    fn default() -> Self {
        Self {
            reserve_bytes: 4 << 30,
            low_bytes: 35_000_000_000,
        }
    }
}

/// Whether there is room to record.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ScratchState {
    /// There is.
    Ok,
    /// Under the policy's low mark.
    Low,
    /// Under the reserve and a segment: a recording is refused.
    Full,
}

/// The scratch space's disk, and the recordings on it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ScratchHealth {
    /// Whether there is room to record.
    pub state: ScratchState,
    /// The bytes free on its disk, for OScope.
    pub available_bytes: u64,
    /// Its disk's size, in bytes.
    pub total_bytes: u64,
    /// The recordings waiting there.
    pub recordings: usize,
    /// Their bytes.
    pub recordings_bytes: u64,
}

/// The scratch space.
pub struct Scratch {
    root: PathBuf,
    policy: ScratchPolicy,
}

impl Scratch {
    /// Opens the scratch space at `root`, making its folders. It empties
    /// `tmp/`, and recovers the recordings a crash cut short.
    pub fn open(root: &Path, policy: ScratchPolicy) -> Result<Self> {
        let scratch = Self {
            root: root.to_owned(),
            policy,
        };
        let tmp = root.join("tmp");
        if tmp.exists() {
            fs::remove_dir_all(&tmp).map_err(io(&tmp))?;
        }
        for folder in [&scratch.recordings_folder(), &tmp] {
            fs::create_dir_all(folder).map_err(io(folder))?;
        }
        for folder in scratch.folders()? {
            match read::<Recording>(&folder.join(DOCUMENT)) {
                Ok(recording) if recording.ended.is_none() && !in_progress(&folder) => {
                    match recover(&folder) {
                        Ok(r) => tracing::warn!(
                            "recording {} ({}) was cut short: recovered its {} frames",
                            r.id,
                            r.name,
                            r.frames
                        ),
                        Err(error) => tracing::warn!("a recording can't be recovered: {error}"),
                    }
                }
                Ok(_) => {}
                Err(error) => tracing::warn!("a recording can't be read: {error}"),
            }
        }
        Ok(scratch)
    }

    /// Its folder.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Its policy.
    pub fn policy(&self) -> ScratchPolicy {
        self.policy
    }

    fn recordings_folder(&self) -> PathBuf {
        self.root.join("recordings")
    }

    /// The recordings' folders.
    fn folders(&self) -> Result<Vec<PathBuf>> {
        let recordings = self.recordings_folder();
        let mut folders = Vec::new();
        for entry in fs::read_dir(&recordings).map_err(io(&recordings))? {
            let path = entry.map_err(io(&recordings))?.path();
            if path.is_dir() {
                folders.push(path);
            }
        }
        Ok(folders)
    }

    /// Its disk's space, and the recordings on it.
    pub fn health(&self) -> Result<ScratchHealth> {
        let stats = fs4::statvfs(&self.root).map_err(io(&self.root))?;
        let (mut recordings, mut recordings_bytes) = (0, 0);
        for folder in self.folders()? {
            recordings += 1;
            for entry in fs::read_dir(&folder).map_err(io(&folder))? {
                let entry = entry.map_err(io(&folder))?;
                recordings_bytes += entry.metadata().map_err(io(&folder))?.len();
            }
        }
        let available_bytes = stats.available_space();
        let state = if available_bytes < self.policy.reserve_bytes + SEGMENT_BYTES {
            ScratchState::Full
        } else if available_bytes < self.policy.low_bytes {
            ScratchState::Low
        } else {
            ScratchState::Ok
        };
        Ok(ScratchHealth {
            state,
            available_bytes,
            total_bytes: stats.total_space(),
            recordings,
            recordings_bytes,
        })
    }

    /// The recordings, the newest first.
    pub fn recordings(&self) -> Result<Vec<Recording>> {
        let mut recordings = Vec::new();
        for folder in self.folders()? {
            recordings.push(read::<Recording>(&folder.join(DOCUMENT))?);
        }
        recordings.sort_by_key(|r| std::cmp::Reverse(r.id));
        Ok(recordings)
    }

    /// A recording's folder.
    pub fn folder(&self, id: RecordingId) -> Result<PathBuf> {
        let folder = self.recordings_folder().join(id.to_string());
        match folder.join(DOCUMENT).is_file() {
            true => Ok(folder),
            false => Err(StoreError::NotFound {
                kind: "recording",
                id: id.to_string(),
            }),
        }
    }

    /// Starts a recording. It refuses a segment that would leave less than
    /// the reserve free.
    pub fn record(&self, options: RecordingOptions) -> Result<RecordingWriter> {
        RecordingWriter::create(
            &self.recordings_folder(),
            options,
            self.policy.reserve_bytes,
        )
    }

    /// Deletes a recording, which must have ended.
    pub fn delete(&self, id: RecordingId) -> Result<()> {
        let folder = self.folder(id)?;
        if in_progress(&folder) {
            return Err(StoreError::Refused(format!(
                "recording {id} is still recording"
            )));
        }
        fs::remove_dir_all(&folder).map_err(io(&folder))
    }
}
