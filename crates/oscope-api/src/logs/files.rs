//! The daily files: one per UTC day, `oscope-YYYY-MM-DD.jsonl`, with one
//! record per line as JSON.

use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use chrono::{Days, NaiveDate};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::Record;

/// A daily file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct LogFile {
    /// Its name, such as `oscope-2026-11-12.jsonl`.
    pub name: String,
    /// The UTC day it holds.
    pub day: NaiveDate,
    /// Its size.
    pub bytes: u64,
}

/// The daily files in `folder`, newest first. Other files are left out.
pub fn list(folder: &Path) -> io::Result<Vec<LogFile>> {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(folder)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if let Some(day) = day_of(&name) {
            // The file's own, not the folder entry's: on Windows, an entry's
            // size lags while the file is open.
            let bytes = std::fs::metadata(entry.path())?.len();
            files.push(LogFile { name, day, bytes });
        }
    }
    files.sort_by_key(|file| std::cmp::Reverse(file.day));
    Ok(files)
}

/// Deletes the daily files older than the last `keep_days` days, `today`
/// included: with 30, today's file and the 29 before it stay. Returns the
/// files it deleted.
pub fn prune(folder: &Path, today: NaiveDate, keep_days: u32) -> io::Result<Vec<PathBuf>> {
    let oldest = today - Days::new(u64::from(keep_days.max(1) - 1));
    let mut deleted = Vec::new();
    for file in list(folder)? {
        if file.day < oldest {
            let path = folder.join(&file.name);
            std::fs::remove_file(&path)?;
            deleted.push(path);
        }
    }
    Ok(deleted)
}

/// A daily file's name.
fn name(day: NaiveDate) -> String {
    format!("oscope-{day}.jsonl")
}

/// The day of a daily file, from its name.
fn day_of(name: &str) -> Option<NaiveDate> {
    let day = name.strip_prefix("oscope-")?.strip_suffix(".jsonl")?;
    NaiveDate::parse_from_str(day, "%Y-%m-%d").ok()
}

/// Writes records to the file of their day, starting a new file, and
/// deleting old ones, when the day changes.
#[derive(Debug)]
pub(crate) struct DailyFiles {
    folder: PathBuf,
    keep_days: u32,
    open: Option<(NaiveDate, File)>,
}

impl DailyFiles {
    /// Writes into `folder`, which it creates. It deletes old files now,
    /// and at each new day.
    pub(crate) fn new(folder: &Path, today: NaiveDate, keep_days: u32) -> io::Result<Self> {
        std::fs::create_dir_all(folder)?;
        prune(folder, today, keep_days)?;
        Ok(Self {
            folder: folder.to_owned(),
            keep_days,
            open: None,
        })
    }

    /// The folder.
    pub(crate) fn folder(&self) -> &Path {
        &self.folder
    }

    /// Appends `record` to its day's file, in one write, so that the file is
    /// whole up to the last record even if the program stops.
    pub(crate) fn write(&mut self, record: &Record) -> io::Result<()> {
        let day = record.time.date_naive();
        let file = match &mut self.open {
            Some((open, file)) if *open == day => file,
            _ => {
                let path = self.folder.join(name(day));
                let file = OpenOptions::new().create(true).append(true).open(path)?;
                prune(&self.folder, day, self.keep_days)?;
                &mut self.open.insert((day, file)).1
            }
        };
        let mut line = serde_json::to_vec(record)?;
        line.push(b'\n');
        file.write_all(&line)
    }
}
