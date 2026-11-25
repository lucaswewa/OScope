//! `cargo xtask`: the workspace's own tasks (P53).
//!
//! - `perf [--quick] [--cuda] [--web]`: plan §5.20's benchmarks, in release
//!   builds, and a report against the budgets in `target/perf/`; with
//!   `--web`, the app's too (W10.3), in Chromium against the release server.
//! - `soak [--minutes N] [--web]`: the soak, against the release server, its
//!   report in `target/soak/`; with `--web`, the app's live view kept open
//!   as long, and its memory's growth held to its budget.

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::time::Instant;

use xtask::perf::{self, About, BENCHES, Ran};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let flag = |name: &str| args.iter().any(|a| a == name);
    let done = match args.first().map(String::as_str) {
        Some("perf") => run_perf(flag("--quick"), flag("--cuda"), flag("--web")),
        Some("soak") => run_soak(value(&args, "--minutes").unwrap_or("240"), flag("--web")),
        _ => Err(
            "usage: cargo xtask perf [--quick] [--cuda] [--web] | soak [--minutes N] [--web]"
                .to_owned(),
        ),
    };
    match done {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(why) => {
            eprintln!("xtask: {why}");
            ExitCode::from(2)
        }
    }
}

/// The value after `name` in `args`.
fn value<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    let at = args.iter().position(|a| a == name)?;
    args.get(at + 1).map(String::as_str)
}

/// The workspace's folder, and its build output's.
fn folders() -> (PathBuf, PathBuf) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap_or(Path::new("."))
        .to_owned();
    let target = std::env::var_os("CARGO_TARGET_DIR").map_or(root.join("target"), PathBuf::from);
    (root, target)
}

/// What `command` prints on stdout, if it runs.
fn said(command: &str, args: &[&str]) -> String {
    let output = Command::new(command).args(args).output();
    output.map_or(String::new(), |o| {
        String::from_utf8_lossy(&o.stdout).trim().to_owned()
    })
}

/// Runs `cargo args` in `root`, echoing what it prints: whether it exited
/// well, and its output.
fn cargo(root: &Path, args: &[String]) -> Result<(bool, String), String> {
    echoed(
        Command::new("cargo").args(args).current_dir(root),
        "cargo",
        args,
    )
}

/// Runs `command`, `name` and `args` echoed first, echoing what it prints:
/// whether it exited well, and its output.
fn echoed(command: &mut Command, name: &str, args: &[String]) -> Result<(bool, String), String> {
    println!("\n$ {name} {}", args.join(" "));
    let mut child = command
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|e| format!("{name}: {e}"))?;
    let mut output = String::new();
    if let Some(stdout) = child.stdout.take() {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            println!("{line}");
            output.push_str(&line);
            output.push('\n');
        }
    }
    let status = child.wait().map_err(|e| format!("{name}: {e}"))?;
    Ok((status.success(), output))
}

/// The app's budgets (W10.3): the release server built, with CUDA if asked,
/// the app built, and `web/tests/perf/` run in Chromium against them, its
/// memory watched for `minutes`, only the tests `grep` names if given.
/// Whether it ran to its end, and what it printed.
fn run_web(
    root: &Path,
    target: &Path,
    cuda: bool,
    minutes: f64,
    grep: Option<&str>,
) -> Result<(bool, String), String> {
    let mut build = vec!["build", "--release", "-p", "oscope-server"];
    if cuda {
        build.extend(["--features", "cuda"]);
    }
    let build: Vec<String> = build.into_iter().map(str::to_owned).collect();
    if !cargo(root, &build)?.0 {
        return Ok((false, String::new()));
    }
    let web = root.join("web");
    // Node runs the tools' own scripts: npm's and npx's are batch files on Windows.
    let node = |script: &str, args: &[&str]| -> Vec<String> {
        let mut all = vec![web.join(script).to_string_lossy().into_owned()];
        all.extend(args.iter().map(|a| (*a).to_owned()));
        all
    };
    let vite = node("node_modules/vite/bin/vite.js", &["build"]);
    if !echoed(
        Command::new("node").args(&vite).current_dir(&web),
        "node",
        &vite,
    )?
    .0
    {
        return Ok((false, String::new()));
    }
    let server = target
        .join("release")
        .join(format!("oscope-server{}", std::env::consts::EXE_SUFFIX));
    let mut tests = vec!["test", "-c", "playwright.perf.config.ts", "--reporter=list"];
    if let Some(grep) = grep {
        tests.extend(["--grep", grep]);
    }
    let playwright = node("node_modules/@playwright/test/cli.js", &tests);
    let mut command = Command::new("node");
    command
        .args(&playwright)
        .current_dir(&web)
        .env("OSCOPE_SERVER_BIN", server)
        .env("OSCOPE_PERF_MINUTES", minutes.to_string());
    echoed(&mut command, "node", &playwright)
}

/// The computer, as the report names it.
fn machine() -> String {
    let threads = std::thread::available_parallelism().map_or(0, usize::from);
    let processor = match std::env::consts::OS {
        "windows" => {
            let name = "(Get-CimInstance Win32_Processor).Name";
            said("powershell", &["-NoProfile", "-Command", name])
        }
        _ => std::fs::read_to_string("/proc/cpuinfo")
            .unwrap_or_default()
            .lines()
            .find_map(|l| {
                l.strip_prefix("model name")
                    .map(|m| m.trim_start_matches([' ', '\t', ':']).to_owned())
            })
            .unwrap_or_default(),
    };
    let gpu = said("nvidia-smi", &["--query-gpu=name", "--format=csv,noheader"]);
    let gpu = if gpu.is_empty() {
        "no NVIDIA GPU".to_owned()
    } else {
        gpu
    };
    format!(
        "{processor}, {threads} threads, {gpu}, {}",
        std::env::consts::OS
    )
}

/// `cargo xtask perf`: whether every budget measured was met.
fn run_perf(quick: bool, cuda: bool, web: bool) -> Result<bool, String> {
    let (root, target) = folders();
    let out = target.join("perf");
    let scratch = out.join("scratch");
    std::fs::create_dir_all(&scratch).map_err(|e| format!("{}: {e}", scratch.display()))?;
    let mut measured = Vec::new();
    let mut ran = Vec::new();
    for bench in BENCHES {
        let args = perf::with_cuda(
            (bench.args)(quick, &scratch.to_string_lossy()),
            cuda && bench.cuda,
        );
        let started = Instant::now();
        let (ok, output) = cargo(&root, &args)?;
        measured.extend(perf::measures(&output));
        ran.push(Ran {
            name: bench.name.to_owned(),
            ok,
            seconds: started.elapsed().as_secs_f64(),
        });
    }
    // The recording's frames are gone with it.
    let _ = std::fs::remove_dir_all(&scratch);
    if web {
        let started = Instant::now();
        let minutes = if quick { 2.0 } else { 10.0 };
        let (ok, output) = run_web(&root, &target, cuda, minutes, None)?;
        measured.extend(perf::measures(&output));
        ran.push(Ran {
            name: "the app (W10.3)".to_owned(),
            ok,
            seconds: started.elapsed().as_secs_f64(),
        });
    }
    let about = About {
        when: said("git", &["log", "-1", "--format=%cI"]),
        commit: said("git", &["rev-parse", "--short", "HEAD"]),
        machine: machine(),
        quick,
        cuda,
    };
    let html = out.join("report.html");
    let json = perf::json(&about, &ran, &measured);
    let written = std::fs::write(&html, perf::html(&about, &ran, &measured))
        .and_then(|()| std::fs::write(out.join("report.json"), format!("{json:#}\n")));
    written.map_err(|e| format!("{}: {e}", out.display()))?;
    println!();
    let rows = perf::rows(&measured, quick);
    for (budget, value, verdict) in &rows {
        let value = value.map_or("—".to_owned(), |v| format!("{v} {}", budget.unit));
        println!("{:<55} {:>14}  {verdict:?}", budget.what, value);
    }
    println!("\nthe report: {}", html.display());
    Ok(perf::passed(&rows, &ran))
}

/// `cargo xtask soak`: whether the soak found nothing wrong, and, with
/// `web`, the app's memory kept within its budget as long.
fn run_soak(minutes: &str, web: bool) -> Result<bool, String> {
    let (root, target) = folders();
    let report = target.join("soak");
    let minutes: f64 = minutes
        .parse()
        .map_err(|_| format!("--minutes {minutes}: a number"))?;
    // The app's live view, kept open beside the soak, on a server of its own.
    let app = web.then(|| {
        let (root, target) = (root.clone(), target.clone());
        std::thread::spawn(move || run_web(&root, &target, false, minutes, Some("memory")))
    });
    let args = "test --release -p oscope-server --test soak -- --ignored --nocapture --exact soak";
    let status = Command::new("cargo")
        .args(args.split(' '))
        .current_dir(&root)
        .env("OSCOPE_SOAK_MINUTES", minutes.to_string())
        .env("OSCOPE_SOAK_REPORT", &report)
        .status()
        .map_err(|e| format!("cargo: {e}"))?;
    println!("the report: {}", report.join("soak.json").display());
    let Some(app) = app else {
        return Ok(status.success());
    };
    let joined = app
        .join()
        .map_err(|_| "the app's soak panicked".to_owned())?;
    let (ran, output) = joined?;
    let rows = perf::rows(&perf::measures(&output), false);
    let memory = rows.iter().find(|r| r.0.key == "web.memory.mb_per_hour");
    let held = memory.is_some_and(|r| r.2 == perf::Verdict::Met);
    let growth = memory.and_then(|r| r.1);
    let said = growth.map_or("not measured".to_owned(), |v| format!("{v:.1} MB/h"));
    let verdict = if held { "within" } else { "NOT within" };
    println!("the app's memory over the soak: {said}, {verdict} its budget");
    Ok(status.success() && ran && held)
}
