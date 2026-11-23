//! Templates: OScope's own, `standard`, and the user's, each a Typst
//! template for PDF and a minijinja one for HTML, kept as
//! `<folder>/<name>/report.typ` and `report.html`. A template is checked
//! by rendering the sample report through it before it is kept.

use std::fs;
use std::path::{Path, PathBuf};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{ReportError, html, pdf, sample};

/// The built-in template's name.
pub const STANDARD: &str = "standard";
const TYPST: &str = include_str!("../templates/standard.typ");
const HTML: &str = include_str!("../templates/standard.html");

/// A template: its Typst and its minijinja.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Template {
    /// Its name.
    pub name: String,
    /// Its Typst, for PDF.
    pub typst: String,
    /// Its minijinja, for HTML.
    pub html: String,
    /// Whether it is OScope's own, which can't be changed.
    pub builtin: bool,
}

/// The templates in a folder, and the built-in one.
#[derive(Clone, Debug)]
pub struct Templates {
    folder: PathBuf,
}

fn io(path: &Path) -> impl Fn(std::io::Error) -> ReportError + '_ {
    move |e| ReportError::Io(format!("{}: {e}", path.display()))
}

/// `name`, if a user's template may have it: 1 to 64 characters of `a`–`z`,
/// `0`–`9`, `-` and `_`, not the built-in's.
fn checked(name: &str) -> Result<(), ReportError> {
    let allowed = |c: char| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_';
    let fine = (1..=64).contains(&name.len()) && name.chars().all(allowed);
    match (fine, name == STANDARD) {
        (true, false) => Ok(()),
        (_, true) => Err(ReportError::Refused(format!(
            "{STANDARD} is OScope's own, which can't be changed"
        ))),
        _ => Err(ReportError::Refused(format!(
            "a template's name has 1 to 64 characters of a–z, 0–9, - and _: {name:?}"
        ))),
    }
}

impl Templates {
    /// The templates kept in `folder`, made when one is first kept.
    pub fn open(folder: &Path) -> Self {
        Self {
            folder: folder.to_owned(),
        }
    }

    /// Their names, the built-in one first.
    pub fn names(&self) -> Result<Vec<String>, ReportError> {
        let mut names = vec![STANDARD.to_owned()];
        let Ok(entries) = fs::read_dir(&self.folder) else {
            return Ok(names);
        };
        let mut found: Vec<String> = entries
            .filter_map(Result::ok)
            .filter(|e| {
                e.path().join("report.typ").is_file() && e.path().join("report.html").is_file()
            })
            .filter_map(|e| e.file_name().into_string().ok())
            .collect();
        found.sort();
        names.extend(found);
        Ok(names)
    }

    /// Template `name`.
    pub fn get(&self, name: &str) -> Result<Template, ReportError> {
        if name == STANDARD {
            return Ok(Template {
                name: name.to_owned(),
                typst: TYPST.to_owned(),
                html: HTML.to_owned(),
                builtin: true,
            });
        }
        checked(name)?;
        let folder = self.folder.join(name);
        if !folder.is_dir() {
            return Err(ReportError::Refused(format!("no template {name}")));
        }
        let read = |file: &str| {
            let path = folder.join(file);
            fs::read_to_string(&path).map_err(io(&path))
        };
        Ok(Template {
            name: name.to_owned(),
            typst: read("report.typ")?,
            html: read("report.html")?,
            builtin: false,
        })
    }

    /// Keeps template `name`, in place of one of that name, once the sample
    /// report renders through both its parts: or the first part's error.
    pub fn save(&self, name: &str, typst: &str, html_template: &str) -> Result<(), ReportError> {
        checked(name)?;
        let (report, images) = sample();
        pdf(typst, &report, &images)?;
        html(html_template, &report, &images)?;
        let folder = self.folder.join(name);
        fs::create_dir_all(&folder).map_err(io(&folder))?;
        for (file, text) in [("report.typ", typst), ("report.html", html_template)] {
            let path = folder.join(file);
            fs::write(&path, text).map_err(io(&path))?;
        }
        Ok(())
    }

    /// Deletes template `name`, a user's.
    pub fn delete(&self, name: &str) -> Result<(), ReportError> {
        checked(name)?;
        let folder = self.folder.join(name);
        match folder.is_dir() {
            true => fs::remove_dir_all(&folder).map_err(io(&folder)),
            false => Err(ReportError::Refused(format!("no template {name}"))),
        }
    }
}
