//! The manifest: `manifest.toml`, at the root of every package (ADR-0018).

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use schemars::JsonSchema;
use semver::{Version, VersionReq};
use serde::{Deserialize, Serialize};

use crate::{ExtError, OWN_DRIVERS};

/// The version of the extension API this OScope offers, which a package's
/// `oscope-api` range must match. A minor version adds to the API; a major
/// version breaks it (ADR-0018).
pub const API_VERSION: Version = Version::new(1, 0, 0);

/// The platforms a Rust package can have a library for (ADR-0003).
pub const PLATFORMS: [&str; 2] = ["windows-x86_64", "linux-x86_64"];

/// This platform, as a manifest names it; `None` on a platform OScope
/// doesn't support, where no library can be loaded.
pub const PLATFORM: Option<&str> = if cfg!(all(windows, target_arch = "x86_64")) {
    Some("windows-x86_64")
} else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
    Some("linux-x86_64")
} else {
    None
};

/// What a package says about itself, in `manifest.toml`:
///
/// ```toml
/// id = "org.example.emboss"
/// name = "Emboss"
/// version = "1.0.0"
/// license = "MIT"
/// publisher = "Example Lab"
/// oscope-api = "^1.0"
/// kind = "rust"
///
/// [provides]
/// nodes = ["example.emboss"]
///
/// [rust.libraries]
/// windows-x86_64 = "lib/emboss.dll"
/// linux-x86_64 = "lib/libemboss.so"
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct Manifest {
    /// Its ID, in reverse DNS: at least two dotted names, in lower case.
    pub id: String,
    /// Its name, for people.
    pub name: String,
    /// Its version.
    #[schemars(with = "String")]
    pub version: Version,
    /// Its licence, as an SPDX expression such as `MIT`.
    pub license: String,
    /// Who publishes it.
    pub publisher: String,
    /// What it does.
    #[serde(default)]
    pub description: String,
    /// The versions of the extension API it works with, such as `^1.0`.
    #[schemars(with = "String")]
    pub oscope_api: VersionReq,
    /// What it is written in.
    pub kind: Kind,
    /// The files in the package it uses as they are: panel schemas and
    /// report templates.
    #[serde(default)]
    pub assets: Vec<String>,
    /// What it adds to OScope: names, by contribution.
    pub provides: BTreeMap<Contribution, Vec<String>>,
    /// A Rust package's libraries.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rust: Option<Rust>,
    /// A Python package's environment and entry point.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub python: Option<Python>,
}

/// What an extension is written in (D9, D10).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// A native library behind the Rust SDK's C ABI (ADR-0019).
    Rust,
    /// A Python package, run in a worker of its own (ADR-0020).
    Python,
}

/// What an extension can add to OScope (D11). Each arrives with the feature
/// it extends: nodes in P25 and P26, drivers and panels in P27, importers
/// and exporters in P30, workflows in P36 and reports in P47.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "lowercase")]
pub enum Contribution {
    /// Processing nodes, by node ID.
    Nodes,
    /// Hardware drivers.
    Drivers,
    /// Scan workflows.
    Workflows,
    /// Panels, which the app draws from their schemas.
    Panels,
    /// Importers of other formats.
    Importers,
    /// Exporters to other formats.
    Exporters,
    /// Report generators.
    Reports,
}

/// A Rust package's libraries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Rust {
    /// For each platform it is built for, its library's path in the package.
    pub libraries: BTreeMap<String, String>,
}

/// A Python package's environment and entry point (ADR-0020).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct Python {
    /// The Python versions it needs, such as `>=3.12`.
    pub requires_python: String,
    /// The packages it needs, as uv takes them, such as `numpy>=2`.
    #[serde(default)]
    pub dependencies: Vec<String>,
    /// The module OScope imports, such as `emboss.nodes`.
    pub entry: String,
}

impl fmt::Display for Kind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Kind::Rust => "rust",
            Kind::Python => "python",
        })
    }
}

impl fmt::Display for Contribution {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Contribution::Nodes => "nodes",
            Contribution::Drivers => "drivers",
            Contribution::Workflows => "workflows",
            Contribution::Panels => "panels",
            Contribution::Importers => "importers",
            Contribution::Exporters => "exporters",
            Contribution::Reports => "reports",
        })
    }
}

fn wrong(problem: impl Into<String>) -> ExtError {
    ExtError::Manifest(problem.into())
}

fn incompatible(range: &VersionReq) -> ExtError {
    ExtError::Incompatible {
        needs: range.to_string(),
        offers: API_VERSION.to_string(),
    }
}

impl Manifest {
    /// Reads a manifest, and [checks](Self::check) it. When it can't be
    /// read, and its `oscope-api` range doesn't match this OScope, it says
    /// so instead: a newer API's manifest may have fields this one lacks.
    pub fn parse(text: &str) -> Result<Manifest, ExtError> {
        let manifest: Manifest = match toml::from_str(text) {
            Ok(manifest) => manifest,
            Err(error) => {
                let range = toml::from_str::<toml::Table>(text)
                    .ok()
                    .and_then(|table| VersionReq::parse(table.get("oscope-api")?.as_str()?).ok());
                if let Some(range) = range
                    && !range.matches(&API_VERSION)
                {
                    return Err(incompatible(&range));
                }
                let line = error
                    .span()
                    .map(|s| text[..s.start].matches('\n').count() + 1);
                return Err(wrong(match line {
                    Some(line) => format!("line {line}: {}", error.message()),
                    None => error.message().to_owned(),
                }));
            }
        };
        manifest.check()?;
        Ok(manifest)
    }

    /// Checks the manifest on its own, wherever it is:
    ///
    /// - its ID is reverse DNS, and its name, licence and publisher aren't
    ///   empty;
    /// - it provides something, each name dotted names, given once, and no
    ///   driver named as one of OScope's own ([`OWN_DRIVERS`]);
    /// - a Rust package has `[rust]` and no `[python]`, with a library for
    ///   at least one of the [`PLATFORMS`], and no other; a Python package
    ///   has `[python]` and no `[rust]`, with an entry that is a module's
    ///   dotted name;
    /// - the paths it names are plain relative paths, as in a package;
    /// - each panel it provides has its document, `panels/<name>.json`,
    ///   among its assets (ADR-0036).
    pub fn check(&self) -> Result<(), ExtError> {
        if !is_id(&self.id) {
            return Err(wrong(format!(
                "id {:?} isn't reverse DNS: at least two dotted names, in lower case, \
                 such as org.example.emboss",
                self.id
            )));
        }
        for (field, value) in [
            ("name", &self.name),
            ("license", &self.license),
            ("publisher", &self.publisher),
        ] {
            if value.trim().is_empty() {
                return Err(wrong(format!("{field} is empty")));
            }
        }
        for (contribution, names) in &self.provides {
            let mut seen = BTreeSet::new();
            if let Some(name) = names.iter().find(|n| !is_dotted(n) || !seen.insert(*n)) {
                return Err(wrong(format!(
                    "provides.{contribution} {name:?} isn't dotted names, or is there twice"
                )));
            }
        }
        let drivers = self.provides.get(&Contribution::Drivers);
        if let Some(name) = drivers
            .into_iter()
            .flatten()
            .find(|n| OWN_DRIVERS.contains(&n.as_str()))
        {
            return Err(wrong(format!(
                "provides.drivers {name:?} is one of OScope's own drivers"
            )));
        }
        if self.provides.values().all(Vec::is_empty) {
            return Err(wrong("[provides] names nothing"));
        }
        match (self.kind, &self.rust, &self.python) {
            (Kind::Rust, Some(rust), None) => {
                if rust.libraries.is_empty() {
                    return Err(wrong("[rust.libraries] names no library"));
                }
                for (platform, path) in &rust.libraries {
                    if !PLATFORMS.contains(&platform.as_str()) {
                        return Err(wrong(format!(
                            "rust.libraries.{platform}: the platforms are {}",
                            PLATFORMS.join(" and ")
                        )));
                    }
                    check_path(path).map_err(|why| {
                        wrong(format!("rust.libraries.{platform} {path:?} {why}"))
                    })?;
                }
            }
            (Kind::Python, None, Some(python)) => {
                if !python.entry.split('.').all(is_name) {
                    return Err(wrong(format!(
                        "python.entry {:?} isn't a module's dotted name",
                        python.entry
                    )));
                }
                if python.requires_python.trim().is_empty() {
                    return Err(wrong("python.requires-python is empty"));
                }
            }
            (kind, _, _) => {
                return Err(wrong(format!(
                    "a {kind} package has a [{kind}] table, and no other"
                )));
            }
        }
        for asset in &self.assets {
            check_path(asset).map_err(|why| wrong(format!("assets {asset:?} {why}")))?;
        }
        let panels = self
            .provides
            .get(&Contribution::Panels)
            .into_iter()
            .flatten();
        for panel in panels {
            let asset = crate::panels::asset(panel);
            if !self.assets.contains(&asset) {
                return Err(wrong(format!(
                    "provides.panels {panel:?}: {asset:?} isn't among its assets"
                )));
            }
        }
        Ok(())
    }

    /// Checks the manifest against this OScope: its `oscope-api` range
    /// matches [`API_VERSION`], and a Rust package has a library for this
    /// [`PLATFORM`].
    pub fn check_host(&self) -> Result<(), ExtError> {
        if !self.oscope_api.matches(&API_VERSION) {
            return Err(incompatible(&self.oscope_api));
        }
        match &self.rust {
            Some(rust) if self.library().is_none() => Err(ExtError::NoBuild {
                platform: PLATFORM.unwrap_or("this platform").to_owned(),
                builds: rust.libraries.keys().cloned().collect(),
            }),
            _ => Ok(()),
        }
    }

    /// The library for this platform, for a Rust package that has one.
    pub fn library(&self) -> Option<&str> {
        let rust = self.rust.as_ref()?;
        rust.libraries.get(PLATFORM?).map(String::as_str)
    }

    /// The files in the package it names: its libraries, then its assets.
    pub fn files(&self) -> impl Iterator<Item = &str> {
        let libraries = self.rust.iter().flat_map(|rust| rust.libraries.values());
        libraries.chain(&self.assets).map(String::as_str)
    }

    /// Every name it provides, with its contribution.
    pub fn provided<'a>(&'a self) -> impl Iterator<Item = (Contribution, &'a str)> {
        let names = |(&c, names): (&Contribution, &'a Vec<String>)| {
            names.iter().map(move |name| (c, name.as_str()))
        };
        self.provides.iter().flat_map(names)
    }
}

/// Whether `id` is reverse DNS: at least two dotted names of lower-case
/// letters, digits, `-` and `_`, each starting with a letter, at most 100
/// characters in all. Lower case only, as IDs name folders, and two IDs
/// differing only in case would share one on Windows.
pub(crate) fn is_id(id: &str) -> bool {
    let part = |p: &str| {
        p.starts_with(|c: char| c.is_ascii_lowercase())
            && p.chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || "-_".contains(c))
    };
    id.len() <= 100 && id.contains('.') && id.split('.').all(part)
}

/// Whether `text` is a name: ASCII letters, digits and `_`, not starting
/// with a digit, as node IDs' parts and Python modules are.
fn is_name(text: &str) -> bool {
    text.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_')
        && text.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Whether `text` is dotted names, such as `example.emboss`.
fn is_dotted(text: &str) -> bool {
    text.split('.').all(is_name)
}

/// The names Windows keeps for devices, whatever their extension.
const DEVICES: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// Checks a path in a package: relative, its parts separated by `/`, each
/// a name that Windows and Linux both create as it is. Why not, if not.
pub(crate) fn check_path(path: &str) -> Result<(), &'static str> {
    if path.starts_with('/') {
        return Err("is absolute");
    }
    if path.len() > 200 {
        return Err("is longer than 200 bytes");
    }
    for part in path.split('/') {
        match part {
            "" | "." => return Err("has an empty part"),
            ".." => return Err("leaves the package"),
            _ => {}
        }
        if part
            .chars()
            .any(|c| c.is_control() || "\\:*?\"<>|".contains(c))
        {
            return Err("has a control character, or one of \\ : * ? \" < > |");
        }
        if part.ends_with(['.', ' ']) {
            return Err("has a name ending in a dot or a space");
        }
        let stem = part.split('.').next().unwrap_or(part).trim_end();
        if DEVICES
            .iter()
            .any(|device| stem.eq_ignore_ascii_case(device))
        {
            return Err("has a name Windows keeps for a device");
        }
    }
    Ok(())
}
