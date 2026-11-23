//! The `projects` Thing (D16, plan §5.12, ADR-0021): the projects on disk,
//! as the Gallery shows them: listed, searched and paged; made, renamed,
//! tagged, described and deleted; each one's contents; and thumbnails. It
//! registers the jobs that import files into projects and export their
//! images, `projects.import` and `projects.export`, and lists their formats
//! (ADR-0037); with a scratch space, the one that ingests recordings,
//! `projects.ingest` (ADR-0022); the one that stitches a scan's
//! acquisition into a mosaic, `stitch.run` (ADR-0027); and the one that
//! finds a z-stack's heights and all-in-focus image,
//! `volume.depth_from_focus` (P42); and the one that deconvolves a volume,
//! `volume.deconvolve` (P43); and the one that separates an image's
//! stains, `stain.separate` (P45); and the one that counts its cells,
//! `cells.count` (P46); and the one that makes reports, `report.make`,
//! through templates it keeps and checks (P47). Annotations, which P44
//! put here, are the `workspace` Thing's (P48).

use std::sync::{Arc, OnceLock};

use oscope_acquire::{INGEST, ingest_jobs};
use oscope_core::ProjectId;
use oscope_ext::Extensions;
use oscope_jobs::JobService;
use oscope_report::{ReportError, Template, Templates, to_base64};
use oscope_store::{
    Change, Contents, NewProject, Page, Project, Query as Search, RecordKind, Scratch, Store,
    StoreError,
};
use oscope_workflows::cells::{COUNT, count_jobs};
use oscope_workflows::deconvolve::{DECONVOLVE, deconvolve_jobs};
use oscope_workflows::depth::{DEPTH, depth_jobs};
use oscope_workflows::report::{Generators, MAKE, preview, report_jobs};
use oscope_workflows::stain::{SEPARATE, stain_jobs};
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
use crate::upload::Uploads;

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

pub(crate) fn action_error(error: StoreError) -> ActionError {
    let (status, kind) = status(&error);
    ActionError::handled(format!("{kind} ({status}): {error}"))
}

/// Problem details, for an endpoint.
pub(crate) fn failed(error: &StoreError) -> Response {
    let (code, kind) = status(error);
    let code = StatusCode::from_u16(code).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    let body = json!({"title": kind, "status": code.as_u16(), "detail": error.to_string()});
    (code, Json(body)).into_response()
}

/// The project a path names, or why not.
pub(crate) fn project_id(params: &PathParams) -> Result<ProjectId, StoreError> {
    params.parse::<ProjectId>("id").ok_or_else(|| {
        let text = params.get("id").unwrap_or_default();
        StoreError::Refused(format!("{text:?} isn't a project's ID"))
    })
}

/// A template's error, as an action's: refused (400).
fn template_error(error: ReportError) -> ActionError {
    ActionError::handled(format!("refused (400): {error}"))
}

/// A template's error, as an endpoint's.
fn template_failed(error: &ReportError) -> Response {
    let body = json!({"title": "refused", "status": 400, "detail": error.to_string()});
    (StatusCode::BAD_REQUEST, Json(body)).into_response()
}

/// The record a path names: its project, its kind by its folder's name,
/// `acquisitions` or `results`, and its ID.
pub(crate) fn record(params: &PathParams) -> Result<(ProjectId, RecordKind, String), StoreError> {
    let project = project_id(params)?;
    let folder = params.get("kind").unwrap_or_default();
    let kind = [RecordKind::Acquisition, RecordKind::Result]
        .into_iter()
        .find(|k| k.folder() == folder)
        .ok_or_else(|| StoreError::Refused(format!("{folder:?}: acquisitions or results")))?;
    Ok((
        project,
        kind,
        params.get("record").unwrap_or_default().to_owned(),
    ))
}

/// Which measurements, and as what.
#[derive(Debug, Deserialize)]
pub struct MeasureQuery {
    /// One layer's, or with none, every layer's.
    pub(crate) layer: Option<String>,
    /// A stack's plane: its first by default.
    pub(crate) plane: Option<u32>,
    /// `csv` for CSV, and JSON otherwise.
    pub(crate) format: Option<String>,
}

/// A report template's preview, kept nowhere (W7.9).
#[derive(Debug, Serialize, JsonSchema)]
pub struct Preview {
    /// The PDF, in base64.
    pub pdf: String,
    /// The HTML.
    pub html: String,
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
    templates: OnceLock<Templates>,
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
        let data = store.root().parent().unwrap_or(store.root());
        let templates = Templates::open(&data.join("templates"));
        if let Some(jobs) = ctx.server().service::<JobService>() {
            let uploads = ctx.server().service::<Uploads>();
            jobs.register(
                IMPORT,
                import_jobs(Arc::clone(&store), Arc::clone(&codecs), uploads),
            );
            jobs.register(EXPORT, export_jobs(Arc::clone(&store), Arc::clone(&codecs)));
            jobs.register(STITCH, stitch_jobs(Arc::clone(&store)));
            jobs.register(DEPTH, depth_jobs(Arc::clone(&store)));
            jobs.register(DECONVOLVE, deconvolve_jobs(Arc::clone(&store)));
            jobs.register(SEPARATE, stain_jobs(Arc::clone(&store)));
            jobs.register(COUNT, count_jobs(Arc::clone(&store)));
            // Reports, by OScope's generator or the extensions' (P47).
            let generators = extensions.clone().map(|e| -> Generators {
                Arc::new(move |name, params| e.reports().generate(name, params.clone()))
            });
            jobs.register(
                MAKE,
                report_jobs(Arc::clone(&store), templates.clone(), generators),
            );
            // Recordings from the scratch space, ingested as jobs (ADR-0022).
            if let Some(scratch) = ctx.server().service::<Scratch>() {
                jobs.register(INGEST, ingest_jobs(Arc::clone(&store), scratch));
            }
        }
        let _ = self.codecs.set(codecs);
        let _ = self.templates.set(templates);
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
    /// 500 at most); by name with `&sort=name`. Each with how many
    /// acquisitions, results and reports it has.
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

    /// A record's annotations.
    ///
    /// The report templates' names, OScope's own first.
    #[endpoint(get, "templates")]
    async fn templates(&self) -> Response {
        match self.templates.get().map(Templates::names) {
            Some(Ok(names)) => Json::<Vec<String>>(names).into_response(),
            Some(Err(error)) => template_failed(&error),
            None => failed(&StoreError::Index("the Thing hasn't started".to_owned())),
        }
    }

    /// A report template: its Typst, for PDF, and its minijinja, for HTML.
    #[endpoint(get, "template/{name}")]
    async fn template(&self, params: PathParams) -> Response {
        let name = params.get("name").unwrap_or_default().to_owned();
        match self.templates.get().map(|t| t.get(&name)) {
            Some(Ok(template)) => Json::<Template>(template).into_response(),
            Some(Err(error)) => template_failed(&error),
            None => failed(&StoreError::Index("the Thing hasn't started".to_owned())),
        }
    }

    /// Keeps a report template.
    ///
    /// Its Typst, for PDF, and its minijinja, for HTML, once a sample
    /// report renders through both: otherwise refused, saying where the
    /// template is wrong.
    #[action]
    async fn save_template(
        &self,
        name: String,
        typst: String,
        html: String,
    ) -> Result<(), ActionError> {
        let templates = self
            .templates
            .get()
            .cloned()
            .ok_or_else(|| ActionError::handled("the Thing hasn't started"))?;
        let saved = tokio::task::spawn_blocking(move || templates.save(&name, &typst, &html)).await;
        saved
            .map_err(|e| ActionError::handled(e.to_string()))?
            .map_err(template_error)
    }

    /// Previews a report template, keeping nothing.
    ///
    /// OScope's report on `result` of `project`, or a sample report without
    /// them, through `typst`, for PDF, and `html`, for HTML: the PDF, in
    /// base64, and the HTML. Refused, saying where, when the template is
    /// wrong, as `save_template` is.
    #[action]
    async fn preview_template(
        &self,
        typst: String,
        html: String,
        #[param(default)] project: Option<ProjectId>,
        #[param(default)] result: Option<String>,
    ) -> Result<Preview, ActionError> {
        let on = match (project, result) {
            (Some(project), Some(result)) => Some((project, result)),
            (None, None) => None,
            _ => {
                return Err(ActionError::handled(
                    "refused (400): a result is previewed with its project, and a project with a result",
                ));
            }
        };
        let made = self.blocking(move |store| {
            let on = on.as_ref().map(|(p, r)| (*p, r.as_str()));
            preview(store, on, &typst, &html)
        });
        let (pdf, html) = made.await.map_err(action_error)?;
        Ok(Preview {
            pdf: to_base64(&pdf),
            html,
        })
    }

    /// Deletes a report template, a user's.
    #[action]
    async fn delete_template(&self, name: String) -> Result<(), ActionError> {
        let templates = self
            .templates
            .get()
            .ok_or_else(|| ActionError::handled("the Thing hasn't started"))?;
        templates.delete(&name).map_err(template_error)
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

    /// Deletes a record of a project, with its files.
    ///
    /// `kind` is `acquisition`, `result` or `report`. Refused while a
    /// result was made from it (W6.2).
    #[action]
    async fn delete_record(
        &self,
        project: ProjectId,
        kind: RecordKind,
        record: String,
    ) -> Result<(), ActionError> {
        let deleted = self.blocking(move |store| store.delete_record(project, kind, &record));
        deleted.await.map_err(action_error)?;
        self.changed.emit(project);
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
