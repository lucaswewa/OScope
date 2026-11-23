//! The `logs` Thing.

use teta_wot::http::PathParams;
use teta_wot::http::axum::Json;
use teta_wot::http::axum::extract::Query as UrlQuery;
use teta_wot::http::axum::http::StatusCode;
use teta_wot::http::axum::http::header::{CONTENT_DISPOSITION, CONTENT_TYPE};
use teta_wot::http::axum::response::{IntoResponse, Response};
use teta_wot::prelude::*;

use super::{Level, LogFile, LogStore, Page, Query};

/// The target of the records written with `emit`.
pub const EMIT_TARGET: &str = "oscope::note";

/// OScope's log: the newest records, with filters, a page at a time, at
/// `records`; the daily files, to download at `files/{name}`; and a way to
/// write a record. The live tail is the WebSocket at
/// `/api/v1/media/logs`.
#[derive(Thing)]
pub struct Logs {
    store: LogStore,
}

impl Logs {
    /// The Thing, over `store`.
    pub fn new(store: LogStore) -> Self {
        Self { store }
    }
}

#[thing_impl]
impl Logs {
    /// The daily files, newest first: `oscope-YYYY-MM-DD.jsonl` for each UTC
    /// day, with one record per line as JSON.
    #[property(blocking)]
    fn files(&self) -> Result<Vec<LogFile>, PropertyError> {
        self.store.files().map_err(PropertyError::failed)
    }

    /// Writes a record: a note in the log, such as "the slide is loaded", or
    /// a test of the log.
    #[action]
    async fn emit(&self, level: Level, message: String) {
        match level {
            Level::Trace => tracing::trace!(target: EMIT_TARGET, "{message}"),
            Level::Debug => tracing::debug!(target: EMIT_TARGET, "{message}"),
            Level::Info => tracing::info!(target: EMIT_TARGET, "{message}"),
            Level::Warn => tracing::warn!(target: EMIT_TARGET, "{message}"),
            Level::Error => tracing::error!(target: EMIT_TARGET, "{message}"),
            Level::Critical => tracing::error!(target: EMIT_TARGET, critical = true, "{message}"),
        }
    }

    /// `GET records?level=warn&text=stage&limit=50`: a page of the newest
    /// records that match, newest first. The query's fields are
    /// [`Query`]'s. `400` for a wrong one.
    #[endpoint(get, "records")]
    async fn records(&self, UrlQuery(query): UrlQuery<Query>) -> Json<Page> {
        Json(self.store.query(&query))
    }

    /// `GET files/{name}`: a daily file, to download. `404` for any name
    /// that `files` doesn't list.
    #[endpoint(get, "files/{name}")]
    async fn download(&self, params: PathParams) -> Response {
        let Some(name) = params.get("name") else {
            return StatusCode::NOT_FOUND.into_response();
        };
        let Some(path) = self.store.file(name) else {
            return StatusCode::NOT_FOUND.into_response();
        };
        match tokio::fs::read(&path).await {
            Ok(bytes) => {
                let headers = [
                    (CONTENT_TYPE, "application/x-ndjson".to_owned()),
                    (
                        CONTENT_DISPOSITION,
                        format!("attachment; filename=\"{name}\""),
                    ),
                ];
                (headers, bytes).into_response()
            }
            Err(_) => StatusCode::NOT_FOUND.into_response(),
        }
    }
}
