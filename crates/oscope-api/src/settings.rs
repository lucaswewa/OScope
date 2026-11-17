//! The `settings` Thing (P52; plan §5.4): one place for the settings.
//!
//! - **The system's:** the data and scratch roots, and the network's
//!   address and port. They are the configuration file's, which the next
//!   start reads, so a change sets `restart_needed`.
//! - **The simulator's,** from the file too.
//! - **Display preferences:** defaults, and each account's own. Writing
//!   one's own needs no control lease: it isn't the instrument's state.
//! - **One document** of them all, which `import` takes.

use std::collections::BTreeMap;
use std::net::IpAddr;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

use oscope_sim::SimulatorConfig;
use schemars::JsonSchema;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use teta_wot::Principal;
use teta_wot::prelude::*;

use crate::auth::{LOCAL, Security};
use crate::devices::property_refused;

/// What a settings document says it is.
pub const FORMAT: &str = "oscope.settings";

/// The settings document's version, which this OScope writes and reads.
pub const FORMAT_VERSION: u64 = 1;

/// How things are shown: an account's own, or the defaults.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct Display {
    /// Light, dark, or as the computer is.
    pub theme: Theme,
    /// The unit of lengths and positions.
    pub units: Units,
    /// Whether images have a scale bar.
    pub scale_bar: bool,
    /// How one-channel images are coloured.
    pub colormap: Colormap,
}

impl Default for Display {
    fn default() -> Self {
        Self {
            theme: Theme::System,
            units: Units::Um,
            scale_bar: true,
            colormap: Colormap::Grey,
        }
    }
}

/// A colour theme.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    /// As the computer is.
    System,
    /// Light.
    Light,
    /// Dark.
    Dark,
}

/// A unit of length.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Units {
    /// Micrometres.
    Um,
    /// Millimetres.
    Mm,
}

/// A colour map, for one-channel images.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Colormap {
    /// Black to white.
    Grey,
    /// Viridis.
    Viridis,
    /// Magma.
    Magma,
    /// Inferno.
    Inferno,
}

/// The system's settings in the configuration file: what the server starts
/// with next.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct System {
    /// Projects and their data. A relative folder is from the file's.
    pub data_root: PathBuf,
    /// Fast scratch space, for frame logs and caches.
    pub scratch_root: PathBuf,
    /// The address to listen on: 127.0.0.1 is this computer only.
    pub address: IpAddr,
    /// The port: 0 lets the system choose.
    pub port: u16,
}

/// What a configuration file holds for the `settings` Thing.
#[derive(Clone, Debug, PartialEq)]
pub struct Saved {
    /// The system's settings.
    pub system: System,
    /// The simulator's.
    pub simulator: SimulatorConfig,
}

/// The configuration file the server started with, which the `settings`
/// Thing reads and changes: `oscope-server` has the one it serves with.
pub trait ConfigFile: Send + Sync + 'static {
    /// What it holds now, as the next start reads it.
    fn read(&self) -> Result<Saved, String>;
    /// Makes it hold `saved`. Only what differs is written, the rest of the
    /// file kept as it is, and the old file kept as a backup.
    fn write(&self, saved: &Saved) -> Result<(), String>;
}

/// The configuration file, as a server service.
pub(crate) struct SettingsFile(pub(crate) Arc<dyn ConfigFile>);

/// Why a change was refused: its HTTP status, and why.
struct Refusal(u16, String);

impl Refusal {
    fn kind(&self) -> &'static str {
        match self.0 {
            400 => "refused",
            409 => "conflict",
            503 => "unavailable",
            _ => "failed",
        }
    }

    fn property(self) -> PropertyError {
        property_refused(self.0, self.kind(), self.1)
    }

    fn action(self) -> ActionError {
        ActionError::handled(format!("{} ({}): {}", self.kind(), self.0, self.1))
    }
}

/// `value`, read as a `T`, or a 400 saying which part was wrong.
fn parse<T: DeserializeOwned>(part: &str, value: Value) -> Result<T, Refusal> {
    serde_json::from_value(value).map_err(|e| Refusal(400, format!("{part}: {e}")))
}

/// Refuses what the next start would: a root missing, the same folder for
/// both, or an address on the network without accounts.
fn check_system(system: &System, secure: bool) -> Result<(), Refusal> {
    let (data, scratch) = (&system.data_root, &system.scratch_root);
    if data.as_os_str().is_empty() || scratch.as_os_str().is_empty() {
        return Err(Refusal(
            400,
            "a data root and a scratch root are needed".into(),
        ));
    }
    if data == scratch {
        let why = "the data root and the scratch root must be different folders";
        return Err(Refusal(400, why.into()));
    }
    if !system.address.is_loopback() && !secure {
        let address = system.address;
        return Err(Refusal(
            400,
            format!(
                "listening on {address} needs accounts first: oscope-server account add <name>"
            ),
        ));
    }
    Ok(())
}

/// The caller's account: theirs, or the local one's.
fn caller() -> String {
    Principal::current().id().unwrap_or(LOCAL).to_owned()
}

/// What the `settings` Thing needs from the server.
struct State {
    file: Option<Arc<dyn ConfigFile>>,
    secure: bool,
}

/// Settings: the system's, the simulator's, and how things are shown.
#[derive(Thing)]
#[thing(
    title = "Settings",
    semantic_type = "saref:Function",
    context(saref = "https://saref.etsi.org/core/")
)]
pub struct SettingsThing {
    /// Display preferences, for accounts without their own.
    #[setting]
    display_defaults: Prop<Display>,
    /// Each account's own display preferences, by its name.
    #[setting(read_only)]
    accounts: Prop<BTreeMap<String, Display>>,
    /// Whether the configuration file changed since the start.
    ///
    /// A restart takes the change up.
    #[property(read_only)]
    restart_needed: Prop<bool>,
    state: OnceLock<State>,
}

impl SettingsThing {
    fn state(&self) -> Result<&State, Refusal> {
        self.state
            .get()
            .ok_or_else(|| Refusal(503, "not started".into()))
    }

    /// What the configuration file holds; none without one.
    fn saved(&self) -> Result<Option<Saved>, Refusal> {
        let file = self.state()?.file.as_ref();
        file.map(|f| f.read().map_err(|e| Refusal(500, e)))
            .transpose()
    }

    /// Writes the `system` and `simulator` settings given to the file, once
    /// each is checked as the next start would.
    fn change(&self, system: Option<System>, simulator: Option<Value>) -> Result<(), Refusal> {
        if system.is_none() && simulator.is_none() {
            return Ok(());
        }
        let state = self.state()?;
        let no_file = "the server has no configuration file: start it with --config";
        let file = state.file.as_ref().ok_or(Refusal(409, no_file.into()))?;
        let mut saved = file.read().map_err(|e| Refusal(500, e))?;
        if let Some(system) = system {
            check_system(&system, state.secure)?;
            saved.system = system;
        }
        if let Some(simulator) = simulator {
            let config: SimulatorConfig = parse("the simulator's settings", simulator)?;
            let checked = config.check();
            checked.map_err(|e| Refusal(400, format!("the simulator's settings: {e}")))?;
            saved.simulator = config;
        }
        file.write(&saved).map_err(|e| Refusal(500, e))?;
        let _ = self.restart_needed.set(true);
        Ok(())
    }

    /// Imports `document`: each part checked before any is changed.
    fn import_document(&self, document: Value) -> Result<Vec<String>, Refusal> {
        if document["format"] != FORMAT {
            let why = format!("not a settings document: its format isn't {FORMAT}");
            return Err(Refusal(400, why));
        }
        let version = &document["version"];
        if version
            .as_u64()
            .is_none_or(|v| v == 0 || v > FORMAT_VERSION)
        {
            let why = format!("a settings document of version {version}: this OScope reads 1");
            return Err(Refusal(400, why));
        }
        let part = |name: &str| document.get(name).filter(|v| !v.is_null()).cloned();
        let system: Option<System> = part("system").map(|v| parse("system", v)).transpose()?;
        let simulator = part("simulator");
        let defaults = part("display_defaults");
        let defaults: Option<Display> =
            defaults.map(|v| parse("display_defaults", v)).transpose()?;
        let accounts = part("accounts");
        let accounts: Option<BTreeMap<String, Display>> =
            accounts.map(|v| parse("accounts", v)).transpose()?;
        let mut imported: Vec<String> = Vec::new();
        let names = ["system", "simulator", "display_defaults", "accounts"];
        let present = [
            system.is_some(),
            simulator.is_some(),
            defaults.is_some(),
            accounts.is_some(),
        ];
        for (name, present) in names.into_iter().zip(present) {
            if present {
                imported.push(name.to_owned());
            }
        }
        self.change(system, simulator)?;
        let failed = |e: PropertyError| Refusal(500, e.to_string());
        if let Some(defaults) = defaults {
            self.display_defaults.set(defaults).map_err(failed)?;
        }
        if let Some(accounts) = accounts {
            self.accounts
                .update(|all| all.extend(accounts))
                .map_err(failed)?;
        }
        Ok(imported)
    }
}

#[thing_impl]
impl SettingsThing {
    #[on_start]
    async fn connect(&self, ctx: ThingCtx) -> anyhow::Result<()> {
        let server = ctx.server();
        let _ = self.state.set(State {
            file: server.service::<SettingsFile>().map(|f| Arc::clone(&f.0)),
            secure: server.service::<Security>().is_some(),
        });
        Ok(())
    }

    /// The system's settings in the configuration file.
    ///
    /// What the next start reads: the data and scratch roots, the address
    /// and the port. Null when the server has no file.
    #[property]
    async fn system(&self) -> Result<Option<System>, PropertyError> {
        let saved = self.saved().map_err(Refusal::property)?;
        Ok(saved.map(|s| s.system))
    }

    #[setter(system)]
    async fn set_system(&self, system: Option<System>) -> Result<(), PropertyError> {
        let system = system.ok_or_else(|| Refusal(400, "system: not null".into()).property())?;
        self.change(Some(system), None).map_err(Refusal::property)
    }

    /// The simulator's settings in the configuration file.
    ///
    /// What the next start reads, whole: a key left out is its default.
    /// Null when the server has no file.
    #[property]
    async fn simulator(&self) -> Result<Option<Value>, PropertyError> {
        let saved = self.saved().map_err(Refusal::property)?;
        Ok(saved.map(|s| json!(s.simulator)))
    }

    #[setter(simulator)]
    async fn set_simulator(&self, simulator: Option<Value>) -> Result<(), PropertyError> {
        let why = || Refusal(400, "the simulator's settings: not null".into()).property();
        let simulator = simulator.ok_or_else(why)?;
        self.change(None, Some(simulator))
            .map_err(Refusal::property)
    }

    /// Your display preferences: your own, or else the defaults.
    ///
    /// Writing them needs no control lease.
    #[property]
    async fn display(&self) -> Display {
        let name = caller();
        let own = self.accounts.read(|all| all.get(&name).cloned());
        own.unwrap_or_else(|| self.display_defaults.get())
    }

    #[setter(display)]
    async fn set_display(&self, display: Display) -> Result<(), PropertyError> {
        let name = caller();
        self.accounts.update(|all| {
            all.insert(name, display);
        })
    }

    #[resetter(display)]
    async fn reset_display(&self) -> Result<(), PropertyError> {
        let name = caller();
        self.accounts.update(|all| {
            all.remove(&name);
        })
    }

    /// All the settings, as one document, which `import` takes.
    #[property]
    async fn document(&self) -> Result<Value, PropertyError> {
        let saved = self.saved().map_err(Refusal::property)?;
        Ok(json!({
            "format": FORMAT,
            "version": FORMAT_VERSION,
            "system": saved.as_ref().map(|s| &s.system),
            "simulator": saved.as_ref().map(|s| &s.simulator),
            "display_defaults": self.display_defaults.get(),
            "accounts": self.accounts.get(),
        }))
    }

    /// Imports a settings document, as `document` gives one.
    ///
    /// Each part it has is checked before any is changed: what it imported.
    #[action]
    async fn import(&self, document: Value) -> Result<Vec<String>, ActionError> {
        self.import_document(document).map_err(Refusal::action)
    }
}
