//! The `tracing` layer that turns events into records in a [`LogStore`].

use std::fmt;

use serde_json::{Map, Value};
use tracing::field::{Field, Visit};
use tracing::span::{Attributes, Id};
use tracing::{Event, Subscriber};
use tracing_subscriber::layer::{Context, Layer};
use tracing_subscriber::registry::LookupSpan;

use super::{Level, LogStore};

/// The name of the span a job runs in, with its id in a `job_id` field:
/// `tracing::info_span!("job", job_id = %id)`. The records made in it
/// carry the job's id.
pub const JOB_SPAN: &str = "job";

/// Records every event in a [`LogStore`]: [`LogStore::layer`].
#[derive(Debug, Clone)]
pub struct LogLayer {
    store: LogStore,
}

impl LogStore {
    /// The `tracing` layer that records into this store.
    pub fn layer(&self) -> LogLayer {
        LogLayer {
            store: self.clone(),
        }
    }
}

/// A job span's id, kept with the span.
struct JobId(String);

impl<S> Layer<S> for LogLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_new_span(&self, attributes: &Attributes<'_>, id: &Id, context: Context<'_, S>) {
        if attributes.metadata().name() != JOB_SPAN {
            return;
        }
        let mut fields = Fields::default();
        attributes.record(&mut fields);
        if let (Some(job), Some(span)) = (fields.0.remove("job_id"), context.span(id)) {
            span.extensions_mut().insert(JobId(text(job)));
        }
    }

    fn on_event(&self, event: &Event<'_>, context: Context<'_, S>) {
        let mut fields = Fields::default();
        event.record(&mut fields);
        let message = fields.0.remove("message").map(text).unwrap_or_default();
        let metadata = event.metadata();
        let critical = *metadata.level() == tracing::Level::ERROR
            && fields.0.get("critical") == Some(&Value::Bool(true));
        let level = if critical {
            fields.0.remove("critical");
            Level::Critical
        } else {
            Level::from(*metadata.level())
        };
        // The innermost job span's.
        let job = context.event_scope(event).and_then(|mut spans| {
            spans.find_map(|span| span.extensions().get::<JobId>().map(|job| job.0.clone()))
        });
        self.store
            .push(level, metadata.target(), message, fields.0, job);
    }
}

/// An event's or span's fields, as JSON values.
#[derive(Default)]
struct Fields(Map<String, Value>);

impl Fields {
    fn insert(&mut self, field: &Field, value: impl Into<Value>) {
        self.0.insert(field.name().to_owned(), value.into());
    }
}

impl Visit for Fields {
    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        self.insert(field, format!("{value:?}"));
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        self.insert(field, value);
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        self.insert(field, value);
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        self.insert(field, value);
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        self.insert(field, value);
    }

    fn record_f64(&mut self, field: &Field, value: f64) {
        self.insert(field, value);
    }
}

/// A value as text: a string as it is, anything else as JSON.
fn text(value: Value) -> String {
    match value {
        Value::String(text) => text,
        other => other.to_string(),
    }
}
