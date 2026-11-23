//! The records the store keeps, each a versioned JSON document in its
//! folder (D16, plan §5.12).

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use oscope_core::version::Versioned;
use oscope_core::{AcquisitionId, ProjectId, ReportId, ResultId, Timestamp, Xyz};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::ImageInfo;

/// A project: one slide or one experiment, holding acquisitions, results
/// and reports. The Gallery shows each as a card.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Project {
    /// Its ID, which is also its folder's name.
    pub id: ProjectId,
    /// Its name, which may change.
    pub name: String,
    /// The sample: what is on the slide.
    #[serde(default)]
    pub sample: String,
    /// Its tags, sorted, each once.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Who made it.
    #[serde(default)]
    pub owner: String,
    /// When it was made.
    pub created: DateTime<Utc>,
    /// When it, or anything in it, last changed.
    pub modified: DateTime<Utc>,
    /// Anything else, as the app or a workflow keeps it.
    #[serde(default)]
    pub metadata: Map<String, Value>,
}

impl Versioned for Project {
    const SCHEMA: &'static str = "oscope.project";
    const VERSION: u32 = 1;
}

/// A frame of an acquisition: where the stage was and when it was exposed.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct FrameRecord {
    /// Its number in the acquisition, from 0.
    pub index: u64,
    /// The stage's position, in µm.
    #[serde(default)]
    pub position: Option<Xyz>,
    /// When it was exposed, on the host's clock.
    #[serde(default)]
    pub timestamp: Option<Timestamp>,
    /// Where the stage's trigger output fired it: reserved for M3's
    /// continuous scans (ADR-0025), none until then.
    #[serde(default)]
    pub trigger: Option<Xyz>,
}

/// An acquisition: the raw data of one run, in `raw.ome.zarr`, and how it
/// was taken.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Acquisition {
    /// Its ID, which is also its folder's name.
    pub id: AcquisitionId,
    /// Its project.
    pub project: ProjectId,
    /// Its name.
    pub name: String,
    /// When it was made.
    pub created: DateTime<Utc>,
    /// Its image.
    pub image: ImageInfo,
    /// The instrument, as it was.
    #[serde(default)]
    pub instrument: Value,
    /// The objective it was taken with.
    #[serde(default)]
    pub objective: Value,
    /// The calibration it used (P32).
    #[serde(default)]
    pub calibration: Value,
    /// The camera's settings.
    #[serde(default)]
    pub camera: Value,
    /// The plan it followed (P36).
    #[serde(default)]
    pub plan: Value,
    /// Its frames.
    #[serde(default)]
    pub frames: Vec<FrameRecord>,
}

impl Versioned for Acquisition {
    const SCHEMA: &'static str = "oscope.acquisition";
    const VERSION: u32 = 1;
}

/// A node that made a result, and its version: OScope's, or an extension's.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct NodeVersion {
    /// The node's ID.
    pub node: String,
    /// Its version.
    pub version: String,
    /// The extension that provides it, `id@version`, if one does.
    #[serde(default)]
    pub extension: Option<String>,
}

/// What a result was made from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "kind", content = "id")]
pub enum Input {
    /// An acquisition.
    Acquisition(AcquisitionId),
    /// Another result.
    Result(ResultId),
}

/// How a result was made: enough to make it again.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Provenance {
    /// The recipe, as it ran (ADR-0017).
    #[serde(default)]
    pub recipe: Value,
    /// The nodes it ran, and their versions.
    #[serde(default)]
    pub nodes: Vec<NodeVersion>,
    /// What it was made from.
    #[serde(default)]
    pub inputs: Vec<Input>,
    /// What it read of its inputs, hashed, by their IDs: `sha256:` and the
    /// hash in hex (P48).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub hashes: BTreeMap<String, String>,
    /// The software that made it, such as `OScope 0.1.0`.
    pub software: String,
}

/// A result: derived data, in its folder, with its provenance. An image
/// result is in `image.ome.zarr`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ResultRecord {
    /// Its ID, which is also its folder's name.
    pub id: ResultId,
    /// Its project.
    pub project: ProjectId,
    /// Its name.
    pub name: String,
    /// When it was made.
    pub created: DateTime<Utc>,
    /// How it was made.
    pub provenance: Provenance,
    /// Its image, if it is one.
    #[serde(default)]
    pub image: Option<ImageInfo>,
}

impl Versioned for ResultRecord {
    const SCHEMA: &'static str = "oscope.result";
    const VERSION: u32 = 1;
}

/// A report: documents made from a project's data, in its folder (P47).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Report {
    /// Its ID, which is also its folder's name.
    pub id: ReportId,
    /// Its project.
    pub project: ProjectId,
    /// Its name.
    pub name: String,
    /// When it was made.
    pub created: DateTime<Utc>,
    /// Its files, in its folder, such as `report.pdf`.
    #[serde(default)]
    pub files: Vec<String>,
}

impl Versioned for Report {
    const SCHEMA: &'static str = "oscope.report";
    const VERSION: u32 = 1;
}

/// The kinds of record in a project.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum RecordKind {
    /// Acquisitions.
    Acquisition,
    /// Results.
    Result,
    /// Reports.
    Report,
}

impl RecordKind {
    /// Every kind.
    pub const ALL: [Self; 3] = [Self::Acquisition, Self::Result, Self::Report];

    /// Its name, as the index and errors say it.
    pub fn name(self) -> &'static str {
        match self {
            Self::Acquisition => "acquisition",
            Self::Result => "result",
            Self::Report => "report",
        }
    }

    /// The folder in a project that holds records of this kind.
    pub fn folder(self) -> &'static str {
        match self {
            Self::Acquisition => "acquisitions",
            Self::Result => "results",
            Self::Report => "reports",
        }
    }
}

/// A record of a project: an acquisition, a result or a report.
pub trait Record: Versioned + Clone {
    /// Its kind.
    const KIND: RecordKind;
    /// The name of its image's store in its folder.
    const IMAGE: &'static str;

    /// Its ID, as text.
    fn id(&self) -> String;
    /// Its project.
    fn project(&self) -> ProjectId;
    /// Its name.
    fn name(&self) -> &str;
    /// When it was made.
    fn created(&self) -> DateTime<Utc>;
    /// Records its image, once written.
    fn set_image(&mut self, image: ImageInfo);
}

macro_rules! record {
    ($type:ty, $kind:ident, $image:literal, |$record:ident, $info:ident| $set:expr) => {
        impl Record for $type {
            const KIND: RecordKind = RecordKind::$kind;
            const IMAGE: &'static str = $image;

            fn id(&self) -> String {
                self.id.to_string()
            }
            fn project(&self) -> ProjectId {
                self.project
            }
            fn name(&self) -> &str {
                &self.name
            }
            fn created(&self) -> DateTime<Utc> {
                self.created
            }
            fn set_image(&mut self, $info: ImageInfo) {
                let $record = self;
                $set
            }
        }
    };
}

record!(Acquisition, Acquisition, "raw.ome.zarr", |record, info| {
    record.image = info
});
record!(ResultRecord, Result, "image.ome.zarr", |record, info| {
    record.image = Some(info)
});
record!(Report, Report, "image.ome.zarr", |_record, _info| ());
