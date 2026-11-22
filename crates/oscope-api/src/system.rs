//! The `system` Thing: OScope's build, the computer it runs on, its
//! folders, and its third-party notices.

use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::Instant;

use oscope_store::{Recording, Scratch, ScratchHealth};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sysinfo::{CpuRefreshKind, RefreshKind, System as Host};
use teta_wot::http::PathParams;
use teta_wot::http::axum::http::StatusCode;
use teta_wot::http::axum::http::header::CONTENT_TYPE;
use teta_wot::http::axum::response::{IntoResponse, Response};
use teta_wot::prelude::*;

use crate::{Restart, Roots};

/// OScope's version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The Git commit OScope was built from, as 12 hex digits, or `unknown`
/// when it wasn't built from a Git checkout.
pub const GIT_COMMIT: &str = env!("OSCOPE_GIT_COMMIT");

/// The operating system.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Os {
    /// Its name and version, such as `Windows 11 Pro` or
    /// `Linux (Ubuntu 24.04)`.
    pub name: String,
    /// The kernel's version.
    pub kernel: String,
    /// The processor architecture, such as `x86_64`.
    pub arch: String,
}

/// The processor.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Cpu {
    /// Its model.
    pub model: String,
    /// Its physical cores.
    pub cores: usize,
    /// Its logical processors.
    pub threads: usize,
}

/// The memory, in bytes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Memory {
    /// All of it.
    pub total_bytes: u64,
    /// How much programs can still have.
    pub available_bytes: u64,
}

/// A folder, and the space on its disk, in bytes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Folder {
    /// The folder's absolute path.
    pub path: String,
    /// The space OScope can still use.
    pub available_bytes: u64,
    /// The disk's size.
    pub total_bytes: u64,
}

/// One of the server's third-party notices.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Notice {
    /// Its file's name, such as `THIRD-PARTY-NOTICES.html`.
    pub name: String,
    /// Its size, in bytes.
    pub bytes: u64,
}

/// The folder of the server's third-party notices (W4.3).
pub(crate) struct Notices(pub(crate) PathBuf);

/// OScope's build, the computer it runs on, and its folders.
#[derive(Thing)]
#[thing(config = Roots)]
pub struct System {
    #[thing(init = Instant::now())]
    started: Instant,
    #[thing(init = os())]
    os: Os,
    #[thing(init = cpu())]
    cpu: Cpu,
    #[thing(init = absolute(&config.data))]
    data_root: PathBuf,
    #[thing(init = absolute(&config.scratch))]
    scratch_root: PathBuf,
    #[thing(init = OnceLock::new())]
    restart: OnceLock<Arc<Restart>>,
    #[thing(init = OnceLock::new())]
    scratch: OnceLock<Arc<Scratch>>,
    #[thing(init = OnceLock::new())]
    notices: OnceLock<PathBuf>,
}

#[thing_impl]
impl System {
    #[on_start]
    async fn connect(&self, ctx: ThingCtx) -> anyhow::Result<()> {
        if let Some(restart) = ctx.server().service::<Restart>() {
            let _ = self.restart.set(restart);
        }
        if let Some(scratch) = ctx.server().service::<Scratch>() {
            let _ = self.scratch.set(scratch);
        }
        if let Some(notices) = ctx.server().service::<Notices>() {
            let _ = self.notices.set(notices.0.clone());
        }
        Ok(())
    }

    /// The server's third-party notices, by name: those in the folder its
    /// package put them in, `notices/{name}` to read each. None when the
    /// server has no such folder, as a build from source hasn't.
    #[property(blocking)]
    fn notices(&self) -> Vec<Notice> {
        self.notices
            .get()
            .map(|folder| listed(folder))
            .unwrap_or_default()
    }

    /// `GET notices/{name}`: one of the notices, to read. `404` for any
    /// name that `notices` doesn't list.
    #[endpoint(get, "notices/{name}")]
    async fn notice(&self, params: PathParams) -> Response {
        let (Some(folder), Some(name)) = (self.notices.get(), params.get("name")) else {
            return StatusCode::NOT_FOUND.into_response();
        };
        // A notice's name, and only a name: never a path out of the folder.
        if !is_notice(name) || name.contains(['/', '\\']) {
            return StatusCode::NOT_FOUND.into_response();
        }
        let kind = match name.ends_with(".html") {
            true => "text/html; charset=utf-8",
            false => "text/plain; charset=utf-8",
        };
        match tokio::fs::read(folder.join(name)).await {
            Ok(bytes) => ([(CONTENT_TYPE, kind)], bytes).into_response(),
            Err(_) => StatusCode::NOT_FOUND.into_response(),
        }
    }

    /// Restarts the server.
    ///
    /// It stops as it does when asked to, then exits with code 75, which
    /// its supervisor (the desktop app, systemd, or Windows' service
    /// manager) takes as a request to start it again (ADR-0015). Run by
    /// hand, it just stops.
    #[action]
    async fn restart(&self) -> Result<(), ActionError> {
        let restart = self
            .restart
            .get()
            .ok_or_else(|| ActionError::handled("the server can't restart"))?;
        restart.request();
        Ok(())
    }

    /// OScope's version.
    #[property]
    async fn version(&self) -> String {
        VERSION.to_owned()
    }

    /// The Git commit OScope was built from, or "unknown".
    #[property]
    async fn git_commit(&self) -> String {
        GIT_COMMIT.to_owned()
    }

    /// The operating system.
    #[property]
    async fn os(&self) -> Os {
        self.os.clone()
    }

    /// The processor.
    #[property]
    async fn cpu(&self) -> Cpu {
        self.cpu.clone()
    }

    /// The memory, now.
    #[property(blocking)]
    fn memory(&self) -> Memory {
        let mut host = Host::new();
        host.refresh_memory();
        Memory {
            total_bytes: host.total_memory(),
            available_bytes: host.available_memory(),
        }
    }

    /// How long the server has been running.
    #[property(unit = "s")]
    async fn uptime(&self) -> f64 {
        self.started.elapsed().as_secs_f64()
    }

    /// The data root, for projects and their data, and its disk's space.
    #[property(blocking)]
    fn data_root(&self) -> Result<Folder, PropertyError> {
        folder(&self.data_root)
    }

    /// The scratch root, for frame logs and caches, and its disk's space.
    #[property(blocking)]
    fn scratch_root(&self) -> Result<Folder, PropertyError> {
        folder(&self.scratch_root)
    }

    /// The scratch space's health: whether there is room to record, its
    /// disk's space, and the recordings waiting there to be ingested
    /// (ADR-0022).
    #[property(blocking)]
    fn scratch(&self) -> Result<ScratchHealth, PropertyError> {
        let scratch = self.scratch.get().ok_or_else(|| {
            PropertyError::failed(std::io::Error::other("the server has no scratch space"))
        })?;
        scratch.health().map_err(PropertyError::failed)
    }

    /// The recordings waiting in the scratch space, the newest first: each
    /// ingested into a project with the `projects.ingest` job, or left
    /// (W6.3).
    #[property(blocking)]
    fn recordings(&self) -> Result<Vec<Recording>, PropertyError> {
        let scratch = self.scratch.get().ok_or_else(|| {
            PropertyError::failed(std::io::Error::other("the server has no scratch space"))
        })?;
        scratch.recordings().map_err(PropertyError::failed)
    }
}

/// The operating system, from the system.
fn os() -> Os {
    Os {
        name: Host::long_os_version().unwrap_or_default(),
        kernel: Host::kernel_version().unwrap_or_default(),
        arch: std::env::consts::ARCH.to_owned(),
    }
}

/// The processor, from the system.
fn cpu() -> Cpu {
    let host = Host::new_with_specifics(RefreshKind::nothing().with_cpu(CpuRefreshKind::nothing()));
    Cpu {
        model: host
            .cpus()
            .first()
            .map(|c| c.brand().trim().to_owned())
            .unwrap_or_default(),
        cores: Host::physical_core_count().unwrap_or_default(),
        threads: host.cpus().len(),
    }
}

/// The notices in `folder`, by name: its files whose names say they are
/// notices; none if it can't be read.
fn listed(folder: &Path) -> Vec<Notice> {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return Vec::new();
    };
    let mut notices: Vec<Notice> = entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            let metadata = entry.metadata().ok()?;
            (metadata.is_file() && is_notice(&name)).then_some(Notice {
                name,
                bytes: metadata.len(),
            })
        })
        .collect();
    notices.sort_by(|a, b| a.name.cmp(&b.name));
    notices
}

/// Whether a file called `name` is a notice, as `packaging/notices/make.sh`
/// names them: HTML or text, a notice or a licence. The folder may hold the
/// server and its other files too.
fn is_notice(name: &str) -> bool {
    let text = name.ends_with(".html") || name.ends_with(".txt");
    text && (name.contains("NOTICE") || name.contains("LICENSE"))
}

/// `path` made absolute, without resolving links; as it is if that fails.
fn absolute(path: &Path) -> PathBuf {
    std::path::absolute(path).unwrap_or_else(|_| path.to_owned())
}

/// The folder at `path`, and its disk's space; an error if it isn't a
/// folder, for example because it was deleted.
fn folder(path: &Path) -> Result<Folder, PropertyError> {
    let stats = match path.is_dir() {
        true => fs4::statvfs(path),
        false => Err(std::io::Error::other("there is no such folder")),
    };
    let stats = stats.map_err(|error| {
        PropertyError::failed(std::io::Error::other(format!(
            "{}: {error}",
            path.display()
        )))
    })?;
    Ok(Folder {
        path: path.display().to_string(),
        available_bytes: stats.available_space(),
        total_bytes: stats.total_space(),
    })
}
