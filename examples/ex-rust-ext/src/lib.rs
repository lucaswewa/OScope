//! What `ex-rust-ext` and its tests share: the sample extension's library,
//! as Cargo built it beside them, and packing it for this platform.

use std::env::consts::{DLL_PREFIX, DLL_SUFFIX};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use oscope_ext::{Manifest, PLATFORM, PLATFORMS};

/// The sample's folder, `examples/ext-rust-node`, with its manifest.
pub fn sample() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../ext-rust-node")
}

/// The sample's library: `ext_rust_node.dll` on Windows,
/// `libext_rust_node.so` on Linux. Cargo builds it as this crate's
/// dependency, in `deps` beside the program, or beside a test.
pub fn library() -> anyhow::Result<PathBuf> {
    let name = format!("{DLL_PREFIX}ext_rust_node{DLL_SUFFIX}");
    let program = std::env::current_exe()?;
    for folder in program.ancestors().skip(1).take(2) {
        for candidate in [folder.join(&name), folder.join("deps").join(&name)] {
            if candidate.is_file() {
                return Ok(candidate);
            }
        }
    }
    bail!(
        "{name} isn't beside {}: Cargo builds it with this example",
        program.display()
    )
}

/// Packs the sample into `out`, for this platform: its manifest, and its
/// `library`, in a folder made in `work`. The manifest names a library for
/// each platform, so the lines of the platforms not built here are left out.
pub fn pack(library: &Path, work: &Path, out: &Path) -> anyhow::Result<Manifest> {
    let platform = PLATFORM.context("OScope's platforms are Windows and Linux on x86_64")?;
    let manifest = fs::read_to_string(sample().join("manifest.toml"))?;
    let built_elsewhere = |line: &&str| {
        let mut others = PLATFORMS.iter().filter(|other| **other != platform);
        others.any(|other| line.starts_with(&format!("{other} =")))
    };
    let lines = manifest.lines().filter(|line| !built_elsewhere(line));
    let folder = work.join("emboss");
    if folder.exists() {
        fs::remove_dir_all(&folder)?;
    }
    fs::create_dir_all(folder.join("lib"))?;
    fs::write(
        folder.join("manifest.toml"),
        lines.map(|line| format!("{line}\n")).collect::<String>(),
    )?;
    let name = library.file_name().context("a library's name")?;
    fs::copy(library, folder.join("lib").join(name))?;
    Ok(oscope_ext::pack(&folder, out)?)
}
