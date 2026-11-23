//! `.omext` packages: zip files, read with care, and made the same way each
//! time (ADR-0018).

use std::collections::{BTreeMap, HashSet};
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, DateTime, ZipArchive, ZipWriter};

use crate::manifest::check_path;
use crate::{ExtError, Manifest, OSCOPE_FOLDER, io};

/// The manifest's name, at a package's root.
pub const MANIFEST: &str = "manifest.toml";

/// The most entries a package may have.
pub const MAX_ENTRIES: usize = 10_000;

/// The most a package may unpack to: 4 GiB.
pub const MAX_UNPACKED: u64 = 4 << 30;

/// The largest manifest read: 64 KiB.
const MAX_MANIFEST: u64 = 64 << 10;

fn bad(problem: impl Into<String>) -> ExtError {
    ExtError::Package(problem.into())
}

/// A package, opened: its entries' names checked, its manifest read and
/// [checked](Manifest::check), and the files it names found. It isn't yet
/// checked against this OScope ([`Manifest::check_host`]).
pub struct Package {
    archive: ZipArchive<File>,
    /// Each entry's name, by index; a directory's ends with `/`.
    names: Vec<String>,
    manifest: Manifest,
    sha256: String,
}

impl std::fmt::Debug for Package {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let id = (&self.manifest.id, &self.manifest.version);
        f.debug_struct("Package")
            .field("id", &id)
            .finish_non_exhaustive()
    }
}

impl Package {
    /// Opens the package at `path`. Refused, unless each of these holds:
    ///
    /// - it is a zip file of at most [`MAX_ENTRIES`] entries, saying they
    ///   unpack to at most [`MAX_UNPACKED`] bytes;
    /// - each entry's name is a plain relative path: no absolute paths, no
    ///   `..`, no drive letters or `\`, and only names that Windows and
    ///   Linux both create as they are;
    /// - no two names differ only in case, as they would be one file on
    ///   Windows;
    /// - no entry is a symbolic link, or encrypted, or in `.oscope`, which
    ///   OScope keeps for itself;
    /// - its manifest is right, and the files it names are there.
    pub fn open(path: &Path) -> Result<Package, ExtError> {
        let sha256 = sha256(path)?;
        let file = File::open(path).map_err(io(path))?;
        let mut archive = ZipArchive::new(file).map_err(|e| bad(format!("not a zip file: {e}")))?;
        if archive.len() > MAX_ENTRIES {
            return Err(bad(format!("more than {MAX_ENTRIES} entries")));
        }
        let (mut names, mut seen, mut total) = (Vec::new(), HashSet::new(), 0_u64);
        for index in 0..archive.len() {
            let entry = archive
                .by_index_raw(index)
                .map_err(|e| bad(e.to_string()))?;
            let name = entry.name().map_err(|e| bad(e.to_string()))?.into_owned();
            let refuse = |why: &str| bad(format!("the entry {name:?} {why}"));
            let path = name.strip_suffix('/').filter(|_| entry.is_dir());
            let path = path.unwrap_or(&name).to_owned();
            check_path(&path).map_err(refuse)?;
            if path.split('/').next() == Some(OSCOPE_FOLDER) {
                return Err(refuse("is a name OScope keeps for itself"));
            }
            if !seen.insert(path.to_lowercase()) {
                return Err(refuse("is there twice, perhaps in another case"));
            }
            if entry.is_symlink() {
                return Err(refuse("is a symbolic link"));
            }
            if entry.encrypted() {
                return Err(refuse("is encrypted"));
            }
            total = total.saturating_add(entry.size());
            drop(entry);
            names.push(name);
        }
        if total > MAX_UNPACKED {
            return Err(bad(format!("it unpacks to more than {MAX_UNPACKED} bytes")));
        }
        let text = {
            let mut text = String::new();
            let manifest = archive
                .by_name(MANIFEST)
                .map_err(|_| bad("no manifest.toml"))?;
            manifest
                .take(MAX_MANIFEST)
                .read_to_string(&mut text)
                .map_err(|e| bad(format!("manifest.toml: {e}")))?;
            text
        };
        let manifest = Manifest::parse(&text)?;
        if let Some(file) = manifest
            .files()
            .find(|file| !names.iter().any(|n| n == file))
        {
            return Err(bad(format!(
                "the manifest names {file:?}, which it doesn't have"
            )));
        }
        Ok(Package {
            archive,
            names,
            manifest,
            sha256,
        })
    }

    /// Its manifest.
    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    /// The SHA-256 of the package file, in hex.
    pub fn sha256(&self) -> &str {
        &self.sha256
    }

    /// Unpacks every entry into the folder `into`, which must be empty:
    /// only where [`open`](Self::open) checked, and refusing an entry that
    /// unpacks to more than it says, or isn't what its checksum says.
    pub fn unpack(&mut self, into: &Path) -> Result<(), ExtError> {
        for (index, name) in self.names.iter().enumerate() {
            let mut entry = self
                .archive
                .by_index(index)
                .map_err(|e| bad(e.to_string()))?;
            let parts = name.trim_end_matches('/').split('/');
            let target = parts.fold(into.to_owned(), |path, part| path.join(part));
            if entry.is_dir() {
                fs::create_dir_all(&target).map_err(io(&target))?;
                continue;
            }
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent).map_err(io(parent))?;
            }
            // zip refuses an entry that unpacks to more than it says.
            let mut out = File::create_new(&target).map_err(io(&target))?;
            io::copy(&mut entry, &mut out).map_err(|e| bad(format!("the entry {name:?}: {e}")))?;
        }
        Ok(())
    }
}

/// The SHA-256 of the file at `path`, in lower-case hex.
pub fn sha256(path: &Path) -> Result<String, ExtError> {
    let mut hasher = Sha256::new();
    io::copy(&mut File::open(path).map_err(io(path))?, &mut hasher).map_err(io(path))?;
    Ok(hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}

/// Packs the folder `folder` into the package `out`, and gives its
/// manifest. Every file in the folder goes in, in the order of their names,
/// with the same time and permissions, so that a folder always packs to the
/// same bytes.
///
/// Refused when the manifest is wrong, a file it names isn't there, a file
/// is a symbolic link, or a name is one that [`Package::open`] refuses.
pub fn pack(folder: &Path, out: &Path) -> Result<Manifest, ExtError> {
    let path = folder.join(MANIFEST);
    let manifest = Manifest::parse(&fs::read_to_string(&path).map_err(io(&path))?)?;
    let mut files = BTreeMap::new();
    collect(folder, "", &mut files)?;
    if let Some(file) = manifest.files().find(|file| !files.contains_key(*file)) {
        return Err(bad(format!(
            "the manifest names {file:?}, which isn't there"
        )));
    }
    let written = write(&files, out);
    if written.is_err() {
        let _ = fs::remove_file(out);
    }
    written.map(|()| manifest)
}

/// Writes `files`, by their names in the package, into the package `out`.
fn write(files: &BTreeMap<String, PathBuf>, out: &Path) -> Result<(), ExtError> {
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .last_modified_time(DateTime::default())
        .unix_permissions(0o644);
    let failed = |e: zip::result::ZipError| bad(format!("{}: {e}", out.display()));
    let mut zip = ZipWriter::new(File::create(out).map_err(io(out))?);
    for (name, path) in files {
        zip.start_file(name.as_str(), options).map_err(failed)?;
        io::copy(&mut File::open(path).map_err(io(path))?, &mut zip).map_err(io(out))?;
    }
    zip.finish().map_err(failed)?;
    Ok(())
}

/// Adds each file under `folder` to `files`, by its name in the package.
fn collect(
    folder: &Path,
    prefix: &str,
    files: &mut BTreeMap<String, PathBuf>,
) -> Result<(), ExtError> {
    for entry in fs::read_dir(folder).map_err(io(folder))? {
        let entry = entry.map_err(io(folder))?;
        let path = entry.path();
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| bad(format!("{path:?} isn't UTF-8")))?;
        let name = match prefix {
            "" => name,
            _ => format!("{prefix}/{name}"),
        };
        check_path(&name).map_err(|why| bad(format!("{name:?} {why}")))?;
        if name == OSCOPE_FOLDER {
            return Err(bad(format!("{name:?} is a name OScope keeps for itself")));
        }
        let kind = entry.file_type().map_err(io(&path))?;
        if kind.is_symlink() {
            return Err(bad(format!("{name:?} is a symbolic link")));
        } else if kind.is_dir() {
            collect(&path, &name, files)?;
        } else {
            files.insert(name, path);
        }
    }
    Ok(())
}
