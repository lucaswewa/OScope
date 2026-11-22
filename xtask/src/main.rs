//! Workspace architecture checks, independent of devices or a running server.

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() > 1 || args.first().is_some_and(|arg| arg != "check-layering") {
        eprintln!("usage: cargo xtask [check-layering]");
        return ExitCode::from(2);
    }
    let workspace = xtask::workspace();
    let found = xtask::violations(&workspace);
    if found.is_empty() {
        println!("Layering passed for {} workspace packages", workspace.len());
        ExitCode::SUCCESS
    } else {
        eprintln!("{}", found.join("\n"));
        ExitCode::FAILURE
    }
}
