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

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};

use schemars::JsonSchema;
use semver::Version;
use serde::{Deserialize, Serialize};
use tempfile::NamedTempFile;

use crate::manifest::is_id;
use crate::{Contribution, ExtError, MANIFEST, Manifest, Package, io};

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

/// What a start loads: each extension that is enabled and suits this
/// OScope, in the order of their IDs, unless an extension loaded before it
/// provides one of its names. Then it loads none of them.
fn plan(installed: &BTreeMap<String, Installed>) -> (BTreeMap<&str, Plan>, Registry) {
    let (mut plans, mut registry) = (BTreeMap::new(), Registry::new());
    for (id, Installed { state, manifest }) in installed {
        let mut plan = Plan::default();
        if state.enabled && !state.uninstalled {
            let taken = manifest.provided().find_map(|(contribution, name)| {
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

/// The installed extensions, in a folder of the data root: what this start
/// loaded, and what the next start will.
///
/// Extensions load only at start-up, as teta-wot fixes its Things when the
/// server is built. So installing, enabling, disabling and uninstalling
/// change the files, and [`open`](Self::open) acts on them at the next
/// start.
#[derive(Debug)]
pub struct Extensions {
    root: PathBuf,
    loaded: Vec<Loaded>,
    registry: Registry,
    installed: Mutex<BTreeMap<String, Installed>>,
}

impl Extensions {
    /// Opens the extensions in the folder `root`, as a start does.
    ///
    /// First it finishes the changes made for this start: it removes the
    /// uninstalled extensions, every version but the installed one, and
    /// unfinished installs. Then it loads each extension that is enabled
    /// and suits this OScope, in the order of their IDs. One that provides
    /// a name that an extension loaded before it provides isn't loaded at
    /// all. An extension whose folder can't be read is left out, with a
    /// warning, and left on disk.
    pub fn open(root: impl Into<PathBuf>) -> Result<Extensions, ExtError> {
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
        let (plans, registry) = plan(&installed);
        let mut loaded = Vec::new();
        for (id, plan) in &plans {
            let Installed { state, manifest } = &installed[*id];
            if let Some(version) = &plan.version {
                tracing::info!("extension {id} {version}: loaded");
                loaded.push(Loaded {
                    manifest: manifest.clone(),
                    folder: root.join(id).join(version.to_string()),
                    sha256: state.sha256.clone(),
                });
            } else if let Some(problem) = &plan.problem {
                tracing::warn!("extension {id} isn't loaded: {problem}");
            }
        }
        drop(plans);
        Ok(Extensions {
            root,
            loaded,
            registry,
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
                true
            }
        };
        if unpacked || before.as_ref() != Some(&state) {
            write_state(&folder, &state)?;
        }
        installed.insert(id.clone(), Installed { state, manifest });
        Ok((self.describe_one(&installed, &id)?, unpacked))
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
        installed
            .iter()
            .map(|(id, Installed { state, manifest })| Extension {
                manifest: manifest.clone(),
                sha256: state.sha256.clone(),
                enabled: state.enabled,
                uninstalled: state.uninstalled,
                loaded: loaded(id),
                next: plans[id.as_str()].version.clone(),
                problem: plans[id.as_str()].problem.clone(),
            })
            .collect()
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
