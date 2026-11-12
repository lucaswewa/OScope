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
/// [devices]                       # kept for P16, not read yet
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
    /// The devices, kept as they are until the device Things read them.
    pub devices: toml::Table,
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
