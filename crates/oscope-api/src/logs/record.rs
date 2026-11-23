//! Log records, their levels, and queries.

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// How serious a record is, from `trace` to `critical`. A critical record
/// is an error written with `critical = true`:
/// `tracing::error!(critical = true, "…")`.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    /// The finest detail.
    Trace,
    /// Detail for diagnosis.
    Debug,
    /// What happened.
    Info,
    /// Something unexpected, which OScope worked around.
    Warn,
    /// Something failed.
    Error,
    /// Something failed that needs attention now.
    Critical,
}

impl From<tracing::Level> for Level {
    fn from(level: tracing::Level) -> Self {
        match level {
            tracing::Level::TRACE => Self::Trace,
            tracing::Level::DEBUG => Self::Debug,
            tracing::Level::INFO => Self::Info,
            tracing::Level::WARN => Self::Warn,
            tracing::Level::ERROR => Self::Error,
        }
    }
}

/// One log record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Record {
    /// Its number: records are numbered from 1, in the order they are made.
    pub seq: u64,
    /// When it was made.
    pub time: DateTime<Utc>,
    /// How serious it is.
    pub level: Level,
    /// Where it comes from: a module path, such as `oscope_api::server`.
    pub target: String,
    /// What it says.
    pub message: String,
    /// Its other fields.
    #[serde(default, skip_serializing_if = "Map::is_empty")]
    pub fields: Map<String, Value>,
    /// The job it was made in, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub job: Option<String>,
}

/// Which records to return, and how many. Every condition is optional.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct Query {
    /// The lowest level: `warn` gives warnings, errors and critical records.
    pub level: Option<Level>,
    /// Made at or after this time.
    pub since: Option<DateTime<Utc>>,
    /// Made before this time.
    pub until: Option<DateTime<Utc>>,
    /// The target starts with this, such as `oscope_api`.
    pub target: Option<String>,
    /// The message or a field's value contains this, ignoring case.
    pub text: Option<String>,
    /// Made in this job.
    pub job: Option<String>,
    /// Only records numbered below this: a page's `next`, for the page
    /// after it.
    pub before: Option<u64>,
    /// At most this many: 100 by default, 1,000 at most.
    pub limit: Option<usize>,
}

impl Query {
    /// Whether `record` meets the conditions; `before` and `limit` aren't
    /// conditions.
    pub fn matches(&self, record: &Record) -> bool {
        let contains = |text: &str| {
            let wanted = text.to_lowercase();
            let found = |value: &str| value.to_lowercase().contains(&wanted);
            found(&record.message)
                || record.fields.values().any(|value| match value {
                    Value::String(value) => found(value),
                    other => found(&other.to_string()),
                })
        };
        self.level.is_none_or(|level| record.level >= level)
            && self.since.is_none_or(|since| record.time >= since)
            && self.until.is_none_or(|until| record.time < until)
            && self
                .target
                .as_deref()
                .is_none_or(|target| record.target.starts_with(target))
            && self.text.as_deref().is_none_or(contains)
            && self
                .job
                .as_deref()
                .is_none_or(|job| record.job.as_deref() == Some(job))
    }

    /// How many records a page holds: from 1 to 1,000.
    pub fn page_size(&self) -> usize {
        self.limit.unwrap_or(100).clamp(1, 1000)
    }
}

/// A page of records, newest first.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Page {
    /// The records.
    pub records: Vec<Record>,
    /// For the next, older page, the query's `before`; absent on the last
    /// page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next: Option<u64>,
}
