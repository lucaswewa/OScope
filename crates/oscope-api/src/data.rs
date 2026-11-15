//! The data route (plan §4.3; P30): a project's files as downloads, with
//! Range requests, at `GET /api/v1/data/{project}/{path}`.
//!
//! - **Its files:** documents, exports, renditions, thumbnails, and the
//!   OME-Zarr stores' files, which a Zarr reader can open over HTTP.
//! - **A range** of bytes, one: `bytes=a-b`, `bytes=a-` or `bytes=-n`, as
//!   RFC 9110 §14 has it. Several ranges, or a form it doesn't know, are
//!   ignored, and the whole file sent.
//! - **Never outside the project:** each part of the path must be a plain
//!   name, and not hidden.

use std::io::SeekFrom;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use futures_util::stream;
use oscope_core::ProjectId;
use oscope_store::Store;
use serde_json::json;
use teta_wot::http::axum::Router;
use teta_wot::http::axum::body::{Body, Bytes};
use teta_wot::http::axum::extract::{Path as UrlPath, State};
use teta_wot::http::axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use teta_wot::http::axum::response::{IntoResponse, Response};
use teta_wot::http::axum::routing::get;
use tokio::io::{AsyncReadExt, AsyncSeekExt};

use crate::API_PREFIX;

/// The bytes read at a time.
const CHUNK: u64 = 1 << 20;

/// The data route, over `store`'s projects.
pub(crate) fn router(store: Arc<Store>) -> Router {
    Router::new()
        .route(
            &format!("{API_PREFIX}/data/{{project}}/{{*path}}"),
            get(download),
        )
        .with_state(store)
}

pub(crate) fn problem(status: StatusCode, detail: impl std::fmt::Display) -> Response {
    let body = json!({
        "title": status.canonical_reason().unwrap_or("error"),
        "status": status.as_u16(),
        "detail": detail.to_string(),
    });
    let mut response = (status, body.to_string()).into_response();
    let kind = HeaderValue::from_static("application/problem+json");
    response.headers_mut().insert(header::CONTENT_TYPE, kind);
    response
}

/// `path` in a project's folder: each part a plain name, not hidden.
fn within(path: &str) -> Option<PathBuf> {
    let mut within = PathBuf::new();
    for part in path.split('/') {
        let plain = !part.is_empty() && !part.starts_with('.') && !part.contains(['\\', ':']);
        if !plain {
            return None;
        }
        within.push(part);
    }
    Some(within)
}

/// The content type of the file at `path`, by its name.
fn content_type(path: &Path) -> &'static str {
    let extension = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    match extension.to_ascii_lowercase().as_str() {
        "json" => "application/json",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "tif" | "tiff" => "image/tiff",
        "csv" => "text/csv; charset=utf-8",
        "txt" | "log" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

/// What a `Range` header asks of a file.
#[derive(Debug, PartialEq, Eq)]
enum Ranged {
    /// The whole file.
    Whole,
    /// The bytes from the first, up to the second.
    Part(u64, u64),
    /// Nothing it holds.
    Unsatisfiable,
}

/// What `header` asks of a file of `length` bytes.
fn ranged(header: &str, length: u64) -> Ranged {
    let Some(spec) = header.trim().strip_prefix("bytes=") else {
        return Ranged::Whole;
    };
    let Some((first, last)) = spec.split_once('-').filter(|_| !spec.contains(',')) else {
        return Ranged::Whole;
    };
    let (first, last) = (first.trim(), last.trim());
    let number = |text: &str| text.parse::<u64>().ok();
    match (first.is_empty(), number(first), number(last)) {
        (true, _, Some(0)) => Ranged::Unsatisfiable,
        (true, _, Some(suffix)) if length > 0 => Ranged::Part(length - suffix.min(length), length),
        (false, Some(from), _) if from >= length => Ranged::Unsatisfiable,
        (false, Some(from), None) if last.is_empty() => Ranged::Part(from, length),
        (false, Some(from), Some(to)) if to >= from => Ranged::Part(from, (to + 1).min(length)),
        (true, _, Some(_)) => Ranged::Unsatisfiable,
        _ => Ranged::Whole,
    }
}

async fn download(
    State(store): State<Arc<Store>>,
    UrlPath((project, path)): UrlPath<(String, String)>,
    headers: HeaderMap,
) -> Response {
    let Ok(id) = project.parse::<ProjectId>() else {
        return problem(
            StatusCode::BAD_REQUEST,
            format!("{project:?} isn't a project's ID"),
        );
    };
    let Some(relative) = within(&path) else {
        return problem(
            StatusCode::BAD_REQUEST,
            format!("{path:?} isn't a path in a project"),
        );
    };
    let file = store.folder(id).join(relative);
    let length = match tokio::fs::metadata(&file).await {
        Ok(found) if found.is_file() => found.len(),
        _ => {
            return problem(
                StatusCode::NOT_FOUND,
                format!("no file {path} in project {id}"),
            );
        }
    };
    let range = headers.get(header::RANGE).and_then(|v| v.to_str().ok());
    let (status, from, to) = match range.map_or(Ranged::Whole, |r| ranged(r, length)) {
        Ranged::Whole => (StatusCode::OK, 0, length),
        Ranged::Part(from, to) => (StatusCode::PARTIAL_CONTENT, from, to),
        Ranged::Unsatisfiable => {
            let mut response = problem(StatusCode::RANGE_NOT_SATISFIABLE, "no such range");
            let all = HeaderValue::from_str(&format!("bytes */{length}"));
            if let Ok(all) = all {
                response.headers_mut().insert(header::CONTENT_RANGE, all);
            }
            return response;
        }
    };
    let mut opened = match tokio::fs::File::open(&file).await {
        Ok(opened) => opened,
        Err(error) => return problem(StatusCode::INTERNAL_SERVER_ERROR, error),
    };
    if let Err(error) = opened.seek(SeekFrom::Start(from)).await {
        return problem(StatusCode::INTERNAL_SERVER_ERROR, error);
    }
    let chunks = stream::unfold((opened, to - from), |(mut opened, left)| async move {
        if left == 0 {
            return None;
        }
        let mut buffer = vec![0; left.min(CHUNK) as usize];
        match opened.read(&mut buffer).await {
            Ok(0) => None,
            Ok(read) => {
                buffer.truncate(read);
                Some((Ok(Bytes::from(buffer)), (opened, left - read as u64)))
            }
            Err(error) => Some((Err(error), (opened, 0))),
        }
    });
    let mut response = Response::new(Body::from_stream(chunks));
    *response.status_mut() = status;
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static(content_type(&file)),
    );
    headers.insert(header::CONTENT_LENGTH, HeaderValue::from(to - from));
    headers.insert(header::ACCEPT_RANGES, HeaderValue::from_static("bytes"));
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    if status == StatusCode::PARTIAL_CONTENT
        && let Ok(part) = HeaderValue::from_str(&format!("bytes {from}-{}/{length}", to - 1))
    {
        headers.insert(header::CONTENT_RANGE, part);
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranges_are_read_as_rfc_9110_has_them() {
        let cases = [
            ("bytes=0-99", Ranged::Part(0, 100)),
            ("bytes=100-", Ranged::Part(100, 1000)),
            ("bytes=-100", Ranged::Part(900, 1000)),
            ("bytes=-5000", Ranged::Part(0, 1000)),
            ("bytes=990-2000", Ranged::Part(990, 1000)),
            ("bytes=1000-", Ranged::Unsatisfiable),
            ("bytes=-0", Ranged::Unsatisfiable),
            ("bytes=5-1", Ranged::Whole),
            ("bytes=0-1,5-6", Ranged::Whole),
            ("items=0-1", Ranged::Whole),
            ("bytes=x-1", Ranged::Whole),
        ];
        for (header, expected) in cases {
            assert_eq!(ranged(header, 1000), expected, "{header}");
        }
        assert_eq!(ranged("bytes=-1", 0), Ranged::Unsatisfiable);
    }

    #[test]
    fn paths_stay_in_the_project() {
        assert_eq!(
            within("acquisitions/x/exports/a.ome.tif"),
            Some(PathBuf::from("acquisitions/x/exports/a.ome.tif"))
        );
        for bad in [
            "", "../x", "a/../b", "a//b", "./a", ".lock", "a/.part", "C:/x", "a\\b",
        ] {
            assert_eq!(within(bad), None, "{bad}");
        }
    }
}
