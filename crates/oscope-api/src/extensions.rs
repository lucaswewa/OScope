//! The `extensions` Thing (plan §5.8; ADR-0018): the installed extensions,
//! what the loaded ones provide, and installing, enabling, disabling and
//! uninstalling them, each for the next start.

use std::sync::{Arc, OnceLock};

use futures_util::StreamExt;
use oscope_ext::{API_VERSION, ExtError, Extension, Extensions, PLATFORM, Panel, Registry};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use teta_wot::http::PathParams;
use teta_wot::http::axum::Json;
use teta_wot::http::axum::body::Body;
use teta_wot::http::axum::http::StatusCode;
use teta_wot::http::axum::response::{IntoResponse, Response};
use teta_wot::prelude::*;
use tokio::io::AsyncWriteExt;

/// The largest package the upload route takes: 1 GiB.
pub const UPLOAD_LIMIT: u64 = 1 << 30;

/// What extensions this OScope can load.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Host {
    /// The extension API it offers, which a package's `oscope-api` range
    /// must match.
    pub api: String,
    /// Its platform, which a Rust package needs a library for.
    pub platform: Option<String>,
}

/// A panel of the extensions loaded, as `panels` lists it (ADR-0036).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PanelEntry {
    /// Its name.
    pub name: String,
    /// The extension that describes it.
    pub extension: String,
    /// Its title.
    pub title: String,
    /// What it is for.
    pub description: String,
}

/// The HTTP status and kind an error answers with (ADR-0034's shape).
fn status(error: &ExtError) -> (u16, &'static str) {
    match error {
        ExtError::Unknown(_) => (404, "unknown"),
        ExtError::Conflict(_) => (409, "conflict"),
        ExtError::Io { .. } => (500, "internal"),
        _ => (400, "refused"),
    }
}

fn action_error(error: ExtError) -> ActionError {
    let (status, kind) = status(&error);
    ActionError::handled(format!("{kind} ({status}): {error}"))
}

/// Problem details, for an endpoint.
fn problem(status: u16, kind: &str, detail: impl std::fmt::Display) -> Response {
    let status = StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    let body = json!({"title": kind, "status": status.as_u16(), "detail": detail.to_string()});
    (status, Json(body)).into_response()
}

fn failed(error: &ExtError) -> Response {
    let (status, kind) = status(error);
    problem(status, kind, error)
}

/// The extensions: packages that add processing nodes, drivers, workflows,
/// panels, importers, exporters and reports (ADR-0018). They load when the
/// server starts, so each change here waits for the next start: `system`'s
/// `restart`.
///
/// Install a package by posting it to `install`, as the request's body.
#[derive(Thing)]
#[thing(
    title = "Extensions",
    semantic_type = "saref:Function",
    context(saref = "https://saref.etsi.org/core/")
)]
pub struct ExtensionsThing {
    /// An extension was installed, enabled, disabled or uninstalled.
    ///
    /// Its ID.
    #[event]
    changed: Event<String>,
    service: OnceLock<Arc<Extensions>>,
}

/// An error of the server's own, not of the extensions.
fn internal(error: impl Into<Box<dyn std::error::Error + Send + Sync>>) -> ExtError {
    ExtError::Io {
        path: "extensions".into(),
        source: std::io::Error::other(error),
    }
}

impl ExtensionsThing {
    fn service(&self) -> Result<Arc<Extensions>, ExtError> {
        let service = self.service.get().cloned();
        service.ok_or_else(|| internal("the Thing hasn't started"))
    }

    fn reading(&self) -> Result<Arc<Extensions>, PropertyError> {
        self.service()
            .map_err(|e| PropertyError::failed(anyhow::anyhow!(e)))
    }

    /// Does `work` on the extensions, away from the async threads.
    async fn blocking<T: Send + 'static>(
        &self,
        work: impl FnOnce(&Extensions) -> Result<T, ExtError> + Send + 'static,
    ) -> Result<T, ExtError> {
        let service = self.service()?;
        let done = tokio::task::spawn_blocking(move || work(&service)).await;
        done.map_err(internal)?
    }

    /// Enables or disables an extension, and says so.
    async fn set_enabled(&self, id: String, enabled: bool) -> Result<Extension, ActionError> {
        let set = self.blocking(move |service| match enabled {
            true => service.enable(&id),
            false => service.disable(&id),
        });
        let extension = set.await.map_err(action_error)?;
        self.changed.emit(extension.manifest.id.clone());
        Ok(extension)
    }
}

#[thing_impl]
impl ExtensionsThing {
    #[on_start]
    async fn connect(&self, ctx: ThingCtx) -> anyhow::Result<()> {
        let service = ctx
            .server()
            .service::<Extensions>()
            .ok_or_else(|| anyhow::anyhow!("the server has no extensions"))?;
        let _ = self.service.set(service);
        Ok(())
    }

    /// The installed extensions, by ID.
    ///
    /// Each with its manifest, the SHA-256 of its package, the version this
    /// start loaded and the one the next start will.
    #[property]
    async fn installed(&self) -> Result<Vec<Extension>, PropertyError> {
        Ok(self.reading()?.list())
    }

    /// What the extensions this start loaded provide.
    ///
    /// For each contribution, each name and the extension that provides it.
    #[property]
    async fn provided(&self) -> Result<Registry, PropertyError> {
        Ok(self.reading()?.registry().clone())
    }

    /// Whether the next start loads other extensions than this one did.
    #[property]
    async fn restart_needed(&self) -> Result<bool, PropertyError> {
        Ok(self.reading()?.restart_needed())
    }

    /// The extension API this OScope offers, and its platform.
    #[property]
    async fn host(&self) -> Host {
        Host {
            api: API_VERSION.to_string(),
            platform: PLATFORM.map(str::to_owned),
        }
    }

    /// The panels of the extensions this start loaded, for the app to draw.
    ///
    /// Each one's name, extension, title and description: `panel/{name}`
    /// gives it whole.
    #[property]
    async fn panels(&self) -> Result<Vec<PanelEntry>, PropertyError> {
        let service = self.reading()?;
        let entry = |(name, (extension, panel)): (&String, &(String, Panel))| PanelEntry {
            name: name.clone(),
            extension: extension.clone(),
            title: panel.title.clone(),
            description: panel.description.clone(),
        };
        Ok(service.panels().iter().map(entry).collect())
    }

    /// One panel of the extensions loaded: its form, what submitting it
    /// does, and how to show what comes back.
    #[endpoint(get, "panel/{name}")]
    async fn panel(&self, params: PathParams) -> Response {
        let name = params.get("name").unwrap_or_default().to_owned();
        let service = match self.service() {
            Ok(service) => service,
            Err(error) => return failed(&error),
        };
        match service.panels().get(&name) {
            Some((_, panel)) => Json(panel.clone()).into_response(),
            None => problem(
                404,
                "unknown",
                format!("no extension loaded has a panel `{name}`"),
            ),
        }
    }

    /// One installed extension.
    #[endpoint(get, "extension/{id}")]
    async fn extension(&self, params: PathParams) -> Response {
        let id = params.get("id").unwrap_or_default().to_owned();
        match self.service().and_then(|service| service.get(&id)) {
            Ok(extension) => Json(extension).into_response(),
            Err(error) => failed(&error),
        }
    }

    /// Installs the package in the request's body, an `.omext` of at most
    /// 1 GiB, for the next start.
    ///
    /// Answers the extension: 201 when it unpacked the package, 200 when the
    /// same package was there already. 400 for a package it refuses, 409 for
    /// the same version from another package, 413 for one too large.
    #[endpoint(post, "install")]
    async fn install(&self, body: Body) -> Response {
        let file = match self.service().and_then(|service| service.incoming()) {
            Ok(file) => file,
            Err(error) => return failed(&error),
        };
        if let Err((status, kind, detail)) = receive(body, &file).await {
            return problem(status, kind, detail);
        }
        let installed = self.blocking(move |service| service.install(file.path()));
        match installed.await {
            Ok((extension, unpacked)) => {
                self.changed.emit(extension.manifest.id.clone());
                let status = if unpacked {
                    StatusCode::CREATED
                } else {
                    StatusCode::OK
                };
                (status, Json(extension)).into_response()
            }
            Err(error) => failed(&error),
        }
    }

    /// Enables an extension: the next start loads it, if it can.
    #[action]
    async fn enable(&self, id: String) -> Result<Extension, ActionError> {
        self.set_enabled(id, true).await
    }

    /// Disables an extension: the next start doesn't load it.
    #[action]
    async fn disable(&self, id: String) -> Result<Extension, ActionError> {
        self.set_enabled(id, false).await
    }

    /// Uninstalls an extension.
    ///
    /// If this start didn't load it, its files go at once, and it answers
    /// `null`; if it did, they go at the next start.
    #[action]
    async fn uninstall(&self, id: String) -> Result<Option<Extension>, ActionError> {
        let said = id.clone();
        let uninstall = self.blocking(move |service| service.uninstall(&id));
        let uninstalled = uninstall.await.map_err(action_error)?;
        self.changed.emit(said);
        Ok(uninstalled)
    }
}

/// Why an upload failed: its status, its kind and what happened.
type Failed = (u16, &'static str, String);

/// Receives a request's body into `file`, up to [`UPLOAD_LIMIT`].
async fn receive(body: Body, file: &tempfile::NamedTempFile) -> Result<(), Failed> {
    let internal = |error: std::io::Error| (500, "internal", error.to_string());
    let mut out = tokio::fs::File::from_std(file.reopen().map_err(internal)?);
    let (mut stream, mut total) = (body.into_data_stream(), 0_u64);
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| (400, "refused", e.to_string()))?;
        total += chunk.len() as u64;
        if total > UPLOAD_LIMIT {
            let detail = format!("a package is at most {UPLOAD_LIMIT} bytes");
            return Err((413, "too large", detail));
        }
        out.write_all(&chunk).await.map_err(internal)?;
    }
    out.flush().await.map_err(internal)?;
    if total == 0 {
        let detail = "the request has no package in its body".to_owned();
        return Err((400, "refused", detail));
    }
    Ok(())
}
