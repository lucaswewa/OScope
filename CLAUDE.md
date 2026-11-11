# Working on OScope

OScope (OpenScope) is the control and analysis software for a motorised brightfield microscope:

- a Tucsen MIChrome 20 camera;
- a grblHAL XYZ stage;
- Mitutoyo objectives, swapped by hand.

It has a layered Rust backend on [`teta-wot`](https://github.com/lucaswewa/teta-wot) and a Vue 3 frontend shared by a web build and a Tauri 2 desktop build.

**Where to start.** The documentation index is [`docs/README.md`](docs/README.md).

- **The plan** is [`docs/implementation-plan.md`](docs/implementation-plan.md). Its §7 table shows every phase's status.
- **The phase specs** are in [`docs/milestone-1/phases.md`](docs/milestone-1/phases.md).
- **Decisions** are the ADRs, indexed in [`docs/adr/README.md`](docs/adr/README.md).

## Delivering a phase

- **Phases follow the plan's order:** P00, then the upstream phases U01–U06, then P01 onwards. P02b runs whenever the owner is at the instrument, before P03.
- **One branch and one pull request per phase.**
  - OScope phases: the branch `m1/pNN-short-name` and the PR title `PNN: <title>`.
  - Upstream phases: the branch `u/uNN-short-name` in the owner's `teta-wot` clone, a sibling folder of this repository, with the PR title `UNN: <title>` on `lucaswewa/teta-wot`.
- **The owner approves every pull request before it is merged.**
  - Never merge without that approval.
  - Start the next phase only after the merge, from an up-to-date `main`.
  - Push only phase branches. Never push to `main`, and never create a tag, without the owner's word.
- **Each pull request contains:**
  - the code and its tests;
  - its examples, each with a README saying how to try it;
  - the phase's notes, `docs/milestone-1/notes/PNN-short-name.md`, from [the template](docs/milestone-1/notes/template.md);
  - any new ADRs, marked *Accepted* (approving the PR accepts them), with their rows in the ADR index;
  - the phase marked *In review* in plan §7 and in the notes index.

  The next phase's PR marks the previous one *Done*: in plan §7, in the notes index, and on its notes' Status line.
- **Upstream phases** put their code, tests, example, CHANGELOG entry (*Unreleased*) and guide pages in `teta-wot`. Their notes go here, as `docs/milestone-1/notes/UNN-short-name.md`, in a small OScope PR of their own.
- **The size budget** counts every hand-written changed line: source, tests, examples, HTML, CSS, comments and blank lines. Lock files, generated files, vendored code and docs don't count.
  - The budgets are S ≤ 300, M ≤ 600 and L ≤ 900.
  - Estimate code, tests, examples and glue separately before starting. Tests are often a third of the lines or more.
  - If the estimate is near or over the budget, ask the owner whether to split the phase. A split updates `phases.md`, plan §7 and the plan's change log.
  - Report the measured size honestly in the notes and the PR.
- **Spec-first phases** open their notes with a behaviour spec, with literature sources, before any code. They are listed in plan §6.
- **Changes outside a phase,** this file included, still go through a branch and an approved PR.

## Rules

- **Clean room** ([ADR-0002](docs/adr/0002-clean-room-and-dependency-licences.md)):
  - Nothing is copied from `microscope` or any other codebase, nor ported line by line. Its ideas and its look may be reused, described in our own words.
  - Algorithms are written from published methods, cited in the phase's notes.
  - Tucsen's sample images are never committed. The Tucsen SDK is never redistributed unless its licence allows it.
- **Dependencies:**
  - Their licences must allow MIT distribution: MIT, Apache-2.0, BSD, ISC, Zlib, Unicode, or public domain. MPL-2.0 only as an unmodified development dependency.
  - Use only versions at least two weeks old. For npm: `npm install --before=<date two weeks ago>`. For crates: check the release date.
  - `cargo-deny` enforces licences from P01.
- **Unsafe code** is forbidden, except in these crates:
  - `oscope-compute` (CUDA);
  - `oscope-shm` (shared memory);
  - `oscope-ext` (loading native extensions);
  - `oscope-ext-sdk` (generated C ABI glue);
  - `oscope-tucsen` (the SDK's C API).

  Every `unsafe` block has a `SAFETY:` comment.
- **Platforms** are Windows 11 and Ubuntu 22.04/24.04 LTS, both x86_64 ([ADR-0003](docs/adr/0003-platforms-windows-and-ubuntu-x86-64.md)).
  - CI checks both on every PR.
  - Keep platform-specific code in small `cfg` blocks, each with an equivalent for the other platform or a clear error.
  - On the owner's Windows PC, check Linux locally under WSL (`Ubuntu-24.04`), with a Linux-side `CARGO_TARGET_DIR` so the Windows and Linux builds don't share a target folder.
- **Line endings are LF** (`.gitattributes`). Scripts that write files must not produce CRLF.
- **The GPU and hardware.** CI has neither. Tests that need them run only when their environment variable is set, and are skipped otherwise:
  - `OSCOPE_CUDA=1`: CUDA tests, on the owner's machine before merging any phase that touches a CUDA path;
  - `OSCOPE_GRBL_PORT`, `OSCOPE_TUCSEN=1`: hardware tests, from M3.

  Every driver also has a fake at its protocol or SDK boundary.
- **Tests:**
  - Wait for a condition, never a fixed time.
  - Give each test its own data root, port and build directory.
  - Test with the default samples and over real distances, not only with samples chosen to make tests quick.
  - For anything that supervises, recovers or detects, break the code on purpose once and check that the test fails; say so in the notes.
- **Ports:**
  - the owner's server: 8210;
  - manual checks: 8211–8219;
  - tests: 8220–8229;
  - Vite (M2): 8280–8289.

  Stop only processes you started, by their process ID. Never stop "whatever is listening on a port".

## Checks before opening a pull request

From P01 onwards, these match CI:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked
cargo deny check
# CUDA paths, on the owner's machine:
OSCOPE_CUDA=1 cargo test --workspace --locked --features cuda
```

Docs-only phases check that their relative links and anchors resolve.
