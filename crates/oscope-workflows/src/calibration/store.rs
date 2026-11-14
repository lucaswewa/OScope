//! The calibration store (ADR-0023): every set kept as a JSON file named by
//! its ID, and which set of each key is active, in a folder of the data
//! root.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};

use chrono::Utc;
use serde_json::Value;
use uuid::Uuid;

use super::set::{CalibrationSet, FORMAT, Instrument, SetSummary, Values};

/// The file that says which set of each key is active.
const ACTIVE: &str = "active.json";

/// Why the store can't do something.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum StoreError {
    /// No set has the ID.
    #[error("not found (404): no calibration set {0}")]
    NotFound(Uuid),
    /// A set to import isn't one.
    #[error("refused (400): {0}")]
    Invalid(String),
    /// A file couldn't be read or written.
    #[error("the calibration store's {}: {source}", path.display())]
    Io {
        /// The file.
        path: PathBuf,
        /// What went wrong.
        source: io::Error,
    },
}

fn io(path: &Path) -> impl FnOnce(io::Error) -> StoreError + '_ {
    move |source| StoreError::Io {
        path: path.to_owned(),
        source,
    }
}

#[derive(Default)]
struct State {
    sets: BTreeMap<Uuid, CalibrationSet>,
    /// Each key's active set.
    active: BTreeMap<String, Uuid>,
}

/// The calibration sets, in a folder.
pub struct CalibrationStore {
    folder: PathBuf,
    state: Mutex<State>,
}

impl std::fmt::Debug for CalibrationStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CalibrationStore")
            .field("folder", &self.folder)
            .finish_non_exhaustive()
    }
}

/// Writes `value` to `path` whole: to a part file, renamed into place.
fn write_json(path: &Path, value: &impl serde::Serialize) -> Result<(), StoreError> {
    let text = serde_json::to_string_pretty(value).expect("a set's JSON");
    let part = path.with_extension("json.part");
    fs::write(&part, text).map_err(io(&part))?;
    fs::rename(&part, path).map_err(io(path))
}

impl CalibrationStore {
    /// The store in `folder`, which is made if it is missing. Every set
    /// there is read; a file that isn't one is skipped, with a warning.
    pub fn open(folder: impl Into<PathBuf>) -> Result<Self, StoreError> {
        let folder = folder.into();
        fs::create_dir_all(&folder).map_err(io(&folder))?;
        let mut state = State::default();
        for entry in fs::read_dir(&folder).map_err(io(&folder))? {
            let path = entry.map_err(io(&folder))?.path();
            let named = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if !named.ends_with(".json") || named == ACTIVE {
                continue;
            }
            let read = fs::read(&path).map_err(io(&path))?;
            match serde_json::from_slice::<CalibrationSet>(&read) {
                Ok(set) if set.format == FORMAT => {
                    state.sets.insert(set.id, set);
                }
                Ok(set) => tracing::warn!("{}: a set of format {}", path.display(), set.format),
                Err(error) => tracing::warn!("{} isn't a calibration set: {error}", path.display()),
            }
        }
        let active = folder.join(ACTIVE);
        if active.exists() {
            let read = fs::read(&active).map_err(io(&active))?;
            let map: BTreeMap<String, Uuid> = serde_json::from_slice(&read)
                .map_err(|e| io(&active)(io::Error::new(io::ErrorKind::InvalidData, e)))?;
            state.active = map
                .into_iter()
                .filter(|(_, id)| state.sets.contains_key(id))
                .collect();
        }
        Ok(Self {
            folder,
            state: Mutex::new(state),
        })
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Every set in brief, newest first.
    pub fn list(&self) -> Vec<SetSummary> {
        let state = self.state();
        let mut sets: Vec<&CalibrationSet> = state.sets.values().collect();
        sets.sort_by_key(|s| std::cmp::Reverse((s.created, s.id)));
        let summary = |set: &CalibrationSet| SetSummary {
            id: set.id,
            kind: set.values.kind().to_owned(),
            key: set.values.key(),
            version: set.version,
            created: set.created,
            account: set.account.clone(),
            active: state.active.get(&set.values.key()) == Some(&set.id),
            headline: set.values.headline(),
        };
        sets.into_iter().map(summary).collect()
    }

    /// The set `id`.
    pub fn get(&self, id: Uuid) -> Result<CalibrationSet, StoreError> {
        let state = self.state();
        state.sets.get(&id).cloned().ok_or(StoreError::NotFound(id))
    }

    /// The active set of `key`, if one is.
    pub fn active(&self, key: &str) -> Option<CalibrationSet> {
        let state = self.state();
        let id = state.active.get(key)?;
        state.sets.get(id).cloned()
    }

    /// Saves `set` as the next version of its key, active if `activate`.
    fn add(&self, mut set: CalibrationSet, activate: bool) -> Result<CalibrationSet, StoreError> {
        let mut state = self.state();
        let key = set.values.key();
        let last = state.sets.values().filter(|s| s.values.key() == key);
        set.version = last.map(|s| s.version).max().unwrap_or(0) + 1;
        write_json(&self.folder.join(format!("{}.json", set.id)), &set)?;
        state.sets.insert(set.id, set.clone());
        if activate {
            let mut active = state.active.clone();
            active.insert(key, set.id);
            write_json(&self.folder.join(ACTIVE), &active)?;
            state.active = active;
        }
        Ok(set)
    }

    /// Saves a new set of `values`, measured on `instrument` by `account`,
    /// as the next version of its key; and makes it active if `activate`.
    pub fn save(
        &self,
        values: Values,
        instrument: Instrument,
        account: Option<String>,
        activate: bool,
    ) -> Result<CalibrationSet, StoreError> {
        let values = values.checked().map_err(StoreError::Invalid)?;
        let set = CalibrationSet {
            format: FORMAT,
            id: Uuid::now_v7(),
            version: 0,
            created: Utc::now(),
            account,
            instrument,
            values,
            imported_from: None,
        };
        self.add(set, activate)
    }

    /// Makes the set `id` its key's active one.
    pub fn activate(&self, id: Uuid) -> Result<CalibrationSet, StoreError> {
        let mut state = self.state();
        let set = state
            .sets
            .get(&id)
            .cloned()
            .ok_or(StoreError::NotFound(id))?;
        let mut active = state.active.clone();
        active.insert(set.values.key(), id);
        write_json(&self.folder.join(ACTIVE), &active)?;
        state.active = active;
        Ok(set)
    }

    /// The set `id`, as its JSON.
    pub fn export(&self, id: Uuid) -> Result<Value, StoreError> {
        Ok(serde_json::to_value(self.get(id)?).expect("a set's JSON"))
    }

    /// A set exported elsewhere: checked, and saved as the next version of
    /// its key, not active, recording the ID it came with. When it was made,
    /// and by whom, are kept.
    pub fn import(&self, set: Value) -> Result<CalibrationSet, StoreError> {
        let invalid = |e: String| StoreError::Invalid(format!("not a calibration set: {e}"));
        let set: CalibrationSet =
            serde_json::from_value(set).map_err(|e| invalid(e.to_string()))?;
        if set.format != FORMAT {
            return Err(invalid(format!(
                "format {}, where {FORMAT} is read",
                set.format
            )));
        }
        let values = set.values.checked().map_err(invalid)?;
        let imported = CalibrationSet {
            id: Uuid::now_v7(),
            values,
            imported_from: Some(set.id),
            ..set
        };
        self.add(imported, false)
    }
}
