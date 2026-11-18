# Scripts

## Running CI's checks locally

Two scripts run the checks that CI runs on every pull request ([`.github/workflows/ci.yml`](../.github/workflows/ci.yml)), in the same order. They are the merge gate ([ADR-0038](../docs/adr/0038-local-ci-replays-side-by-side.md)): a pull request merges when both pass, each run alone, as CLAUDE.md asks.

| Step | Command |
|---|---|
| toolchain | `rustup toolchain install` |
| fmt | `cargo fmt --all --check` |
| clippy | `cargo clippy --workspace --all-targets --locked -- -D warnings` |
| tests build | `cargo test --workspace --locked --no-run`, its messages as JSON |
| tests | each test binary built, the layering test's among them, run in its package's folder as `cargo test` runs it; with `OSCOPE_PYTHON=1` where uv is found, as CI does on Ubuntu |
| doc tests | `cargo test --workspace --locked --doc` |
| python sdk tests | `uv run --locked pytest` in `python/oscope-ext`. The step fails if uv isn't on `PATH`, in `~/.local/bin` or `~/.cargo/bin`, or in winget's folder on Windows. GitHub CI runs it on Ubuntu alone |
| docs | `cargo doc --workspace --no-deps --locked`, with `RUSTDOCFLAGS="-D warnings"` |
| deny | `cargo deny --locked check` |
| server --version | `cargo run -p oscope-server --locked -- --version` |
| web install | `npm ci` in `web/`: the app's packages, from its lock file. The step fails if Node isn't installed |
| web typecheck, lint, unit tests, build | `npm run typecheck`, `lint`, `test:unit` and `build` in `web/` ([`web/README.md`](../web/README.md)) |
| web licences, advisories | `npm run licences` and `advisories`: every package, by its tier ([ADR-0040](../docs/adr/0040-testing-the-app.md)). The advisories need the network |
| web browsers | `npx --no-install playwright install chromium firefox webkit`, the lock file's Playwright and never another: quick once they are there. On Linux their system libraries are installed once, as root: `npx playwright install-deps chromium firefox webkit` |
| web end to end | `npm run test:e2e`: the built app in three browsers, each test with the server just built, on a port of its own |
| examples build | `cargo build --locked`, every example with a binary |
| *each example* --smoke | the example's binary, with `--smoke`, from the repository's root: always here, though GitHub CI skips them while its `RUN_EXAMPLES` is `"false"` |
| CUDA tests, with `--cuda` | `OSCOPE_CUDA=1 cargo test --workspace --locked --features cuda` |

Each step's output goes to a log file, and each suite's and smoke run's to its own. A failing step shows the end of its log. Both scripts exit with 0 only if every step passed.

### Side by side, on two CPUs each

The test suites, and then the smoke runs, go eight at a time, each limited to two CPUs of its own, as on CI's two-core runners. Slot k gets CPUs 2k and 2k+1: on the owner's PC, an i9-12900, CPUs 0–15 are its eight performance cores' threads. `--jobs N` runs N at a time, at most one for each pair of CPUs there is. On Windows the limit is set through PowerShell; on Linux, with `taskset`.

- **The longest go first,** by the last run's times, kept in `target/ci-times.tsv`, so that none is left to run alone at the end.
- **The suites are quiet** but for any that fails, which shows the end of its log; the smoke runs each show a line.
- **Disk:** suites that record preallocate their segments, several at once. Keep some 20 GB free.

[`pinned.sh`](pinned.sh) holds the runner, which both scripts and `two-core.sh` share.

### `ci-local.sh`: this checkout, as it is

```sh
scripts/ci-local.sh            # in Git Bash on Windows, or a Linux shell
scripts/ci-local.sh --cuda     # also the CUDA tests: a GPU and the CUDA toolkit
scripts/ci-local.sh --jobs 4   # four at a time
```

It checks the checkout you are in, uncommitted changes included, and builds in the usual `target/`. Its logs go to `target/ci-logs/<date-time>/`, or to `--logs DIR`. It ends like this:

```text
ok    ex-autofocus --smoke             99 s
tests: 504 passed, in 186 suites
steps: 53 passed, 0 failed
```

**Not under a running dev server.** Its web install is `npm ci`, which deletes `web/node_modules` first. On Windows, a Vite dev server running from the same checkout holds the bundler's native module: the install fails, after it has deleted the rest from under the dev server. Stop the dev server first, or replay a worktree of the commit, sharing the build folder:

```sh
git worktree add --detach ../OScope-replay HEAD
CARGO_TARGET_DIR="$PWD/target" ../OScope-replay/scripts/ci-local.sh
```

### `ci-clone.sh`: a commit, as CI sees it

```sh
scripts/ci-clone.sh            # HEAD
scripts/ci-clone.sh 73fffde    # any commit
```

It clones the commit afresh, with LF line endings and nothing uncommitted, then runs `ci-local.sh` on the clone. A file holding a CR fails the run.

- **The clone** goes to `~/ci/oscope-<commit>`, replacing an earlier clone of the same commit, or under `$CI_CLONE_DIR`.
- **The build** goes to `~/target/oscope-ci`, or to `$CARGO_TARGET_DIR`.

**On Windows, run it under WSL** (`Ubuntu-24.04`, CLAUDE.md), so that the clone and its build are on the Linux side, apart from Windows's `target/`. From Git Bash:

```sh
MSYS_NO_PATHCONV=1 wsl.exe -d Ubuntu-24.04 -- bash /mnt/c/repos/OScope/scripts/ci-clone.sh
```

`MSYS_NO_PATHCONV=1` stops Git Bash from turning `/mnt/c/...` into a Windows path. From PowerShell, the same command works without it.

### What they need

- **Rust, by rustup:** the toolchain step installs the one that `rust-toolchain.toml` pins.
- **cargo-deny 0.20.2,** CI's version: `cargo install cargo-deny --version 0.20.2 --locked`. Without it, the deny step fails and says so.
- **For `--cuda`:** an NVIDIA GPU and the CUDA toolkit, with `nvcc` on the `PATH` ([`docs/hardware.md`](../docs/hardware.md)).

They don't run the weekly CUDA build (`.github/workflows/cuda.yml`), which builds the kernels for every GPU architecture on a CI runner.

## Two cores, five times: `two-core.sh`

Before a pull request, the suites and smoke runs a change touches, and only those, run five times each on two CPUs, as CLAUDE.md asks. GitHub's runners have two cores, and this PC's 24 threads hide timing faults: rates, pacing and ordering that only hold on a fast machine.

```sh
scripts/two-core.sh oscope-workflows:stitch oscope-stitch:lib ex-stitch
scripts/two-core.sh --repeat 10 oscope-api:preview
```

- **A target** is a test suite, `PACKAGE:TEST` for its `tests/TEST.rs` or `PACKAGE:lib` for its unit tests, or an example by its name, for its `--smoke`.
- **The app's tests** are `web:unit` and `web:e2e`. The end-to-end tests run with one worker, as Playwright chooses on a two-core runner: it counts the computer's CPUs, not the pair it is held to. The visual tests run among them.
- **Its runs go side by side,** eight at a time on their own pairs of CPUs, as `ci-local.sh`'s do: a target's five copies run together.
- **`--repeat N`** runs each N times, and `--jobs N` N at a time. Logs go to `target/two-core/<date-time>/`, or to `--logs DIR`.

It prints each run as it ends, then each target's passes and times, and exits with 0 only if every run passed:

```text
oscope-api:devices                       5 of 5 passed, 19.5-20.1 s
ex-core-tour                             5 of 5 passed, 0.2-0.3 s
```

**Which suites:** those a change adds or edits, and those that run code it changes. A suite whose code behaves as before needn't run: one whose simulator only gained settings, their defaults the old behaviour, for instance.

## The visual tests' baselines: `baselines.sh`

The app's visual tests compare a page with a picture of it, a baseline, in `web/tests/e2e/baselines/` ([ADR-0041](../docs/adr/0041-design-tokens-and-themes.md)). A baseline is one system's picture: Windows and Linux each have their own. After a change that is meant to be seen, make them again on both, and look at each before committing it:

```sh
scripts/baselines.sh                                                       # this system's: Windows', in Git Bash
MSYS_NO_PATHCONV=1 wsl.exe -d Ubuntu-24.04 -- bash /mnt/c/repos/OScope/scripts/baselines.sh    # Linux's, from the same checkout
```

- **It needs** what the end-to-end tests need: the packages, the built app, the browsers and a server ([`web/README.md`](../web/README.md)).
- **Under WSL** it runs in the Windows checkout, with its packages and its built app. The server is the one the Linux replay built, in `~/target/oscope-ci`, or the one `OSCOPE_SERVER_BIN` names.
- **Its arguments** go to Playwright: a test's file, to make only that test's baselines.
