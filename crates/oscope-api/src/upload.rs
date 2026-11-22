//! The upload route (W6.3, ADR-0051): a client's file, staged in the
//! scratch space for a job to use, at `POST /api/v1/upload/{name}`.
//!
//! ```text
//! {scratch root}/uploads/<upload>/<name>   staged, until used or a day old
//!     tmp/                                 an upload as it comes
//! ```
//!
//! - **Its name** is the file's, one plain part of a path: no separator,
//!   not hidden, and none Windows keeps for a device.
//! - **Its size** is at most the limit, and leaves the scratch space's
//!   reserve free (ADR-0022).
//! - **It needs the lease,** as every request that changes state does
//!   (ADR-0031).
//! - **Cut short, it leaves nothing:** it is written in `tmp/`, and moved
//!   into place only once whole.
//! - **It is removed** once used, by the job that imports it; or a day after
//!   it came, at the next upload, or the hourly sweep.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use chrono::{DateTime, Utc};
use futures_util::StreamExt;
use oscope_store::Scratch;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use teta_wot::http::axum::body::Body;
use teta_wot::http::axum::extract::{Path as UrlPath, State};
use teta_wot::http::axum::http::{HeaderMap, StatusCode, header};
use teta_wot::http::axum::response::{IntoResponse, Response};
use teta_wot::http::axum::routing::post;
use teta_wot::http::axum::{Json, Router};
use tokio::io::AsyncWriteExt;
use uuid::Uuid;

use crate::API_PREFIX;
use crate::data::problem;

/// The largest file the route takes by default: 16 GiB, a stack of some
/// hundred of the camera's full frames.
pub const UPLOAD_LIMIT: u64 = 16 << 30;

/// How long a staged file waits to be used: a day.
pub const UPLOAD_LIFETIME: Duration = Duration::from_secs(24 * 60 * 60);

/// A file the route staged.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Staged {
    /// Its ID, which a job that uses it names.
    pub id: Uuid,
    /// Its name, as it was sent.
    pub name: String,
    /// Its size, in bytes.
    pub bytes: u64,
    /// When it is removed, if nothing has used it.
    pub expires: DateTime<Utc>,
}

/// The files staged in the scratch space.
pub struct Uploads {
    scratch: Arc<Scratch>,
    limit: u64,
    lifetime: Duration,
}

impl Uploads {
    /// The uploads in `scratch`, each at most `limit` bytes, waiting
    /// `lifetime` to be used. Those older are removed now.
    pub fn new(scratch: Arc<Scratch>, limit: u64, lifetime: Duration) -> Self {
        let uploads = Self {
            scratch,
            limit,
            lifetime,
        };
        uploads.sweep(SystemTime::now());
        uploads
    }

    fn folder(&self) -> PathBuf {
        self.scratch.root().join("uploads")
    }

    /// The staged file `id`.
    pub fn staged(&self, id: Uuid) -> Result<PathBuf, String> {
        let folder = self.folder().join(id.to_string());
        let mut files = std::fs::read_dir(&folder).into_iter().flatten().flatten();
        let file = files.find(|entry| entry.path().is_file());
        file.map(|entry| entry.path())
            .ok_or_else(|| format!("no upload is {id}"))
    }

    /// Removes the staged file `id`, if it is there.
    pub fn remove(&self, id: Uuid) {
        let folder = self.folder().join(id.to_string());
        if let Err(error) = std::fs::remove_dir_all(&folder)
            && error.kind() != std::io::ErrorKind::NotFound
        {
            tracing::warn!("upload {id} wasn't removed: {error}");
        }
    }

    /// Removes the staged files older, at `now`, than their lifetime.
    pub fn sweep(&self, now: SystemTime) {
        let Ok(entries) = std::fs::read_dir(self.folder()) else {
            return;
        };
        let modified = |path: &std::path::Path| path.metadata().and_then(|m| m.modified()).ok();
        for entry in entries.flatten() {
            let path = entry.path();
            // When its file came; or, before it has, when its folder was made.
            let files = std::fs::read_dir(&path).into_iter().flatten().flatten();
            let came = files.filter_map(|file| modified(&file.path())).max();
            let came = came.or_else(|| modified(&path));
            if came.is_some_and(|came| came + self.lifetime > now) {
                continue;
            }
            let removed = match path.is_dir() {
                true => std::fs::remove_dir_all(&path),
                false => std::fs::remove_file(&path),
            };
            if let Err(error) = removed {
                tracing::warn!("{} wasn't removed: {error}", path.display());
            }
        }
    }

    /// Stages the request's `body` as `name`: the file staged, or why not.
    async fn receive(
        &self,
        name: &str,
        length: Option<u64>,
        body: Body,
    ) -> Result<Staged, Refused> {
        if !plain(name) {
            let why = format!("{name:?} isn't a file's name: one part, plain, not hidden");
            return Err(refused(StatusCode::BAD_REQUEST, why));
        }
        let too_large = || {
            let why = format!("an upload is at most {} bytes", self.limit);
            refused(StatusCode::PAYLOAD_TOO_LARGE, why)
        };
        let room = fs4::statvfs(self.scratch.root())
            .map_or(u64::MAX, |disk| disk.available_space())
            .saturating_sub(self.scratch.policy().reserve_bytes);
        match length {
            Some(length) if length > self.limit => return Err(too_large()),
            Some(length) if length > room => {
                let why = format!("{length} bytes would leave the scratch space under its reserve");
                return Err(refused(StatusCode::INSUFFICIENT_STORAGE, why));
            }
            _ => {}
        }
        self.sweep(SystemTime::now());
        let internal = |error: std::io::Error| refused(StatusCode::INTERNAL_SERVER_ERROR, error);
        // Dropped, as it is when the connection is lost, the file goes.
        let tmp = self.scratch.root().join("tmp");
        let file = tempfile::NamedTempFile::new_in(&tmp).map_err(internal)?;
        let mut out = tokio::fs::File::from_std(file.reopen().map_err(internal)?);
        let (mut stream, mut bytes) = (body.into_data_stream(), 0_u64);
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| refused(StatusCode::BAD_REQUEST, e))?;
            bytes += chunk.len() as u64;
            if bytes > self.limit.min(room) {
                return Err(match bytes > self.limit {
                    true => too_large(),
                    false => refused(
                        StatusCode::INSUFFICIENT_STORAGE,
                        "the scratch space is full",
                    ),
                });
            }
            out.write_all(&chunk).await.map_err(internal)?;
        }
        out.flush().await.map_err(internal)?;
        drop(out);
        let id = Uuid::now_v7();
        let folder = self.folder().join(id.to_string());
        std::fs::create_dir_all(&folder).map_err(internal)?;
        file.persist(folder.join(name))
            .map_err(|e| internal(e.error))?;
        let expires = SystemTime::now() + self.lifetime;
        Ok(Staged {
            id,
            name: name.to_owned(),
            bytes,
            expires: expires.into(),
        })
    }

    /// Sweeps every hour, while the runtime it is started on runs.
    fn sweeping(self: &Arc<Self>) {
        let Ok(runtime) = tokio::runtime::Handle::try_current() else {
            return;
        };
        let uploads = Arc::downgrade(self);
        runtime.spawn(async move {
            // The first an hour after the start's own.
            let hour = Duration::from_secs(60 * 60);
            let mut hourly = tokio::time::interval_at(tokio::time::Instant::now() + hour, hour);
            loop {
                hourly.tick().await;
                let Some(uploads) = uploads.upgrade() else {
                    return;
                };
                let swept = tokio::task::spawn_blocking(move || uploads.sweep(SystemTime::now()));
                let _ = swept.await;
            }
        });
    }
}

/// Why an upload was refused: its status, and what happened.
type Refused = (StatusCode, String);

fn refused(status: StatusCode, why: impl std::fmt::Display) -> Refused {
    (status, why.to_string())
}

/// Whether `name` is a file's own: one part of a path, plain, not hidden,
/// and not a name Windows keeps for a device, on either platform.
fn plain(name: &str) -> bool {
    let stem = name.split('.').next().unwrap_or("").to_ascii_uppercase();
    let device = ["CON", "PRN", "AUX", "NUL"].contains(&stem.as_str())
        || (stem.len() == 4
            && (stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.ends_with(|c: char| c.is_ascii_digit()));
    !name.is_empty()
        && name.len() <= 255
        && !name.starts_with('.')
        && !name.ends_with(['.', ' '])
        && !name.contains(['/', '\\', ':', '*', '?', '"', '<', '>', '|'])
        && !name.chars().any(char::is_control)
        && !device
}

/// The upload route, staging in `uploads`.
pub(crate) fn router(uploads: Arc<Uploads>) -> Router {
    uploads.sweeping();
    Router::new()
        .route(&format!("{API_PREFIX}/upload/{{name}}"), post(upload))
        .with_state(uploads)
}

/// Stages the request's body: 201 and the file staged; 400 for a name not
/// a file's, 413 for a file over the limit, 507 for one the scratch space
/// has no room for.
async fn upload(
    State(uploads): State<Arc<Uploads>>,
    UrlPath(name): UrlPath<String>,
    headers: HeaderMap,
    body: Body,
) -> Response {
    let length = headers
        .get(header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok()?.parse().ok());
    match uploads.receive(&name, length, body).await {
        Ok(staged) => (StatusCode::CREATED, Json(staged)).into_response(),
        Err((status, why)) => problem(status, why),
    }
}

/// The staged file an import names: its path, which the job removes once it
/// has used it.
pub(crate) fn staged(uploads: Option<&Uploads>, id: Uuid) -> Result<PathBuf, String> {
    uploads
        .ok_or_else(|| "the server stages no uploads".to_owned())?
        .staged(id)
}

#[cfg(test)]
mod tests {
    use super::plain;

    #[test]
    fn a_name_is_one_plain_part() {
        for name in [
            "slide.tif",
            "a stack (2).tiff",
            "émail.png",
            "COM.png",
            "console.jpg",
        ] {
            assert!(plain(name), "{name}");
        }
        for name in [
            "", ".", "..", ".hidden", "../x.tif", "a/b.tif", "a\\b.tif", "c:x.tif", "x.tif.",
            "x.tif ", "nul", "CON.tif", "com1.png", "lpt9", "a\u{0}b", "a\nb",
        ] {
            assert!(!plain(name), "{name:?}");
        }
        assert!(!plain(&"a".repeat(256)) && plain(&"a".repeat(255)));
    }
}
