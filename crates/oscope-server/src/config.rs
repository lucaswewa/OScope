//! The configuration file.

use std::net::{IpAddr, Ipv4Addr};
use std::path::{Path, PathBuf};

use oscope_api::logs;
use serde::Deserialize;

/// The port the server listens on unless told otherwise (plan D34).
pub const DEFAULT_PORT: u16 = 8210;

/// The server's configuration, from a TOML file. Every key is optional:
///
/// ```toml
/// [server]
/// address = "127.0.0.1"           # this computer only
/// port = 8210
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
/// [jobs]                          # the job service (ADR-0016)
/// workers = 2                     # compute jobs at once
/// gpu_mib = 6000                  # their GPU memory between them; not counted when not given
///
/// [shm]                           # the desktop app's preview ring (ADR-0015)
/// enabled = true                  # by default only with --managed
/// name = "preview"                # by default; preview-{pid} with --managed
/// slots = 3
/// slot_bytes = 8388608            # the largest preview
/// ```
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// Where the server listens.
    pub server: Listen,
    /// Where it keeps its files.
    pub storage: Storage,
    /// What it logs, and how much it keeps.
    pub logging: Logging,
    /// The devices: which driver each uses, and the simulator's
    /// configuration (`oscope_api::devices::DevicesConfig`).
    pub devices: toml::Table,
    /// The desktop app's preview ring.
    pub shm: Ring,
    /// The job service.
    pub jobs: Jobs,
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
}

impl Default for Listen {
    fn default() -> Self {
        Self {
            address: IpAddr::V4(Ipv4Addr::LOCALHOST),
            port: DEFAULT_PORT,
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
}

impl Config {
    /// Reads a configuration file. Relative folders in it are relative to
    /// the file's own folder, so that it means the same from anywhere.
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        let text = std::fs::read_to_string(path).map_err(|source| ConfigError::Read {
            path: path.to_owned(),
            source,
        })?;
        let mut config: Self = toml::from_str(&text).map_err(|source| ConfigError::Invalid {
            path: path.to_owned(),
            source,
        })?;
        let folder = path.parent().unwrap_or(Path::new(""));
        config.storage.data_root = folder.join(&config.storage.data_root);
        config.storage.scratch_root = folder.join(&config.storage.scratch_root);
        Ok(config)
    }
}
