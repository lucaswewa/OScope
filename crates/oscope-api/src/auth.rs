//! Accounts, sessions and the control lease (P49, ADR-0031; plan §5.18).
//!
//! - **Accounts:** names, and argon2id hashes of their passwords, in
//!   `{data}/accounts.json`, readable only by the server's account. All
//!   have equal rights in M1.
//! - **Sessions:** a bearer token each, from logging in. Logging out ends
//!   one, and so does going unused for longer than the idle time.
//! - **The local token** (ADR-0015): the desktop app's, new at each start.
//! - **The control lease:** one principal at a time holds it, and it is
//!   needed for every request that changes state, but the `auth` Thing's.
//!   It goes when its holder is idle too long, and the local desktop app
//!   can always take it.
//!
//! The server turns this on by being given [`Security`]; without it, as in
//! the examples, everyone is anonymous and may do anything.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError};
use std::time::{Duration, Instant};

use argon2::Argon2;
use argon2::password_hash::phc::PasswordHash;
use argon2::password_hash::{PasswordHasher, PasswordVerifier};
use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use teta_wot::http::axum::Json;
use teta_wot::http::axum::body::{Body, to_bytes};
use teta_wot::http::axum::http::request::Parts;
use teta_wot::http::axum::http::{Method, StatusCode, header};
use teta_wot::http::axum::response::{IntoResponse, Response};
use teta_wot::prelude::*;
use teta_wot::server::{Access, Authenticator, Authorizer, Rejection};
use teta_wot::{BoxFuture, Principal};

use crate::API_PREFIX;

/// The local desktop app's principal: the holder of the local token.
pub const LOCAL: &str = "local";

/// Why an account can't be kept or changed.
#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    /// A name or password that isn't allowed, or an account that is or
    /// isn't there.
    #[error("{0}")]
    Refused(String),
    /// The accounts' file.
    #[error("{path}: {source}")]
    Io {
        /// The file.
        path: PathBuf,
        /// Why.
        source: std::io::Error,
    },
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// A token's key: its SHA-256, so the tokens themselves aren't kept.
fn key(token: &str) -> String {
    Sha256::digest(token.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// 32 random bytes from an OS-seeded generator, in hex: a token.
fn token() -> String {
    let bytes: [u8; 32] = rand::random();
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// An account: a name, its password's hash, and when it was made.
#[derive(Clone, Debug, Serialize, Deserialize)]
struct Account {
    name: String,
    hash: String,
    created: DateTime<Utc>,
}

#[derive(Serialize, Deserialize)]
struct AccountsFile {
    format: u32,
    accounts: Vec<Account>,
}

/// The accounts, in their file.
pub struct Accounts {
    path: PathBuf,
    list: Mutex<Vec<Account>>,
    /// A hash to verify unknown names against, so they take as long as
    /// known ones.
    decoy: OnceLock<String>,
}

impl std::fmt::Debug for Accounts {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list().entries(self.names()).finish()
    }
}

/// `name`, if an account may have it: 1 to 64 of `a`–`z`, `0`–`9`, `.`,
/// `_` and `-`, starting with a letter, and not `local`.
fn checked(name: &str) -> Result<(), AuthError> {
    let allowed = |c: char| c.is_ascii_lowercase() || c.is_ascii_digit() || ".-_".contains(c);
    let fine = (1..=64).contains(&name.len())
        && name.starts_with(|c: char| c.is_ascii_lowercase())
        && name.chars().all(allowed)
        && name != LOCAL;
    match fine {
        true => Ok(()),
        false => Err(AuthError::Refused(format!(
            "an account's name has 1 to 64 of a–z, 0–9, '.', '_' and '-', starts with a letter, and isn't {LOCAL}: {name:?}"
        ))),
    }
}

fn hashed(password: &str) -> Result<String, AuthError> {
    let chars = password.chars().count();
    if !(8..=1024).contains(&chars) {
        return Err(AuthError::Refused(
            "a password has 8 to 1024 characters".to_owned(),
        ));
    }
    let hash = Argon2::default().hash_password(password.as_bytes());
    let hash = hash.map_err(|e| AuthError::Refused(format!("hashing the password: {e}")))?;
    Ok(hash.to_string())
}

impl Accounts {
    /// The accounts in the file at `path`: none if it isn't there.
    pub fn open(path: &Path) -> Result<Accounts, AuthError> {
        let io = |source| AuthError::Io {
            path: path.to_owned(),
            source,
        };
        let list = match std::fs::read(path) {
            Ok(bytes) => {
                let file: AccountsFile = serde_json::from_slice(&bytes)
                    .map_err(|e| io(std::io::Error::new(std::io::ErrorKind::InvalidData, e)))?;
                file.accounts
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(error) => return Err(io(error)),
        };
        Ok(Accounts {
            path: path.to_owned(),
            list: Mutex::new(list),
            decoy: OnceLock::new(),
        })
    }

    /// Their names.
    pub fn names(&self) -> Vec<String> {
        lock(&self.list).iter().map(|a| a.name.clone()).collect()
    }

    /// Whether there are none.
    pub fn is_empty(&self) -> bool {
        lock(&self.list).is_empty()
    }

    /// Writes `list` to the file, readable only by this account: a new
    /// file, then renamed over the old.
    fn save(&self, list: &[Account]) -> Result<(), AuthError> {
        let file = AccountsFile {
            format: 1,
            accounts: list.to_vec(),
        };
        let text = serde_json::to_vec_pretty(&file).unwrap_or_default();
        let new = self.path.with_extension("json.new");
        let io = |source| AuthError::Io {
            path: self.path.clone(),
            source,
        };
        if let Some(folder) = self.path.parent() {
            std::fs::create_dir_all(folder).map_err(io)?;
        }
        oscope_shm::write_private(&new, &text).map_err(io)?;
        std::fs::rename(&new, &self.path).map_err(io)
    }

    /// Adds account `name`, with `password`: 8 characters at least.
    pub fn add(&self, name: &str, password: &str) -> Result<(), AuthError> {
        checked(name)?;
        let hash = hashed(password)?;
        let mut list = lock(&self.list);
        if list.iter().any(|a| a.name == name) {
            return Err(AuthError::Refused(format!("there is an account {name}")));
        }
        let mut next = list.clone();
        next.push(Account {
            name: name.to_owned(),
            hash,
            created: Utc::now(),
        });
        self.save(&next)?;
        *list = next;
        Ok(())
    }

    /// Changes account `name`'s password.
    pub fn passwd(&self, name: &str, password: &str) -> Result<(), AuthError> {
        let hash = hashed(password)?;
        let mut list = lock(&self.list);
        let mut next = list.clone();
        let account = next.iter_mut().find(|a| a.name == name);
        let account = account.ok_or_else(|| AuthError::Refused(format!("no account {name}")))?;
        account.hash = hash;
        self.save(&next)?;
        *list = next;
        Ok(())
    }

    /// Removes account `name`.
    pub fn remove(&self, name: &str) -> Result<(), AuthError> {
        let mut list = lock(&self.list);
        let next: Vec<Account> = list.iter().filter(|a| a.name != name).cloned().collect();
        if next.len() == list.len() {
            return Err(AuthError::Refused(format!("no account {name}")));
        }
        self.save(&next)?;
        *list = next;
        Ok(())
    }

    /// Whether `password` is account `name`'s. An unknown name is checked
    /// against a decoy, so it takes as long.
    pub fn verify(&self, name: &str, password: &str) -> bool {
        let found = lock(&self.list)
            .iter()
            .find(|a| a.name == name)
            .map(|a| a.hash.clone());
        let decoy = || {
            let made = Argon2::default().hash_password(b"a decoy password");
            made.map(|h| h.to_string()).unwrap_or_default()
        };
        let hash = found
            .clone()
            .unwrap_or_else(|| self.decoy.get_or_init(decoy).clone());
        let Ok(parsed) = PasswordHash::new(&hash) else {
            return false;
        };
        let matches = Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok();
        matches && found.is_some()
    }
}

/// Whose a session is, and when it was last used.
struct Session {
    account: String,
    last: Instant,
}

/// The sessions open, by their tokens' keys.
struct Sessions {
    idle: Duration,
    open: Mutex<HashMap<String, Session>>,
}

impl Sessions {
    /// A new session for `account`: its token.
    fn start(&self, account: &str) -> String {
        let made = token();
        let session = Session {
            account: account.to_owned(),
            last: Instant::now(),
        };
        lock(&self.open).insert(key(&made), session);
        made
    }

    /// The account of the session whose token's key is `key`, which is
    /// used now; none if there is none, or it has been idle too long.
    fn check(&self, key: &str) -> Option<String> {
        let mut open = lock(&self.open);
        let now = Instant::now();
        open.retain(|_, s| now.duration_since(s.last) <= self.idle);
        let session = open.get_mut(key)?;
        session.last = now;
        Some(session.account.clone())
    }

    fn end(&self, key: &str) -> bool {
        lock(&self.open).remove(key).is_some()
    }
}

/// Who holds control, and who asked for it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LeaseState {
    /// Its holder: an account, or `local`, the desktop app.
    pub holder: Option<String>,
    /// Since when.
    pub since: Option<DateTime<Utc>>,
    /// Who asked for it since, oldest first.
    pub requests: Vec<String>,
    /// How long its holder may be idle before it goes, in seconds.
    pub idle_s: f64,
}

struct Held {
    holder: Option<String>,
    since: Option<DateTime<Utc>>,
    last: Instant,
    requests: Vec<String>,
}

/// The control lease.
struct Lease {
    idle: Duration,
    held: Mutex<Held>,
}

impl Lease {
    /// The lease, its holder gone if idle too long.
    fn current(&self) -> MutexGuard<'_, Held> {
        let mut held = lock(&self.held);
        if held.holder.is_some() && held.last.elapsed() > self.idle {
            held.holder = None;
            held.since = None;
        }
        held
    }

    fn state(&self) -> LeaseState {
        let held = self.current();
        LeaseState {
            holder: held.holder.clone(),
            since: held.since,
            requests: held.requests.clone(),
            idle_s: self.idle.as_secs_f64(),
        }
    }

    /// Whether `who` holds it; if so, they are active now.
    fn holds(&self, who: &str) -> bool {
        let mut held = self.current();
        let holds = held.holder.as_deref() == Some(who);
        if holds {
            held.last = Instant::now();
        }
        holds
    }

    fn give(held: &mut Held, to: &str) {
        held.holder = Some(to.to_owned());
        held.since = Some(Utc::now());
        held.last = Instant::now();
        held.requests.retain(|r| r != to);
    }

    /// `who` takes it if it is free, or already theirs.
    fn acquire(&self, who: &str) -> Result<(), String> {
        let mut held = self.current();
        match held.holder.clone() {
            Some(holder) if holder != who => Err(format!(
                "{holder} has control: ask for it with request_control"
            )),
            _ => {
                Self::give(&mut held, who);
                Ok(())
            }
        }
    }

    fn release(&self, who: &str) -> Result<(), String> {
        let mut held = self.current();
        if held.holder.as_deref() != Some(who) {
            return Err(format!("{who} doesn't have control"));
        }
        held.holder = None;
        held.since = None;
        Ok(())
    }

    /// `who` asks for it: its holder sees the request, or if it is free,
    /// `who` has it.
    fn request(&self, who: &str) {
        let mut held = self.current();
        match held.holder.as_deref() {
            None => Self::give(&mut held, who),
            Some(holder) if holder == who => {}
            Some(_) => {
                if !held.requests.iter().any(|r| r == who) {
                    held.requests.push(who.to_owned());
                }
            }
        }
    }

    fn hand_over(&self, who: &str, to: &str) -> Result<(), String> {
        let mut held = self.current();
        if held.holder.as_deref() != Some(who) {
            return Err(format!("{who} doesn't have control to hand over"));
        }
        Self::give(&mut held, to);
        Ok(())
    }

    /// The local desktop app takes it, whoever has it.
    fn take(&self, who: &str) {
        Self::give(&mut self.current(), who);
    }
}

/// How long sessions and the lease last when idle.
#[derive(Clone, Copy, Debug)]
pub struct Idle {
    /// A session: 8 hours by default.
    pub session: Duration,
    /// The lease: 10 minutes by default.
    pub lease: Duration,
}

impl Default for Idle {
    fn default() -> Self {
        Idle {
            session: Duration::from_secs(8 * 3600),
            lease: Duration::from_secs(600),
        }
    }
}

/// Security: the accounts, their sessions, the local token and the lease.
pub struct Security {
    accounts: Arc<Accounts>,
    sessions: Sessions,
    local: Option<String>,
    lease: Lease,
}

impl Security {
    /// Security over `accounts`, the desktop app's `local` token if it has
    /// one, idle for at most `idle`.
    pub fn new(accounts: Arc<Accounts>, local: Option<String>, idle: Idle) -> Security {
        Security {
            accounts,
            sessions: Sessions {
                idle: idle.session,
                open: Mutex::new(HashMap::new()),
            },
            local: local.map(|t| key(&t)),
            lease: Lease {
                idle: idle.lease,
                held: Mutex::new(Held {
                    holder: None,
                    since: None,
                    last: Instant::now(),
                    requests: Vec::new(),
                }),
            },
        }
    }

    /// The principal a request's `Authorization` header names, if it names
    /// one: the desktop app, or a session's account.
    fn principal(&self, header: Option<&str>) -> Option<Principal> {
        let token = header?.strip_prefix("Bearer ")?.trim();
        let key = key(token);
        if self.local.as_deref() == Some(key.as_str()) {
            return Some(Principal::new(LOCAL).with_attribute("session", key));
        }
        let account = self.sessions.check(&key)?;
        Some(Principal::new(account).with_attribute("session", key))
    }
}

/// The authenticator: a bearer token, a session's or the local one, on
/// every request to the API but logging in.
pub(crate) struct Guard(pub(crate) Arc<Security>);

impl Authenticator for Guard {
    fn authenticate<'a>(
        &'a self,
        request: &'a Parts,
    ) -> BoxFuture<'a, Result<Principal, Rejection>> {
        Box::pin(async move {
            // Logging in, and what isn't the API, pages and `/health`, are
            // open: a browser can't send a token for a page.
            let path = request.uri.path();
            let login = format!("{API_PREFIX}/auth/login");
            if (request.method == Method::POST && path == login) || !path.starts_with("/api/") {
                return Ok(Principal::anonymous());
            }
            let header = request.headers.get(header::AUTHORIZATION);
            let header = header.and_then(|v| v.to_str().ok());
            let challenge = "Bearer realm=\"oscope\"".to_owned();
            self.0
                .principal(header)
                .ok_or_else(|| Rejection::unauthorized(Some(challenge)))
        })
    }
}

/// The authoriser: anyone may read and watch; changing anything needs the
/// lease, but the `auth` Thing's own affordances.
pub(crate) struct LeaseCheck(pub(crate) Arc<Security>);

impl Authorizer for LeaseCheck {
    fn authorize(&self, principal: &Principal, access: &Access<'_>) -> bool {
        // One's own display preferences aren't the instrument's state (P52).
        let own = (access.thing, access.affordance) == (Some("settings"), Some("display"));
        if !access.changes_state || access.thing == Some("auth") || own {
            return true;
        }
        principal.id().is_some_and(|who| self.0.lease.holds(who))
    }
}

/// The `auth` Thing: logging in and out, who is logged in, and the lease.
#[derive(Thing)]
#[thing(
    title = "Authentication",
    semantic_type = "saref:Function",
    context(saref = "https://saref.etsi.org/core/")
)]
pub struct AuthThing {
    /// The lease changed hands, or someone asked for it.
    ///
    /// Its state now.
    #[event]
    lease_changed: Event<LeaseState>,
    security: OnceLock<Arc<Security>>,
}

/// Who made a request: their name, from the principal.
fn caller(ctx: &ActionCtx) -> Result<String, ActionError> {
    let who = ctx.principal().id().map(str::to_owned);
    who.ok_or_else(|| ActionError::handled("refused (401): log in first"))
}

impl AuthThing {
    fn security(&self) -> Result<&Arc<Security>, ActionError> {
        self.security
            .get()
            .ok_or_else(|| ActionError::handled("unavailable (503): no security"))
    }

    /// Says the lease changed, and gives `done`'s refusal as a 409.
    fn changed(&self, done: Result<(), String>) -> Result<LeaseState, ActionError> {
        done.map_err(|why| ActionError::handled(format!("conflict (409): {why}")))?;
        let state = self.security()?.lease.state();
        self.lease_changed.emit(state.clone());
        Ok(state)
    }
}

/// What logging in takes.
#[derive(Deserialize)]
struct Login {
    name: String,
    password: String,
}

#[thing_impl]
impl AuthThing {
    #[on_start]
    async fn connect(&self, ctx: ThingCtx) -> anyhow::Result<()> {
        let security = ctx.server().service::<Security>();
        let security = security.ok_or_else(|| anyhow::anyhow!("the server has no security"))?;
        let _ = self.security.set(security);
        Ok(())
    }

    /// Logs in: a session's token.
    ///
    /// `{"name", "password"}`, answered with `{"token", "name"}`, or 401.
    /// Send the token as `Authorization: Bearer …`, and on a WebSocket in
    /// its first message.
    #[endpoint(post, "login")]
    async fn login(&self, body: Body) -> Response {
        let Some(security) = self.security.get().cloned() else {
            return StatusCode::SERVICE_UNAVAILABLE.into_response();
        };
        let bytes = to_bytes(body, 64 * 1024).await.unwrap_or_default();
        let Ok(login) = serde_json::from_slice::<Login>(&bytes) else {
            let detail =
                json!({"title": "refused", "status": 400, "detail": "{\"name\", \"password\"}"});
            return (StatusCode::BAD_REQUEST, Json(detail)).into_response();
        };
        let checking = Arc::clone(&security.accounts);
        let (name, password) = (login.name.clone(), login.password);
        let fine = tokio::task::spawn_blocking(move || checking.verify(&name, &password)).await;
        if !fine.unwrap_or(false) {
            let detail = json!({"title": "unauthorized", "status": 401, "detail": "no such name and password"});
            return (StatusCode::UNAUTHORIZED, Json(detail)).into_response();
        }
        let token = security.sessions.start(&login.name);
        Json(json!({"token": token, "name": login.name})).into_response()
    }

    /// Logs out: ends the caller's session, answered with 204.
    #[endpoint(post, "logout")]
    async fn logout(&self) -> Response {
        let principal = Principal::current();
        let key = principal.attribute("session").and_then(|v| v.as_str());
        match (key, self.security.get()) {
            (Some(key), Some(security)) => {
                security.sessions.end(key);
                StatusCode::NO_CONTENT.into_response()
            }
            _ => StatusCode::UNAUTHORIZED.into_response(),
        }
    }

    /// Who the caller is: `{"name", "control"}`.
    #[property]
    async fn me(&self) -> Result<serde_json::Value, PropertyError> {
        let principal = Principal::current();
        let name = principal.id().map(str::to_owned);
        let security = self.security.get();
        let control = match (&name, security) {
            (Some(name), Some(security)) => security.lease.state().holder.as_ref() == Some(name),
            _ => false,
        };
        Ok(json!({"name": name, "control": control}))
    }

    /// The control lease: who holds it, since when, and who asked for it.
    #[property]
    async fn lease(&self) -> Result<LeaseState, PropertyError> {
        let security = self.security.get();
        Ok(security.map(|s| s.lease.state()).unwrap_or_default())
    }

    /// The accounts' names, to hand control over to.
    #[property]
    async fn accounts(&self) -> Result<Vec<String>, PropertyError> {
        Ok(self
            .security
            .get()
            .map(|s| s.accounts.names())
            .unwrap_or_default())
    }

    /// Takes control, if no one has it.
    #[action]
    async fn acquire_control(&self, ctx: ActionCtx) -> Result<LeaseState, ActionError> {
        let who = caller(&ctx)?;
        self.changed(self.security()?.lease.acquire(&who))
    }

    /// Gives control up.
    #[action]
    async fn release_control(&self, ctx: ActionCtx) -> Result<LeaseState, ActionError> {
        let who = caller(&ctx)?;
        self.changed(self.security()?.lease.release(&who))
    }

    /// Asks for control: its holder sees the request; if no one holds
    /// it, the caller has it.
    #[action]
    async fn request_control(&self, ctx: ActionCtx) -> Result<LeaseState, ActionError> {
        let who = caller(&ctx)?;
        self.security()?.lease.request(&who);
        self.changed(Ok(()))
    }

    /// Hands control over to `to`, an account or `local`.
    #[action]
    async fn hand_over_control(
        &self,
        ctx: ActionCtx,
        to: String,
    ) -> Result<LeaseState, ActionError> {
        let who = caller(&ctx)?;
        let security = self.security()?;
        if to != LOCAL && !security.accounts.names().contains(&to) {
            return Err(ActionError::handled(format!(
                "refused (400): no account {to}"
            )));
        }
        self.changed(security.lease.hand_over(&who, &to))
    }

    /// Takes control, whoever has it: the local desktop app alone may.
    #[action]
    async fn take_control(&self, ctx: ActionCtx) -> Result<LeaseState, ActionError> {
        let who = caller(&ctx)?;
        if who != LOCAL {
            return Err(ActionError::handled(
                "forbidden (403): only the local desktop app takes control",
            ));
        }
        self.security()?.lease.take(&who);
        self.changed(Ok(()))
    }
}
