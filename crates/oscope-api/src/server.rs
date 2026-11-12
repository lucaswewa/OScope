//! The server: OScope's Things and its own routes, in one `teta-wot` server.

use std::future::Future;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use teta_wot::http::axum::routing::get;
use teta_wot::http::axum::{Json, Router};
use teta_wot::prelude::FromConfig;
use teta_wot::server::{Authenticator, HttpOptions, ThingServer, ThingServerBuilder};
use tokio::net::TcpListener;

use crate::system::{System, VERSION};
use crate::{media, pages};

/// The control plane's prefix: the `system` Thing is at `/api/v1/system/`.
/// The OpenAPI document is at the root, `/openapi.json`, with its docs pages
/// at `/docs` and `/redoc`.
pub const API_PREFIX: &str = "/api/v1";

/// The media plane's prefix.
pub const MEDIA_PREFIX: &str = "/api/v1/media";

/// The API's title, in the OpenAPI document.
const TITLE: &str = "OScope";

/// Where OScope keeps its files.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Roots {
    /// Projects and their data.
    pub data: PathBuf,
    /// Fast scratch space: frame logs and caches.
    pub scratch: PathBuf,
}

/// Why the server can't be built, described or run.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ApiError {
    /// `teta-wot` refused the Things or the routes.
    #[error(transparent)]
    Build(#[from] teta_wot::server::ServerBuildError),
    /// A folder of pages can't be served.
    #[error("the pages in {}: {reason}", folder.display())]
    Pages {
        /// The folder.
        folder: PathBuf,
        /// What is wrong.
        reason: String,
    },
    /// A Thing Description can't be made.
    #[error("the Thing Description of `{thing}`: {reason}")]
    Description {
        /// The Thing.
        thing: String,
        /// What is wrong.
        reason: String,
    },
    /// Serving failed: a Thing didn't start, or the listener failed.
    #[error(transparent)]
    Serve(#[from] teta_wot::server::ServeError),
}

/// Builds an [`Api`].
#[must_use]
#[derive(Debug)]
pub struct ApiBuilder {
    things: ThingServerBuilder,
    pages: Option<PathBuf>,
}

impl ApiBuilder {
    /// Checks every request with `authenticator`: the Things', the media
    /// plane's, `/health` and the pages. The Thing Descriptions and the
    /// OpenAPI document stay public.
    pub fn authenticator(mut self, authenticator: impl Authenticator) -> Self {
        self.things = self.things.authenticator(authenticator);
        self
    }

    /// Also serves the files in `folder`, at the API's origin: `/` and
    /// `/index.html` give its `index.html`, `/js/app.js` its `js/app.js`.
    /// Each file is read when it is asked for, so an edit shows without a
    /// restart. The examples' pages use it.
    pub fn pages(mut self, folder: impl Into<PathBuf>) -> Self {
        self.pages = Some(folder.into());
        self
    }

    /// Builds the server. Nothing starts until [`Api::serve`].
    pub fn build(self) -> Result<Api, ApiError> {
        let mut routes = Router::new()
            .route("/health", get(|| async { Json(json!({"status": "ok"})) }))
            .merge(media::router());
        if let Some(folder) = &self.pages {
            routes = routes.merge(pages::router(folder)?);
        }
        let server = self.things.merge(routes).build()?;
        Ok(Api { server })
    }
}

/// OScope's server, built and ready to serve.
#[derive(Debug)]
pub struct Api {
    server: ThingServer,
}

impl Api {
    /// Starts building the server, with the `system` Thing reporting on
    /// `roots`.
    pub fn builder(roots: &Roots) -> ApiBuilder {
        let things = ThingServer::builder()
            .api_prefix(API_PREFIX)
            .api_info(TITLE, VERSION)
            .thing("system", System::from_config(roots.clone()));
        ApiBuilder {
            things,
            pages: None,
        }
    }

    /// The OpenAPI document of the control plane, as the server serves it
    /// without an authenticator.
    pub fn openapi(&self) -> Value {
        teta_wot::server::openapi(self.server.runtime(), &options())
    }

    /// Each Thing's Thing Description, by name, as the server serves it at
    /// `base`, such as `http://127.0.0.1:8210/`, without an authenticator.
    pub fn thing_descriptions(&self, base: &str) -> Result<Map<String, Value>, ApiError> {
        let runtime = self.server.runtime();
        runtime
            .things()
            .map(|thing| {
                let name = thing.name().to_owned();
                match teta_wot::server::thing_description(runtime, &name, base, &options()) {
                    Ok(td) => Ok((name, td)),
                    Err(reason) => Err(ApiError::Description {
                        thing: name,
                        reason,
                    }),
                }
            })
            .collect()
    }

    /// Starts the Things and serves on `listener` until `shutdown`
    /// completes. Then it stops accepting connections, closes the media
    /// plane's WebSockets, waits up to 5 s for requests in progress, and
    /// stops the Things.
    pub async fn serve(
        self,
        listener: TcpListener,
        shutdown: impl Future<Output = ()> + Send + 'static,
    ) -> Result<(), ApiError> {
        Ok(self.server.serve_with(listener, shutdown).await?)
    }

    /// The `teta-wot` server underneath, for tests with its test client.
    pub fn into_thing_server(self) -> ThingServer {
        self.server
    }
}

/// The options the server is built with, for the documents made without it.
fn options() -> HttpOptions {
    HttpOptions {
        api_prefix: API_PREFIX.to_owned(),
        api_title: TITLE.to_owned(),
        api_version: VERSION.to_owned(),
        ..HttpOptions::default()
    }
}
