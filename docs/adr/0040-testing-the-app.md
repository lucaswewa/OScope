# ADR-0040: Testing the app: unit, contract, end to end and visual

- Status: Accepted
- Date: 2026-11-17
- Phase: W0.1

## Context

- **The Rust workspace's checks are the model** (plan §5.21, ADR-0038): formatting, lints, tests, docs, and `cargo-deny` for licences and advisories, run by the local replays on Windows and on Linux, which are the merge gate.
- **The app needs the same,** with what a front end adds: a real browser, and a real server behind it.
- **Milestone 1's lessons hold** (its look back, and `microscope`'s):
  - a test waits for a condition, never a time;
  - each test has its own data, and a port the system picks;
  - what detects or supervises is broken on purpose once.
- **npm brings hundreds of packages,** most of them tools that never reach the user. ADR-0002's licence list was written for what OScope ships. The owner chose, on 2026-11-17, how far it stretches for packages.

## Decision

**1. The checks,** each an npm script in `web/`, run by the replays and by CI after the Rust ones:

| Script | Checks |
|---|---|
| `typecheck` | vue-tsc on the app; tsc on the configurations, scripts and tests |
| `lint` | ESLint, with ADR-0039's rules; Prettier's formatting |
| `test:unit` | Vitest: the app's logic and components in a page's environment, and the workspace's tools in Node's |
| `build` | Vite's build, which also writes the packages in the bundle |
| `licences` | Every package's licence, by its tier |
| `advisories` | npm's advisories, by tier |
| `test:e2e` | Playwright, on the built app |

**2. Tests are of four kinds** (the specs name them):
- **unit:** beside the code, as `*.test.ts`;
- **contract:** the client against a real server (from W1.1);
- **end to end:** in `tests/e2e/`, each test with a server of its own;
- **visual:** screenshots against baselines, in both themes (from W0.2).

**3. An end-to-end test starts its own `oscope-server`:** the workspace's debug build, with a data root of its own, on a port the system picks, serving the built app with `--web`. Tests then run side by side, and none sees another's state. The server's log is shown only if it doesn't come up.

**4. Browsers:** Chromium runs every end-to-end test. Firefox and WebKit run those tagged `@main`, the main path. WebKit stands in for the Linux desktop app's webview until W9.1 runs the tests in it.

**5. Packages come in two tiers.** What the app ships is the packages whose code is in the bundle: the build writes their names, so the tier is measured, not guessed from the lock file.

| | What the app ships | Development tools |
|---|---|---|
| **Licences** | `deny.toml`'s list, with 0BSD and MIT-0 | Those, with Python-2.0, BlueOak-1.0.0, CC-BY's data files, and MPL-2.0 unmodified (ADR-0002) |
| **Advisories** | None | Only those listed in `web/advisories.json`, each with its reason |

- **A licence outside its list** fails the check, and goes to the owner.
- **A new advisory in a tool** fails the check until someone has looked at it and listed it, as `deny.toml` lists the crates' ignored ones.
- **A listed advisory that is no longer reported** is said, so the list shrinks.

**6. Packages are added with `npm install --before`,** two weeks back (ADR-0002), and installed from the lock file with `npm ci`.

**7. Vitest starts through `scripts/vitest.mjs`,** from the folder's path as the disk spells it. On Windows a terminal may hand Node a lower-case drive letter, as VS Code's do. Vitest is then loaded twice, and every test fails before it starts.

**8. Screenshots for a phase's notes** are taken by its end-to-end test when `OSCOPE_SHOTS` is set, into `docs/milestone-2/notes/shots/`. A run without it changes nothing in the repository.

## Consequences

- **Gains:**
  - The app is checked by the same replays as the server, on both platforms.
  - An end-to-end test proves the app against the server it will meet, with no mock between them.
  - A package's licence is judged by whether it reaches the user.
- **Costs:**
  - The end-to-end tests need the server built and three browsers installed. In WSL, the browsers' system libraries are installed as root, once.
  - The advisories' check needs the network, as `cargo-deny`'s does.
  - A replay installs the packages afresh each time.
- **To watch:**
  - Advisories listed for tools: three on the day this was written, none of them with a fix old enough to take.
  - Vite's packages for other platforms are in the lock file, and are checked from it, though only one platform's are installed.

## Alternatives considered

- **A mocked server** for end-to-end tests: faster, but it would test the app against what we believe the server does.
- **One shared server** for a run of tests: fewer processes, but tests would have to clean up after each other, and couldn't run side by side.
- **`npm audit` as it is,** failing on anything: Vite itself depends on a package with an advisory and no fix, so it would fail every run.
- **The lock file's `dev` mark** as the tier: Vue Router's build plugin comes as a production dependency, though none of it is in the bundle.
