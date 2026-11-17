//! The `power` Thing (P51, ADR-0032; plan §5.19): the service restarted,
//! and the host rebooted or shut down, safely.
//!
//! - **Restarting** stops the server, which then exits with code 75 for its
//!   supervisor to start it again (ADR-0015).
//! - **Rebooting and shutting down** run `shutdown.exe` on Windows, and
//!   `systemctl` on Linux, which the packaging's polkit rule lets the
//!   service's account use (P54).
//! - **The safeguards:**
//!   - the control lease, when the server has accounts (ADR-0031);
//!   - `confirm: true`;
//!   - no hardware job running, unless `force`.
//! - **A dry run** does all but act: it says what it would run.

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
    /// The host, rebooted.
    Reboot,
    /// The host, shut down.
    Shutdown,
}

/// The platforms OScope runs on (ADR-0003).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Platform {
    /// Windows 11.
    Windows,
    /// Ubuntu LTS.
    Linux,
}

impl Platform {
    /// This one.
    pub fn this() -> Platform {
        match cfg!(windows) {
            true => Platform::Windows,
            false => Platform::Linux,
        }
    }
}

/// The command that does `action` on `platform`: none for a restart, which
/// the server does itself.
pub fn command(action: Action, platform: Platform) -> Option<Vec<String>> {
    let words: &[&str] = match (action, platform) {
        (Action::Restart, _) => return None,
        (Action::Reboot, Platform::Windows) => &["shutdown.exe", "/r", "/t", "0"],
        (Action::Shutdown, Platform::Windows) => &["shutdown.exe", "/s", "/t", "0"],
        (Action::Reboot, Platform::Linux) => &["systemctl", "reboot"],
        (Action::Shutdown, Platform::Linux) => &["systemctl", "poweroff"],
    };
    Some(words.iter().map(|w| (*w).to_owned()).collect())
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

/// Whether the `power` Thing acts, or only says what it would do.
pub(crate) struct PowerConfig {
    pub(crate) dry_run: bool,
}

struct State {
    dry_run: bool,
    jobs: Option<Arc<JobService>>,
    restart: Option<Arc<Restart>>,
}

/// Power: the service restarted, the host rebooted or shut down.
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
        let powered = Powered {
            action,
            command: command(action, Platform::this()),
            dry_run: state.dry_run,
        };
        self.powering.emit(powered.clone());
        tracing::warn!("power: {powered:?}");
        if state.dry_run {
            return Ok(powered);
        }
        match &powered.command {
            None => {
                let restart = state.restart.as_ref();
                let restart =
                    restart.ok_or_else(|| ActionError::handled("the server can't restart"))?;
                restart.request();
            }
            Some(words) => {
                let ran = std::process::Command::new(&words[0])
                    .args(&words[1..])
                    .spawn();
                ran.map_err(|e| ActionError::handled(format!("{}: {e}", words.join(" "))))?;
            }
        }
        Ok(powered)
    }
}

#[thing_impl]
impl PowerThing {
    #[on_start]
    async fn connect(&self, ctx: ThingCtx) -> anyhow::Result<()> {
        let server = ctx.server();
        let config = server.service::<PowerConfig>();
        let _ = self.state.set(State {
            dry_run: config.is_none_or(|c| c.dry_run),
            jobs: server.service::<JobService>(),
            restart: server.service::<Restart>(),
        });
        Ok(())
    }

    /// Whether this is a dry run: each action says what it would do, and
    /// does nothing.
    #[property]
    async fn dry_run(&self) -> bool {
        self.state.get().is_none_or(|s| s.dry_run)
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

    /// Reboots the host.
    ///
    /// Needs `confirm`, and no hardware job running unless `force`.
    #[action]
    async fn reboot(
        &self,
        #[param(default)] confirm: bool,
        #[param(default)] force: bool,
    ) -> Result<Powered, ActionError> {
        self.act(Action::Reboot, confirm, force)
    }

    /// Shuts the host down.
    ///
    /// Needs `confirm`, and no hardware job running unless `force`.
    #[action]
    async fn shutdown(
        &self,
        #[param(default)] confirm: bool,
        #[param(default)] force: bool,
    ) -> Result<Powered, ActionError> {
        self.act(Action::Shutdown, confirm, force)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_platform_has_its_commands() {
        let line = |action, platform| command(action, platform).map(|w| w.join(" "));
        assert_eq!(line(Action::Restart, Platform::Windows), None);
        assert_eq!(line(Action::Restart, Platform::Linux), None);
        let windows = (
            line(Action::Reboot, Platform::Windows),
            line(Action::Shutdown, Platform::Windows),
        );
        assert_eq!(
            windows,
            (
                Some("shutdown.exe /r /t 0".into()),
                Some("shutdown.exe /s /t 0".into())
            )
        );
        let linux = (
            line(Action::Reboot, Platform::Linux),
            line(Action::Shutdown, Platform::Linux),
        );
        assert_eq!(
            linux,
            (
                Some("systemctl reboot".into()),
                Some("systemctl poweroff".into())
            )
        );
        assert_eq!(Platform::this() == Platform::Windows, cfg!(windows));
    }
}
