# Working on OScope

OScope (OpenScope) is the control and analysis software for a motorised brightfield microscope:

- a Tucsen MIChrome 20 camera;
- a grblHAL XYZ stage;
- Mitutoyo objectives, swapped by hand.

It has a layered Rust backend on [`teta-wot`](https://github.com/lucaswewa/teta-wot) and a Vue 3 frontend shared by a web build and a Tauri 2 desktop build.

**Where to start.** The documentation index is [`docs/README.md`](docs/README.md).

- **The plan** is [`docs/implementation-plan.md`](docs/implementation-plan.md). Its §7 and §8 tables show every phase's status, Milestone 1's and Milestone 2's.
- **The phase specs** are in [`docs/milestone-1/phases.md`](docs/milestone-1/phases.md) and [`docs/milestone-2/phases.md`](docs/milestone-2/phases.md).
- **Decisions** are the ADRs, indexed in [`docs/adr/README.md`](docs/adr/README.md).

## Delivering a phase

- **Phases follow the plan's order:** P00, then the upstream phases U01–U06, then P01 onwards. P02b runs whenever the owner is at the instrument, before P03. Milestone 2 follows, from W0.1, in plan §8's order.
- **One branch and one pull request per phase.**
  - Milestone 1's phases: the branch `m1/pNN-short-name` and the PR title `PNN: <title>`.
  - Milestone 2's phases: the branch `m2/w3.2-short-name` and the PR title `W3.2: <title>`, for the second phase of stage W3.
  - Upstream phases: the branch `u/uNN-short-name` in the owner's `teta-wot` clone, a sibling folder of this repository, with the PR title `UNN: <title>` on `lucaswewa/teta-wot`.
- **The owner approves every pull request before it is merged.**
  - Never merge without that approval.
  - Start the next phase only after the merge, from an up-to-date `main`.
  - Push only phase branches. Never push to `main`, and never create a tag, without the owner's word.
- **Each pull request contains:**
  - the code and its tests;
  - its examples, each with a README saying how to try it. A Milestone 2 phase is tried in the app itself: its notes say how, with screenshots of both themes;
  - the phase's notes, from the milestone's template: `docs/milestone-1/notes/PNN-short-name.md` ([template](docs/milestone-1/notes/template.md)), or `docs/milestone-2/notes/W3.2-short-name.md` ([template](docs/milestone-2/notes/template.md));
  - any new ADRs, marked *Accepted* (approving the PR accepts them), with their rows in the ADR index;
  - the phase marked *In review* in the plan's table, §7 or §8, and in the notes index.

  The next phase's PR marks the previous one *Done*: in the plan's table, in the notes index, and on its notes' Status line.
- **Upstream phases** put their code, tests, example, CHANGELOG entry (*Unreleased*) and guide pages in `teta-wot`. Their notes go here, as `docs/milestone-1/notes/UNN-short-name.md`, in a small OScope PR of their own.
- **The size budget** counts every hand-written changed line: source, tests, examples, HTML, CSS, comments and blank lines. Lock files, generated files, vendored code and docs don't count.
  - The budgets are S ≤ 300, M ≤ 600 and L ≤ 900.
  - Estimate code, tests, examples and glue separately before starting. Tests are often a third of the lines or more.
  - If the estimate is near or over the budget, ask the owner whether to split the phase. A split updates `phases.md`, plan §7 and the plan's change log.
  - Report the measured size honestly in the notes and the PR.
- **Milestone 2 is sized by estimates,** not by those budgets ([its specs say how](docs/milestone-2/phases.md#how-milestone-2s-phases-work)):
  - Each spec has a first estimate.
  - Before a phase's code, write a second estimate in five parts: code, tests, styles, the try-it's page and end-to-end test, and glue. It goes in the plan's change log, in a commit of its own.
  - Ask the owner before the code when the second estimate is over 2,500 lines, or a third over the first.
- **Spec-first phases** open their notes with a behaviour spec, with literature sources, before any code. Milestone 1's are listed in plan §6, and Milestone 2's are marked in its specs.
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
  - **npm packages come in two tiers** ([ADR-0040](docs/adr/0040-testing-the-app.md)), which `npm run licences` and `npm run advisories` enforce:
    - what the app's bundle holds: that list, with 0BSD and MIT-0, and no advisory;
    - development tools: also Python-2.0, BlueOak-1.0.0 and CC-BY data files, and only the advisories listed in `web/advisories.json`, each with its reason.

    Anything else goes to the owner.
- **Unsafe code** is forbidden, except in these crates:
  - `oscope-compute` (CUDA);
  - `oscope-shm` (shared memory);
  - `oscope-ext` (loading native extensions);
  - `oscope-ext-sdk` (generated C ABI glue);
  - `oscope-tucsen` (the SDK's C API).

  Every `unsafe` block has a `SAFETY:` comment.
- **Platforms** are Windows 11 and Ubuntu 22.04/24.04 LTS, both x86_64 ([ADR-0003](docs/adr/0003-platforms-windows-and-ubuntu-x86-64.md)).
  - The local replays check both before every merge ([ADR-0038](docs/adr/0038-local-ci-replays-side-by-side.md)).
  - Keep platform-specific code in small `cfg` blocks, each with an equivalent for the other platform or a clear error.
  - On the owner's Windows PC, check Linux locally under WSL (`Ubuntu-24.04`), with a Linux-side `CARGO_TARGET_DIR` so the Windows and Linux builds don't share a target folder.
- **Line endings are LF** (`.gitattributes`). Scripts that write files must not produce CRLF.
- **The GPU, hardware, Python and pictures.** CI has no GPU or hardware. Tests that need one of these run only when its environment variable is set, and are skipped otherwise:
  - `OSCOPE_CUDA=1`: CUDA tests, on the owner's machine before merging any phase that touches a CUDA path;
  - `OSCOPE_GRBL_PORT`, `OSCOPE_TUCSEN=1`: hardware tests, from M3;
  - `OSCOPE_PYTHON=1`: tests of Python extensions, which need uv. CI sets it on Ubuntu, and the local replays wherever uv is installed.
  - `OSCOPE_VISUAL=1`: the app's visual tests, whose baselines are the owner's PC's, for Windows and for WSL ([ADR-0041](docs/adr/0041-design-tokens-and-themes.md)). The replays and `two-core.sh` set it; GitHub's CI doesn't. `scripts/baselines.sh` makes the baselines again, on each system.

  Every driver also has a fake at its protocol or SDK boundary.
- **Tests:**
  - Wait for a condition, never a fixed time.
  - Give each test its own data root, scratch space and build directory, and a port the system picks: suites run side by side, several copies at once.
    - A test that starts `oscope-server` gives it a configuration of its own, with `scratch_root` and `[power] dry_run = true`. `--data-root` alone leaves the scratch space in the working folder, which every test would share.
  - Keep what a test writes small, unless its size is what it tests: recordings preallocate their segments.
  - Test with the default samples and over real distances, not only with samples chosen to make tests quick.
  - For anything that supervises, recovers or detects, break the code on purpose once and check that the test fails; say so in the notes.
- **Ports:**
  - the owner's server: 8210;
  - manual checks: 8211–8219, then 8230–8249;
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

From W0.1 onwards, the app's too, in `web/` ([`web/README.md`](web/README.md)):

```sh
npm ci
npm run typecheck && npm run lint && npm run test:unit && npm run build
npm run licences && npm run advisories
npm run test:e2e    # after `cargo build -p oscope-server`: each test starts its own server
```

`scripts/ci-local.sh` runs all of CI's checks, every example's `--smoke` among them, and `--cuda` adds the CUDA tests. It runs the suites and smoke runs eight at a time, each on two CPUs of its own. `scripts/ci-clone.sh` runs them on a fresh clone of a commit; on Windows, run it under WSL ([`scripts/README.md`](scripts/README.md)).

- **The replays are the merge gate** ([ADR-0038](docs/adr/0038-local-ci-replays-side-by-side.md)): both pass, each run alone, on the commit merged, or on one that only docs follow. GitHub Actions isn't waited for.
  - **A phase's replays run what its change reaches** ([ADR-0061](docs/adr/0061-gates-test-what-a-change-reaches.md)): `scripts/ci-local.sh --changed main`, and `scripts/ci-clone.sh COMMIT --changed main` under WSL. `web/scripts/changed.mjs` picks the Rust suites, the examples, the unit tests and the end-to-end specs; every other check runs in full.
  - **Both run in full** for a stage's checkpoint phase, and for a change to the build's files or to the gate's own scripts, which reaches everything.
  - **Each end-to-end spec says what it covers,** in a `// Covers:` line naming the feature folders whose changes it runs for.
- **Two cores, five times:** the suites, smoke runs and specs a change reaches, and only those, run five times each, side by side, on two CPUs each: `scripts/two-core.sh PACKAGE:TEST… EXAMPLE…`, with `web:unit`, `web:e2e`, `web:unit:FILE,…` and `web:e2e:SPEC,…` for the app's.
- **Disk:** check `df -h` first. The replays need some 20 GB free, and build output grows by the phase: `cargo clean --profile dev` clears it, with the owner's word.

Docs-only phases check that their relative links and anchors resolve.
