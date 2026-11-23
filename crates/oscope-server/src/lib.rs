//! Linux microscope server configuration, CLI and container lifecycle.
//! Docker supervises process restart; clients use HTTP/WebSocket transport.

use std::future::Future;
use std::net::{IpAddr, SocketAddr};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

use clap::{Parser, Subcommand};
use oscope_api::auth::{Accounts, AuthError, Idle, Security};
use oscope_api::devices::Devices;
use oscope_api::logs::{self, LogConfig, LogError, LogStore};
use oscope_api::origins::Origins;
use oscope_api::{Api, ApiError, Roots, VERSION};
use oscope_ext::{Drivers, ExtError, Extensions, OWN_DRIVERS};
use oscope_jobs::{JobService, JobsConfig};
use oscope_store::ScratchPolicy;
use serde_json::Value;

mod config;

pub use config::{
    CONFIG_VERSION, Config, ConfigError, DEFAULT_PORT, Jobs, Listen, Logging, PythonConfig, Ring,
    Storage, TomlFile, Web, migrate, migrate_file,
};

/// The exit code that asks a supervisor to start the server again
/// (ADR-0015): `EX_TEMPFAIL`.
pub const RESTART: u8 = 75;

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
    /// The web app's built files, in place of the configuration's: served
    /// at `/`, beside the API.
    #[arg(long, value_name = "FOLDER")]
    pub web: Option<PathBuf>,
    /// Print the OpenAPI document and exit.
    #[arg(long, conflicts_with = "print_td")]
    pub print_openapi: bool,
    /// Print each Thing's Thing Description, by name, and exit.
    #[arg(long)]
    pub print_td: bool,
    /// Account, extension and configuration operations.
    #[command(subcommand)]
    pub command: Option<Command>,
}

/// What else the server can do.
#[derive(Debug, Clone, PartialEq, Eq, Subcommand)]
pub enum Command {
    /// Extensions (ADR-0018).
    #[command(subcommand)]
    Ext(ExtCommand),
    /// Accounts, in the data root (P49). The password is asked for, or read
    /// from stdin's first line when stdin isn't a terminal.
    #[command(subcommand)]
    Account(AccountCommand),
    /// The configuration file (P52).
    #[command(subcommand)]
    Config(ConfigCommand),
}

/// What the server can do with its configuration file without serving.
#[derive(Debug, Clone, PartialEq, Eq, Subcommand)]
pub enum ConfigCommand {
    /// Rewrites `--config`'s file in the current version, if it is an older
    /// one's, keeping its comments, and the old file beside it as
    /// `{name}.v{N}.bak`.
    Migrate,
}

/// What the server can do with accounts without serving.
#[derive(Debug, Clone, PartialEq, Eq, Subcommand)]
pub enum AccountCommand {
    /// Adds an account.
    Add {
        /// Its name: 1 to 64 of a–z, 0–9, '.', '_' and '-'.
        name: String,
    },
    /// Removes an account.
    Remove {
        /// Its name.
        name: String,
    },
    /// Changes an account's password.
    Passwd {
        /// Its name.
        name: String,
    },
    /// Lists the accounts' names.
    List,
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

impl Cli {
    /// The configuration: the file's, or the defaults, with the command
    /// line's overrides. Managed, the port is any free one unless given.
    pub fn config(&self) -> Result<Config, ConfigError> {
        let mut config = match &self.config {
            Some(path) => Config::load(path)?,
            None => Config::default(),
        };
        if let Some(port) = self.port {
            config.network.port = port;
        }
        if let Some(root) = &self.data_root {
            config.storage.data_root = root.clone();
        }
        if let Some(folder) = &self.web {
            config.web.root = Some(folder.clone());
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
    /// `config migrate` was given no file.
    #[error("which configuration file? Give it with --config")]
    NoConfig,
    /// The configuration file can't be migrated.
    #[error("can't migrate the configuration: {0}")]
    Migrate(String),
    /// The server couldn't listen.
    #[error("can't listen on {address}: {source}")]
    Listen {
        /// The address.
        address: SocketAddr,
        /// Why.
        source: std::io::Error,
    },
    /// The job service couldn't open its history.
    #[error("the job service: {0}")]
    Jobs(String),
    /// The projects' index couldn't be opened.
    #[error("the projects: {0}")]
    Projects(String),
    /// The scratch space couldn't be opened.
    #[error("the scratch space: {0}")]
    Scratch(String),
    /// The extensions couldn't be read, or a package made.
    #[error("extensions: {0}")]
    Extensions(#[from] ExtError),
    /// An account couldn't be kept, changed or read (P49).
    #[error("accounts: {0}")]
    Accounts(#[from] AuthError),
    /// The server would listen on the network without an account, or TLS
    /// couldn't be set up (P49).
    #[error("{0}")]
    Security(String),
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
/// Linux shutdown signal) or to restart.
///
/// When it serves, it creates the data and scratch roots, writes a new
/// local token, and starts the logs: on stderr, and in daily files in
/// `{data_root}/logs`. Then it prints one line on stdout: `oscope-server
/// {version} serving on http://{address}/`.
pub fn run(cli: &Cli) -> Result<Stopped, Error> {
    match &cli.command {
        Some(Command::Ext(ExtCommand::Pack { folder, out })) => {
            return pack(folder, out.as_deref());
        }
        Some(Command::Config(ConfigCommand::Migrate)) => {
            let path = cli.config.as_deref().ok_or(Error::NoConfig)?;
            let changed = migrate_file(path).map_err(Error::Migrate)?;
            let done = match changed.is_empty() {
                true => format!("already version {CONFIG_VERSION}"),
                false => changed.join("; "),
            };
            println!("{}: {done}", path.display());
            return Ok(Stopped::Asked);
        }
        Some(Command::Account(command)) => {
            let accounts = Accounts::open(&cli.config()?.storage.data_root.join(ACCOUNTS))?;
            account(&accounts, command, &mut std::io::stdout(), password)?;
            return Ok(Stopped::Asked);
        }
        None => {}
    }
    let config = cli.config()?;
    if config.shm.enabled == Some(true) {
        return Err(Error::Devices(
            "preview rings are unavailable until the R07 preview port".into(),
        ));
    }
    let address = SocketAddr::new(config.network.address, config.network.port);
    let runtime =
        tokio::runtime::Runtime::new().map_err(|source| Error::Listen { address, source })?;
    if cli.print_openapi || cli.print_td {
        let roots = roots(&config);
        // No extension is loaded: a driver of one is printed as a fake.
        let devices = runtime.block_on(devices(cli, &config, None))?;
        // A history and an index in memory: printing creates no files.
        let jobs = runtime.block_on(async { jobs(Path::new(":memory:"), &config) })?;
        let projects = projects(&roots.data, Path::new(":memory:"))?;
        // No extension is read either.
        let extensions = Extensions::empty(roots.data.join("extensions"));
        // Security on, so that the `auth` Thing is described too: an app
        // calls it whenever the server has accounts (W1.3). Opening the
        // accounts only reads them, and accounts aren't described.
        let none = Accounts::open(&roots.data.join(ACCOUNTS))?;
        let idle = Idle {
            session: std::time::Duration::from_secs(60),
            lease: std::time::Duration::from_secs(60),
        };
        let api = Api::builder(&roots)
            .devices(devices)
            .jobs(jobs)
            .extensions(extensions)
            .projects(projects)
            .power(true)
            .security(Security::new(Arc::new(none), None, idle))
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
    let mut devices_config = config.devices_config().map_err(Error::Devices)?;
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

/// Serves until a Linux stop signal or a restart request.
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
    let projects = projects(&roots.data, &roots.data.join("index.sqlite"))?;
    // Recordings a crash cut short are recovered here (ADR-0022).
    let scratch = oscope_store::Scratch::open(&roots.scratch, ScratchPolicy::default())
        .map_err(|e| Error::Scratch(e.to_string()))?;
    // Accounts turn security on (P49): on the network, they and TLS must.
    let accounts = Accounts::open(&roots.data.join(ACCOUNTS))?;
    let address = SocketAddr::new(config.network.address, config.network.port);
    let network = !address.ip().is_loopback();
    if network && accounts.is_empty() {
        return Err(Error::Security(format!(
            "listening on {address} needs an account first: oscope-server account add <name>"
        )));
    }
    let tls = tls(&config, &roots.data, network)?;
    let (_, local) = write_token(&roots)?;
    let mut api = Api::builder(&roots)
        .logs(store)
        .settings(roots.data.join("settings"))
        .devices(devices)
        .jobs(jobs)
        .extensions(extensions)
        .projects(projects)
        .scratch(scratch)
        .power_with(
            config.power.dry_run,
            config.power.dry_run_restart.unwrap_or(config.power.dry_run),
        );
    if let Some(path) = &cli.config {
        api = api.config_file(Arc::new(TomlFile(path.clone())));
    }
    // The web app, at the API's own origin (W0.1).
    if let Some(folder) = &config.web.root {
        api = api.pages(folder);
    }
    // The server's third-party notices, for About (W4.3).
    if let Some(folder) = notices(&config) {
        api = api.notices(folder);
    }
    // The origins whose pages may call it, besides its own (W1.3, ADR-0045).
    api = api.origins(Origins::new(config.network.origins.clone()));
    if !accounts.is_empty() {
        let minutes = |m: u64| std::time::Duration::from_secs(60 * m);
        let idle = Idle {
            session: minutes(config.security.session_idle_minutes),
            lease: minutes(config.security.lease_idle_minutes),
        };
        api = api.security(Security::new(Arc::new(accounts), Some(local), idle));
    }
    let api = api.build()?;
    let listener = tokio::net::TcpListener::bind(address)
        .await
        .map_err(|source| Error::Listen { address, source })?;
    let bound = listener.local_addr().unwrap_or(address);
    let scheme = if tls.is_some() { "https" } else { "http" };
    println!("oscope-server {VERSION} serving on {scheme}://{bound}/");
    let restart = api.restart();
    let asked = Arc::clone(&restart);
    let shutdown = async move {
        tokio::select! {
            () = stop => {}
            () = asked.requested() => tracing::info!("restarting, as asked"),
        }
    };
    match tls {
        Some((certificate, key)) => {
            api.serve_tls(listener, &certificate, &key, shutdown)
                .await?
        }
        None => api.serve(listener, shutdown).await?,
    }
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
        ..JobsConfig::default()
    };
    JobService::open(path, jobs).map_err(|e| Error::Jobs(e.to_string()))
}

/// The projects of the data root `data`, and their index at `index`.
fn projects(data: &Path, index: &Path) -> Result<oscope_store::Store, Error> {
    oscope_store::Store::open(data, index).map_err(|e| Error::Projects(e.to_string()))
}

/// The accounts' file, in the data root (P49).
pub const ACCOUNTS: &str = "accounts.json";

/// Does `command` to `accounts`, saying what it did on `out`. `password`
/// gives the password for an account by its name.
pub fn account(
    accounts: &Accounts,
    command: &AccountCommand,
    out: &mut impl std::io::Write,
    password: impl Fn(&str) -> Result<String, Error>,
) -> Result<(), Error> {
    let said = match command {
        AccountCommand::Add { name } => {
            accounts.add(name, &password(name)?)?;
            writeln!(out, "added {name}")
        }
        AccountCommand::Remove { name } => {
            accounts.remove(name)?;
            writeln!(out, "removed {name}")
        }
        AccountCommand::Passwd { name } => {
            if !accounts.names().contains(name) {
                return Err(Error::Security(format!("no account {name}")));
            }
            accounts.passwd(name, &password(name)?)?;
            writeln!(out, "changed {name}'s password")
        }
        AccountCommand::List => accounts
            .names()
            .iter()
            .try_for_each(|name| writeln!(out, "{name}")),
    };
    said.map_err(|e| Error::Security(e.to_string()))
}

/// A password for `name`: asked for twice on a terminal, without showing
/// it; otherwise stdin's first line.
fn password(name: &str) -> Result<String, Error> {
    use std::io::IsTerminal;
    let failed = |e: std::io::Error| Error::Security(format!("reading the password: {e}"));
    if !std::io::stdin().is_terminal() {
        let mut line = String::new();
        std::io::stdin().read_line(&mut line).map_err(failed)?;
        return Ok(line.trim_end_matches(['\r', '\n']).to_owned());
    }
    let first = rpassword::prompt_password(format!("{name}'s password: ")).map_err(failed)?;
    let again = rpassword::prompt_password("again: ").map_err(failed)?;
    match first == again {
        true => Ok(first),
        false => Err(Error::Security("the two passwords differ".to_owned())),
    }
}

/// A TLS certificate chain and its private key, PEM.
pub type Pem = (Vec<u8>, Vec<u8>);

/// The TLS certificate and key to serve with: the configuration's; on the
/// network without them, one made and kept in `data_root/tls`; or none.
fn tls(config: &Config, data: &Path, network: bool) -> Result<Option<Pem>, Error> {
    let read = |path: &Path| {
        std::fs::read(path).map_err(|e| Error::Security(format!("{}: {e}", path.display())))
    };
    match (&config.security.certificate, &config.security.key) {
        (Some(certificate), Some(key)) => Ok(Some((read(certificate)?, read(key)?))),
        (None, None) if network => certificate(&data.join("tls"), config.network.address).map(Some),
        (None, None) => Ok(None),
        _ => Err(Error::Security(
            "[security] takes a certificate and its key, or neither".to_owned(),
        )),
    }
}

/// A self-signed certificate for this computer, made once and kept in
/// `folder`: `certificate.pem`, and `key.pem`, readable only by this
/// account. It names `localhost`, the loopback addresses, `address` unless
/// it is any, and the computer's name if the environment gives it.
pub fn certificate(folder: &Path, address: IpAddr) -> Result<Pem, Error> {
    let (certificate, key) = (folder.join("certificate.pem"), folder.join("key.pem"));
    let failed =
        |path: &Path, e: std::io::Error| Error::Security(format!("{}: {e}", path.display()));
    if certificate.is_file() && key.is_file() {
        let pem = std::fs::read(&certificate).map_err(|e| failed(&certificate, e))?;
        return Ok((pem, std::fs::read(&key).map_err(|e| failed(&key, e))?));
    }
    let mut names = vec![
        "localhost".to_owned(),
        "127.0.0.1".to_owned(),
        "::1".to_owned(),
    ];
    if !address.is_unspecified() && !address.is_loopback() {
        names.push(address.to_string());
    }
    let host = std::env::var("HOSTNAME");
    names.extend(host.ok().map(|h| h.to_lowercase()));
    let made = rcgen::generate_simple_self_signed(names)
        .map_err(|e| Error::Security(format!("making a certificate: {e}")))?;
    let (pem, private) = (made.cert.pem(), made.signing_key.serialize_pem());
    std::fs::create_dir_all(folder).map_err(|e| failed(folder, e))?;
    std::fs::write(&certificate, &pem).map_err(|e| failed(&certificate, e))?;
    oscope_shm::write_private(&key, private.as_bytes()).map_err(|e| failed(&key, e))?;
    Ok((pem.into_bytes(), private.into_bytes()))
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

/// Writes a new local token for the desktop app (ADR-0015): 32 random
/// bytes, in hex, in `{data_root}/run/local.token`, readable by this
/// account alone. Its absolute path, and the token.
fn write_token(roots: &Roots) -> Result<(PathBuf, String), Error> {
    let path = roots.data.join("run").join("local.token");
    let failed = |source| Error::Folder {
        path: path.clone(),
        source,
    };
    std::fs::create_dir_all(roots.data.join("run")).map_err(failed)?;
    let token: [u8; 32] = rand::random();
    let hex: String = token.iter().map(|b| format!("{b:02x}")).collect();
    oscope_shm::write_private(&path, hex.as_bytes()).map_err(failed)?;
    Ok((std::path::absolute(&path).map_err(failed)?, hex))
}

/// The folder of the server's third-party notices: the configuration's, or
/// where the packages put them (P54), beside the server on Windows and in
/// `/usr/share/doc/oscope` on Ubuntu; none for a build from source.
fn notices(config: &Config) -> Option<PathBuf> {
    if let Some(folder) = &config.web.notices {
        return Some(folder.clone());
    }
    let exe = std::env::current_exe().ok();
    let beside = exe.as_deref().and_then(Path::parent).map(Path::to_owned);
    let packaged = PathBuf::from("/usr/share/doc/oscope");
    let made = |folder: &PathBuf| folder.join("THIRD-PARTY-NOTICES.html").is_file();
    beside.into_iter().chain([packaged]).find(made)
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
