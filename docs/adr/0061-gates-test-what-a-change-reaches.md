# ADR-0061: Gates test what a change reaches

- Status: Accepted
- Date: 2026-11-21
- Phase: none: the owner's request, before W5.1

## Context

- **Each phase's gate ran everything,** whatever the phase changed. ADR-0038 made both full replays, `ci-local.sh` on Windows and `ci-clone.sh` under WSL, the merge gate. Before them, the two-core runs repeated each touched suite five times. For an app phase, `web:e2e` was the whole end-to-end suite five times over, on two CPUs each: about 22 minutes. With the two replays, a gate took about an hour, more with a rerun.
- **Most of Milestone 2's phases change the app alone,** yet their gates built and ran every Rust suite and smoked every example. W4.1 changed no Rust, and its gate still ran 640 Rust tests and 58 smoke runs, twice.
- **The owner asked, before W5, to test only what changed,** with both full replays kept for each stage's checkpoint.

## Decision

**1. A phase's gate runs the tests its change reaches.** `web/scripts/changed.mjs BASE` reads the files changed since BASE's merge base with the commit, and picks:

- **Rust:** the workspace packages holding a changed file, and every workspace package that depends on them, as a test or otherwise (`cargo metadata`). Their test binaries run, and the doc tests of those with a library. The examples among them get their smoke runs.
- **The app's unit tests:** those Vitest finds related to the changed files (`vitest related`). All, when the app's tooling changes: its packages, Vite's or Vitest's configuration, a TypeScript configuration.
- **The end-to-end specs:**
  - the specs changed;
  - the spec whose baseline changed, found by the screenshot's name;
  - and the specs covering a changed feature: each spec's `// Covers:` line names the folders whose changes it runs for. A spec without one runs for any change to the app's code.
- **Python:** the Python SDK's tests, when `python/` changes.

**2. Some changes reach everything of a kind.**

- **Every check:** the workspace's manifest or lock file, the toolchain, `.cargo/`, the workflows, `ci-local.sh`, `ci-clone.sh`, `pinned.sh`, or `changed.mjs` itself.
- **Every spec:**
  - the app's shared code, `web/src/` outside `features/`, except its strings in `app/locales/`, which each feature's own specs read;
  - the end-to-end harness, `web/tests/e2e/` outside the specs and their baselines;
  - Vite's or Playwright's configuration, the app's packages, or `index.html`;
  - and any change that reaches `oscope-server`, which every spec runs against.

The plan says why, in its `why:` lines.

**3. The rest of the replay runs in full:** the toolchain, rustfmt, Clippy, the docs, `cargo deny`, the server's build, `npm ci`, the API's types, the typecheck, lint, the app's build, its licences and advisories. A step nothing reaches says `skip`, and isn't a failure. `ci-local.sh --changed BASE` runs this way, and `ci-clone.sh --changed BASE` under WSL, with BASE `main` as the phase's branch left it.

**4. The two-core runs repeat only what the phase reaches.** `two-core.sh` takes `web:e2e:SPEC,...` and `web:unit:FILE,...`, as well as the whole suites.

**5. Both full replays still run:**
- for each stage's checkpoint phase (W5.4's Checkpoint I, W6.4's J, and so on);
- for any change that reaches every check;
- and, as before, the replays' rule about a commit that only docs follow stands.

## Consequences

- **Gains:**
  - An app phase's gate builds and runs no Rust suite and no smoke run, unless it changes the server.
  - Its two-core runs are its own specs, five times, not the whole suite.
  - The plan for each run is written beside its logs, with why.
- **Costs:**
  - A change can break a test it doesn't reach: a spec that misses a `Covers:` folder, or one feature's code another uses without saying so. Such a break is found at the stage's checkpoint, when everything runs.
  - The specs' `Covers:` lines are kept by hand. A test finds every spec has one.
  - Shared app code, the harness and the server still reach every spec. The whole suite takes some two minutes in a replay; its two-core repeats are what this saves.
- **Extends** [ADR-0038](0038-local-ci-replays-side-by-side.md): its replays stay the gate, and they run what the change reaches.
