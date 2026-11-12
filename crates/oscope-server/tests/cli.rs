//! The command line of `oscope-server`.

use std::process::{Command, Output};

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_oscope-server"))
        .args(args)
        .output()
        .expect("oscope-server runs")
}

#[test]
fn version_prints_the_name_and_the_crate_version() {
    for flag in ["--version", "-V"] {
        let output = run(&[flag]);
        assert!(output.status.success(), "{flag}");
        let expected = format!("oscope-server {}\n", env!("CARGO_PKG_VERSION"));
        assert_eq!(String::from_utf8_lossy(&output.stdout), expected, "{flag}");
    }
}

#[test]
fn anything_else_fails_with_the_usage() {
    for args in [&[][..], &["--serve"], &["--version", "--help"]] {
        let output = run(args);
        assert_eq!(output.status.code(), Some(2), "{args:?}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("Usage: oscope-server"),
            "{args:?}: {stderr}"
        );
    }
}
