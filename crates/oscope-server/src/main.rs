//! `oscope-server`, the program that runs the microscope: its configuration,
//! service mode and the sidecar contract.
//!
//! Layer L6 of ADR-0005. For now it answers only `--version` and `--help`.

use std::process::ExitCode;

const USAGE: &str = "Usage: oscope-server [--version | --help]";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.iter().map(String::as_str).collect::<Vec<_>>()[..] {
        ["--version" | "-V"] => {
            println!("oscope-server {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        ["--help" | "-h"] => {
            println!("{USAGE}");
            ExitCode::SUCCESS
        }
        _ => {
            eprintln!("oscope-server: there is nothing to serve yet\n{USAGE}");
            ExitCode::from(2)
        }
    }
}
