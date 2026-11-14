//! Installed extensions: their folders, what each start loads, and the
//! registry of what the loaded ones provide (ADR-0018).
//!
//! ```text
//! <data root>/extensions/
//! ├─ .incoming/        packages arriving and unpacking, emptied at start-up
//! └─ <id>/
//!    ├─ state.json     the version installed, enabled, uninstalled
//!    └─ <version>/     the package, unpacked: manifest.toml, …
//! ```

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError};

use schemars::JsonSchema;
use semver::Version;
use serde::{Deserialize, Serialize};
use tempfile::NamedTempFile;

use oscope_pipeline::Node;

use crate::manifest::is_id;
use crate::native::{check, registered};
use crate::python::prepare;
use crate::worker::{self, Worker};
use crate::{
    Contribution, ExtError, Library, MANIFEST, Manifest, OSCOPE_FOLDER, Package, PythonRuntime, io,
};

/// Where packages arrive and unpack, in the extensions' folder.
const INCOMING: &str = ".incoming";

/// Each extension's state, in its folder.
const STATE: &str = "state.json";

/// What the loaded extensions provide: for each contribution, each name and
/// the ID of the extension that provides it.
pub type Registry = BTreeMap<Contribution, BTreeMap<String, String>>;

/// An installed extension, as the `extensions` Thing describes it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Extension {
    /// The installed version's manifest.
    pub manifest: Manifest,
    /// The SHA-256 of the package it came from, in hex.
    pub sha256: String,
    /// Whether it is enabled: the next start loads it, if it can.
    pub enabled: bool,
    /// Whether it is uninstalled: the next start removes its files.
    pub uninstalled: bool,
    /// The version this start loaded, if it loaded it.
    #[schemars(with = "Option<String>")]
    pub loaded: Option<Version>,
    /// The version the next start loads, if it will.
    #[schemars(with = "Option<String>")]
    pub next: Option<Version>,
    /// Why the next start can't load it, though it is enabled.
    pub problem: Option<String>,
}

/// An extension this start loaded.
#[derive(Debug, Clone)]
pub struct Loaded {
    /// Its manifest.
    pub manifest: Manifest,
    /// Its package, unpacked.
    pub folder: PathBuf,
    /// The SHA-256 of its package, in hex.
    pub sha256: String,
}

/// An extension's `state.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct State {
    /// The version installed: the folder the next start loads.
    version: Version,
    sha256: String,
    enabled: bool,
    uninstalled: bool,
}

/// An installed extension, as its folder has it.
#[derive(Debug)]
struct Installed {
    state: State,
    manifest: Manifest,
}

/// What a start does with an installed extension: the version it loads, or
/// why it can't.
#[derive(Debug, Default)]
struct Plan {
    version: Option<Version>,
    problem: Option<String>,
}

/// The stock nodes' IDs, which no extension may provide.
fn stock() -> &'static BTreeSet<String> {
    static STOCK: OnceLock<BTreeSet<String>> = OnceLock::new();
    STOCK.get_or_init(|| {
        let registry = oscope_pipeline::stock::registry(PathBuf::new());
        registry.descriptors().map(|d| d.id.clone()).collect()
    })
}

/// What a start loads: each extension that is enabled and suits this
/// OScope, in the order of their IDs, unless OScope's stock nodes or an
/// extension loaded before it provide one of its names. Then it loads none
/// of them.
fn plan(installed: &BTreeMap<String, Installed>) -> (BTreeMap<&str, Plan>, Registry) {
    let (mut plans, mut registry) = (BTreeMap::new(), Registry::new());
    for (id, Installed { state, manifest }) in installed {
        let mut plan = Plan::default();
        if state.enabled && !state.uninstalled {
            let taken = manifest.provided().find_map(|(contribution, name)| {
                if contribution == Contribution::Nodes && stock().contains(name) {
                    return Some(format!("OScope's stock nodes have `{name}` already"));
                }
                let other = registry.get(&contribution)?.get(name)?;
                Some(format!(
                    "{other} provides {contribution} `{name}` too, and loads first"
                ))
            });
            plan.problem = manifest.check_host().err().map(|e| e.to_string()).or(taken);
            if plan.problem.is_none() {
                for (contribution, name) in manifest.provided() {
                    let names = registry.entry(contribution).or_default();
                    names.insert(name.to_owned(), id.clone());
                }
                plan.version = Some(state.version.clone());
            }
        }
        plans.insert(id.as_str(), plan);
    }
    (plans, registry)
}

/// The nodes a Python extension's worker described when it was installed.
const NODES: &str = "nodes.json";

/// An extension's code, loaded: its nodes, and a Python extension's worker.
type Code = (Vec<Arc<dyn Node>>, Option<Arc<Worker>>);

/// Loads an extension's code: a Rust extension's library, or a Python
/// extension's worker, to start on first use, when OScope runs Python.
/// Its nodes must be those its manifest provides.
fn load(
    manifest: &Manifest,
    folder: &Path,
    python: Option<&PythonRuntime>,
) -> Result<Code, String> {
    let (nodes, worker) = match (manifest.library(), &manifest.python, python) {
        (Some(library), _, _) => {
            let path = library
                .split('/')
                .fold(folder.to_owned(), |path, part| path.join(part));
            (
                Library::load(&path).map_err(|e| e.to_string())?.nodes(),
                None,
            )
        }
        (None, Some(entry), Some(python)) => {
            let path = folder.join(OSCOPE_FOLDER).join(NODES);
            let failed = |e: &dyn std::fmt::Display| format!("{}: {e}", path.display());
            let text = fs::read(&path).map_err(|e| failed(&e))?;
            let described: Vec<oscope_pipeline::Descriptor> =
                serde_json::from_slice(&text).map_err(|e| failed(&e))?;
            let worker = Worker::new(&manifest.id, folder, &entry.entry, python);
            (worker::nodes(&worker, &described), Some(worker))
        }
        _ => return Ok((Vec::new(), None)),
    };
    provided(manifest, &nodes)?;
    Ok((nodes, worker))
}

/// Checks that `nodes` are those `manifest` provides, and as the pipeline
/// checks nodes.
fn provided(manifest: &Manifest, nodes: &[Arc<dyn Node>]) -> Result<(), String> {
    let descriptors: Vec<_> = nodes.iter().map(|node| node.descriptor().clone()).collect();
    check(&descriptors, "for an extension")?;
    registered(nodes)?;
    let list = |ids: BTreeSet<&str>| match ids.is_empty() {
        true => "none".to_owned(),
        false => ids
            .into_iter()
            .map(|id| format!("`{id}`"))
            .collect::<Vec<_>>()
            .join(", "),
    };
    let described = descriptors.iter().map(|d| d.id.as_str()).collect();
    let provided = manifest
        .provides
        .get(&Contribution::Nodes)
        .into_iter()
        .flatten();
    let provided = provided.map(String::as_str).collect();
    if described != provided {
        let (described, provided) = (list(described), list(provided));
        let code = if manifest.library().is_some() {
            "library"
        } else {
            "module"
        };
        return Err(format!(
            "its {code} has the nodes {described}, and its manifest provides {provided}"
        ));
    }
    Ok(())
}

/// A Python extension's worker, as [`Extensions::workers`] lists it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkerState {
    /// The extension's ID.
    pub id: String,
    /// Its worker's process ID, while it runs.
    pub pid: Option<u32>,
}

/// The installed extensions, in a folder of the data root: what this start
/// loaded, and what the next start will.
///
/// Extensions load only at start-up, as teta-wot fixes its Things when the
/// server is built. So installing, enabling, disabling and uninstalling
/// change the files, and [`open`](Self::open) acts on them at the next
/// start.
pub struct Extensions {
    root: PathBuf,
    loaded: Vec<Loaded>,
    registry: Registry,
    nodes: Vec<Arc<dyn Node>>,
    workers: Vec<Arc<Worker>>,
    /// The extensions whose code this start couldn't load: the version, and
    /// why.
    failed: BTreeMap<String, (Version, String)>,
    python: Option<PythonRuntime>,
    installed: Mutex<BTreeMap<String, Installed>>,
}

impl std::fmt::Debug for Extensions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let loaded: Vec<_> = self.loaded.iter().map(|l| &l.manifest.id).collect();
        f.debug_struct("Extensions")
            .field("root", &self.root)
            .field("loaded", &loaded)
            .finish_non_exhaustive()
    }
}

impl Extensions {
    /// Opens the extensions in the folder `root`, as a start does.
    ///
    /// First it finishes the changes made for this start: it removes the
    /// uninstalled extensions, every version but the installed one, and
    /// unfinished installs. Then it loads each extension that is enabled
    /// and suits this OScope, in the order of their IDs, with its code: a
    /// Rust extension's library. One that provides a name that the stock
    /// nodes or an extension loaded before it provide isn't loaded at all,
    /// nor one whose code fails to load. An extension whose folder can't be
    /// read is left out, with a warning, and left on disk.
    ///
    /// Python extensions' code doesn't load: see
    /// [`open_with`](Self::open_with).
    pub fn open(root: impl Into<PathBuf>) -> Result<Extensions, ExtError> {
        Extensions::open_with(root, None)
    }

    /// As [`open`](Self::open), running Python extensions with `python`:
    /// installing makes their environments, and their nodes run in their
    /// workers (ADR-0020).
    pub fn open_with(
        root: impl Into<PathBuf>,
        python: Option<PythonRuntime>,
    ) -> Result<Extensions, ExtError> {
        let root = root.into();
        let mut installed = BTreeMap::new();
        if root.is_dir() {
            let incoming = root.join(INCOMING);
            if incoming.exists() {
                fs::remove_dir_all(&incoming).map_err(io(&incoming))?;
            }
            for entry in fs::read_dir(&root).map_err(io(&root))? {
                let path = entry.map_err(io(&root))?.path();
                let Some(id) = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .filter(|n| is_id(n))
                else {
                    continue;
                };
                match finish(&path, id) {
                    Ok(Some(found)) => drop(installed.insert(id.to_owned(), found)),
                    Ok(None) => tracing::info!("extension {id}: removed, as it was uninstalled"),
                    Err(error) => tracing::warn!("extension {id} is left out: {error}"),
                }
            }
        }
        let (plans, mut registry) = plan(&installed);
        let (mut loaded, mut nodes, mut workers, mut failed) =
            (Vec::new(), Vec::new(), Vec::new(), BTreeMap::new());
        for (id, plan) in &plans {
            let Installed { state, manifest } = &installed[*id];
            let Some(version) = &plan.version else {
                if let Some(problem) = &plan.problem {
                    tracing::warn!("extension {id} isn't loaded: {problem}");
                }
                continue;
            };
            let folder = root.join(id).join(version.to_string());
            match load(manifest, &folder, python.as_ref()) {
                Ok((code, worker)) => {
                    tracing::info!("extension {id} {version}: loaded");
                    nodes.extend(code);
                    workers.extend(worker);
                    let sha256 = state.sha256.clone();
                    let manifest = manifest.clone();
                    loaded.push(Loaded {
                        manifest,
                        folder,
                        sha256,
                    });
                }
                Err(problem) => {
                    tracing::warn!("extension {id} {version} couldn't be loaded: {problem}");
                    registry
                        .values_mut()
                        .for_each(|names| names.retain(|_, by| by != id));
                    failed.insert((*id).to_owned(), (version.clone(), problem));
                }
            }
        }
        registry.retain(|_, names| !names.is_empty());
        drop(plans);
        Ok(Extensions {
            root,
            loaded,
            registry,
            nodes,
            workers,
            failed,
            python,
            installed: Mutex::new(installed),
        })
    }

    /// Extensions in `root` with none loaded and none read, for the server's
    /// printed documents, which mustn't touch the data root.
    pub fn empty(root: impl Into<PathBuf>) -> Extensions {
        Extensions {
            root: root.into(),
            loaded: Vec::new(),
            registry: Registry::new(),
            nodes: Vec::new(),
            workers: Vec::new(),
            failed: BTreeMap::new(),
            python: None,
            installed: Mutex::default(),
        }
    }

    /// The folder of the extensions.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The extensions this start loaded, in the order it loaded them.
    pub fn loaded(&self) -> &[Loaded] {
        &self.loaded
    }

    /// What the extensions this start loaded provide.
    pub fn registry(&self) -> &Registry {
        &self.registry
    }

    /// The nodes of the extensions this start loaded, for the pipeline's
    /// registry.
    pub fn nodes(&self) -> &[Arc<dyn Node>] {
        &self.nodes
    }

    /// The workers of the Python extensions this start loaded.
    pub fn workers(&self) -> Vec<WorkerState> {
        let state = |worker: &Arc<Worker>| WorkerState {
            id: worker.id().to_owned(),
            pid: worker.pid(),
        };
        self.workers.iter().map(state).collect()
    }

    /// The installed extensions, by ID.
    pub fn list(&self) -> Vec<Extension> {
        self.describe(&self.lock())
    }

    /// The installed extension `id`.
    pub fn get(&self, id: &str) -> Result<Extension, ExtError> {
        self.describe_one(&self.lock(), id)
    }

    /// Whether the next start loads other extensions than this one did.
    pub fn restart_needed(&self) -> bool {
        self.list()
            .iter()
            .any(|extension| extension.loaded != extension.next)
    }

    /// A new file for a package to arrive in, beside the extensions, so that
    /// it can be checked before it is installed. Removed when dropped.
    pub fn incoming(&self) -> Result<NamedTempFile, ExtError> {
        let folder = self.root.join(INCOMING);
        fs::create_dir_all(&folder).map_err(io(&folder))?;
        let mut file = tempfile::Builder::new();
        file.prefix("package-").suffix(".omext");
        file.tempfile_in(&folder).map_err(io(&folder))
    }

    /// Installs the package at `path` for the next start: the extension, and
    /// whether this unpacked it.
    ///
    /// Refused when [`Package::open`] refuses the package, or it doesn't
    /// suit this OScope ([`Manifest::check_host`]). Installing a package
    /// again changes nothing, and installing a version that is there from
    /// another package is a conflict. A new version unpacks beside the one
    /// this start loaded, which the next start removes. The extension is
    /// enabled, unless it was installed and disabled.
    pub fn install(&self, path: &Path) -> Result<(Extension, bool), ExtError> {
        let mut package = Package::open(path)?;
        package.manifest().check_host()?;
        let manifest = package.manifest().clone();
        let (id, version) = (manifest.id.clone(), manifest.version.clone());
        let mut installed = self.lock();
        let before = installed.get(&id).map(|b| b.state.clone());
        let state = State {
            version: version.clone(),
            sha256: package.sha256().to_owned(),
            enabled: before.as_ref().is_none_or(|b| b.enabled || b.uninstalled),
            uninstalled: false,
        };
        // This version is there already: installed, or loaded by this start.
        let there = match &before {
            Some(b) if b.version == version => Some(b.sha256.clone()),
            _ => self
                .loaded
                .iter()
                .find(|l| l.manifest.id == id && l.manifest.version == version)
                .map(|l| l.sha256.clone()),
        };
        let folder = self.root.join(&id);
        let unpacked = match there {
            Some(sha256) if sha256 != state.sha256 => {
                return Err(ExtError::Conflict(format!(
                    "{id} {version} is installed from another package (SHA-256 {sha256}): \
                     give this one another version"
                )));
            }
            Some(_) => false,
            None => {
                let incoming = self.root.join(INCOMING);
                fs::create_dir_all(&incoming).map_err(io(&incoming))?;
                let staging = tempfile::Builder::new()
                    .prefix("unpack-")
                    .tempdir_in(&incoming);
                let staging = staging.map_err(io(&incoming))?;
                package.unpack(staging.path())?;
                // Left by an install that didn't finish: neither installed nor loaded.
                let target = folder.join(version.to_string());
                if target.exists() {
                    fs::remove_dir_all(&target).map_err(io(&target))?;
                }
                fs::create_dir_all(&folder).map_err(io(&folder))?;
                fs::rename(staging.path(), &target).map_err(io(&target))?;
                if let Err(error) = self.prepare(&manifest, &target) {
                    let _ = fs::remove_dir_all(&target);
                    return Err(error);
                }
                true
            }
        };
        if unpacked || before.as_ref() != Some(&state) {
            write_state(&folder, &state)?;
        }
        installed.insert(id.clone(), Installed { state, manifest });
        Ok((self.describe_one(&installed, &id)?, unpacked))
    }

    /// Makes a Python extension's environment in its folder, when OScope
    /// runs Python, and keeps the nodes its worker describes.
    fn prepare(&self, manifest: &Manifest, folder: &Path) -> Result<(), ExtError> {
        let (Some(entry), Some(python)) = (&manifest.python, &self.python) else {
            return Ok(());
        };
        let described = prepare(folder, &manifest.id, entry, python)?;
        let worker = Worker::new(&manifest.id, folder, &entry.entry, python);
        provided(manifest, &worker::nodes(&worker, &described)).map_err(ExtError::Python)?;
        let path = folder.join(OSCOPE_FOLDER).join(NODES);
        let text = serde_json::to_vec_pretty(&described).map_err(|e| io(&path)(e.into()))?;
        fs::write(&path, text).map_err(io(&path))
    }

    /// Enables the extension `id`: the next start loads it, if it can.
    pub fn enable(&self, id: &str) -> Result<Extension, ExtError> {
        self.set_enabled(id, true)
    }

    /// Disables the extension `id`: the next start doesn't load it.
    pub fn disable(&self, id: &str) -> Result<Extension, ExtError> {
        self.set_enabled(id, false)
    }

    fn set_enabled(&self, id: &str, enabled: bool) -> Result<Extension, ExtError> {
        let mut installed = self.lock();
        let entry = installed
            .get_mut(id)
            .ok_or_else(|| ExtError::Unknown(id.to_owned()))?;
        if entry.state.uninstalled {
            return Err(ExtError::Conflict(format!(
                "{id} is uninstalled: install it again first"
            )));
        }
        if entry.state.enabled != enabled {
            let state = State {
                enabled,
                ..entry.state.clone()
            };
            write_state(&self.root.join(id), &state)?;
            entry.state = state;
        }
        self.describe_one(&installed, id)
    }

    /// Uninstalls the extension `id`. If this start didn't load it, its
    /// files go at once, and it answers `None`. If it did, they go at the
    /// next start, and it answers the extension, uninstalled.
    pub fn uninstall(&self, id: &str) -> Result<Option<Extension>, ExtError> {
        let mut installed = self.lock();
        let entry = installed
            .get_mut(id)
            .ok_or_else(|| ExtError::Unknown(id.to_owned()))?;
        let folder = self.root.join(id);
        if !self.loaded.iter().any(|loaded| loaded.manifest.id == id) {
            fs::remove_dir_all(&folder).map_err(io(&folder))?;
            installed.remove(id);
            return Ok(None);
        }
        let state = State {
            enabled: false,
            uninstalled: true,
            ..entry.state.clone()
        };
        write_state(&folder, &state)?;
        entry.state = state;
        self.describe_one(&installed, id).map(Some)
    }

    fn lock(&self) -> MutexGuard<'_, BTreeMap<String, Installed>> {
        self.installed
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    fn describe(&self, installed: &BTreeMap<String, Installed>) -> Vec<Extension> {
        let (plans, _) = plan(installed);
        let loaded = |id: &str| {
            let loaded = self.loaded.iter().find(|loaded| loaded.manifest.id == id);
            loaded.map(|loaded| loaded.manifest.version.clone())
        };
        let describe = |(id, Installed { state, manifest }): (&String, &Installed)| {
            let plan = &plans[id.as_str()];
            // Code that failed to load this start fails at the next, unless
            // another version is installed.
            let failed = self
                .failed
                .get(id)
                .filter(|(version, _)| *version == state.version);
            let (next, problem) = match failed {
                Some((_, why)) if plan.version.is_some() => (None, Some(why.clone())),
                _ => (plan.version.clone(), plan.problem.clone()),
            };
            Extension {
                manifest: manifest.clone(),
                sha256: state.sha256.clone(),
                enabled: state.enabled,
                uninstalled: state.uninstalled,
                loaded: loaded(id),
                next,
                problem,
            }
        };
        installed.iter().map(describe).collect()
    }

    fn describe_one(
        &self,
        installed: &BTreeMap<String, Installed>,
        id: &str,
    ) -> Result<Extension, ExtError> {
        let mut all = self.describe(installed).into_iter();
        all.find(|extension| extension.manifest.id == id)
            .ok_or_else(|| ExtError::Unknown(id.to_owned()))
    }
}

/// Finishes the changes made to an extension's folder for this start, and
/// reads it: removes the folder if it was uninstalled (`None`), and every
/// version but the installed one.
fn finish(folder: &Path, id: &str) -> Result<Option<Installed>, ExtError> {
    let path = folder.join(STATE);
    let text = fs::read(&path).map_err(io(&path))?;
    let state: State = serde_json::from_slice(&text).map_err(|e| io(&path)(e.into()))?;
    if state.uninstalled {
        fs::remove_dir_all(folder).map_err(io(folder))?;
        return Ok(None);
    }
    let version = state.version.to_string();
    for entry in fs::read_dir(folder).map_err(io(folder))? {
        let entry = entry.map_err(io(folder))?;
        let old = entry.file_name().to_str() != Some(version.as_str());
        if old && entry.file_type().map_err(io(folder))?.is_dir() {
            fs::remove_dir_all(entry.path()).map_err(io(&entry.path()))?;
        }
    }
    let path = folder.join(&version).join(MANIFEST);
    let manifest = Manifest::parse(&fs::read_to_string(&path).map_err(io(&path))?)?;
    if manifest.id != id || manifest.version != state.version {
        let found = format!("{} {}", manifest.id, manifest.version);
        return Err(ExtError::Conflict(format!("{} is {found}", path.display())));
    }
    Ok(Some(Installed { state, manifest }))
}

/// Writes an extension's state to a file beside it, then renames it over
/// the old one, so that a crash leaves one or the other.
fn write_state(folder: &Path, state: &State) -> Result<(), ExtError> {
    let (path, part) = (folder.join(STATE), folder.join("state.json.part"));
    let text = serde_json::to_vec_pretty(state).map_err(|e| io(&path)(e.into()))?;
    fs::write(&part, text).map_err(io(&part))?;
    fs::rename(&part, &path).map_err(io(&path))
}
