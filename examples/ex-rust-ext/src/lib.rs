//! What `ex-rust-ext`, its tests and the other examples of Rust extensions
//! share: a sample extension's library, as Cargo built it beside them, and
//! packing it for this platform.

use std::env::consts::{DLL_PREFIX, DLL_SUFFIX};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use oscope_ext::{Manifest, PLATFORM, PLATFORMS};

/// The folder of the sample `name`, in `examples/`, with its manifest.
pub fn sample_folder(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join(name)
}

/// The sample's folder, `examples/ext-rust-node`, with its manifest.
pub fn sample() -> PathBuf {
    sample_folder("ext-rust-node")
}

/// The library of the crate `name` (`ext_rust_node`): `{name}.dll` on
/// Windows, `lib{name}.so` on Linux. Cargo builds it as a dependency of the
/// running program, in `deps` beside it, or beside a test.
pub fn library_of(name: &str) -> anyhow::Result<PathBuf> {
    let file = format!("{DLL_PREFIX}{name}{DLL_SUFFIX}");
    let program = std::env::current_exe()?;
    for folder in program.ancestors().skip(1).take(2) {
        for candidate in [folder.join(&file), folder.join("deps").join(&file)] {
            if candidate.is_file() {
                return Ok(candidate);
            }
        }
    }
    bail!(
        "{file} isn't beside {}: Cargo builds it with this example",
        program.display()
    )
}

/// The sample's library, `ext_rust_node`.
pub fn library() -> anyhow::Result<PathBuf> {
    library_of("ext_rust_node")
}

/// Packs the sample into `out`, for this platform: its manifest, its
/// panels, and its `library`, in a folder made in `work`. The manifest names a library for
/// each platform, so the lines of the platforms not built here are left out.
pub fn pack(library: &Path, work: &Path, out: &Path) -> anyhow::Result<Manifest> {
    pack_sample(&sample(), library, work, out)
}

/// Packs the sample in `sample` into `out`, as [`pack`] does.
pub fn pack_sample(
    sample: &Path,
    library: &Path,
    work: &Path,
    out: &Path,
) -> anyhow::Result<Manifest> {
    let platform = PLATFORM.context("OScope's platforms are Windows and Linux on x86_64")?;
    let manifest = fs::read_to_string(sample.join("manifest.toml"))?;
    let built_elsewhere = |line: &&str| {
        let mut others = PLATFORMS.iter().filter(|other| **other != platform);
        others.any(|other| line.starts_with(&format!("{other} =")))
    };
    let lines = manifest.lines().filter(|line| !built_elsewhere(line));
    let name = sample.file_name().context("a sample's folder")?;
    let folder = work.join(name);
    if folder.exists() {
        fs::remove_dir_all(&folder)?;
    }
    fs::create_dir_all(folder.join("lib"))?;
    let text: String = lines.map(|line| format!("{line}\n")).collect();
    fs::write(folder.join("manifest.toml"), text)?;
    let file = library.file_name().context("a library's name")?;
    fs::copy(library, folder.join("lib").join(file))?;
    // Its panels, assets of the package (W8.5).
    let panels = sample.join("panels");
    if panels.is_dir() {
        fs::create_dir_all(folder.join("panels"))?;
        for entry in fs::read_dir(&panels)? {
            let path = entry?.path();
            let name = path.file_name().context("a panel's name")?;
            fs::copy(&path, folder.join("panels").join(name))?;
        }
    }
    Ok(oscope_ext::pack(&folder, out)?)
}
