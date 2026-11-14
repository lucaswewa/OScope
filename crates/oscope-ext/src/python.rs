//! Python extensions' environments: uv, found, and each extension's
//! environment, made when it is installed (ADR-0020).

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use crate::worker::{Described, Worker};
use crate::{ExtError, Python as Manifest};

/// How OScope runs Python extensions (ADR-0020).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PythonRuntime {
    /// uv; when not given, `OSCOPE_UV`, then `PATH`, then where uv's
    /// installers put it ([`find_uv`]).
    pub uv: Option<PathBuf>,
    /// The SDK's source, `python/oscope-ext`, installed into each
    /// environment.
    pub sdk: PathBuf,
    /// A folder of wheels to install from, offline, instead of the package
    /// index.
    pub wheelhouse: Option<PathBuf>,
    /// How long a worker may take to start and answer.
    pub start_timeout: Duration,
    /// How long a node may take to run.
    pub call_timeout: Duration,
    /// How long an idle worker is kept.
    pub idle: Duration,
}

impl Default for PythonRuntime {
    /// uv found; the SDK in this source tree; the package index; two minutes
    /// to start and to run; five minutes idle.
    fn default() -> Self {
        PythonRuntime {
            uv: None,
            sdk: Path::new(env!("CARGO_MANIFEST_DIR")).join("../../python/oscope-ext"),
            wheelhouse: None,
            start_timeout: Duration::from_secs(120),
            call_timeout: Duration::from_secs(120),
            idle: Duration::from_secs(300),
        }
    }
}

/// Where OScope keeps what it adds to a package's folder, a name packages
/// may not use: the environment, and the nodes it described.
pub const OSCOPE_FOLDER: &str = ".oscope";

fn failed(why: impl Into<String>) -> ExtError {
    ExtError::Python(why.into())
}

/// Finds uv: `configured`, then `OSCOPE_UV`, then each folder of `PATH`,
/// then `~/.local/bin` and `~/.cargo/bin`, where uv's installers put it.
pub fn find_uv(configured: Option<&Path>) -> Result<PathBuf, ExtError> {
    let exe = format!("uv{}", std::env::consts::EXE_SUFFIX);
    let mut candidates: Vec<PathBuf> = configured.into_iter().map(Path::to_owned).collect();
    candidates.extend(std::env::var_os("OSCOPE_UV").map(PathBuf::from));
    let path = std::env::var_os("PATH").unwrap_or_default();
    candidates.extend(std::env::split_paths(&path).map(|folder| folder.join(&exe)));
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" });
    for folder in [".local/bin", ".cargo/bin"] {
        candidates.extend(
            home.iter()
                .map(|home| Path::new(home).join(folder).join(&exe)),
        );
    }
    let found = candidates.into_iter().find(|candidate| candidate.is_file());
    found.ok_or_else(|| {
        failed("uv isn't installed: install it (docs.astral.sh/uv), or name it with OSCOPE_UV")
    })
}

/// The environment's Python.
pub(crate) fn interpreter(folder: &Path) -> PathBuf {
    let venv = folder.join(OSCOPE_FOLDER).join("venv");
    match cfg!(windows) {
        true => venv.join("Scripts").join("python.exe"),
        false => venv.join("bin").join("python"),
    }
}

/// Runs uv with `arguments`, saying what failed with its last lines.
fn uv(
    uv: &Path,
    what: &str,
    arguments: impl FnOnce(&mut Command) -> &mut Command,
) -> Result<(), ExtError> {
    let mut command = Command::new(uv);
    let output = arguments(&mut command).output();
    let output = output.map_err(|e| failed(format!("{what}: can't run {}: {e}", uv.display())))?;
    if output.status.success() {
        return Ok(());
    }
    let said = String::from_utf8_lossy(&output.stderr);
    let lines: Vec<_> = said
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect();
    let tail = lines[lines.len().saturating_sub(8)..].join("\n");
    Err(failed(format!(
        "{what} failed ({}):\n{tail}",
        output.status
    )))
}

/// Makes the environment of the extension unpacked in `folder`, with uv:
/// a uv-managed Python its manifest allows, its dependencies and OScope's
/// SDK. Online, it takes packages at least two weeks old (CLAUDE.md);
/// offline, those in the wheelhouse. Then it starts the extension's worker
/// once, for the nodes and drivers it describes.
pub(crate) fn prepare(
    folder: &Path,
    id: &str,
    manifest: &Manifest,
    settings: &PythonRuntime,
) -> Result<Described, ExtError> {
    let found = find_uv(settings.uv.as_deref())?;
    let venv = folder.join(OSCOPE_FOLDER).join("venv");
    let offline = settings.wheelhouse.is_some();
    uv(&found, "making the environment", |command| {
        command.args([
            "venv",
            "--no-project",
            "--managed-python",
            "--quiet",
            "--python",
        ]);
        command.arg(&manifest.requires_python).arg(&venv);
        if offline {
            command.arg("--offline");
        }
        command
    })?;
    uv(&found, "installing its packages", |command| {
        command
            .args(["pip", "install", "--quiet", "--python"])
            .arg(&venv);
        match &settings.wheelhouse {
            Some(wheels) => command
                .args(["--offline", "--no-index", "--find-links"])
                .arg(wheels),
            None => command.arg("--exclude-newer").arg(two_weeks_ago()),
        };
        command.args(&manifest.dependencies).arg(&settings.sdk)
    })?;
    let worker = Worker::new(id, folder, &manifest.entry, settings);
    let described = worker.describe();
    worker.stop();
    described.map_err(failed)
}

/// Two weeks ago, as uv takes it: the newest packages it may install.
fn two_weeks_ago() -> String {
    let then = chrono::Utc::now() - chrono::Duration::days(14);
    then.format("%Y-%m-%dT%H:%M:%SZ").to_string()
}
