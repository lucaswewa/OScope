//! Identifiers.
//!
//! A [`FrameId`] numbers frames. The others are version 7 UUIDs: unique, and
//! ordered by creation, to the millisecond across processes and exactly within
//! one. They serialise as their hyphenated text.

use std::fmt;
use std::str::FromStr;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::Error;

macro_rules! uuid_id {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
        #[derive(Serialize, Deserialize, JsonSchema)]
        #[serde(transparent)]
        pub struct $name(pub Uuid);

        impl $name {
            /// A new ID, ordered after every ID this process made before it.
            pub fn generate() -> Self {
                Self(Uuid::now_v7())
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                fmt::Display::fmt(&self.0, f)
            }
        }

        impl FromStr for $name {
            type Err = Error;
            fn from_str(text: &str) -> Result<Self, Error> {
                Uuid::try_parse(text).map(Self).map_err(|_| Error::InvalidId {
                    kind: stringify!($name),
                    text: text.to_owned(),
                })
            }
        }
    };
}

uuid_id!(
    /// A job: a long piece of work, run by the job service (P20).
    JobId
);
uuid_id!(
    /// A project: a folder of data on disk (P28).
    ProjectId
);
uuid_id!(
    /// An acquisition: one run that records frames into a project (P29).
    AcquisitionId
);
uuid_id!(
    /// A result: an image, a table or a report that a job made.
    ResultId
);

/// A frame, numbered in the order frames arrive, from 0 (P03).
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
)]
#[serde(transparent)]
pub struct FrameId(pub u64);

impl FrameId {
    /// The frame after this one.
    pub fn next(self) -> FrameId {
        FrameId(self.0 + 1)
    }
}

impl fmt::Display for FrameId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "frame {}", self.0)
    }
}
