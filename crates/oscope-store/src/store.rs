//! The store: projects and their records in folders under the data root,
//! the folders the truth, and the index beside them (ADR-0021).

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use oscope_core::version::{Versioned, from_document, to_document};
use oscope_core::{AcquisitionId, ProjectId, ReportId, ResultId};
use schemars::JsonSchema;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::index::{Index, RecordRow};
use crate::records::{Record, RecordKind};
use crate::{
    Acquisition, ImageInfo, ImageReader, ImageWriter, Input, Project, Provenance, Report, Result,
    ResultRecord, StoreError, io, thumbnail,
};

/// Does `step`, and on Windows again for up to 2 s while access is denied:
/// another program, such as a virus scanner reading new files, may hold a
/// file in a folder for a moment (P53's soak found it).
fn patiently(mut step: impl FnMut() -> std::io::Result<()>) -> std::io::Result<()> {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        match step() {
            Err(e)
                if cfg!(windows)
                    && e.kind() == std::io::ErrorKind::PermissionDenied
                    && Instant::now() < deadline =>
            {
                std::thread::sleep(Duration::from_millis(50));
            }
            done => return done,
        }
    }
}

/// A project's document, in its folder.
const PROJECT: &str = "project.json";
/// The file whose lock a store holds while it changes folders.
const LOCK: &str = ".lock";
/// The prefix of a project's folder while it is deleted.
const DELETING: &str = ".deleting-";

/// A new project.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct NewProject {
    /// Its name: 1 to 200 characters.
    pub name: String,
    /// The sample: what is on the slide.
    #[serde(default)]
    pub sample: String,
    /// Its tags.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Who makes it.
    #[serde(default)]
    pub owner: String,
    /// Anything else.
    #[serde(default)]
    pub metadata: Map<String, Value>,
}

/// Changes to a project. Each field given replaces its own, except
/// `metadata`, whose keys are set one by one, `null` removing one.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Change {
    /// A new name.
    #[serde(default)]
    pub name: Option<String>,
    /// A new sample.
    #[serde(default)]
    pub sample: Option<String>,
    /// New tags, replacing the old.
    #[serde(default)]
    pub tags: Option<Vec<String>>,
    /// Metadata to set, or remove.
    #[serde(default)]
    pub metadata: Option<Map<String, Value>>,
}

/// The projects to list.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Query {
    /// Only projects whose name, sample or tags contain this, in any case.
    #[serde(default)]
    pub text: Option<String>,
    /// Only projects with this tag.
    #[serde(default)]
    pub tag: Option<String>,
    /// How many to skip.
    #[serde(default)]
    pub offset: Option<usize>,
    /// At most how many: 50 if not given, and 500 at most.
    #[serde(default)]
    pub limit: Option<usize>,
    /// Their order: the most recently changed first, if not given (W6.1).
    #[serde(default)]
    pub sort: Option<Order>,
}

/// How projects are listed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Order {
    /// The most recently changed first.
    #[default]
    Changed,
    /// By name, in any case.
    Name,
}

/// A project, as a page lists it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Summary {
    /// The project.
    #[serde(flatten)]
    pub project: Project,
    /// How many acquisitions it has.
    pub acquisitions: usize,
    /// How many results.
    pub results: usize,
    /// How many reports.
    pub reports: usize,
}

/// A page of projects, the most recently changed first.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Page {
    /// How many projects the query finds in all.
    pub total: usize,
    /// How many were skipped before this page.
    pub offset: usize,
    /// This page's.
    pub projects: Vec<Summary>,
}

/// A project, and everything in it, the oldest first.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Contents {
    /// The project.
    pub project: Project,
    /// Its acquisitions.
    pub acquisitions: Vec<Acquisition>,
    /// Its results.
    pub results: Vec<ResultRecord>,
    /// Its reports.
    pub reports: Vec<Report>,
}

fn document_error(path: &Path, reason: impl std::fmt::Display) -> StoreError {
    StoreError::Document {
        path: path.to_owned(),
        reason: reason.to_string(),
    }
}

fn json<T: Serialize>(value: &T) -> Result<String> {
    serde_json::to_string(value).map_err(|e| StoreError::Index(e.to_string()))
}

/// Writes `value` as a versioned document at `path`, through a file renamed
/// over it, so that a reader finds the old document or the new, whole.
pub(crate) fn write<T: Versioned>(path: &Path, value: &T) -> Result<()> {
    let document = to_document(value).map_err(|e| document_error(path, e))?;
    let text = serde_json::to_string_pretty(&document).map_err(|e| document_error(path, e))?;
    let temporary = path.with_extension("json.tmp");
    fs::write(&temporary, text + "\n").map_err(io(&temporary))?;
    fs::rename(&temporary, path).map_err(io(path))
}

pub(crate) fn read<T: Versioned>(path: &Path) -> Result<T> {
    let text = fs::read_to_string(path).map_err(io(path))?;
    let document: Value = serde_json::from_str(&text).map_err(|e| document_error(path, e))?;
    from_document(document).map_err(|e| document_error(path, e))
}

/// `name`, trimmed: 1 to 200 characters.
pub(crate) fn checked_name(name: &str) -> Result<String> {
    let name = name.trim();
    match name.chars().count() {
        1..=200 => Ok(name.to_owned()),
        _ => Err(StoreError::Refused(format!(
            "a name has 1 to 200 characters: {name:?}"
        ))),
    }
}

/// `tags`, trimmed, sorted and each once: each 1 to 64 characters, on one
/// line.
fn checked_tags(tags: Vec<String>) -> Result<Vec<String>> {
    let mut tags: Vec<String> = tags.iter().map(|tag| tag.trim().to_owned()).collect();
    if let Some(tag) = tags
        .iter()
        .find(|tag| !(1..=64).contains(&tag.chars().count()) || tag.contains(['\n', '\r']))
    {
        return Err(StoreError::Refused(format!(
            "a tag has 1 to 64 characters, on one line: {tag:?}"
        )));
    }
    tags.sort();
    tags.dedup();
    Ok(tags)
}

/// `record`, as the index keeps it.
fn row<R: Record>(record: &R) -> Result<RecordRow> {
    Ok(RecordRow {
        kind: R::KIND,
        id: record.id(),
        project: record.project().to_string(),
        name: record.name().to_owned(),
        created: record.created().to_rfc3339(),
        document: json(record)?,
    })
}

fn parse<T: DeserializeOwned>(documents: Vec<String>) -> Result<Vec<T>> {
    documents
        .iter()
        .map(|document| {
            serde_json::from_str(document).map_err(|e| StoreError::Index(e.to_string()))
        })
        .collect()
}

/// The folders in `folder`, if it exists.
fn folders(folder: &Path) -> Result<Vec<PathBuf>> {
    let entries = match fs::read_dir(folder) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(io(folder)(error)),
    };
    let mut found = Vec::new();
    for entry in entries {
        let path = entry.map_err(io(folder))?.path();
        if path.is_dir() {
            found.push(path);
        }
    }
    found.sort();
    Ok(found)
}

/// The projects of a data root, in `projects/`, and their index.
///
/// Every change writes the folders first, then the index; reads list from
/// the index. While it changes folders, a store holds the lock of
/// `projects/.lock`, so that stores in other processes, or other stores in
/// this one, wait their turn.
pub struct Store {
    root: PathBuf,
    index: Mutex<Index>,
}

impl std::fmt::Debug for Store {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Store")
            .field("root", &self.root)
            .finish_non_exhaustive()
    }
}

impl Store {
    /// Opens the projects of `data_root`, whose folder is made with the
    /// first, and their index at `index`: a file, made with its folder if
    /// need be, or `:memory:`.
    pub fn open(data_root: &Path, index: &Path) -> Result<Self> {
        if let Some(folder) = index.parent().filter(|_| index != Path::new(":memory:")) {
            fs::create_dir_all(folder).map_err(io(folder))?;
        }
        Ok(Self {
            root: data_root.join("projects"),
            index: Mutex::new(Index::open(index)?),
        })
    }

    /// The folder of the projects.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// A project's folder.
    pub fn folder(&self, id: ProjectId) -> PathBuf {
        self.root.join(id.to_string())
    }

    fn index(&self) -> MutexGuard<'_, Index> {
        self.index.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Waits for the store's lock, and holds it until the file is dropped.
    pub(crate) fn lock(&self) -> Result<fs::File> {
        fs::create_dir_all(&self.root).map_err(io(&self.root))?;
        let path = self.root.join(LOCK);
        let file = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&path)
            .map_err(io(&path))?;
        file.lock().map_err(io(&path))?;
        Ok(file)
    }

    fn save(&self, project: &Project) -> Result<()> {
        write(&self.folder(project.id).join(PROJECT), project)?;
        self.index().put_project(project, &json(project)?)
    }

    /// Makes a project.
    pub fn create(&self, new: NewProject) -> Result<Project> {
        let now = Utc::now();
        let project = Project {
            id: ProjectId::generate(),
            name: checked_name(&new.name)?,
            sample: new.sample.trim().to_owned(),
            tags: checked_tags(new.tags)?,
            owner: new.owner.trim().to_owned(),
            created: now,
            modified: now,
            metadata: new.metadata,
        };
        let _lock = self.lock()?;
        let folder = self.folder(project.id);
        fs::create_dir_all(&folder).map_err(io(&folder))?;
        self.save(&project)?;
        Ok(project)
    }

    /// A project, from its folder.
    pub fn project(&self, id: ProjectId) -> Result<Project> {
        let path = self.folder(id).join(PROJECT);
        match path.is_file() {
            true => read(&path),
            false => Err(StoreError::NotFound {
                kind: "project",
                id: id.to_string(),
            }),
        }
    }

    /// A project and everything in it.
    pub fn contents(&self, id: ProjectId) -> Result<Contents> {
        let project = self.project(id)?;
        let index = self.index();
        let of = |kind| index.records(&id.to_string(), kind);
        Ok(Contents {
            project,
            acquisitions: parse(of(RecordKind::Acquisition)?)?,
            results: parse(of(RecordKind::Result)?)?,
            reports: parse(of(RecordKind::Report)?)?,
        })
    }

    /// Changes a project: renames it, tags it, or sets its metadata.
    pub fn change(&self, id: ProjectId, change: Change) -> Result<Project> {
        let name = change.name.as_deref().map(checked_name).transpose()?;
        let tags = change.tags.map(checked_tags).transpose()?;
        let _lock = self.lock()?;
        let mut project = self.project(id)?;
        project.name = name.unwrap_or(project.name);
        project.tags = tags.unwrap_or(project.tags);
        if let Some(sample) = change.sample {
            project.sample = sample.trim().to_owned();
        }
        for (key, value) in change.metadata.into_iter().flatten() {
            match value {
                Value::Null => project.metadata.remove(&key),
                value => project.metadata.insert(key, value),
            };
        }
        project.modified = Utc::now();
        self.save(&project)?;
        Ok(project)
    }

    /// Deletes a project, with everything in it. Its folder is renamed out
    /// of the way first, so that nothing lists it while it goes.
    pub fn delete(&self, id: ProjectId) -> Result<()> {
        let _lock = self.lock()?;
        self.project(id)?;
        let (folder, going) = (self.folder(id), self.root.join(format!("{DELETING}{id}")));
        patiently(|| fs::rename(&folder, &going)).map_err(io(&folder))?;
        self.index().remove_project(&id.to_string())?;
        patiently(|| fs::remove_dir_all(&going)).map_err(io(&going))
    }

    /// Deletes result `id` of `project`, with its folder, which is renamed
    /// out of the way first: refused while another result was made from it
    /// (P48).
    pub fn delete_result(&self, project: ProjectId, id: ResultId) -> Result<()> {
        self.delete_record(project, RecordKind::Result, &id.to_string())
    }

    /// Deletes record `id` of `project`, of `kind`, with its folder, which is
    /// renamed out of the way first: refused while a result was made from
    /// it (P48; W6.2).
    pub fn delete_record(&self, project: ProjectId, kind: RecordKind, id: &str) -> Result<()> {
        let _lock = self.lock()?;
        let contents = self.contents(project)?;
        let known = match kind {
            RecordKind::Acquisition => contents.acquisitions.iter().any(|r| r.id.to_string() == id),
            RecordKind::Result => contents.results.iter().any(|r| r.id.to_string() == id),
            RecordKind::Report => contents.reports.iter().any(|r| r.id.to_string() == id),
        };
        if !known {
            return Err(StoreError::NotFound {
                kind: kind.name(),
                id: id.to_owned(),
            });
        }
        let from = |input: &Input| match input {
            Input::Acquisition(a) => a.to_string() == id,
            Input::Result(r) => r.to_string() == id,
        };
        let made: Vec<&str> = contents
            .results
            .iter()
            .filter(|r| r.provenance.inputs.iter().any(from))
            .map(|r| r.name.as_str())
            .collect();
        if !made.is_empty() {
            return Err(StoreError::Refused(format!(
                "{} {id} is what {} was made from: delete that first",
                kind.name(),
                made.join(", ")
            )));
        }
        // The ID is one of the project's records': a plain name, safe in a path.
        let records = self.folder(project).join(kind.folder());
        let (folder, going) = (records.join(id), records.join(format!("{DELETING}{id}")));
        patiently(|| fs::rename(&folder, &going)).map_err(io(&folder))?;
        self.index().remove_record(id)?;
        patiently(|| fs::remove_dir_all(&going)).map_err(io(&going))
    }

    /// A page of the projects `query` finds, the most recently changed
    /// first.
    pub fn list(&self, query: &Query) -> Result<Page> {
        let (total, listed) = self.index().page(query)?;
        let projects = listed
            .into_iter()
            .map(|(document, [acquisitions, results, reports])| {
                Ok(Summary {
                    project: serde_json::from_str(&document)
                        .map_err(|e| StoreError::Index(e.to_string()))?,
                    acquisitions,
                    results,
                    reports,
                })
            })
            .collect::<Result<_>>()?;
        Ok(Page {
            total,
            offset: query.offset.unwrap_or(0),
            projects,
        })
    }

    /// Starts an acquisition of `project`, named `name`, whose raw image is
    /// as `image` describes. Write the image through the draft, then commit
    /// it.
    pub fn acquisition(
        &self,
        project: ProjectId,
        name: &str,
        image: &ImageInfo,
    ) -> Result<Draft<'_, Acquisition>> {
        let record = Acquisition {
            id: AcquisitionId::generate(),
            project,
            name: checked_name(name)?,
            created: Utc::now(),
            image: image.clone(),
            instrument: Value::Null,
            objective: Value::Null,
            calibration: Value::Null,
            camera: Value::Null,
            plan: Value::Null,
            frames: Vec::new(),
        };
        self.draft(record, Some(image))
    }

    /// Starts a result of `project`, named `name`, made as `provenance` says:
    /// an image as `image` describes, if one is given, or files the caller
    /// writes into the draft's folder.
    pub fn result(
        &self,
        project: ProjectId,
        name: &str,
        provenance: Provenance,
        image: Option<&ImageInfo>,
    ) -> Result<Draft<'_, ResultRecord>> {
        let record = ResultRecord {
            id: ResultId::generate(),
            project,
            name: checked_name(name)?,
            created: Utc::now(),
            provenance,
            image: image.cloned(),
        };
        self.draft(record, image)
    }

    /// Starts a report of `project`, named `name`, whose files the caller
    /// writes into the draft's folder, and lists in its record.
    pub fn report(&self, project: ProjectId, name: &str) -> Result<Draft<'_, Report>> {
        let record = Report {
            id: ReportId::generate(),
            project,
            name: checked_name(name)?,
            created: Utc::now(),
            files: Vec::new(),
        };
        self.draft(record, None)
    }

    fn draft<R: Record>(&self, record: R, image: Option<&ImageInfo>) -> Result<Draft<'_, R>> {
        self.project(record.project())?;
        let folder = self
            .folder(record.project())
            .join(R::KIND.folder())
            .join(record.id());
        fs::create_dir_all(&folder).map_err(io(&folder))?;
        let mut draft = Draft {
            store: self,
            record,
            folder,
            image: None,
            committed: false,
        };
        if let Some(info) = image {
            let path = draft.folder.join(R::IMAGE);
            draft.image = Some(ImageWriter::create(&path, info, draft.record.name())?);
        }
        Ok(draft)
    }

    /// A thumbnail: a record's, or else the project's: its newest
    /// acquisition's, the slide as it was taken, or if it has none its newest
    /// image result's.
    pub fn thumbnail(&self, project: ProjectId, record: Option<&str>) -> Result<PathBuf> {
        let folder = self.folder(project);
        let candidates: Vec<PathBuf> = match record {
            // An ID, and no path that could lead out of the project.
            Some(id) if uuid::Uuid::try_parse(id).is_err() => {
                return Err(StoreError::Refused(format!("{id:?} isn't a record's ID")));
            }
            Some(id) => RecordKind::ALL
                .iter()
                .map(|kind| folder.join(kind.folder()).join(id))
                .collect(),
            None => {
                let contents = self.contents(project)?;
                let acquisitions = contents
                    .acquisitions
                    .iter()
                    .map(|a| (a.created, a.id.to_string()));
                let results = contents
                    .results
                    .iter()
                    .map(|r| (r.created, r.id.to_string()));
                let newest_first = |kind: &str, made: Vec<(DateTime<Utc>, String)>| {
                    let mut made = made;
                    made.sort_by_key(|(created, _)| std::cmp::Reverse(*created));
                    made.into_iter()
                        .map(|(_, id)| folder.join(kind).join(id))
                        .collect::<Vec<_>>()
                };
                let mut candidates = newest_first("acquisitions", acquisitions.collect());
                candidates.extend(newest_first("results", results.collect()));
                candidates
            }
        };
        let found = candidates
            .into_iter()
            .map(|c| c.join(thumbnail::FILE))
            .find(|t| t.is_file());
        found.ok_or_else(|| StoreError::NotFound {
            kind: "thumbnail",
            id: record.map_or_else(|| project.to_string(), str::to_owned),
        })
    }

    /// Rebuilds the index from the folders, which are the truth: after a
    /// crash, or with the index lost. Records without their document, being
    /// made or abandoned, are left out; projects half deleted are deleted.
    /// A document that doesn't read is left out too, and logged: it returns
    /// those documents.
    pub fn rebuild_index(&self) -> Result<Vec<PathBuf>> {
        let _lock = self.lock()?;
        let (mut projects, mut records, mut skipped) = (Vec::new(), Vec::new(), Vec::new());
        let mut skip = |path: &Path, error: StoreError| {
            tracing::warn!("the index leaves out {}: {error}", path.display());
            skipped.push(path.to_owned());
        };
        for folder in folders(&self.root)? {
            let name = folder
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default();
            if name.starts_with(DELETING) {
                fs::remove_dir_all(&folder).map_err(io(&folder))?;
                continue;
            }
            let file = folder.join(PROJECT);
            if !file.is_file() {
                continue;
            }
            let project: Project = match read(&file) {
                Ok(project) => project,
                Err(error) => {
                    skip(&file, error);
                    continue;
                }
            };
            for kind in RecordKind::ALL {
                for place in folders(&folder.join(kind.folder()))? {
                    // A record half deleted is deleted.
                    let name = place.file_name().and_then(|n| n.to_str());
                    if name.is_some_and(|n| n.starts_with(DELETING)) {
                        fs::remove_dir_all(&place).map_err(io(&place))?;
                        continue;
                    }
                    let document = place.join(format!("{}.json", kind.name()));
                    if !document.is_file() {
                        continue;
                    }
                    let found = match kind {
                        RecordKind::Acquisition => {
                            read::<Acquisition>(&document).and_then(|r| row(&r))
                        }
                        RecordKind::Result => read::<ResultRecord>(&document).and_then(|r| row(&r)),
                        RecordKind::Report => read::<Report>(&document).and_then(|r| row(&r)),
                    };
                    match found {
                        Ok(found) => records.push(found),
                        Err(error) => skip(&document, error),
                    }
                }
            }
            let document = json(&project)?;
            projects.push((project, document));
        }
        self.index().replace(&projects, &records)?;
        Ok(skipped)
    }

    /// Every row of the index, as text, sorted: what
    /// [`rebuild_index`](Self::rebuild_index) gives again from the folders.
    pub fn index_rows(&self) -> Result<Vec<String>> {
        self.index().rows()
    }
}

/// A record being made. Its folder and its image come first, and its
/// document last, at [`commit`](Self::commit): until then nothing lists it.
/// Dropped without a commit, its folder goes.
pub struct Draft<'a, R: Record> {
    store: &'a Store,
    record: R,
    folder: PathBuf,
    image: Option<ImageWriter>,
    committed: bool,
}

impl<R: Record> Draft<'_, R> {
    /// The record, to fill in: an acquisition's camera and frames, a
    /// report's files.
    pub fn record(&mut self) -> &mut R {
        &mut self.record
    }

    /// Its folder, for files of its own.
    pub fn folder(&self) -> &Path {
        &self.folder
    }

    /// Its image, to write, if it has one.
    pub fn image(&self) -> Option<&ImageWriter> {
        self.image.as_ref()
    }

    /// Finishes its image's pyramid and its thumbnail, then writes its
    /// document and indexes it: the record.
    pub fn commit(mut self) -> Result<R> {
        if let Some(writer) = self.image.take() {
            let info = writer.finish()?;
            self.record.set_image(info);
            let reader = ImageReader::open(&self.folder.join(R::IMAGE))?;
            thumbnail::make(&reader, &self.folder.join(thumbnail::FILE))?;
        }
        let store = self.store;
        let _lock = store.lock()?;
        let mut project = store.project(self.record.project())?;
        write(
            &self.folder.join(format!("{}.json", R::KIND.name())),
            &self.record,
        )?;
        store.index().put_record(&row(&self.record)?)?;
        project.modified = Utc::now();
        store.save(&project)?;
        self.committed = true;
        Ok(self.record.clone())
    }
}

impl<R: Record> Drop for Draft<'_, R> {
    fn drop(&mut self) {
        if !self.committed {
            let _ = fs::remove_dir_all(&self.folder);
        }
    }
}
