//! The configuration file, its versions (P52), and the `settings` Thing's
//! access to it.

use std::net::{IpAddr, Ipv4Addr};
use std::path::{Path, PathBuf};

use std::time::Duration;

use oscope_api::devices::DevicesConfig;
use oscope_api::logs;
use oscope_api::settings::{ConfigFile, Saved, System};
use oscope_ext::PythonRuntime;
use serde::Deserialize;
use serde_json::{Value, json};
use toml_edit::{DocumentMut, Item};

/// The port the server listens on unless told otherwise (plan D34).
pub const DEFAULT_PORT: u16 = 8210;

/// The configuration file's version, which this OScope writes, and the
/// newest it reads (P52). A file without one is version 1's.
pub const CONFIG_VERSION: i64 = 2;

/// The server's configuration, from a TOML file. Every key is optional:
///
/// ```toml
/// version = 2                     # 1 without it: P01–P51's, still read
///
/// [network]                       # [server] in version 1
/// address = "127.0.0.1"           # this computer only
/// port = 8210
/// origins = ["http://127.0.0.1:8280"]  # pages elsewhere that may call it: none by default
///
/// [storage]
/// data_root = ".oscope/data"      # relative to this file's folder
/// scratch_root = ".oscope/scratch"
///
/// [logging]
/// level = "info"                  # tracing's EnvFilter syntax
/// ring = 50000                    # records kept in memory
/// keep_days = 30                  # daily files kept, in data_root/logs
///
/// [devices]                       # each device's driver: see oscope_api::devices
/// camera = "sim"
///
/// [simulator]                     # [devices.simulator] in version 1
/// sample = "H&E tissue"
///
/// [jobs]                          # the job service (ADR-0016)
/// workers = 2                     # compute jobs at once
/// gpu_mib = 6000                  # their GPU memory between them; not counted when not given
///
/// [shm]                           # the desktop app's preview ring (ADR-0015)
/// enabled = true                  # by default only with --managed
/// name = "preview"                # by default; preview-{pid} with --managed
/// slots = 3
/// slot_bytes = 8388608            # the largest preview
///
/// [python]                        # Python extensions (ADR-0020)
/// uv = "C:/tools/uv.exe"          # found when not given: OSCOPE_UV, then PATH
/// sdk = "python/oscope-ext"       # the SDK's source; this source tree's by default
/// wheelhouse = "wheels"           # install from these wheels alone, offline
/// call_timeout_s = 120            # how long a node may run
/// idle_s = 300                    # how long an idle worker is kept
///
/// [security]                      # with accounts (oscope-server account add): P49
/// certificate = "tls/cert.pem"    # TLS; made in data_root/tls on the network if not given
/// key = "tls/key.pem"
/// session_idle_minutes = 480      # a session unused this long ends
/// lease_idle_minutes = 10         # control, its holder idle this long, goes
///
/// [power]                         # P51
/// dry_run = false                 # true: power actions only say what they'd do
/// dry_run_restart = false         # the service's restart apart: dry_run's when not given (W4.3)
///
/// [web]                           # the web app (W0.1)
/// root = "web/dist"               # its built files, served at /; none when not given
/// notices = "notices"             # the third-party notices: where the package put them when not given
/// ```
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// The version the file was written in: none when it doesn't say, as
    /// in version 1; once loaded, the version it was read as.
    pub version: Option<i64>,
    /// Where the server listens: `[server]` in version 1.
    pub network: Listen,
    /// Where it keeps its files.
    pub storage: Storage,
    /// What it logs, and how much it keeps.
    pub logging: Logging,
    /// The devices: which driver each uses
    /// (`oscope_api::devices::DevicesConfig`).
    pub devices: toml::Table,
    /// The simulator's configuration, for the drivers that are `sim`:
    /// `[devices.simulator]` in version 1.
    pub simulator: toml::Table,
    /// The desktop app's preview ring.
    pub shm: Ring,
    /// The job service.
    pub jobs: Jobs,
    /// Python extensions.
    pub python: PythonConfig,
    /// Accounts' sessions, the control lease and TLS (P49).
    pub security: SecurityConfig,
    /// The power actions (P51).
    pub power: PowerSection,
    /// The web app's files (W0.1).
    pub web: Web,
}

/// The web app, served as the API's pages (W0.1, ADR-0006).
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Web {
    /// The folder of its built files, `web/dist`, served at `/` beside the
    /// API, so that both share an origin. Without it, no app is served.
    pub root: Option<PathBuf>,
    /// The folder of the server's third-party notices, for About (W4.3).
    /// Without it, where the packages put them: beside the server, or in
    /// `/usr/share/doc/oscope` (P54).
    pub notices: Option<PathBuf>,
}

/// The power actions (P51, ADR-0032).
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PowerSection {
    /// Only say what restarting, rebooting or shutting down would do: false
    /// by default.
    pub dry_run: bool,
    /// Whether restarting the service only says what it would do, apart
    /// from the host's actions: as `dry_run` when not given (W4.3,
    /// ADR-0060). The app's tests restart their own server, and reboot
    /// nothing.
    pub dry_run_restart: Option<bool>,
}

/// Sessions, the control lease and TLS (P49, ADR-0031). The files are
/// relative to the configuration file's folder.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SecurityConfig {
    /// The TLS certificate chain, PEM. When it isn't given and the server
    /// listens on the network, one is made, and kept in `data_root/tls`.
    pub certificate: Option<PathBuf>,
    /// Its private key, PEM.
    pub key: Option<PathBuf>,
    /// How long a session lasts unused, in minutes: 480.
    pub session_idle_minutes: u64,
    /// How long the control lease lasts while its holder is idle, in
    /// minutes: 10.
    pub lease_idle_minutes: u64,
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            certificate: None,
            key: None,
            session_idle_minutes: 480,
            lease_idle_minutes: 10,
        }
    }
}

/// Python extensions (ADR-0020). Folders are relative to the file's folder.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PythonConfig {
    /// uv: when not given, `OSCOPE_UV`, then `PATH`, then where its
    /// installers put it.
    pub uv: Option<PathBuf>,
    /// The SDK's source folder: this source tree's `python/oscope-ext` when
    /// not given.
    pub sdk: Option<PathBuf>,
    /// A folder of wheels to install from, offline, instead of the package
    /// index.
    pub wheelhouse: Option<PathBuf>,
    /// How long a node may run, in seconds.
    pub call_timeout_s: f64,
    /// How long an idle worker is kept, in seconds.
    pub idle_s: f64,
}

impl Default for PythonConfig {
    fn default() -> Self {
        let defaults = PythonRuntime::default();
        Self {
            uv: None,
            sdk: None,
            wheelhouse: None,
            call_timeout_s: defaults.call_timeout.as_secs_f64(),
            idle_s: defaults.idle.as_secs_f64(),
        }
    }
}

impl PythonConfig {
    /// How the server runs Python extensions.
    pub fn runtime(&self) -> PythonRuntime {
        let defaults = PythonRuntime::default();
        let seconds = |s: f64| Duration::try_from_secs_f64(s.max(0.0)).unwrap_or(Duration::MAX);
        PythonRuntime {
            uv: self.uv.clone(),
            sdk: self.sdk.clone().unwrap_or(defaults.sdk),
            wheelhouse: self.wheelhouse.clone(),
            call_timeout: seconds(self.call_timeout_s),
            idle: seconds(self.idle_s),
            ..defaults
        }
    }
}

/// The job service (ADR-0016).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Jobs {
    /// How many compute jobs run at once.
    pub workers: usize,
    /// The GPU memory compute jobs may hold between them, in MiB: not given,
    /// and it isn't counted.
    pub gpu_mib: Option<u32>,
}

impl Default for Jobs {
    fn default() -> Self {
        let defaults = oscope_jobs::JobsConfig::default();
        Self {
            workers: defaults.workers,
            gpu_mib: defaults.gpu_mib,
        }
    }
}

/// The desktop app's preview ring, in shared memory (ADR-0015).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Ring {
    /// Whether to publish previews to it: when not given, only with
    /// `--managed`.
    pub enabled: Option<bool>,
    /// Its name: `preview` when not given, and `preview-{pid}` with
    /// `--managed`, so that two managed servers don't meet.
    pub name: Option<String>,
    /// How many previews it holds.
    pub slots: u32,
    /// The largest preview, in bytes.
    pub slot_bytes: u32,
}

impl Default for Ring {
    fn default() -> Self {
        let size = oscope_shm::RingConfig::default();
        Self {
            enabled: None,
            name: None,
            slots: size.slots,
            slot_bytes: size.slot_bytes,
        }
    }
}

/// Where the server listens.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Listen {
    /// The address: `127.0.0.1` by default, which only this computer can
    /// reach.
    pub address: IpAddr,
    /// The port: [`DEFAULT_PORT`] by default; 0 lets the system choose.
    pub port: u16,
    /// The origins, besides the server's own, whose pages a browser lets
    /// call it: the dev server's, or the desktop app's (ADR-0045). A
    /// request from any other origin is refused. None by default.
    pub origins: Vec<String>,
}

impl Default for Listen {
    fn default() -> Self {
        Self {
            address: IpAddr::V4(Ipv4Addr::LOCALHOST),
            port: DEFAULT_PORT,
            origins: Vec::new(),
        }
    }
}

/// Where the server keeps its files.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Storage {
    /// Projects and their data: `.oscope/data` by default.
    pub data_root: PathBuf,
    /// Fast scratch space, for frame logs and caches: `.oscope/scratch` by
    /// default.
    pub scratch_root: PathBuf,
}

impl Default for Storage {
    fn default() -> Self {
        Self {
            data_root: PathBuf::from(".oscope/data"),
            scratch_root: PathBuf::from(".oscope/scratch"),
        }
    }
}

/// What the server logs, and how much it keeps.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Logging {
    /// What to record, in `tracing`'s `EnvFilter` syntax: `info` by default,
    /// or for example `info,oscope_api=debug`.
    pub level: String,
    /// How many of the newest records to keep in memory, for queries and
    /// the live tail.
    pub ring: usize,
    /// How many days of daily files to keep, today's included.
    pub keep_days: u32,
}

impl Default for Logging {
    fn default() -> Self {
        Self {
            level: "info".to_owned(),
            ring: logs::DEFAULT_RING,
            keep_days: logs::DEFAULT_KEEP_DAYS,
        }
    }
}

/// A configuration file can't be read.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ConfigError {
    /// The file can't be read.
    #[error("can't read {}: {source}", path.display())]
    Read {
        /// The file.
        path: PathBuf,
        /// Why.
        source: std::io::Error,
    },
    /// The file isn't a valid configuration. The message says where.
    #[error("{}: {source}", path.display())]
    Invalid {
        /// The file.
        path: PathBuf,
        /// What is wrong, and where.
        source: toml::de::Error,
    },
    /// The file's version isn't one this OScope reads, or it can't be
    /// brought up to this one's.
    #[error("{}: {why}", path.display())]
    Version {
        /// The file.
        path: PathBuf,
        /// Why.
        why: String,
    },
}

impl Config {
    /// Reads a configuration file of any version this OScope reads, an
    /// older one [`migrate`]d as it is read. Relative folders in it are
    /// relative to the file's own folder, so that it means the same from
    /// anywhere.
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        let text = std::fs::read_to_string(path).map_err(|source| ConfigError::Read {
            path: path.to_owned(),
            source,
        })?;
        let invalid = |source| ConfigError::Invalid {
            path: path.to_owned(),
            source,
        };
        let wrong = |why| ConfigError::Version {
            path: path.to_owned(),
            why,
        };
        let table: toml::Table = toml::from_str(&text).map_err(invalid)?;
        let from = version(table.get("version")).map_err(wrong)?;
        let text = match from < CONFIG_VERSION {
            true => upgrade(&text, from).map_err(wrong)?,
            false => text,
        };
        let mut config: Self = toml::from_str(&text).map_err(invalid)?;
        if from == CONFIG_VERSION && config.devices.contains_key("simulator") {
            let why = "[devices.simulator] is version 1's: version 2 has [simulator]";
            return Err(wrong(why.to_owned()));
        }
        config.version = Some(from);
        let folder = path.parent().unwrap_or(Path::new(""));
        config.storage.data_root = folder.join(&config.storage.data_root);
        config.storage.scratch_root = folder.join(&config.storage.scratch_root);
        let (python, security) = (&mut config.python, &mut config.security);
        let paths = [&mut python.uv, &mut python.sdk, &mut python.wheelhouse];
        let tls = [&mut security.certificate, &mut security.key];
        let web = [&mut config.web.root, &mut config.web.notices];
        for path in paths.into_iter().chain(tls).chain(web).flatten() {
            *path = folder.join(&*path);
        }
        Ok(config)
    }

    /// The devices' configuration: `[devices]`, with `[simulator]`.
    pub fn devices_config(&self) -> Result<DevicesConfig, String> {
        let mut devices = self.devices.clone();
        if !self.simulator.is_empty() {
            let simulator = toml::Value::Table(self.simulator.clone());
            devices.insert("simulator".to_owned(), simulator);
        }
        let devices = toml::Value::Table(devices).try_into();
        devices.map_err(|e: toml::de::Error| e.to_string())
    }

    /// What the `settings` Thing sees of it.
    pub fn saved(&self) -> Result<Saved, String> {
        let system = System {
            data_root: self.storage.data_root.clone(),
            scratch_root: self.storage.scratch_root.clone(),
            address: self.network.address,
            port: self.network.port,
        };
        let simulator = self.devices_config()?.simulator;
        Ok(Saved { system, simulator })
    }
}

/// The version `item` says, or version 1 without one.
fn version(item: Option<&toml::Value>) -> Result<i64, String> {
    match item.map(toml::Value::as_integer) {
        None => Ok(1),
        Some(Some(v)) if (1..=CONFIG_VERSION).contains(&v) => Ok(v),
        Some(Some(v)) => Err(format!(
            "a configuration of version {v}: this OScope reads versions 1 to {CONFIG_VERSION}"
        )),
        Some(None) => Err("its version isn't a whole number".to_owned()),
    }
}

/// `text`, of version `from`, as [`CONFIG_VERSION`]'s: [`migrate`]d, with its
/// lines where they were, so that errors still say where.
fn upgrade(text: &str, from: i64) -> Result<String, String> {
    let parsed = text.parse::<DocumentMut>();
    let mut document = parsed.map_err(|e| format!("can't migrate it: {e}"))?;
    migrate(&mut document, from);
    Ok(document.to_string())
}

/// Brings `document`, of version `from`, up to [`CONFIG_VERSION`]'s in place, its
/// comments and order kept: what it changed. Version 2 (P52) has
/// `[network]` for version 1's `[server]`, and `[simulator]`, which isn't a
/// device, for its `[devices.simulator]`.
pub fn migrate(document: &mut DocumentMut, from: i64) -> Vec<String> {
    let mut changed = Vec::new();
    let root = document.as_table_mut();
    if from < 2 {
        if let Some(server) = root.remove("server") {
            root.insert("network", server);
            changed.push("[server] is now [network]".to_owned());
        }
        let devices = root.get_mut("devices").and_then(Item::as_table_like_mut);
        if let Some(simulator) = devices.and_then(|d| d.remove("simulator")) {
            root.insert("simulator", simulator);
            changed.push("[devices.simulator] is now [simulator]".to_owned());
        }
    }
    changed
}

/// The file `path`, with a suffix: `oscope.toml.bak`.
fn beside(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

/// Rewrites the file at `path` as [`CONFIG_VERSION`]'s, with `edit` made to it,
/// its comments and order kept. `check` sees the new file's configuration
/// before it takes the old one's place. The old one is kept as
/// `{name}.bak`; one of an older version also as `{name}.v{N}.bak`, if
/// there isn't one. What the migration changed.
fn rewrite(
    path: &Path,
    edit: impl FnOnce(&mut DocumentMut),
    check: impl FnOnce(&Config) -> Result<(), String>,
) -> Result<Vec<String>, String> {
    let from = Config::load(path).map_err(|e| e.to_string())?.version;
    let from = from.unwrap_or(CONFIG_VERSION);
    let failed = |path: &Path, e: std::io::Error| format!("{}: {e}", path.display());
    let text = std::fs::read_to_string(path).map_err(|e| failed(path, e))?;
    let mut document = text.parse::<DocumentMut>().map_err(|e| e.to_string())?;
    let changed = migrate(&mut document, from);
    document["version"] = toml_edit::value(CONFIG_VERSION);
    edit(&mut document);
    let new = beside(path, ".new");
    // The new file takes the old one's mode: whoever could change the old
    // one, such as the packaged service's operators through its group, still
    // can.
    let mode = std::fs::metadata(path)
        .map_err(|e| failed(path, e))?
        .permissions();
    let written = std::fs::write(&new, document.to_string())
        .and_then(|()| std::fs::set_permissions(&new, mode))
        .map_err(|e| failed(&new, e));
    let loaded = written.and_then(|()| Config::load(&new).map_err(|e| e.to_string()));
    if let Err(why) = loaded.and_then(|config| check(&config)) {
        let _ = std::fs::remove_file(&new);
        return Err(why);
    }
    let old = beside(path, &format!(".v{from}.bak"));
    if from < CONFIG_VERSION && !old.exists() {
        std::fs::copy(path, &old).map_err(|e| failed(&old, e))?;
    }
    let backup = beside(path, ".bak");
    std::fs::copy(path, &backup).map_err(|e| failed(&backup, e))?;
    std::fs::rename(&new, path).map_err(|e| failed(path, e))?;
    Ok(changed)
}

/// Rewrites the configuration file at `path` as [`CONFIG_VERSION`]'s, if it is an
/// older version's, keeping the old one as `{name}.v{N}.bak`: what changed.
pub fn migrate_file(path: &Path) -> Result<Vec<String>, String> {
    let old = Config::load(path).map_err(|e| e.to_string())?;
    if old.version == Some(CONFIG_VERSION) {
        return Ok(Vec::new());
    }
    let unversioned = |config: &Config| Config {
        version: None,
        ..config.clone()
    };
    let same = |new: &Config| match unversioned(new) == unversioned(&old) {
        true => Ok(()),
        false => Err("migrated, it would read differently: it is left as it was".to_owned()),
    };
    rewrite(path, |_| {}, same)
}

/// `value` as TOML's: none for null, which TOML hasn't.
fn toml_value(value: &Value) -> Option<toml_edit::Value> {
    Some(match value {
        Value::Null => return None,
        Value::Bool(b) => (*b).into(),
        Value::Number(n) => match n.as_i64() {
            Some(i) => i.into(),
            None => n.as_f64()?.into(),
        },
        Value::String(s) => s.as_str().into(),
        Value::Array(items) => {
            let items = items.iter().filter_map(toml_value);
            items.collect::<toml_edit::Array>().into()
        }
        Value::Object(map) => {
            let mut table = toml_edit::InlineTable::new();
            for (key, value) in map {
                if let Some(value) = toml_value(value) {
                    table.insert(key, value);
                }
            }
            table.into()
        }
    })
}

/// The configuration file a server serves with, which the `settings` Thing
/// reads and changes (P52). It writes only what changed, and checks that
/// the file then reads as asked before it replaces the old one.
pub struct TomlFile(pub PathBuf);

impl ConfigFile for TomlFile {
    fn read(&self) -> Result<Saved, String> {
        Config::load(&self.0).map_err(|e| e.to_string())?.saved()
    }

    fn write(&self, saved: &Saved) -> Result<(), String> {
        let now = self.read()?;
        // A relative folder is from the file's, as it is read.
        let folder = self.0.parent().unwrap_or(Path::new(""));
        let mut wanted = saved.clone();
        wanted.system.data_root = folder.join(&saved.system.data_root);
        wanted.system.scratch_root = folder.join(&saved.system.scratch_root);
        let (old, new) = (&now.system, &wanted.system);
        let text = |path: &Path| path.to_string_lossy().into_owned();
        let simulator = (json!(now.simulator), json!(saved.simulator));
        let edit = |document: &mut DocumentMut| {
            if old.address != new.address {
                document["network"]["address"] = toml_edit::value(new.address.to_string());
            }
            if old.port != new.port {
                document["network"]["port"] = toml_edit::value(i64::from(new.port));
            }
            let storage = &saved.system;
            if old.data_root != new.data_root {
                document["storage"]["data_root"] = toml_edit::value(text(&storage.data_root));
            }
            if old.scratch_root != new.scratch_root {
                let scratch = toml_edit::value(text(&storage.scratch_root));
                document["storage"]["scratch_root"] = scratch;
            }
            let (Value::Object(was), Value::Object(is)) = simulator else {
                return;
            };
            for (key, value) in is {
                if was.get(&key) != Some(&value) {
                    let value = toml_value(&value).map_or(Item::None, Item::Value);
                    document["simulator"][key.as_str()] = value;
                }
            }
        };
        let check = |config: &Config| match config.saved()? == wanted {
            true => Ok(()),
            false => Err("written, it would read differently: it is left as it was".to_owned()),
        };
        rewrite(&self.0, edit, check).map(drop)
    }
}
