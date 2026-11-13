//! `oscope-server`, the program that runs the microscope: its configuration,
//! command line, service mode and the sidecar contract.
//!
//! Layer L6 of ADR-0005. It reads its configuration ([`Config`]) and command
//! line ([`Cli`]), then serves OScope's [`Api`] until it is asked to stop.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

use clap::Parser;
use oscope_api::devices::{Devices, DevicesConfig};
use oscope_api::logs::{self, LogConfig, LogError, LogStore};
use oscope_api::{Api, ApiError, Roots, VERSION};
use serde_json::Value;

mod config;

pub use config::{Config, ConfigError, DEFAULT_PORT, Listen, Logging, Storage};

/// The command line.
#[derive(Debug, Clone, PartialEq, Eq, Parser)]
#[command(name = "oscope-server", bin_name = "oscope-server", version)]
#[command(about = "Serves the OScope microscope: its Things, media and pages.")]
pub struct Cli {
    /// The configuration file (TOML). Without it, the defaults.
    #[arg(long, value_name = "FILE")]
    pub config: Option<PathBuf>,
    /// The port to listen on, in place of the configuration's (0: any).
    #[arg(long)]
    pub port: Option<u16>,
    /// The data root, in place of the configuration's.
    #[arg(long, value_name = "FOLDER")]
    pub data_root: Option<PathBuf>,
    /// Print the OpenAPI document and exit.
    #[arg(long, conflicts_with = "print_td")]
    pub print_openapi: bool,
    /// Print each Thing's Thing Description, by name, and exit.
    #[arg(long)]
    pub print_td: bool,
}

impl Cli {
    /// The configuration: the file's, or the defaults, with the command
    /// line's overrides.
    pub fn config(&self) -> Result<Config, ConfigError> {
        let mut config = match &self.config {
            Some(path) => Config::load(path)?,
            None => Config::default(),
        };
        if let Some(port) = self.port {
            config.server.port = port;
        }
        if let Some(root) = &self.data_root {
            config.storage.data_root = root.clone();
        }
        Ok(config)
    }
}

/// Why the server couldn't run.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// The configuration is wrong.
    #[error(transparent)]
    Config(#[from] ConfigError),
    /// The server couldn't be built or failed.
    #[error(transparent)]
    Api(#[from] ApiError),
    /// The logs couldn't be started.
    #[error(transparent)]
    Log(#[from] LogError),
    /// A folder couldn't be created.
    #[error("can't create {}: {source}", path.display())]
    Folder {
        /// The folder.
        path: PathBuf,
        /// Why.
        source: std::io::Error,
    },
    /// The devices couldn't be built from the configuration's `[devices]`.
    #[error("the devices: {0}")]
    Devices(String),
    /// The server couldn't listen.
    #[error("can't listen on {address}: {source}")]
    Listen {
        /// The address.
        address: SocketAddr,
        /// Why.
        source: std::io::Error,
    },
}

impl Error {
    /// The exit code: 2 for a wrong configuration, as for a wrong command
    /// line; 1 otherwise.
    pub fn exit_code(&self) -> ExitCode {
        match self {
            Self::Config(_) | Self::Devices(_) | Self::Log(LogError::Filter { .. }) => {
                ExitCode::from(2)
            }
            _ => ExitCode::FAILURE,
        }
    }
}

/// Does what the command line asks: prints a document, or serves until the
/// process is asked to stop (Ctrl-C, or the service manager).
///
/// When it serves, it creates the data and scratch roots, and starts the
/// logs: on stderr, and in daily files in `{data_root}/logs`. Then it prints
/// one line on stdout: `oscope-server {version} serving on
/// http://{address}/`.
pub fn run(cli: &Cli) -> Result<(), Error> {
    let config = cli.config()?;
    let roots = Roots {
        data: config.storage.data_root.clone(),
        scratch: config.storage.scratch_root.clone(),
    };
    let address = SocketAddr::new(config.server.address, config.server.port);
    let runtime =
        tokio::runtime::Runtime::new().map_err(|source| Error::Listen { address, source })?;
    // The devices: imports' paths are from the configuration file's folder.
    let devices_config: DevicesConfig = toml::Value::Table(config.devices.clone())
        .try_into()
        .map_err(|e: toml::de::Error| Error::Devices(e.to_string()))?;
    let base = cli
        .config
        .as_deref()
        .and_then(Path::parent)
        .map_or_else(|| PathBuf::from("."), Path::to_owned);
    let devices = runtime
        .block_on(Devices::from_config(&devices_config, &base))
        .map_err(Error::Devices)?;
    let devices = Arc::new(devices);
    if cli.print_openapi || cli.print_td {
        let api = Api::builder(&roots).devices(devices).build()?;
        let base = format!("http://{address}/");
        match cli.print_openapi {
            true => print(&api.openapi()),
            false => print(&Value::Object(api.thing_descriptions(&base)?)),
        }
        return Ok(());
    }
    for path in [&roots.data, &roots.scratch] {
        std::fs::create_dir_all(path).map_err(|source| Error::Folder {
            path: path.clone(),
            source,
        })?;
    }
    let log_folder = roots.data.join("logs");
    let store = LogStore::new(&LogConfig {
        ring: config.logging.ring,
        folder: Some(log_folder.clone()),
        keep_days: config.logging.keep_days,
    })
    .map_err(|source| Error::Folder {
        path: log_folder,
        source,
    })?;
    logs::install(&store, &config.logging.level)?;
    let api = Api::builder(&roots)
        .logs(store)
        .settings(roots.data.join("settings"))
        .devices(devices)
        .build()?;
    runtime.block_on(async {
        let listener = tokio::net::TcpListener::bind(address)
            .await
            .map_err(|source| Error::Listen { address, source })?;
        let bound = listener.local_addr().unwrap_or(address);
        println!("oscope-server {VERSION} serving on http://{bound}/");
        Ok(api.serve(listener, oscope_api::shutdown_signal()).await?)
    })
}

/// Prints a JSON document, indented, on stdout.
fn print(document: &Value) {
    println!("{document:#}");
}

/// Parses the command line, as `main` does: for `--help`, `--version` and
/// wrong arguments, clap prints and exits (with 2 for wrong arguments).
pub fn main() -> ExitCode {
    match run(&Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("oscope-server: {error}");
            error.exit_code()
        }
    }
}
