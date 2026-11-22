//! Importing files into projects and exporting their images (P30,
//! ADR-0037): jobs, and the formats they know, built in and from
//! extensions.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use oscope_core::ProjectId;
use oscope_ext::Extensions;
use oscope_jobs::{Factory, Job, JobSpec, Resource, Stop};
use oscope_store::zoom::export_dzi;
use oscope_store::{Exporter, Format, Importer, RecordKind, Store, export, import};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::upload::{Uploads, staged};

/// An import's kind.
pub(crate) const IMPORT: &str = "projects.import";
/// An export's kind.
pub(crate) const EXPORT: &str = "projects.export";
/// The format of a static Deep Zoom folder (ADR-0028), made from the
/// image's pyramid rather than by an exporter.
pub(crate) const DZI: &str = "dzi";

/// The formats the server imports and exports.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Formats {
    /// What it imports.
    pub importers: Vec<Format>,
    /// What it exports to.
    pub exporters: Vec<Format>,
}

/// The importers and exporters: OScope's own, then the extensions'.
#[derive(Clone)]
pub(crate) struct Codecs {
    importers: Vec<Arc<dyn Importer>>,
    exporters: Vec<Arc<dyn Exporter>>,
}

impl Codecs {
    pub(crate) fn new(extensions: Option<&Extensions>) -> Self {
        let (mut importers, mut exporters) = (oscope_store::importers(), oscope_store::exporters());
        if let Some(extensions) = extensions {
            importers.extend(extensions.formats().importers.iter().cloned());
            exporters.extend(extensions.formats().exporters.iter().cloned());
        }
        Self {
            importers,
            exporters,
        }
    }

    pub(crate) fn formats(&self) -> Formats {
        let mut exporters: Vec<Format> = self.exporters.iter().map(|e| e.format()).collect();
        exporters.push(Format {
            id: DZI.to_owned(),
            name: "Deep Zoom folder (DZI)".to_owned(),
            extensions: vec!["dzi".to_owned()],
        });
        Formats {
            importers: self.importers.iter().map(|i| i.format()).collect(),
            exporters,
        }
    }

    /// The importer named `format`, or else the first whose extensions
    /// match `path`.
    fn importer(&self, format: Option<&str>, path: &Path) -> Result<Arc<dyn Importer>, String> {
        let file = path
            .file_name()
            .map(|f| f.to_string_lossy().into_owned())
            .unwrap_or_default();
        let found = self.importers.iter().find(|i| {
            let named = i.format();
            format.map_or_else(|| named.matches(&file), |id| named.id == id)
        });
        found.cloned().ok_or_else(|| match format {
            Some(id) => format!("no importer is {id:?}"),
            None => format!("no importer reads {file:?}"),
        })
    }

    fn exporter(&self, format: &str) -> Result<Arc<dyn Exporter>, String> {
        let found = self.exporters.iter().find(|e| e.format().id == format);
        found
            .cloned()
            .ok_or_else(|| format!("no exporter is {format:?}"))
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ImportParams {
    project: ProjectId,
    /// A file on the server; or else
    #[serde(default)]
    path: Option<PathBuf>,
    /// a file the upload route staged (W6.3), removed once imported.
    #[serde(default)]
    upload: Option<Uuid>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    format: Option<String>,
}

fn acquisition() -> RecordKind {
    RecordKind::Acquisition
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ExportParams {
    project: ProjectId,
    record: String,
    #[serde(default = "acquisition")]
    kind: RecordKind,
    format: String,
}

fn spec(kind: &str, title: String, params: &serde_json::Value) -> JobSpec {
    JobSpec {
        kind: kind.to_owned(),
        title,
        account: None,
        params: params.clone(),
        resource: Resource::Compute,
        pausable: false,
        gpu_mib: 0,
    }
}

/// Makes import jobs, whose parameters are `project`, and `path`, a file on
/// the server, or `upload`, one the upload route staged; and optionally
/// `name` and `format`, an importer's ID. Each reads the file, with the
/// importer named or else the first whose extensions its name has, into a
/// new acquisition of the project; an upload is then removed.
pub(crate) fn import_jobs(
    store: Arc<Store>,
    codecs: Arc<Codecs>,
    uploads: Option<Arc<Uploads>>,
) -> Factory {
    Arc::new(move |value| {
        let params = ImportParams::deserialize(value).map_err(|e| e.to_string())?;
        let path = match (&params.path, params.upload) {
            (Some(path), None) => path.clone(),
            (None, Some(id)) => staged(uploads.as_deref(), id)?,
            _ => return Err("an import reads a `path` or an `upload`, one of them".to_owned()),
        };
        let importer = codecs.importer(params.format.as_deref(), &path)?;
        let file = path.file_name().unwrap_or_default().to_string_lossy();
        let title = format!("import {file}");
        let (store, uploads) = (Arc::clone(&store), uploads.clone());
        Ok(Job {
            spec: spec(IMPORT, title, value),
            run: Box::new(move |ctx| {
                Box::pin(async move {
                    let project = params.project;
                    let work = tokio::task::spawn_blocking(move || {
                        let name = params.name.as_deref();
                        let made = import(&store, project, &*importer, &path, name);
                        if let (Ok(_), Some(id), Some(uploads)) = (&made, params.upload, uploads) {
                            uploads.remove(id);
                        }
                        made
                    });
                    let acquisition = work.await.map_err(Stop::failed)?.map_err(Stop::failed)?;
                    ctx.progress(Some(1.0), "imported");
                    ctx.add_result(format!(
                        "projects/{project}/acquisitions/{}",
                        acquisition.id
                    ));
                    Ok(())
                })
            }),
        })
    })
}

/// Makes export jobs, whose parameters are `project`, `record`, its
/// `kind` (`acquisition`, the default, or `result`), and `format`, an
/// exporter's ID or `dzi`. Each writes the record's image to its folder's
/// `exports/`, a static Deep Zoom folder for `dzi`: the result is where
/// the data route serves it.
pub(crate) fn export_jobs(store: Arc<Store>, codecs: Arc<Codecs>) -> Factory {
    Arc::new(move |value| {
        let params = ExportParams::deserialize(value).map_err(|e| e.to_string())?;
        let exporter = match params.format.as_str() {
            DZI => None,
            format => Some(codecs.exporter(format)?),
        };
        let title = format!(
            "export {} {} as {}",
            params.kind.name(),
            params.record,
            params.format
        );
        let store = Arc::clone(&store);
        Ok(Job {
            spec: spec(EXPORT, title, value),
            run: Box::new(move |ctx| {
                Box::pin(async move {
                    let project = params.project;
                    let work = tokio::task::spawn_blocking(move || match exporter {
                        Some(exporter) => {
                            export(&store, project, params.kind, &params.record, &*exporter)
                        }
                        None => export_dzi(&store, project, params.kind, &params.record, None),
                    });
                    let path = work.await.map_err(Stop::failed)?.map_err(Stop::failed)?;
                    ctx.progress(Some(1.0), "exported");
                    ctx.add_result(format!("data/{project}/{path}"));
                    Ok(())
                })
            }),
        })
    })
}
