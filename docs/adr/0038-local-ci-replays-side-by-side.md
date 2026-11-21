# ADR-0038: Local CI replays, side by side on two CPUs

- Status: Accepted; extended by [ADR-0061](0061-gates-test-what-a-change-reaches.md), a phase's replays running what its change reaches
- Date: 2026-11-15
- Phase: none: the owner's request, before P40

## Context

- **GitHub Actions hasn't started a job since 2026-11-14,** for the account's billing. P29 to P39 merged on the local replays instead, as the owner chose: `scripts/ci-local.sh` on Windows, and `scripts/ci-clone.sh` on a fresh clone under WSL.
- **The owner then chose to keep it so** (2026-11-15): a pull request passes the replays, not GitHub CI. [ADR-0003](0003-platforms-windows-and-ubuntu-x86-64.md) has GitHub Actions check both platforms for every pull request.
- **A Windows replay took about 21 minutes,** most of it in two steps run one thing at a time:
  - the 186 test suites, 518 s within them;
  - the 42 examples' smoke runs, some 600 s.

  The four slowest suites, a minute or more each, were mostly the simulator rendering in an unoptimised build.
- **Two-core runs,** each suite five times on two CPUs, find what this PC's 24 threads hide: seven faults in P17, P18 and P38. Run one after another, P39's took some 35 minutes, two thirds of it a suite P39 didn't change.
- **The owner's PC** has an i9-12900: eight performance cores with two threads each, CPUs 0–15, and eight efficiency cores, CPUs 16–23.

## Decision

**1. The local replays are the merge gate.**

- Both must pass, each run alone, on the commit merged, or on one that only docs follow: `ci-local.sh` on Windows, and `ci-clone.sh` under WSL.
- GitHub Actions' workflows stay as they are, but a pull request doesn't wait for them.

**2. The replays run the test suites and the smoke runs side by side.**

- **Eight at a time,** each limited to two CPUs of its own, as on CI's two-core runners. Slot k gets CPUs 2k and 2k+1: on the owner's PC, the performance cores' threads. `--jobs N` sets how many.
- **The test binaries** are built once, and each runs in its package's folder, as `cargo test` runs it. The doc tests run as `cargo test --doc`.
- **The examples** are built together, then their smoke runs go side by side.
- **The longest go first,** by the last run's times in `target/ci-times.tsv`, so that none is left to run alone at the end.

**3. Before a pull request,** the suites and smoke runs a change touches, and only those, run five times each, side by side, on two CPUs each: `scripts/two-core.sh`.

**4. Dev and test builds optimise the dependencies, the simulator, imaging and compute** (`opt-level = 2`). The rest of the workspace stays unoptimised, quick to build after a change.

**5. Tests take ports the system picks,** and record small segments where a segment's size isn't what they test, so that copies of them run side by side.

## Consequences

- **A Windows replay takes about 6 minutes, not 21,** with the build up to date:
  - the suites some 130 s, not 615;
  - the smoke runs some 100 s, not 600.
- **Each suite and smoke run has two CPUs,** like CI's runners, not all 24. The replays now find what only two-core runs found before. The first found that P38's halted mosaic could still finish the frame it was drawing, now fixed: drawing and stopping take turns.
- **Disk:** suites that record side by side preallocate their segments at once, 4 GiB each by default. The replays need some 20 GB free.
- **GitHub CI's results aren't read.** If its jobs start again, they still run, and their failures are the owner's to look at.
- **Debugging** the optimised crates shows fewer variables. A local profile override undoes it.
- **The first build after a toolchain or profile change** takes some 4 minutes, the dependencies compiled with optimisation.
- **On Windows, pinning goes through PowerShell,** a fraction of a second for each run.

## Alternatives considered

- **cargo-nextest,** each test in its own process, many at once: a tool to install on both platforms, and a different way to run the suites than `cargo test`'s.
- **Side by side, unpinned:** faster still, but the runs would share all 24 CPUs, the efficiency cores among them, and look less like CI's runners.
- **The whole workspace optimised:** every change would rebuild more slowly.
- **GitHub CI as the gate,** as ADR-0003 has it: its jobs don't start, and its minutes are the owner's cost.
