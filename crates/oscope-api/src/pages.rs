//! Pages served at the API's origin, such as an example's HTML page and its
//! scripts.

use std::path::{Path, PathBuf};

use teta_wot::http::axum::Router;
use teta_wot::http::axum::http::{StatusCode, header};
use teta_wot::http::axum::response::{IntoResponse, Response};
use teta_wot::http::axum::routing::get;

use crate::ApiError;

/// The paths the API answers outside its prefix, which a page would hide.
const RESERVED: &[&str] = &["health", "ready", "openapi.json", "docs", "redoc"];

/// The folders the API answers in.
const RESERVED_FOLDERS: &[&str] = &["api/", "docs/", ".well-known/"];

/// A route for each file under `folder`, by its relative path under
/// `prefix`, and `/` for its `index.html` when there is no prefix.
pub(crate) fn router(prefix: &str, folder: &Path) -> Result<Router, ApiError> {
    let refuse = |reason: String| ApiError::Pages {
        folder: folder.to_owned(),
        reason,
    };
    let mut files = Vec::new();
    collect(folder, &mut files).map_err(|error| refuse(error.to_string()))?;
    let mut router = Router::new();
    for file in files {
        let relative = file.strip_prefix(folder).expect("found under the folder");
        let path = relative
            .components()
            .map(|part| part.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/");
        let path = match prefix {
            "" => path,
            prefix => format!("{prefix}/{path}"),
        };
        if RESERVED.contains(&path.as_str()) || RESERVED_FOLDERS.iter().any(|f| path.starts_with(f))
        {
            return Err(refuse(format!("`{path}` would hide the API's `/{path}`")));
        }
        if path == "index.html" {
            let index = file.clone();
            router = router.route("/", get(move || serve(index.clone())));
        }
        router = router.route(&format!("/{path}"), get(move || serve(file.clone())));
    }
    Ok(router)
}

/// Every file under `folder`, at any depth.
fn collect(folder: &Path, files: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(folder)? {
        let path = entry?.path();
        if path.is_dir() {
            collect(&path, files)?;
        } else {
            files.push(path);
        }
    }
    Ok(())
}

/// The file as it is now, or `404` if it has gone.
async fn serve(file: PathBuf) -> Response {
    match tokio::fs::read(&file).await {
        Ok(bytes) => {
            let headers = [
                (header::CONTENT_TYPE, content_type(&file)),
                (header::CACHE_CONTROL, "no-cache"),
            ];
            (headers, bytes).into_response()
        }
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}

/// A file's content type, from its extension.
fn content_type(file: &Path) -> &'static str {
    match file
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
    {
        "html" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" => "application/json",
        "txt" => "text/plain; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "tif" | "tiff" => "image/tiff",
        "ico" => "image/x-icon",
        "wasm" => "application/wasm",
        _ => "application/octet-stream",
    }
}
