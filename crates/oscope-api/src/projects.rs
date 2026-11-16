//! The `projects` Thing (D16, plan §5.12, ADR-0021): the projects on disk,
//! as the Gallery shows them: listed, searched and paged; made, renamed,
//! tagged, described and deleted; each one's contents; and thumbnails. It
//! registers the jobs that import files into projects and export their
//! images, `projects.import` and `projects.export`, and lists their formats
//! (ADR-0037); with a scratch space, the one that ingests recordings,
//! `projects.ingest` (ADR-0022); the one that stitches a scan's
//! acquisition into a mosaic, `stitch.run` (ADR-0027); and the one that
//! finds a z-stack's heights and all-in-focus image,
//! `volume.depth_from_focus` (P42).

use std::sync::{Arc, OnceLock};

use oscope_acquire::{INGEST, ingest_jobs};
use oscope_core::ProjectId;
use oscope_ext::Extensions;
use oscope_jobs::JobService;
use oscope_store::{
    Change, Contents, NewProject, Page, Project, Query as Search, Scratch, Store, StoreError,
};
use oscope_workflows::depth::{DEPTH, depth_jobs};
use oscope_workflows::stitch::{STITCH, stitch_jobs};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use teta_wot::http::PathParams;
use teta_wot::http::axum::Json;
use teta_wot::http::axum::extract::Query;
use teta_wot::http::axum::http::{StatusCode, header};
use teta_wot::http::axum::response::{IntoResponse, Response};
use teta_wot::prelude::*;

use crate::files::{Codecs, EXPORT, Formats, IMPORT, export_jobs, import_jobs};

/// What a rebuild of the index found.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Rebuilt {
    /// How many projects the index lists.
    pub projects: usize,
    /// The documents it left out, because they didn't read.
    pub skipped: Vec<String>,
}

/// The HTTP status and kind an error answers with (ADR-0034's shape).
fn status(error: &StoreError) -> (u16, &'static str) {
    match error {
        StoreError::NotFound { .. } => (404, "unknown"),
        StoreError::Refused(_) => (400, "refused"),
        _ => (500, "internal"),
    }
}

fn action_error(error: StoreError) -> ActionError {
    let (status, kind) = status(&error);
    ActionError::handled(format!("{kind} ({status}): {error}"))
}

/// Problem details, for an endpoint.
fn failed(error: &StoreError) -> Response {
    let (code, kind) = status(error);
    let code = StatusCode::from_u16(code).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    let body = json!({"title": kind, "status": code.as_u16(), "detail": error.to_string()});
    (code, Json(body)).into_response()
}

/// The project a path names, or why not.
fn project_id(params: &PathParams) -> Result<ProjectId, StoreError> {
    params.parse::<ProjectId>("id").ok_or_else(|| {
        let text = params.get("id").unwrap_or_default();
        StoreError::Refused(format!("{text:?} isn't a project's ID"))
    })
}

/// The projects: one slide or experiment each, holding its acquisitions,
/// results and reports, in folders under the data root, with an index for
/// listing them.
#[derive(Thing)]
#[thing(
    title = "Projects",
    semantic_type = "saref:Function",
    context(saref = "https://saref.etsi.org/core/")
)]
pub struct ProjectsThing {
    /// A project was made, changed or deleted.
    ///
    /// Its ID.
    #[event]
    changed: Event<ProjectId>,
    store: OnceLock<Arc<Store>>,
    codecs: OnceLock<Arc<Codecs>>,
}

impl ProjectsThing {
    fn store(&self) -> Result<Arc<Store>, StoreError> {
        let store = self.store.get().cloned();
        store.ok_or_else(|| StoreError::Index("the Thing hasn't started".to_owned()))
    }

    /// Does `work` on the store, away from the async threads.
    async fn blocking<T: Send + 'static>(
        &self,
        work: impl FnOnce(&Store) -> Result<T, StoreError> + Send + 'static,
    ) -> Result<T, StoreError> {
        let store = self.store()?;
        let done = tokio::task::spawn_blocking(move || work(&store)).await;
        done.map_err(|e| StoreError::Index(e.to_string()))?
    }

    /// Changes a project, and says so.
    async fn change(&self, id: ProjectId, change: Change) -> Result<Project, ActionError> {
        let changed = self.blocking(move |store| store.change(id, change));
        let project = changed.await.map_err(action_error)?;
        self.changed.emit(project.id);
        Ok(project)
    }

    /// Answers the thumbnail `record` of project `id`, or the project's.
    async fn thumbnail_of(&self, params: &PathParams, record: Option<String>) -> Response {
        let id = match project_id(params) {
            Ok(id) => id,
            Err(error) => return failed(&error),
        };
        let read = self.blocking(move |store| {
            let path = store.thumbnail(id, record.as_deref())?;
            std::fs::read(&path).map_err(|source| StoreError::Io { path, source })
        });
        match read.await {
            Ok(jpeg) => ([(header::CONTENT_TYPE, "image/jpeg")], jpeg).into_response(),
            Err(error) => failed(&error),
        }
    }
}

#[thing_impl]
impl ProjectsThing {
    #[on_start]
    async fn connect(&self, ctx: ThingCtx) -> anyhow::Result<()> {
        let store = ctx
            .server()
            .service::<Store>()
            .ok_or_else(|| anyhow::anyhow!("the server has no project store"))?;
        // Importers and exporters: OScope's own, then the extensions'.
        let extensions = ctx.server().service::<Extensions>();
        let codecs = Arc::new(Codecs::new(extensions.as_deref()));
        if let Some(jobs) = ctx.server().service::<JobService>() {
            jobs.register(IMPORT, import_jobs(Arc::clone(&store), Arc::clone(&codecs)));
            jobs.register(EXPORT, export_jobs(Arc::clone(&store), Arc::clone(&codecs)));
            jobs.register(STITCH, stitch_jobs(Arc::clone(&store)));
            jobs.register(DEPTH, depth_jobs(Arc::clone(&store)));
            // Recordings from the scratch space, ingested as jobs (ADR-0022).
            if let Some(scratch) = ctx.server().service::<Scratch>() {
                jobs.register(INGEST, ingest_jobs(Arc::clone(&store), scratch));
            }
        }
        let _ = self.codecs.set(codecs);
        let _ = self.store.set(store);
        Ok(())
    }

    /// The formats the server imports, with the `projects.import` job, and
    /// exports to, with `projects.export` (ADR-0037): OScope's own, then its
    /// extensions'.
    #[endpoint(get, "formats")]
    async fn formats(&self) -> Response {
        match self.codecs.get() {
            Some(codecs) => Json::<Formats>(codecs.formats()).into_response(),
            None => failed(&StoreError::Index("the Thing hasn't started".to_owned())),
        }
    }

    /// A page of the projects, the most recently changed first.
    ///
    /// Searched by `?text=…` in their names, samples and tags, in any case,
    /// and `&tag=…` for a whole tag; paged by `&offset=…&limit=…` (50, and
    /// 500 at most). Each with how many acquisitions, results and reports
    /// it has.
    #[endpoint(get, "list")]
    async fn list(&self, Query(search): Query<Search>) -> Response {
        match self.blocking(move |store| store.list(&search)).await {
            Ok(page) => Json::<Page>(page).into_response(),
            Err(error) => failed(&error),
        }
    }

    /// One project, with its acquisitions, results and reports.
    #[endpoint(get, "project/{id}")]
    async fn project(&self, params: PathParams) -> Response {
        let id = match project_id(&params) {
            Ok(id) => id,
            Err(error) => return failed(&error),
        };
        match self.blocking(move |store| store.contents(id)).await {
            Ok(contents) => Json::<Contents>(contents).into_response(),
            Err(error) => failed(&error),
        }
    }

    /// A project's thumbnail, a JPEG: its newest image's.
    #[endpoint(get, "thumbnail/{id}")]
    async fn thumbnail(&self, params: PathParams) -> Response {
        self.thumbnail_of(&params, None).await
    }

    /// The thumbnail of a project's acquisition or result, a JPEG.
    #[endpoint(get, "thumbnail/{id}/{record}")]
    async fn record_thumbnail(&self, params: PathParams) -> Response {
        let record = params.get("record").unwrap_or_default().to_owned();
        self.thumbnail_of(&params, Some(record)).await
    }

    /// Makes a project.
    ///
    /// Its name has 1 to 200 characters; its tags 1 to 64 each.
    #[action]
    async fn create(
        &self,
        name: String,
        #[param(default)] sample: Option<String>,
        #[param(default)] tags: Option<Vec<String>>,
        #[param(default)] owner: Option<String>,
        #[param(default)] metadata: Option<Map<String, Value>>,
    ) -> Result<Project, ActionError> {
        let new = NewProject {
            name,
            sample: sample.unwrap_or_default(),
            tags: tags.unwrap_or_default(),
            owner: owner.unwrap_or_default(),
            metadata: metadata.unwrap_or_default(),
        };
        let project = self
            .blocking(move |store| store.create(new))
            .await
            .map_err(action_error)?;
        self.changed.emit(project.id);
        Ok(project)
    }

    /// Renames a project.
    #[action]
    async fn rename(&self, id: ProjectId, name: String) -> Result<Project, ActionError> {
        let change = Change {
            name: Some(name),
            ..Change::default()
        };
        self.change(id, change).await
    }

    /// Says what is on a project's slide.
    #[action]
    async fn set_sample(&self, id: ProjectId, sample: String) -> Result<Project, ActionError> {
        let change = Change {
            sample: Some(sample),
            ..Change::default()
        };
        self.change(id, change).await
    }

    /// Replaces a project's tags.
    #[action]
    async fn tag(&self, id: ProjectId, tags: Vec<String>) -> Result<Project, ActionError> {
        let change = Change {
            tags: Some(tags),
            ..Change::default()
        };
        self.change(id, change).await
    }

    /// Sets keys of a project's metadata; `null` removes one.
    #[action]
    async fn set_metadata(
        &self,
        id: ProjectId,
        metadata: Map<String, Value>,
    ) -> Result<Project, ActionError> {
        let change = Change {
            metadata: Some(metadata),
            ..Change::default()
        };
        self.change(id, change).await
    }

    /// Deletes a project, with everything in it.
    #[action]
    async fn delete(&self, id: ProjectId) -> Result<(), ActionError> {
        let deleted = self.blocking(move |store| store.delete(id));
        deleted.await.map_err(action_error)?;
        self.changed.emit(id);
        Ok(())
    }

    /// Rebuilds the index from the folders, which are the truth.
    ///
    /// For an index lost or damaged: how many projects it then lists, and
    /// the documents left out because they didn't read.
    #[action]
    async fn rebuild_index(&self) -> Result<Rebuilt, ActionError> {
        let rebuilt = self.blocking(|store| {
            let skipped = store.rebuild_index()?;
            let projects = store.list(&Search::default())?.total;
            let skipped = skipped
                .iter()
                .map(|path| path.display().to_string())
                .collect();
            Ok(Rebuilt { projects, skipped })
        });
        rebuilt.await.map_err(action_error)
    }
}
