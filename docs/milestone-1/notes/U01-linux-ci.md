# U01: Linux support, Ubuntu CI, licence metadata

- Status: Done (merged 2026-11-11)
- Branch / PR: `u/u01-linux-ci` in `teta-wot` / [lucaswewa/teta-wot#1](https://github.com/lucaswewa/teta-wot/pull/1)
- Size: estimated M · measured **359** hand-written changed lines. The rest is documentation.
  - The signal test is 156, the soak test +43 / −7, `shutdown_signal` +38 / −18, and the service example's new `main` 26. CI, the Cargo manifests and the doc comments make up the rest.
  - The 252 lines moved unchanged into `microscope-service/src/windows.rs` count once, as their single changed line. Git can't show the move as a rename, because a new `main.rs` replaces the old one.

## Summary

`teta-wot` now builds, tests and runs on Linux x86_64 as well as Windows ([spec](../phases.md#u01)):

- CI runs every job on Ubuntu 24.04 beside Windows.
- A server stops gracefully on the Unix signals a service manager or terminal sends.
- The licence is stated consistently as MIT.
- The docs use the crate's current name.
- The repository has working rules (`CLAUDE.md`).

## What was built

All of it is in `teta-wot`.

| Change | Where |
|---|---|
| **Unix signals.** `shutdown_signal` completes on `SIGINT` (Ctrl-C), `SIGTERM` (`kill`, systemd) and `SIGHUP` (the terminal closing), logging which one arrived. It used to handle only Ctrl-C off Windows, so a `SIGTERM` killed the process without a graceful stop | `crates/teta-wot-server/src/lib.rs` |
| **CI on both systems.** A matrix of `windows-latest` and `ubuntu-24.04`, with `fail-fast: false` | `.github/workflows/ci.yml` |
| **Toolchain.** The pinned target `x86_64-pc-windows-msvc` is removed, so each platform builds for itself | `rust-toolchain.toml` |
| **Licence metadata.** `license = "MIT"` and `repository` in the workspace, inherited by the six library crates. The "no licence" text is gone from the guide, `Cargo.toml` and the licence-gathering tool's header | `Cargo.toml`, `crates/*/Cargo.toml`, `docs/guide/src/introduction.md`, `tools/licenses.py` |
| **`microscope-service` on Linux.** Its code moved, unchanged, into a `windows` module. On other systems `main` says it's a Windows service, points to `simulated-microscope` under systemd, and exits successfully, so CI's run-every-example loop passes | `examples/microscope-service/src/` |
| **The soak test on Linux.** It reads memory and open file descriptors from `/proc/<pid>/status` and `/proc/<pid>/fd`, and `--server` defaults to the platform's executable name. It used to read memory through PowerShell only | `tools/soak/src/main.rs`, `tools/README.md` |
| **The crate's name in docs.** `wot::` became `teta_wot::`, `wot-core` and the like became `teta-wot-core`, and `lucaswewa/wot` links became `lucaswewa/teta-wot`. This covers the guide (installing, running, structure, examples, see also), the example READMEs, and the doc comments, including one string the `#[thing_impl]` macro generates | as listed |
| **Linux in the guide.** Installing on Linux (`build-essential`, `pkg-config`), the platform statement, and how to stop a server on Linux (quickstart, running, the graceful-shutdown example) | `docs/guide/src/…`, `examples/graceful-shutdown/README.md` |
| **The release checklist.** The link to the missing ADR-0061 is gone, and the CI jobs it names match the real ones | `docs/releasing.md` |
| **Working rules.** The process, the CHANGELOG discipline, the stable surfaces, the identifiers that must not change, the platform rules, and the checks, including running Linux checks under WSL | `CLAUDE.md` (new) |
| **The CHANGELOG** has entries under *Unreleased* | `CHANGELOG.md` |

## How to try it

**On Linux, or in WSL's Ubuntu:**

```sh
cargo run -p simulated-microscope -- -c examples/simulated-microscope/microscope.json --port 0
# In another terminal (or press Ctrl-C):
kill -TERM <its process id>
```

The server logs `received SIGTERM`, stops its Things, logs `stopped`, and exits with status 0.

**The tests:**

```sh
cargo test -p simulated-microscope --test signals
```

**In the PR:** CI shows two jobs, `Rust (Windows x86_64)` and `Rust (Ubuntu 24.04 x86_64)`.

## Design notes and deviations from the plan

- **`SIGHUP` is handled too,** beyond the spec's `SIGINT` and `SIGTERM`. It is what a terminal sends when it closes: the Linux counterpart of the Windows console-close event, which `shutdown_signal` already handled.
- **The test runs a real server process.**
  - It lives in `examples/simulated-microscope/tests/signals.rs`, because it needs a server binary (`CARGO_BIN_EXE_simulated-microscope`).
  - It waits for the `listening on …` line, checks that the server answers HTTP, sends the signal with `kill`, then waits for the signal's log line and `stopped`, and checks the exit status.
  - It runs on Unix only. The Windows console events can't be sent to a child process from a test, and their handling is unchanged.
- **Identifiers kept on purpose.** These look like leftovers of the old name, but changing any of them would be a breaking change to the wire format or to users' setups. `teta-wot`'s new `CLAUDE.md` records them:
  - the namespace URL from which Thing Description `id`s are derived (`…/lucaswewa/wot#td-id`);
  - `PROBLEM_TYPES_URL`;
  - the log target `wot::things`;
  - the `wot` wire profile's name.
- **Beyond the spec,** because the spec says `teta-wot` must *run* on Ubuntu:
  - the soak test now works on Linux;
  - `microscope-service` builds and runs (it exits at once) there.
- **The release workflow stays Windows-only.** OScope depends on the source by tag, so it doesn't need Linux binaries.
- **No `.gitattributes` in `teta-wot`.** The repository already stores LF. This machine's Windows checkout has CRLF (`core.autocrlf`), and so does WSL, which builds from that checkout. Linux CI checks out LF, which covers the difference.
- **Setting up WSL.**
  - `build-essential`, `pkg-config` and `curl` were installed in `Ubuntu-24.04` with `wsl -u root`, rather than asking for the owner's sudo password.
  - rustup was installed for the user `lucas`.
  - Builds use `CARGO_TARGET_DIR=~/target/teta-wot`.

## Tests

- **New:** three Unix tests (`sigterm_…`, `sigint_…`, `sighup_…_stops_the_server_gracefully`). Each starts its own server, on its own port, in its own folder.
- **Mutation check:**
  - With the `SIGTERM` branch deleted, `sigterm_stops_the_server_gracefully` fails: 2 passed, 1 failed, since the signal now ends the process abruptly.
  - With the branch restored, all three pass.
- **Every CI step, run locally:** fmt, clippy, tests, docs, the docs-pages tests, and all 35 examples.

  | Platform | Result |
  |---|---|
  | Windows | All 41 steps passed; 71 test suites ok |
  | Linux (WSL Ubuntu 24.04.5) | All 41 steps passed; 71 test suites ok |

  A first run on Windows failed only because rustup was updating itself at the time ("could not remove … rustup-init.exe"); it was re-run.

  The guide's `mdbook build` runs only in CI.
- **GitHub CI on the PR didn't run:** its jobs stayed pending, because the account's GitHub Actions minutes had run out. The CI jobs were replayed locally instead:
  - **Ubuntu job:**
    - a fresh clone of the PR's commit (`42db2b8`) inside WSL's Linux file system, with LF line endings (0 CRLF files), as GitHub's runner checks it out;
    - its own `target/`, and mdbook 0.5.4;
    - all 42 steps passed (fmt, clippy, tests, docs, docs-pages, the 35 examples, and the guide), with 71 test suites ok, the three signal tests included.
  - **Windows job:** the full run above, plus `mdbook build docs/guide` with mdbook 0.5.4. All passed.

## Follow-ups and known gaps

These are stale references this phase found but left alone, as they're outside its scope:

- **ADR citations.** About twenty source files cite ADRs (ADR-0059, ADR-0062, …) that are kept outside the repository.
- **Missing problem-types page.** `docs/problems.md`, the page `PROBLEM_TYPES_URL` points to, doesn't exist, and the guide links to it.
- **Missing conformance project.** The guide's installing page mentions a `conformance/reference` project that isn't in the repository.
- **The soak in CI.** `tools/README.md` says CI runs the soak test for 3 minutes; it doesn't.
- **Missing notices file.** `THIRD_PARTY_NOTICES.md` is referred to, but doesn't exist.
- **Linux release binaries** aren't published.

**Before P01 (OScope's CI):** decide how CI runs while GitHub Actions minutes are limited. Windows minutes count double, and OScope's plan runs both systems on every PR. The options:

- self-hosted runners on the owner's PC, one on Windows and one in WSL, which use no minutes;
- more minutes;
- replaying CI locally, as here.

Next phase: U02 (path parameters for endpoints).

## Clean room

All of it was written for this phase. The signal handling uses `tokio::signal::unix`, as tokio documents it. Nothing was copied.
