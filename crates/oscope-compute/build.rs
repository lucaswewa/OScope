//! With the `cuda` feature, compiles each CUDA kernel file, `kernels/*.cu`,
//! to a fat binary in `OUT_DIR`, for the crate to embed (ADR-0011). Without
//! nvcc, the build fails and says how to get it.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Machine code for the first GPU of each major architecture CUDA supports,
/// which runs on any driver of the toolkit's major version, and PTX for the
/// newest, which a newer GPU's driver compiles. PTX alone would need a driver
/// at least as new as the toolkit.
const ARCH: &str = "all-major";

fn main() {
    println!("cargo:rerun-if-changed=kernels");
    println!("cargo:rerun-if-env-changed=NVCC");
    println!("cargo:rerun-if-env-changed=CUDA_PATH");
    if std::env::var_os("CARGO_FEATURE_CUDA").is_none() {
        return;
    }
    let nvcc = nvcc().unwrap_or_else(|| {
        panic!(
            "the `cuda` feature needs the CUDA toolkit's nvcc, and none was found in NVCC, \
             CUDA_PATH or PATH. Install CUDA 13.4 (https://developer.nvidia.com/cuda-downloads), \
             or build without the feature."
        )
    });
    let out = PathBuf::from(std::env::var_os("OUT_DIR").expect("cargo sets OUT_DIR"));
    let mut kernels: Vec<PathBuf> = std::fs::read_dir("kernels")
        .expect("the kernels folder")
        .map(|entry| entry.expect("a folder entry").path())
        .filter(|path| path.extension().is_some_and(|e| e == "cu"))
        .collect();
    kernels.sort();
    for kernel in kernels {
        let binary = out
            .join(kernel.file_stem().expect("a file name"))
            .with_extension("fatbin");
        let mut command = Command::new(&nvcc);
        host_compiler(&mut command);
        let status = command
            // No fused multiply-adds: each product and sum rounds as the CPU
            // twin's does, so that the twins agree to the bit.
            .args([
                "--fatbin",
                "-O3",
                "--fmad=false",
                &format!("-arch={ARCH}"),
                "-o",
            ])
            .arg(&binary)
            .arg(&kernel)
            .status()
            .unwrap_or_else(|error| panic!("{} didn't run: {error}", nvcc.display()));
        assert!(
            status.success(),
            "nvcc couldn't compile {}",
            kernel.display()
        );
    }
}

/// On Windows, nvcc preprocesses with MSVC's cl.exe: this finds it, with
/// the environment it needs, as the `cc` crate does.
#[cfg(windows)]
fn host_compiler(command: &mut Command) {
    let on_path = Command::new("cl.exe").output().is_ok();
    if let (false, Some(cl)) = (on_path, find_msvc_tools::find_tool("x86_64", "cl.exe")) {
        let folder = cl.path().parent().expect("cl.exe is in a folder");
        command.arg("-ccbin").arg(folder);
        command.envs(cl.env().into_iter().map(|(k, v)| (k, v)));
    }
}

/// Elsewhere, nvcc finds the host compiler on PATH.
#[cfg(not(windows))]
fn host_compiler(_command: &mut Command) {}

/// nvcc: from `NVCC`, from `CUDA_PATH`'s `bin`, or on `PATH`.
fn nvcc() -> Option<PathBuf> {
    let name = if cfg!(windows) { "nvcc.exe" } else { "nvcc" };
    let candidates = [
        std::env::var_os("NVCC").map(PathBuf::from),
        std::env::var_os("CUDA_PATH").map(|cuda| Path::new(&cuda).join("bin").join(name)),
        Some(PathBuf::from(name)),
    ];
    candidates.into_iter().flatten().find(|path| {
        Command::new(path)
            .arg("--version")
            .output()
            .is_ok_and(|output| output.status.success())
    })
}
