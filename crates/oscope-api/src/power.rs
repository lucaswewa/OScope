//! Container process restart. Host power control is deliberately absent.

use std::sync::{Arc, OnceLock};

use oscope_jobs::{JobFilter, JobService, JobState, Resource};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use teta_wot::prelude::*;

use crate::server::Restart;

/// A power action.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    /// The service, stopped and started again.
    Restart,
}

/// What a power action did, or with a dry run, would do.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Powered {
    /// The action.
    pub action: Action,
    /// The command it runs: none for a restart.
    pub command: Option<Vec<String>>,
    /// Whether it was a dry run, which did nothing.
    pub dry_run: bool,
}

/// The source-compatible dry-run settings for process restart.
pub(crate) struct PowerConfig {
    pub(crate) dry_run: bool,
    pub(crate) dry_run_restart: bool,
}

struct State {
    dry_run: bool,
    dry_run_restart: bool,
    jobs: Option<Arc<JobService>>,
    restart: Option<Arc<Restart>>,
}

/// Power: the container process restarted, without host power actions.
#[derive(Thing)]
#[thing(
    title = "Power",
    semantic_type = "saref:Function",
    context(saref = "https://saref.etsi.org/core/")
)]
pub struct PowerThing {
    /// A power action is about to be done, or in a dry run, would be.
    ///
    /// What it is.
    #[event]
    powering: Event<Powered>,
    state: OnceLock<State>,
}

impl PowerThing {
    /// Does `action`, `confirm`ed, `force`d past a hardware job or not.
    fn act(&self, action: Action, confirm: bool, force: bool) -> Result<Powered, ActionError> {
        let state = self.state.get();
        let state = state.ok_or_else(|| ActionError::handled("unavailable (503): not started"))?;
        if !confirm {
            return Err(ActionError::handled(format!(
                "refused (400): confirm it, with confirm: true, to {} it",
                format!("{action:?}").to_lowercase()
            )));
        }
        let running = state.jobs.as_ref().map(|jobs| {
            let filter = JobFilter {
                state: Some(JobState::Running),
                ..JobFilter::default()
            };
            let listed = jobs.list(&filter).unwrap_or_default();
            listed
                .into_iter()
                .find(|j| j.spec.resource == Resource::Hardware)
        });
        if let (Some(job), false) = (running.flatten(), force) {
            return Err(ActionError::handled(format!(
                "conflict (409): {} is running on the instrument: let it end, cancel it, or force it",
                job.spec.kind
            )));
        }
        let dry_run = state.dry_run_restart;
        let powered = Powered {
            action,
            command: None,
            dry_run,
        };
        self.powering.emit(powered.clone());
        tracing::warn!("power: {powered:?}");
        if dry_run {
            return Ok(powered);
        }
        let restart = state
            .restart
            .as_ref()
            .ok_or_else(|| ActionError::handled("the server cannot restart"))?;
        restart.request();
        Ok(powered)
    }
}

#[thing_impl]
impl PowerThing {
    #[on_start]
    async fn connect(&self, ctx: ThingCtx) -> anyhow::Result<()> {
        let server = ctx.server();
        let config = server.service::<PowerConfig>();
        let config = config.as_deref();
        let _ = self.state.set(State {
            dry_run: config.is_none_or(|c| c.dry_run),
            dry_run_restart: config.is_none_or(|c| c.dry_run_restart),
            jobs: server.service::<JobService>(),
            restart: server.service::<Restart>(),
        });
        Ok(())
    }

    /// The legacy dry-run setting, retained for API compatibility.
    /// Host power actions are unavailable regardless of this value.
    #[property]
    async fn dry_run(&self) -> bool {
        self.state.get().is_none_or(|s| s.dry_run)
    }

    /// Whether restarting the service is a dry run: as `dry_run`, unless
    /// the server's configuration says otherwise (ADR-0060).
    #[property]
    async fn dry_run_restart(&self) -> bool {
        self.state.get().is_none_or(|s| s.dry_run_restart)
    }

    /// Restarts the service.
    ///
    /// It stops, then exits with code 75 for its supervisor to start it
    /// again. Needs `confirm`, and no hardware job running unless `force`.
    #[action]
    async fn restart(
        &self,
        #[param(default)] confirm: bool,
        #[param(default)] force: bool,
    ) -> Result<Powered, ActionError> {
        self.act(Action::Restart, confirm, force)
    }
}
