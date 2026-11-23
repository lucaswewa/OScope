//! The `workspace` Thing (P48, plan §5.4): the Workspace tab's backend.
//!
//! - **Analyses:** recipes run on a project's images, or on regions of
//!   them, as jobs (`workspace.run`), on an engine of its own with the
//!   stock nodes and the extensions'.
//! - **Their results,** with their provenance and files.
//! - **Again:** a result made again with its nodes' parameters changed.
//! - **Differences:** two results' provenance compared.
//! - **Deletion:** results deleted.
//! - **Annotations** in µm, kept, removed and measured (P44), moved here
//!   from the `projects` Thing.

use std::collections::BTreeMap;
use std::sync::{Arc, OnceLock};

use oscope_compute::{Compute, ComputeConfig};
use oscope_core::{ProjectId, ResultId};
use oscope_ext::{Contribution, Extensions};
use oscope_jobs::JobService;
use oscope_pipeline::{Engine, stock};
use oscope_store::{
    Annotations, CHUNK, Feature, Measurement, RecordKind, ResultRecord, Store, StoreError, measure,
    measurements_csv,
};
use oscope_workflows::workspace::{
    Analysis, Change, Origins, RUN, again, difference, workspace_jobs,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use teta_wot::http::PathParams;
use teta_wot::http::axum::Json;
use teta_wot::http::axum::extract::Query;
use teta_wot::http::axum::http::header;
use teta_wot::http::axum::response::{IntoResponse, Response};
use teta_wot::prelude::*;

use crate::jobs::{JobRef, action_error as job_error};
use crate::projects::{MeasureQuery, action_error, failed, project_id, record};
use crate::server::Roots;

/// A result as the Workspace lists it: its record, and the files in its
/// folder.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct Analysed {
    /// Its record.
    #[serde(flatten)]
    pub result: ResultRecord,
    /// Its files: `summary.json` and its outputs', its image's store
    /// apart.
    pub files: Vec<String>,
}

/// What the Thing works with, once started.
struct State {
    store: Arc<Store>,
    jobs: Option<Arc<JobService>>,
}

/// The Workspace: analyses on a project's images, their results and their
/// provenance, and annotations.
#[derive(Thing)]
#[thing(
    title = "Workspace",
    semantic_type = "saref:Function",
    context(saref = "https://saref.etsi.org/core/")
)]
pub struct WorkspaceThing {
    state: OnceLock<State>,
}

impl WorkspaceThing {
    fn state(&self) -> Result<&State, StoreError> {
        let state = self.state.get();
        state.ok_or_else(|| StoreError::Index("the Thing hasn't started".to_owned()))
    }

    /// Does `work` on the store, away from the async threads.
    async fn blocking<T: Send + 'static>(
        &self,
        work: impl FnOnce(&Store) -> Result<T, StoreError> + Send + 'static,
    ) -> Result<T, StoreError> {
        let store = Arc::clone(&self.state()?.store);
        let done = tokio::task::spawn_blocking(move || work(&store)).await;
        done.map_err(|e| StoreError::Index(e.to_string()))?
    }

    /// Submits `analysis` of `project` as a job.
    fn submit(
        &self,
        ctx: &ActionCtx,
        project: ProjectId,
        analysis: &Analysis,
    ) -> Result<JobRef, ActionError> {
        let jobs = self.state().map_err(action_error)?.jobs.as_ref();
        let jobs =
            jobs.ok_or_else(|| ActionError::handled("unavailable (503): the server runs no jobs"))?;
        let params = json!({"project": project, "analysis": analysis});
        let account = ctx.principal().id().map(str::to_owned);
        let id = jobs.submit_kind(RUN, params, account).map_err(job_error)?;
        Ok(JobRef::new(id))
    }
}

/// The nodes the extensions this start loaded provide, and which,
/// `id@version`.
fn origins(extensions: Option<&Extensions>) -> Origins {
    let loaded = extensions.iter().flat_map(|e| e.loaded());
    loaded
        .flat_map(|extension| {
            let manifest = &extension.manifest;
            let by = format!("{}@{}", manifest.id, manifest.version);
            let nodes = manifest
                .provides
                .get(&Contribution::Nodes)
                .into_iter()
                .flatten();
            nodes.map(move |node| (node.clone(), by.clone()))
        })
        .collect()
}

#[thing_impl]
impl WorkspaceThing {
    #[on_start]
    async fn connect(&self, ctx: ThingCtx) -> anyhow::Result<()> {
        let server = ctx.server();
        let store = server
            .service::<Store>()
            .ok_or_else(|| anyhow::anyhow!("the server has no project store"))?;
        let jobs = server.service::<JobService>();
        if let Some(jobs) = &jobs {
            // Its own engine: the stock nodes and the extensions', as live
            // recipes have them (P22, P25).
            let references = server
                .service::<Roots>()
                .map_or_else(|| "references".into(), |r| r.data.join("references"));
            let extensions = server.service::<Extensions>();
            let mut registry = stock::registry(&references);
            for node in extensions.iter().flat_map(|e| e.nodes()) {
                if let Err(error) = registry.register(Arc::clone(node)) {
                    tracing::warn!("the Workspace can't use an extension's node: {error}");
                }
            }
            let compute =
                tokio::task::spawn_blocking(|| Compute::new(&ComputeConfig::default())).await??;
            let engine = Arc::new(Engine::new(registry, Arc::new(compute), 256 << 20));
            let origins = Arc::new(origins(extensions.as_deref()));
            jobs.register(RUN, workspace_jobs(Arc::clone(&store), engine, origins));
        }
        let _ = self.state.set(State { store, jobs });
        Ok(())
    }

    /// A project's results, the newest last, each with its provenance and
    /// files.
    #[endpoint(get, "results/{id}")]
    async fn results(&self, params: PathParams) -> Response {
        let project = match project_id(&params) {
            Ok(project) => project,
            Err(error) => return failed(&error),
        };
        let listed = self.blocking(move |store| {
            let contents = store.contents(project)?;
            let folder = store.folder(project).join(RecordKind::Result.folder());
            let analysed = contents.results.into_iter().map(|result| {
                let place = folder.join(result.id.to_string());
                let mut files: Vec<String> = std::fs::read_dir(&place)
                    .into_iter()
                    .flatten()
                    .flatten()
                    .filter_map(|e| e.file_name().into_string().ok())
                    .filter(|n| !n.starts_with('.') && n != "result.json" && n != "image.ome.zarr")
                    .collect();
                files.sort();
                Analysed { result, files }
            });
            Ok(analysed.collect::<Vec<_>>())
        });
        match listed.await {
            Ok(listed) => Json(listed).into_response(),
            Err(error) => failed(&error),
        }
    }

    /// What differs between two results' provenance.
    ///
    /// Of project `id`: result `a`'s and result `b`'s, each a JSON Pointer
    /// and the two values. A recipe's nodes go by their IDs.
    #[endpoint(get, "difference/{id}/{a}/{b}")]
    async fn difference(&self, params: PathParams) -> Response {
        let project = match project_id(&params) {
            Ok(project) => project,
            Err(error) => return failed(&error),
        };
        let ids = (params.parse::<ResultId>("a"), params.parse::<ResultId>("b"));
        let (Some(a), Some(b)) = ids else {
            return failed(&StoreError::Refused("a and b are results' IDs".to_owned()));
        };
        let compared = self.blocking(move |store| {
            let contents = store.contents(project)?;
            let find = |id: ResultId| {
                let found = contents.results.iter().find(|r| r.id == id);
                found.ok_or_else(|| StoreError::NotFound {
                    kind: "result",
                    id: id.to_string(),
                })
            };
            Ok(difference(&find(a)?.provenance, &find(b)?.provenance))
        });
        match compared.await {
            Ok(changes) => Json::<Vec<Change>>(changes).into_response(),
            Err(error) => failed(&error),
        }
    }

    /// Runs an analysis: a job.
    ///
    /// A recipe that takes an image, `image`, run on a project's
    /// acquisition or result, or a region of it in µm, its outputs kept as
    /// a result of the project.
    #[action]
    async fn run(
        &self,
        ctx: ActionCtx,
        project: ProjectId,
        analysis: Analysis,
    ) -> Result<JobRef, ActionError> {
        self.submit(&ctx, project, &analysis)
    }

    /// Makes a result again: a job.
    ///
    /// Result `result` of a project, made in the Workspace, its analysis
    /// run again, with its nodes' parameters changed as `changes` says: by
    /// the node's ID in the recipe, each parameter's new value, or `null`
    /// for its default.
    #[action]
    async fn rerun(
        &self,
        ctx: ActionCtx,
        project: ProjectId,
        result: ResultId,
        #[param(default)] changes: BTreeMap<String, Map<String, Value>>,
    ) -> Result<JobRef, ActionError> {
        let made = self.blocking(move |store| again(store, project, result, &changes));
        let analysis = made.await.map_err(action_error)?;
        self.submit(&ctx, project, &analysis)
    }

    /// Deletes a result, with its files.
    ///
    /// Refused while another result was made from it.
    #[action]
    async fn delete_result(&self, project: ProjectId, result: ResultId) -> Result<(), ActionError> {
        let deleted = self.blocking(move |store| store.delete_result(project, result));
        deleted.await.map_err(action_error)
    }

    /// A project's acquisition's or result's, `kind` being `acquisitions` or
    /// `results`: its layers by name, each a GeoJSON FeatureCollection, in
    /// µm from the image's top-left corner.
    #[endpoint(get, "annotations/{id}/{kind}/{record}")]
    async fn annotations(&self, params: PathParams) -> Response {
        let read = match record(&params) {
            Ok((project, kind, id)) => {
                let read = self.blocking(move |store| store.annotations(project, kind, &id));
                read.await
            }
            Err(error) => Err(error),
        };
        match read {
            Ok(annotations) => Json::<Annotations>(annotations).into_response(),
            Err(error) => failed(&error),
        }
    }

    /// What a record's annotations measure.
    ///
    /// In µm: lengths, areas, centres, and the statistics of a polygon's
    /// pixels, a raw image's by the colours of its filters. `?layer=…` for one layer's, `&plane=…` for a stack's plane, and
    /// `&format=csv` for CSV.
    #[endpoint(get, "measurements/{id}/{kind}/{record}")]
    async fn measurements(
        &self,
        params: PathParams,
        Query(query): Query<MeasureQuery>,
    ) -> Response {
        let (project, kind, id) = match record(&params) {
            Ok(record) => record,
            Err(error) => return failed(&error),
        };
        let (layer, plane) = (query.layer, query.plane.unwrap_or(0));
        let measured = self.blocking(move |store| {
            measure(store, project, kind, &id, layer.as_deref(), plane, CHUNK)
        });
        match (measured.await, query.format.as_deref()) {
            (Ok(measured), Some("csv")) => {
                let disposition = "attachment; filename=\"measurements.csv\"";
                let headers = [
                    (header::CONTENT_TYPE, "text/csv; charset=utf-8"),
                    (header::CONTENT_DISPOSITION, disposition),
                ];
                (headers, measurements_csv(&measured)).into_response()
            }
            (Ok(measured), _) => Json::<Vec<Measurement>>(measured).into_response(),
            (Err(error), _) => failed(&error),
        }
    }

    /// Keeps an annotation.
    ///
    /// On a project's acquisition or result, in layer `layer`, made if need
    /// be: a GeoJSON Feature in µm, which replaces the one with its `id`, or
    /// with none is new. The feature as kept, with its ID.
    #[action]
    async fn annotate(
        &self,
        project: ProjectId,
        kind: RecordKind,
        record: String,
        layer: String,
        feature: Feature,
    ) -> Result<Feature, ActionError> {
        let kept =
            self.blocking(move |store| store.annotate(project, kind, &record, &layer, feature));
        kept.await.map_err(action_error)
    }

    /// Removes an annotation, or a layer.
    ///
    /// Feature `feature` of layer `layer` of a project's acquisition or
    /// result, or with no `feature`, the whole layer.
    #[action]
    async fn remove_annotation(
        &self,
        project: ProjectId,
        kind: RecordKind,
        record: String,
        layer: String,
        #[param(default)] feature: Option<String>,
    ) -> Result<(), ActionError> {
        let removed = self.blocking(move |store| {
            store.remove_annotation(project, kind, &record, &layer, feature.as_deref())
        });
        removed.await.map_err(action_error)
    }
}
