//! `oscope-server`, the program that runs the microscope: its configuration,
//! command line, service mode and the sidecar contract.
//!
//! Layer L6 of ADR-0005. It reads its configuration ([`Config`]) and command
//! line ([`Cli`]), then serves OScope's [`Api`] until it is asked to stop:
//!
//! - **in a console,** until Ctrl-C;
//! - **managed** (`--managed`), under a supervisor such as the desktop app,
//!   by the sidecar contract (ADR-0015): a JSON ready line, stop when stdin
//!   closes, and exit code 75 to be restarted;
//! - **as a Windows service** (`service install`, `uninstall`, `run`), or
//!   under systemd (`packaging/linux/oscope.service`).
//!
//! `ext pack` makes an extension's package from a folder (ADR-0018).

use std::future::Future;
use std::io::Read;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::process::ExitCode;
use std::sync::Arc;

use clap::{Parser, Subcommand};
use oscope_api::devices::{Devices, DevicesConfig};
use oscope_api::logs::{self, LogConfig, LogError, LogStore};
use oscope_api::{Api, ApiError, Roots, VERSION};
use oscope_ext::{Drivers, ExtError, Extensions, OWN_DRIVERS};
use oscope_jobs::{JobService, JobsConfig};
use oscope_shm::{RingConfig, RingWriter, ShmError};
use serde_json::{Value, json};

mod config;

pub use config::{
    Config, ConfigError, DEFAULT_PORT, Jobs, Listen, Logging, PythonConfig, Ring, Storage,
};

/// The exit code that asks a supervisor to start the server again
/// (ADR-0015): `EX_TEMPFAIL`.
pub const RESTART: u8 = 75;

/// The Windows service's name, unless told otherwise.
pub const SERVICE: &str = "oscope";

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
    /// Run under a supervisor, such as the desktop app (ADR-0015): on any
    /// free port unless one is given, one JSON line on stdout when ready,
    /// stop when stdin closes, and exit with code 75 to be restarted.
    #[arg(long, conflicts_with_all = ["print_openapi", "print_td"])]
    pub managed: bool,
    /// Service mode, and extensions.
    #[command(subcommand)]
    pub command: Option<Command>,
}

/// What else the server can do.
#[derive(Debug, Clone, PartialEq, Eq, Subcommand)]
pub enum Command {
    /// Windows service mode: install, uninstall, and what the service
    /// manager runs. On Linux, systemd runs the server as it is
    /// (`packaging/linux/oscope.service`).
    #[command(subcommand)]
    Service(ServiceCommand),
    /// Extensions (ADR-0018).
    #[command(subcommand)]
    Ext(ExtCommand),
}

/// What the server can do with extensions without serving.
#[derive(Debug, Clone, PartialEq, Eq, Subcommand)]
pub enum ExtCommand {
    /// Packs a folder with a `manifest.toml` into an `.omext` package, the
    /// same bytes each time.
    Pack {
        /// The folder.
        folder: PathBuf,
        /// The package to write; by default `{id}-{version}.omext`, here.
        #[arg(long, value_name = "FILE")]
        out: Option<PathBuf>,
    },
}

/// Windows service mode.
#[derive(Debug, Clone, PartialEq, Eq, Subcommand)]
pub enum ServiceCommand {
    /// Registers the server as a service, with this command line's
    /// configuration and data root, restarted when it exits with code 75.
    /// Needs an administrator.
    Install {
        /// The service's name.
        #[arg(long, default_value = SERVICE)]
        name: String,
    },
    /// Stops the service and removes it. Needs an administrator.
    Uninstall {
        /// The service's name.
        #[arg(long, default_value = SERVICE)]
        name: String,
    },
    /// Runs as the service: what the service manager starts.
    Run {
        /// The service's name.
        #[arg(long, default_value = SERVICE)]
        name: String,
    },
}

impl Cli {
    /// The configuration: the file's, or the defaults, with the command
    /// line's overrides. Managed, the port is any free one unless given.
    pub fn config(&self) -> Result<Config, ConfigError> {
        let mut config = match &self.config {
            Some(path) => Config::load(path)?,
            None => Config::default(),
        };
        if self.managed {
            config.server.port = 0;
        }
        if let Some(port) = self.port {
            config.server.port = port;
        }
        if let Some(root) = &self.data_root {
            config.storage.data_root = root.clone();
        }
        Ok(config)
    }
}

/// How the server stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stopped {
    /// As it was asked to, or after a command that doesn't serve.
    Asked,
    /// For a restart: the `system` Thing's `restart` asked for one.
    Restart,
}

impl Stopped {
    /// Its exit code: 0, or [`RESTART`].
    pub fn exit_code(self) -> ExitCode {
        match self {
            Stopped::Asked => ExitCode::SUCCESS,
            Stopped::Restart => ExitCode::from(RESTART),
        }
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
    /// A folder or file couldn't be created.
    #[error("can't create {}: {source}", path.display())]
    Folder {
        /// The folder or file.
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
    /// The preview ring couldn't be made.
    #[error(transparent)]
    Ring(#[from] ShmError),
    /// Service mode failed, or isn't this platform's.
    #[error("service mode: {0}")]
    Service(String),
    /// The job service couldn't open its history.
    #[error("the job service: {0}")]
    Jobs(String),
    /// The extensions couldn't be read, or a package made.
    #[error("extensions: {0}")]
    Extensions(#[from] ExtError),
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

/// Does what the command line asks: prints a document, runs a service
/// command, or serves until the process is asked to stop (Ctrl-C, the
/// service manager, or, managed, stdin closing) or to restart.
///
/// When it serves, it creates the data and scratch roots, writes a new
/// local token, and starts the logs: on stderr, and in daily files in
/// `{data_root}/logs`. Then it prints one line on stdout: `oscope-server
/// {version} serving on http://{address}/`, or managed, the JSON ready line
/// (ADR-0015).
pub fn run(cli: &Cli) -> Result<Stopped, Error> {
    match &cli.command {
        Some(Command::Service(command)) => return service(cli, command),
        Some(Command::Ext(ExtCommand::Pack { folder, out })) => {
            return pack(folder, out.as_deref());
        }
        None => {}
    }
    let config = cli.config()?;
    let address = SocketAddr::new(config.server.address, config.server.port);
    let runtime =
        tokio::runtime::Runtime::new().map_err(|source| Error::Listen { address, source })?;
    if cli.print_openapi || cli.print_td {
        let roots = roots(&config);
        // No extension is loaded: a driver of one is printed as a fake.
        let devices = runtime.block_on(devices(cli, &config, None))?;
        // A history in memory: printing creates no files.
        let jobs = runtime.block_on(async { jobs(Path::new(":memory:"), &config) })?;
        // No extension is read either.
        let extensions = Extensions::empty(roots.data.join("extensions"));
        let api = Api::builder(&roots)
            .devices(devices)
            .jobs(jobs)
            .extensions(extensions)
            .build()?;
        let base = format!("http://{address}/");
        match cli.print_openapi {
            true => print(&api.openapi()),
            false => print(&Value::Object(api.thing_descriptions(&base)?)),
        }
        return Ok(Stopped::Asked);
    }
    runtime.block_on(serve(cli, config, oscope_api::shutdown_signal()))
}

fn roots(config: &Config) -> Roots {
    Roots {
        data: config.storage.data_root.clone(),
        scratch: config.storage.scratch_root.clone(),
    }
}

/// The devices the configuration's `[devices]` names, among them the
/// drivers of the extensions loaded; imports' paths are from the
/// configuration file's folder. Without the drivers, a driver that isn't
/// OScope's own is taken as a fake, for the printed documents.
async fn devices(
    cli: &Cli,
    config: &Config,
    drivers: Option<&Drivers>,
) -> Result<Arc<Devices>, Error> {
    let mut devices_config: DevicesConfig =
        toml::Value::Table(config.devices.clone())
            .try_into()
            .map_err(|e: toml::de::Error| Error::Devices(e.to_string()))?;
    let none = Drivers::default();
    let drivers = drivers.unwrap_or_else(|| {
        let c = &mut devices_config;
        for name in [&mut c.camera, &mut c.stage, &mut c.illumination] {
            if !OWN_DRIVERS.contains(&name.as_str()) {
                *name = "fake".to_owned();
            }
        }
        &none
    });
    let base = cli
        .config
        .as_deref()
        .and_then(Path::parent)
        .map_or_else(|| PathBuf::from("."), Path::to_owned);
    let devices = Devices::from_config_with(&devices_config, &base, drivers)
        .await
        .map_err(Error::Devices)?;
    Ok(Arc::new(devices))
}

/// Serves until `stop` completes, a restart is asked for, or, managed,
/// stdin closes.
async fn serve(
    cli: &Cli,
    config: Config,
    stop: impl Future<Output = ()> + Send + 'static,
) -> Result<Stopped, Error> {
    let roots = roots(&config);
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
    // Extensions first: the configuration may name their drivers.
    let python = Some(config.python.runtime());
    let extensions = Extensions::open_with(roots.data.join("extensions"), python)?;
    let devices = devices(cli, &config, Some(extensions.drivers())).await?;
    let jobs = jobs(&roots.data.join("index.sqlite"), &config)?;
    let token = write_token(&roots)?;
    let ring = ring(cli, &config)?;
    let ring_name = ring.as_ref().map(|r| r.name().to_owned());
    let mut api = Api::builder(&roots)
        .logs(store)
        .settings(roots.data.join("settings"))
        .devices(devices)
        .jobs(jobs)
        .extensions(extensions);
    if let Some(ring) = ring {
        api = api.ring(ring);
    }
    let api = api.build()?;
    let address = SocketAddr::new(config.server.address, config.server.port);
    let listener = tokio::net::TcpListener::bind(address)
        .await
        .map_err(|source| Error::Listen { address, source })?;
    let bound = listener.local_addr().unwrap_or(address);
    let closed: Pin<Box<dyn Future<Output = ()> + Send>> = match cli.managed {
        true => {
            let ready = json!({
                "ready": true,
                "version": VERSION,
                "port": bound.port(),
                "pid": std::process::id(),
                "token": token,
                "shm": ring_name,
            });
            println!("{ready}");
            Box::pin(stdin_closed())
        }
        false => {
            println!("oscope-server {VERSION} serving on http://{bound}/");
            Box::pin(std::future::pending())
        }
    };
    let restart = api.restart();
    let asked = Arc::clone(&restart);
    let shutdown = async move {
        tokio::select! {
            () = stop => {}
            () = asked.requested() => tracing::info!("restarting, as asked"),
            () = closed => tracing::info!("stdin closed: stopping, as the supervisor asks"),
        }
    };
    api.serve(listener, shutdown).await?;
    Ok(match restart.is_requested() {
        true => Stopped::Restart,
        false => Stopped::Asked,
    })
}

/// The job service, its history in `path`: jobs a stop cut short are marked
/// interrupted.
fn jobs(path: &Path, config: &Config) -> Result<JobService, Error> {
    let jobs = JobsConfig {
        workers: config.jobs.workers,
        gpu_mib: config.jobs.gpu_mib,
    };
    JobService::open(path, jobs).map_err(|e| Error::Jobs(e.to_string()))
}

/// Packs `folder` into `out`, or `{id}-{version}.omext` here, and prints
/// the package's path, size and SHA-256.
fn pack(folder: &Path, out: Option<&Path>) -> Result<Stopped, Error> {
    let out = match out {
        Some(out) => out.to_owned(),
        None => {
            let path = folder.join(oscope_ext::MANIFEST);
            let text =
                std::fs::read_to_string(&path).map_err(|source| ExtError::Io { path, source })?;
            let manifest = oscope_ext::Manifest::parse(&text)?;
            PathBuf::from(format!("{}-{}.omext", manifest.id, manifest.version))
        }
    };
    let manifest = oscope_ext::pack(folder, &out)?;
    let size = std::fs::metadata(&out).map_or(0, |m| m.len());
    let sha256 = oscope_ext::sha256(&out)?;
    println!(
        "packed {} {} into {}",
        manifest.id,
        manifest.version,
        out.display()
    );
    println!("{size} bytes, SHA-256 {sha256}");
    Ok(Stopped::Asked)
}

/// Completes when stdin closes: the supervisor's way to stop the server.
fn stdin_closed() -> impl Future<Output = ()> {
    let (closed, wait) = tokio::sync::oneshot::channel::<()>();
    std::thread::spawn(move || {
        let (mut stdin, mut buffer) = (std::io::stdin().lock(), [0; 256]);
        while matches!(stdin.read(&mut buffer), Ok(n) if n > 0) {}
        let _ = closed.send(());
    });
    async move {
        let _ = wait.await;
    }
}

/// Writes a new local token for the desktop app (ADR-0015): 32 random
/// bytes, in hex, in `{data_root}/run/local.token`, readable by this
/// account alone. Its absolute path.
fn write_token(roots: &Roots) -> Result<PathBuf, Error> {
    let path = roots.data.join("run").join("local.token");
    let failed = |source| Error::Folder {
        path: path.clone(),
        source,
    };
    std::fs::create_dir_all(roots.data.join("run")).map_err(failed)?;
    let token: [u8; 32] = rand::random();
    let hex: String = token.iter().map(|b| format!("{b:02x}")).collect();
    oscope_shm::write_private(&path, hex.as_bytes()).map_err(failed)?;
    std::path::absolute(&path).map_err(failed)
}

/// The preview ring, if the configuration's `[shm]` asks for one: by
/// default only when managed.
fn ring(cli: &Cli, config: &Config) -> Result<Option<RingWriter>, Error> {
    let shm = &config.shm;
    if !shm.enabled.unwrap_or(cli.managed) {
        return Ok(None);
    }
    let name = match (&shm.name, cli.managed) {
        (Some(name), _) => name.clone(),
        (None, true) => format!("preview-{}", std::process::id()),
        (None, false) => "preview".to_owned(),
    };
    let size = RingConfig {
        slots: shm.slots,
        slot_bytes: shm.slot_bytes,
    };
    Ok(Some(RingWriter::create(&name, size)?))
}

/// The service commands, on Windows.
#[cfg(windows)]
fn service(cli: &Cli, command: &ServiceCommand) -> Result<Stopped, Error> {
    use std::ffi::OsString;
    use std::time::Duration;

    use oscope_api::service;

    let failed = |e: &dyn std::fmt::Display| Error::Service(e.to_string());
    match command {
        ServiceCommand::Install { name } => {
            let config = cli.config()?;
            let absolute = |path: &Path| std::path::absolute(path).map_err(|e| failed(&e));
            let mut arguments: Vec<OsString> = Vec::new();
            if let Some(path) = &cli.config {
                arguments.extend(["--config".into(), absolute(path)?.into()]);
            }
            let data = absolute(&config.storage.data_root)?;
            arguments.extend(["--data-root".into(), data.into()]);
            if let Some(port) = cli.port {
                arguments.extend(["--port".into(), port.to_string().into()]);
            }
            arguments.extend(["service", "run", "--name"].map(OsString::from));
            arguments.push(name.into());
            let options = service::InstallOptions {
                name: name.clone(),
                display_name: format!("OScope ({name})"),
                description: "Serves the OScope microscope.".to_owned(),
                executable: std::env::current_exe().map_err(|e| failed(&e))?,
                arguments,
                automatic: false,
            };
            service::install(&options).map_err(|e| failed(&e))?;
            // Restarted when it fails, exit code 75 included: teta-wot's
            // install leaves recovery unset, so sc.exe sets it (ADR-0015).
            for arguments in [
                &[
                    "failure",
                    name,
                    "reset=",
                    "86400",
                    "actions=",
                    "restart/2000",
                ][..],
                &["failureflag", name, "1"],
            ] {
                let status = std::process::Command::new("sc.exe")
                    .args(arguments)
                    .output()
                    .map_err(|e| failed(&e))?;
                if !status.status.success() {
                    let said = String::from_utf8_lossy(&status.stdout).trim().to_owned();
                    return Err(Error::Service(format!("sc.exe {}: {said}", arguments[0])));
                }
            }
            println!("installed the service {name}");
            Ok(Stopped::Asked)
        }
        ServiceCommand::Uninstall { name } => {
            service::uninstall(name).map_err(|e| failed(&e))?;
            println!("removed the service {name}");
            Ok(Stopped::Asked)
        }
        ServiceCommand::Run { name } => {
            let (cli, config) = (cli.clone(), cli.config()?);
            let stop_hint = Duration::from_secs(30);
            service::run_as_service(name, stop_hint, move |_, stop| {
                Box::pin(async move {
                    match serve(&cli, config, stop).await {
                        Ok(stopped) => stopped.exit_code(),
                        Err(error) => {
                            tracing::error!("the service failed: {error}");
                            error.exit_code()
                        }
                    }
                })
            })
            .map_err(|e| failed(&e))?;
            Ok(Stopped::Asked)
        }
    }
}

/// The service commands, which are Windows': systemd runs the server as it
/// is.
#[cfg(not(windows))]
fn service(_: &Cli, _: &ServiceCommand) -> Result<Stopped, Error> {
    Err(Error::Service(
        "the service commands are for Windows; on Linux, systemd runs oscope-server \
         as it is (packaging/linux/oscope.service)"
            .to_owned(),
    ))
}

/// Prints a JSON document, indented, on stdout.
fn print(document: &Value) {
    println!("{document:#}");
}

/// Parses the command line, as `main` does: for `--help`, `--version` and
/// wrong arguments, clap prints and exits (with 2 for wrong arguments).
/// Exits with 0 when stopped as asked, 75 for a restart.
pub fn main() -> ExitCode {
    match run(&Cli::parse()) {
        Ok(stopped) => stopped.exit_code(),
        Err(error) => {
            eprintln!("oscope-server: {error}");
            error.exit_code()
        }
    }
}
