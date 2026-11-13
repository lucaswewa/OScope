# Scripts

## Running CI's checks locally

Two scripts run the checks that CI runs on every pull request ([`.github/workflows/ci.yml`](../.github/workflows/ci.yml)), in the same order. They are for the last check before opening a pull request, as CLAUDE.md asks.

| Step | Command |
|---|---|
| toolchain | `rustup toolchain install` |
| fmt | `cargo fmt --all --check` |
| clippy | `cargo clippy --workspace --all-targets --locked -- -D warnings` |
| tests | `cargo test --workspace --locked`, the layering test among them |
| docs | `cargo doc --workspace --no-deps --locked`, with `RUSTDOCFLAGS="-D warnings"` |
| deny | `cargo deny --locked check` |
| server --version | `cargo run -p oscope-server --locked -- --version` |
| *each example* --smoke | `cargo run -p <example> --locked -- --smoke`, for every example with a binary |
| CUDA tests, with `--cuda` | `OSCOPE_CUDA=1 cargo test --workspace --locked --features cuda` |

Each step's output goes to a log file. A failing step shows the end of its log. Both scripts exit with 0 only if every step passed.

### `ci-local.sh`: this checkout, as it is

```sh
scripts/ci-local.sh            # in Git Bash on Windows, or a Linux shell
scripts/ci-local.sh --cuda     # also the CUDA tests: a GPU and the CUDA toolkit
```

It checks the checkout you are in, uncommitted changes included, and builds in the usual `target/`. Its logs go to `target/ci-logs/<date-time>/`, or to `--logs DIR`. It ends like this:

```text
ok    ex-sim-render --smoke            21 s
tests: 145 passed, in 77 suites
steps: 17 passed, 0 failed
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
