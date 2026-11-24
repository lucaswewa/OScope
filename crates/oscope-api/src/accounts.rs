//! The `accounts` Thing (W8.6, ADR-0055): the accounts kept from the app,
//! where M1 had only `oscope-server account`. It is served with security
//! or without, on the accounts' file the server reads at its start, the
//! same accounts security checks logins against when it is on.
//!
//! - **Listed, added and removed,** with the lease. The first account, on a
//!   server without, turns security on at the next start; the last can't
//!   be removed while the next start listens on the network, which needs
//!   accounts.
//! - **One's own password,** changed with the old one, without the lease:
//!   it isn't the instrument's.
//! - **The sessions open,** and the idle times of sessions and the lease,
//!   from security.

use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

use serde::{Deserialize, Serialize};
use teta_wot::prelude::*;

use crate::auth::{ACCOUNTS, Accounts, AuthError, IdleTimes, OpenSession, Security};
use crate::server::Roots;
use crate::settings::SettingsFile;

/// What the Thing reads and changes, found as it starts.
struct State {
    accounts: Arc<Accounts>,
    security: Option<Arc<Security>>,
    file: Option<SettingsFile>,
}

/// The accounts, kept from the app (W8.6).
#[derive(Thing)]
#[thing(
    title = "Accounts",
    semantic_type = "saref:Function",
    context(saref = "https://saref.etsi.org/core/")
)]
pub struct AccountsThing {
    state: OnceLock<State>,
}

/// An account's name and password, to add it.
#[derive(Clone, Debug, Deserialize, Serialize, schemars::JsonSchema)]
pub struct NewAccount {
    /// Its name: 1 to 64 of `a`–`z`, `0`–`9`, `.`, `_` and `-`, starting
    /// with a letter, and not `local`.
    pub name: String,
    /// Its password: 8 characters at least.
    pub password: String,
}

fn refused(error: AuthError) -> ActionError {
    match error {
        AuthError::Refused(why) => ActionError::handled(format!("refused (400): {why}")),
        other => ActionError::handled(other.to_string()),
    }
}

impl AccountsThing {
    fn state(&self) -> Result<&State, ActionError> {
        let state = self.state.get();
        state.ok_or_else(|| ActionError::handled("unavailable (503): the Thing hasn't started"))
    }

    /// Runs `work` on the accounts, off the async threads: hashing a
    /// password takes a moment.
    async fn on_accounts<T: Send + 'static>(
        &self,
        work: impl FnOnce(&Accounts) -> Result<T, AuthError> + Send + 'static,
    ) -> Result<T, ActionError> {
        let accounts = Arc::clone(&self.state()?.accounts);
        let done = tokio::task::spawn_blocking(move || work(&accounts)).await;
        done.map_err(|e| ActionError::handled(e.to_string()))?
            .map_err(refused)
    }

    /// Whether the next start listens on the network, as its file says.
    fn next_on_network(&self) -> Result<bool, ActionError> {
        let Some(file) = &self.state()?.file else {
            return Ok(false);
        };
        let saved = file.0.read().map_err(ActionError::handled)?;
        Ok(!saved.system.address.is_loopback())
    }
}

#[thing_impl]
impl AccountsThing {
    #[on_start]
    async fn connect(&self, ctx: ThingCtx) -> anyhow::Result<()> {
        let server = ctx.server();
        let security = server.service::<Security>();
        let accounts = match &security {
            Some(security) => Arc::clone(security.accounts()),
            None => {
                let roots = server.service::<Roots>();
                let data = roots.map_or_else(|| PathBuf::from("."), |r| r.data.clone());
                Arc::new(Accounts::open(&data.join(ACCOUNTS))?)
            }
        };
        let file = server
            .service::<SettingsFile>()
            .map(|f| SettingsFile(Arc::clone(&f.0)));
        let _ = self.state.set(State {
            accounts,
            security,
            file,
        });
        Ok(())
    }

    /// The accounts, by name.
    #[property]
    async fn names(&self) -> Result<Vec<String>, PropertyError> {
        Ok(self
            .state
            .get()
            .map(|s| s.accounts.names())
            .unwrap_or_default())
    }

    /// Whether this start has security on: logins, sessions and the lease.
    #[property]
    async fn secure(&self) -> Result<bool, PropertyError> {
        Ok(self.state.get().is_some_and(|s| s.security.is_some()))
    }

    /// Whether the next start has security otherwise than this one: the
    /// first account added, or the last removed.
    #[property]
    async fn restart_needed(&self) -> Result<bool, PropertyError> {
        Ok(self
            .state
            .get()
            .is_some_and(|s| s.accounts.is_empty() == s.security.is_some()))
    }

    /// The sessions open, the longest idle first: none without security.
    #[property]
    async fn sessions(&self) -> Result<Vec<OpenSession>, PropertyError> {
        let security = self.state.get().and_then(|s| s.security.as_ref());
        Ok(security.map(|s| s.sessions_open()).unwrap_or_default())
    }

    /// How long sessions and the lease may be idle, and how long the lease's
    /// holder has been: none without security.
    #[property]
    async fn idle(&self) -> Result<Option<IdleTimes>, PropertyError> {
        let security = self.state.get().and_then(|s| s.security.as_ref());
        Ok(security.map(|s| s.idle_times()))
    }

    /// Adds an account.
    ///
    /// The first, on a server without, turns security on at the next
    /// start. Answers the accounts' names.
    #[action]
    async fn add(&self, account: NewAccount) -> Result<Vec<String>, ActionError> {
        self.on_accounts(move |accounts| {
            accounts.add(&account.name, &account.password)?;
            Ok(accounts.names())
        })
        .await
    }

    /// Removes an account, and ends its sessions.
    ///
    /// The last is refused while the next start listens on the network,
    /// which needs accounts; removed, it turns security off at the next
    /// start. Answers the accounts' names.
    #[action]
    async fn remove(&self, name: String) -> Result<Vec<String>, ActionError> {
        let state = self.state()?;
        let last = state.accounts.names() == [name.clone()];
        if last && self.next_on_network()? {
            return Err(ActionError::handled(format!(
                "conflict (409): {name} is the last account, and the next start listens on the \
                 network, which needs one: listen on this computer alone first"
            )));
        }
        let gone = name.clone();
        let names = self
            .on_accounts(move |accounts| {
                accounts.remove(&gone)?;
                Ok(accounts.names())
            })
            .await?;
        if let Some(security) = &state.security {
            security.end_sessions_of(&name);
        }
        Ok(names)
    }

    /// Changes one's own password, with the old one.
    ///
    /// Needs a login, and not the lease. Refused if `old` isn't the
    /// password, or `password` is shorter than 8 characters.
    #[action]
    async fn change_password(
        &self,
        ctx: ActionCtx,
        old: String,
        password: String,
    ) -> Result<(), ActionError> {
        let name = ctx.principal().id().map(str::to_owned);
        let name = name.filter(|n| self.state().is_ok_and(|s| s.accounts.names().contains(n)));
        let name = name.ok_or_else(|| {
            ActionError::handled("refused (401): log in as the account whose password it is")
        })?;
        self.on_accounts(move |accounts| {
            if !accounts.verify(&name, &old) {
                return Err(AuthError::Refused("the old password isn't right".into()));
            }
            accounts.passwd(&name, &password)
        })
        .await
    }
}
