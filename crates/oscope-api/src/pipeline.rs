//! The `pipeline` Thing (W7.1, ADR-0052; plan §5.4): recipes over the API,
//! for the graph editor.
//!
//! - **`nodes`:** the catalogue, the stock nodes' and the extensions', each
//!   with its ports and its parameters' schema: as `preview/nodes` gives it.
//! - **`check`:** a recipe's faults, all at once, each with the node and the
//!   port it is at. It changes nothing, so it needs no lease.
//! - **The library:** `recipes`, `recipe/{name}`, `save_recipe` and
//!   `delete_recipe`: the stock recipes, and the user's in the data root's
//!   `recipes/`.
//! - **`run`:** a recipe on a project's records, each a `workspace.run` job.

use std::sync::{Arc, OnceLock};

use oscope_core::ProjectId;
use oscope_ext::Extensions;
use oscope_jobs::JobService;
use oscope_pipeline::{
    Descriptor, Library, LibraryError, Listed, Problem, Recipe, Registry, check, stock,
};
use oscope_store::Input;
use oscope_workflows::workspace::{Analysis, RUN, Roi};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use teta_wot::http::PathParams;
use teta_wot::http::axum::Json;
use teta_wot::http::axum::http::StatusCode;
use teta_wot::http::axum::response::{IntoResponse, Response};
use teta_wot::prelude::*;

use crate::jobs::{JobRef, action_error as job_error};
use crate::server::Roots;

/// The HTTP status and kind a library's error answers with.
fn status(error: &LibraryError) -> (u16, &'static str) {
    match error {
        LibraryError::Refused(_) => (400, "refused"),
        LibraryError::NotFound(_) => (404, "not found"),
        _ => (500, "internal"),
    }
}

fn action_error(error: LibraryError) -> ActionError {
    let (status, kind) = status(&error);
    ActionError::handled(format!("{kind} ({status}): {error}"))
}

fn failed(error: &LibraryError) -> Response {
    let (code, kind) = status(error);
    let code = StatusCode::from_u16(code).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    let body = json!({"title": kind, "status": code.as_u16(), "detail": error.to_string()});
    (code, Json(body)).into_response()
}

/// A recipe's faults, as `check` gives them.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct Checked {
    /// Every fault found, each where it is: none if the recipe can run.
    pub faults: Vec<Problem>,
}

/// The recipe a run uses: one in the library, by name, or one given.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Which {
    /// The library's recipe of this name.
    Named(String),
    /// This recipe.
    Given(Recipe),
}

/// What the Thing works with, once started.
struct State {
    registry: Registry,
    library: Library,
    jobs: Option<Arc<JobService>>,
}

/// Recipes over the API: the nodes they may use, their faults, the library
/// they are kept in, and their runs on a project's records.
#[derive(Thing)]
#[thing(
    title = "Pipeline",
    semantic_type = "saref:Function",
    context(saref = "https://saref.etsi.org/core/")
)]
pub struct PipelineThing {
    /// A recipe in the library was kept or deleted.
    ///
    /// Its name.
    #[event]
    changed: Event<String>,
    state: OnceLock<State>,
}

impl PipelineThing {
    fn state(&self) -> Result<&State, LibraryError> {
        let state = self.state.get();
        state.ok_or_else(|| LibraryError::Io("the Thing hasn't started".to_owned()))
    }

    /// Does `work` on the library, away from the async threads.
    async fn blocking<T: Send + 'static>(
        &self,
        work: impl FnOnce(&Library) -> Result<T, LibraryError> + Send + 'static,
    ) -> Result<T, LibraryError> {
        let library = self.state()?.library.clone();
        let done = tokio::task::spawn_blocking(move || work(&library)).await;
        done.map_err(|e| LibraryError::Io(e.to_string()))?
    }
}

#[thing_impl]
impl PipelineThing {
    #[on_start]
    async fn connect(&self, ctx: ThingCtx) -> anyhow::Result<()> {
        let server = ctx.server();
        let data = server.service::<Roots>().map(|roots| roots.data.clone());
        let data = data.unwrap_or_else(|| ".".into());
        // The stock nodes and the extensions', as the Workspace's engine has them.
        let mut registry = stock::registry(data.join("references"));
        for node in server
            .service::<Extensions>()
            .iter()
            .flat_map(|e| e.nodes())
        {
            if let Err(error) = registry.register(Arc::clone(node)) {
                tracing::warn!("recipes can't use an extension's node: {error}");
            }
        }
        let library = Library::open(&data.join("recipes"));
        let jobs = server.service::<JobService>();
        let _ = self.state.set(State {
            registry,
            library,
            jobs,
        });
        Ok(())
    }

    /// The nodes recipes may use.
    ///
    /// The stock nodes (P22) and the extensions' (P25), each with its ports
    /// and its parameters' schema, as `preview/nodes` gives them.
    #[property]
    async fn nodes(&self) -> Result<Vec<Descriptor>, PropertyError> {
        let state = self.state().map_err(PropertyError::failed)?;
        Ok(state.registry.descriptors().cloned().collect())
    }

    /// Checks a recipe: every fault, at once.
    ///
    /// Each fault says where it is, with the node and the port it is at
    /// where it is at one: none if the recipe can run here. JSON that isn't
    /// a recipe is one fault, at `recipe`. It changes nothing, and needs no
    /// lease (ADR-0052).
    #[action]
    async fn check(&self, recipe: Value) -> Result<Checked, ActionError> {
        let state = self.state().map_err(action_error)?;
        let faults = check(&state.registry, &recipe, cfg!(feature = "cuda"));
        Ok(Checked { faults })
    }

    /// The recipes in the library: the stock ones first, each by name.
    #[endpoint(get, "recipes")]
    async fn recipes(&self) -> Response {
        match self.blocking(|library| library.list()).await {
            Ok(listed) => Json::<Vec<Listed>>(listed).into_response(),
            Err(error) => failed(&error),
        }
    }

    /// A recipe in the library, by its name: its JSON, and whether it is
    /// stock.
    #[endpoint(get, "recipe/{name}")]
    async fn recipe(&self, params: PathParams) -> Response {
        let name = params.get("name").unwrap_or_default().to_owned();
        match self.blocking(move |library| library.get(&name)).await {
            Ok(kept) => Json(kept).into_response(),
            Err(error) => failed(&error),
        }
    }

    /// Keeps a recipe in the library, by name.
    ///
    /// In place of one of that name. A name has 1 to 64 characters of
    /// `a`–`z`, `0`–`9`, `-` and `_`, and isn't a stock recipe's. The recipe
    /// is kept whether or not `check` finds faults in it.
    #[action]
    async fn save_recipe(&self, name: String, recipe: Recipe) -> Result<(), ActionError> {
        let said = name.clone();
        let saved = self.blocking(move |library| library.save(&name, &recipe));
        saved.await.map_err(action_error)?;
        self.changed.emit(said);
        Ok(())
    }

    /// Deletes a recipe from the library: a user's.
    #[action]
    async fn delete_recipe(&self, name: String) -> Result<(), ActionError> {
        let said = name.clone();
        let deleted = self.blocking(move |library| library.delete(&name));
        deleted.await.map_err(action_error)?;
        self.changed.emit(said);
        Ok(())
    }

    /// Runs a recipe on a project's records: a `workspace.run` job each.
    ///
    /// The recipe is the library's, `{"named": "count-nuclei"}`, or one
    /// given, `{"given": {…}}`. It takes an image, `image`, and runs on each
    /// record, or on a region of it in µm, its outputs kept as a result of
    /// the project (P48). Refused, before any job, for a recipe with faults.
    #[action]
    async fn run(
        &self,
        ctx: ActionCtx,
        project: ProjectId,
        recipe: Which,
        records: Vec<Input>,
        #[param(default)] roi: Option<Roi>,
        #[param(default)] tile: Option<u32>,
    ) -> Result<Vec<JobRef>, ActionError> {
        let recipe = match recipe {
            Which::Given(recipe) => recipe,
            Which::Named(name) => {
                let kept = self.blocking(move |library| library.get(&name));
                kept.await.map_err(action_error)?.recipe
            }
        };
        let state = self.state().map_err(action_error)?;
        let json =
            serde_json::to_value(&recipe).map_err(|e| ActionError::handled(e.to_string()))?;
        let faults = check(&state.registry, &json, cfg!(feature = "cuda"));
        if !faults.is_empty() {
            let said: Vec<String> = faults.iter().map(ToString::to_string).collect();
            let said = said.join("; ");
            return Err(ActionError::handled(format!(
                "refused (400): the recipe can't run: {said}"
            )));
        }
        if records.is_empty() {
            return Err(ActionError::handled("refused (400): no records to run on"));
        }
        let jobs = state.jobs.as_ref();
        let jobs =
            jobs.ok_or_else(|| ActionError::handled("unavailable (503): the server runs no jobs"))?;
        let account = ctx.principal().id().map(str::to_owned);
        let mut submitted = Vec::new();
        for input in records {
            let analysis = Analysis {
                recipe: recipe.clone(),
                input,
                roi,
                tile,
                name: None,
            };
            let params = json!({"project": project, "analysis": analysis});
            let id = jobs
                .submit_kind(RUN, params, account.clone())
                .map_err(job_error)?;
            submitted.push(JobRef::new(id));
        }
        Ok(submitted)
    }
}
