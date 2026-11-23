//! Records the Git commit that OScope is built from, for the `system` Thing.

use std::process::Command;

/// `git` with `args`: its output, trimmed, if it succeeded.
fn git(args: &[&str]) -> Option<String> {
    let output = Command::new("git").args(args).output().ok()?;
    let text = String::from_utf8(output.stdout).ok()?;
    output.status.success().then(|| text.trim().to_owned())
}

fn main() {
    println!("cargo:rerun-if-env-changed=OSCOPE_BUILD_COMMIT");
    let commit = std::env::var("OSCOPE_BUILD_COMMIT")
        .ok()
        .filter(|value| {
            value == "unknown" || value.len() == 12 && value.chars().all(|c| c.is_ascii_hexdigit())
        })
        .or_else(|| git(&["rev-parse", "--short=12", "HEAD"]))
        .unwrap_or_else(|| "unknown".into());
    println!("cargo:rustc-env=OSCOPE_GIT_COMMIT={commit}");
    // Builds again when HEAD moves: a commit, checkout or reset writes its log.
    if let Some(dir) = git(&["rev-parse", "--absolute-git-dir"]) {
        println!("cargo:rerun-if-changed={dir}/HEAD");
        println!("cargo:rerun-if-changed={dir}/logs/HEAD");
    }
}
