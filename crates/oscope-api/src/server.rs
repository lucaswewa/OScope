//! The server: OScope's Things and its own routes, in one `teta-wot` server.

use std::future::Future;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use oscope_jobs::JobService;
use oscope_shm::RingWriter;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use teta_wot::http::axum::middleware;
use teta_wot::http::axum::routing::get;
use teta_wot::http::axum::{Json, Router};
use teta_wot::prelude::FromConfig;
use teta_wot::server::{Authenticator, HttpOptions, ThingServer, ThingServerBuilder};
use tokio::net::TcpListener;
use tokio::sync::Notify;

use crate::devices::{
    CameraThing, Devices, IlluminationThing, ObjectiveThing, SimulationThing, StageThing, statuses,
};
use crate::jobs::JobsThing;
use crate::logs::{LogStore, Logs};
use crate::preview::PreviewThing;
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
    logs: LogStore,
    restart: Arc<Restart>,
}

/// A request to restart the server, which the `system` Thing's `restart`
/// action makes, and whoever serves the [`Api`] waits for
/// ([`Api::restart`]). The sidecar contract (ADR-0015) exits with code 75
/// for it.
#[derive(Debug, Default)]
pub struct Restart {
    asked: AtomicBool,
    notify: Notify,
}

impl Restart {
    /// Asks for a restart.
    pub fn request(&self) {
        self.asked.store(true, Ordering::SeqCst);
        self.notify.notify_waiters();
    }

    /// Whether a restart was asked for.
    pub fn is_requested(&self) -> bool {
        self.asked.load(Ordering::SeqCst)
    }

    /// Completes when a restart is asked for.
    pub async fn requested(&self) {
        loop {
            let asked = self.notify.notified();
            if self.is_requested() {
                return;
            }
            asked.await;
        }
    }
}

/// The preview ring, until the `preview` Thing starts publishing to it.
pub(crate) struct RingSlot(pub(crate) std::sync::Mutex<Option<RingWriter>>);

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

    /// Serves `store`: the `logs` Thing queries it, and the media plane
    /// tails it. Without this, the server has a store of its own, in
    /// memory, that nothing records into.
    pub fn logs(mut self, store: LogStore) -> Self {
        self.logs = store;
        self
    }

    /// Saves the Things' settings in `folder`, each Thing's in a folder of
    /// its own, created when the server is built; the server uses
    /// `{data root}/settings`. Without this, settings aren't saved.
    pub fn settings(mut self, folder: impl Into<PathBuf>) -> Self {
        self.things = self.things.settings_folder(folder.into());
        self
    }

    /// Serves the instrument's `devices` as the Things `camera`, `stage`,
    /// `illumination` and `objective`, and `simulation` when a device is
    /// simulated; and their frames' live `preview`.
    pub fn devices(mut self, devices: Arc<Devices>) -> Self {
        let simulated = devices.simulator.is_some();
        self.things = self
            .things
            .service(devices)
            .thing("camera", CameraThing::default())
            .thing("stage", StageThing::default())
            .thing("illumination", IlluminationThing::default())
            .thing("objective", ObjectiveThing::default())
            .thing("preview", PreviewThing::default());
        if simulated {
            self.things = self.things.thing("simulation", SimulationThing::default());
        }
        self
    }

    /// Publishes previews of the camera's newest frames to `ring`, the
    /// desktop app's fast path (ADR-0015), at the `preview` Thing's
    /// defaults. Needs [`devices`](Self::devices).
    pub fn ring(mut self, ring: RingWriter) -> Self {
        let slot = RingSlot(std::sync::Mutex::new(Some(ring)));
        self.things = self.things.service(Arc::new(slot));
        self
    }

    /// Serves the job service as the `jobs` Thing (ADR-0016). Its hardware
    /// lock also stops the device Things' manual moves while a hardware job
    /// runs.
    pub fn jobs(mut self, jobs: JobService) -> Self {
        self.things = self
            .things
            .service(Arc::new(jobs))
            .thing("jobs", JobsThing::default());
        self
    }

    /// Builds the server. Nothing starts until [`Api::serve`].
    pub fn build(self) -> Result<Api, ApiError> {
        let mut routes = Router::new()
            .route("/health", get(|| async { Json(json!({"status": "ok"})) }))
            .merge(media::router(self.logs.clone()));
        if let Some(folder) = &self.pages {
            routes = routes.merge(pages::router(folder)?);
        }
        let things = self
            .things
            .thing("logs", Logs::new(self.logs))
            .layer(middleware::from_fn(statuses));
        let server = things.merge(routes).build()?;
        Ok(Api {
            server,
            restart: self.restart,
        })
    }
}

/// OScope's server, built and ready to serve.
#[derive(Debug)]
pub struct Api {
    server: ThingServer,
    restart: Arc<Restart>,
}

impl Api {
    /// Starts building the server, with the `system` Thing reporting on
    /// `roots`, and the `logs` Thing.
    pub fn builder(roots: &Roots) -> ApiBuilder {
        let restart = Arc::new(Restart::default());
        let things = ThingServer::builder()
            .api_prefix(API_PREFIX)
            .api_info(TITLE, VERSION)
            .service(Arc::clone(&restart))
            .service(Arc::new(roots.clone()))
            .thing("system", System::from_config(roots.clone()));
        ApiBuilder {
            things,
            pages: None,
            logs: LogStore::default(),
            restart,
        }
    }

    /// The server's restart request: whoever serves waits for it, and stops.
    pub fn restart(&self) -> Arc<Restart> {
        Arc::clone(&self.restart)
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
