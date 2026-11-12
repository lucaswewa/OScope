# P01: Rust workspace, layering test, CI

- Status: In review
- Branch / PR: `m1/p01-workspace` / [#8](https://github.com/lucaswewa/OScope/pull/8)
- Size: planned M, re-sized to L by the owner before it started · estimated ~695 (code ~250, tests ~110, examples 0, glue ~335) · measured **761** hand-written lines (code 259, tests 158, examples 0, glue 344). Docs and `Cargo.lock` aren't counted. The tests came out about 50 lines over the estimate: rustfmt wrapped the assertions, and there are three more cases than planned.

## Summary

OScope has its Rust workspace ([spec](../phases.md#p01)):

- **The workspace:**
  - the toolchain pinned to `teta-wot` v0.2.0's, 1.98.1;
  - `teta-wot` pinned to its `v0.2.0` tag;
  - lints that require docs and forbid unsafe code.
- **A skeleton of each of the 17 Milestone 1 crates** of plan §5.2, each with its crate documentation.
- **The example:** `oscope-server --version`.
- **The layering test** checks every dependency against the layers of the new [ADR-0005](../../adr/0005-layered-crates-and-their-dependency-rules.md). It also checks that only `oscope-api` uses `teta-wot`, and that unsafe code stays forbidden outside the five crates of plan §4.3.
- **`cargo-deny`** enforces ADR-0002's licences, the RustSec advisories, and the allowed sources.
- **CI** runs every check on Windows and on Ubuntu 24.04.

## What was built

| Change | Where |
|---|---|
| **The workspace:**<ul><li>resolver 3, edition 2024, `rust-version = "1.98"` (`teta-wot` v0.2.0's), MIT, `publish = false`;</li><li>members `crates/*`, `examples/*` and `xtask`;</li><li>`teta-wot = { git, tag = "v0.2.0" }` and `serde_json` as workspace dependencies;</li><li>lints: `missing_docs = "warn"`, `unsafe_code = "forbid"`, clippy `all = "warn"`, and `undocumented_unsafe_blocks = "warn"`.</li></ul> | `Cargo.toml` |
| **The toolchain:** 1.98.1 with rustfmt and clippy, the minimal profile | `rust-toolchain.toml` |
| **The lock file,** begun from `teta-wot` v0.2.0's (see *Design notes*): 175 packages, with `teta-wot` at `338336f` | `Cargo.lock` |
| **The library skeletons,** one per crate of §5.2 in Milestone 1: `oscope-core`, `-compute`, `-shm`, `-imaging`, `-stitch`, `-volume`, `-hal`, `-sim`, `-store`, `-jobs`, `-pipeline`, `-ext`, `-ext-sdk`, `-acquire`, `-workflows` and `-api`.<ul><li>Each has a manifest and crate documentation: what it will hold, and its layer.</li><li>`oscope-api` depends on `teta-wot`. No other crate has a dependency yet.</li><li>`oscope-compute`, `-shm`, `-ext` and `-ext-sdk` allow unsafe code, and require a `SAFETY:` comment on every `unsafe` block.</li></ul> | `crates/oscope-*/` |
| **`oscope-server`:**<ul><li>`--version` (or `-V`) prints `oscope-server 0.1.0`;</li><li>`--help` (or `-h`) prints the usage;</li><li>anything else prints the usage to standard error, and exits with code 2.</li></ul> | `crates/oscope-server/` |
| **The layering check:**<ul><li>the tables: `LAYERS`, `SAME_LAYER`, `RESTRICTED`, `UNSAFE_ALLOWED`;</li><li>`workspace()` reads `cargo metadata --no-deps`;</li><li>`violations()` returns one sentence per broken rule.</li></ul> | `xtask/src/lib.rs` |
| **The layering test:** the real workspace, and six made-up graphs | `xtask/tests/layering.rs` |
| **`cargo-deny`:**<ul><li>both platforms' targets, all features;</li><li>RustSec advisories and yanked versions refused;</li><li>ADR-0002's licences;</li><li>duplicate versions reported;</li><li>crates.io and `lucaswewa/teta-wot` the only sources.</li></ul> | `deny.toml` |
| **CI** on `windows-latest` and `ubuntu-24.04`: toolchain, fmt, clippy `-D warnings`, tests, docs `-D warnings`, `cargo deny check`, and the example | `.github/workflows/ci.yml` |
| **Ignored:** `.oscope/`, `node_modules/`, `web/dist/` | `.gitignore` |
| **The example conventions:**<ul><li>`examples/ex-<name>/`;</li><li>`--smoke`;</li><li>the ports;</li><li>data roots;</li><li>pages;</li><li>vendored files;</li><li>dependencies.</li></ul> | `examples/README.md` |
| **The README:** a CI badge, *Building*, and the status | `README.md`, `crates/oscope-server/README.md` |
| **ADR-0005,** and its row in the index | `docs/adr/` |
| **Plan and notes:**<ul><li>P01 re-sized to L (§7, the phase spec, the change log);</li><li>P01 *In review*;</li><li>U06 *Done*.</li></ul> | `docs/` |

## How to try it

```sh
cargo run -p oscope-server -- --version      # oscope-server 0.1.0
cargo test -p xtask --test layering          # the layering rules, 7 tests
cargo deny check                             # advisories ok, bans ok, licenses ok, sources ok
```

To see the layering test fail, give `oscope-core` a dependency on a higher layer:

```toml
# crates/oscope-core/Cargo.toml
[dependencies]
oscope-hal = { path = "../oscope-hal" }
```

`cargo test -p xtask --test layering` then fails with: `oscope-core (L0) depends on oscope-hal (L2), in a higher layer`.

## Performance

Not relevant to this phase: there is no code on a §5.20 path yet.

## Design notes and deviations from the plan

- **The layers.**
  - §5.2 has two groups in L1 (compute, algorithms) and two in L2 (devices, data). ADR-0005 makes each layer one layer, and lists the edges allowed within a layer, each one the plan implies.
  - The rules apply to dev- and build-dependencies too, so a test can't reach up a layer either.
- **The check reads the declared dependencies** (`cargo metadata --no-deps`), not the resolved graph.
  - It is quick and needs no network.
  - The rules are about the edges a crate declares; what reaches it through them, such as `teta-wot` through `oscope-api`, is allowed.
  - Renamed dependencies are still seen by their package name.
- **Examples and `xtask` are free:** they may depend on any crate, and no layered crate may depend on them. Any `oscope-*` member must be in the table, so a new crate can't skip it.
- **Beyond the spec, the test also checks:**
  - `teta-wot` and `axum`, as ADR-0004 says: `teta-wot` only in `oscope-api`, and `axum` nowhere, as `oscope-api` uses `teta_wot::http::axum`;
  - unsafe code: every crate outside plan §4.3's five takes `[lints] workspace = true`, the only way it gets the forbid.
- **The manifests are matched as text,** to find `[lints] workspace = true`. That avoids a TOML parser as a new dependency. A unit test covers comments, CRLF, and `workspace = true` in another table.
- **The crates allowed unsafe code repeat the workspace's lints,** because a crate can't take the workspace's `forbid` and relax it. They add clippy's `undocumented_unsafe_blocks`, which checks the `SAFETY:` comments that `CLAUDE.md` requires. The workspace table has it too, so the copies differ only in `unsafe_code`.
- **`oscope-grbl` and `oscope-tucsen`** (M3) are in the tables already. Their layer and `oscope-tucsen`'s unsafe exception are fixed before they exist.
- **Only `oscope-api` has a dependency** (`teta-wot`). That way the lock, `cargo-deny` and CI cover `teta-wot`'s tree from the start. The other edges come with the phases that need them; `oscope-server` doesn't depend on `oscope-api` yet.
- **`--version` is parsed by hand,** without `clap`. It is a single flag, and the phase that adds configuration will choose the parser.
- **The lock file was begun from `teta-wot` v0.2.0's.**
  - Cargo kept every registry crate at the version locked there, and added only the six `teta-wot` packages.
  - So OScope builds on the versions `teta-wot` was released and soaked with, rather than on whatever was newest.
  - All of them are at least two weeks old: see *Tests*.
- **The licences in `deny.toml`** are ADR-0002's list, in SPDX: `MIT`, `Apache-2.0`, `BSD-2-Clause`, `BSD-3-Clause`, `ISC`, `Zlib` and `Unicode-3.0`. For "public domain" the list has `CC0-1.0` and `Unlicense`.
  - **Two other licences appear only as alternatives.** `Apache-2.0 WITH LLVM-exception` (`rustix`, `linux-raw-sys`) and `BSL-1.0` (`ryu`) are each offered beside MIT or Apache-2.0, so they aren't added.
  - **No crate needs `MIT-0` or `MPL-2.0`.**
  - **Licences no crate uses yet** (ISC, Zlib, CC0-1.0) stay in the list: it is the policy, so the "not encountered" warning is turned off.
- **Duplicate versions are reported, not refused.** `getrandom` 0.3 and 0.4, and `syn` 2 and 3, both come from `teta-wot`'s tree.
- **Advisories fail the check,** so a new RustSec advisory can fail a pull request that didn't cause it. That is intended. An advisory that doesn't apply goes in `ignore`, with the reason.
- **CI** follows `teta-wot`'s layout, with the same job names and action versions: `actions/checkout@v7`, `Swatinem/rust-cache@v2`, `taiki-e/install-action@v2`. It installs `cargo-deny` 0.20.2, released 2026-07-09.
- **No `--smoke` step yet:** there are no examples. P02's `ex-core-tour` is the first, and its phase adds the step.
- **`examples/*` as a member glob** saves each example from editing the root manifest. A folder without a `Cargo.toml` breaks the glob, so `examples/vendor/` will need an `exclude`; the conventions say so.
- **`xtask` is a library with tests,** not a `cargo xtask` command. The check runs as part of `cargo test`, so it needs no alias.

## Tests

- **The layering test** (`xtask/tests/layering.rs`), 7 tests:
  - the workspace follows the rules;
  - lower layers, and the listed edges within a layer, are allowed;
  - upward edges and unlisted edges within a layer are refused;
  - only `oscope-api` may depend on `teta-wot`, and nothing on `axum`;
  - `oscope-*` crates must be in the table, and may not depend on other members;
  - unsafe code is forbidden outside the listed crates;
  - the tables name only layered crates, and `SAME_LAYER` joins crates of one layer.
- **A unit test** of the manifest check (`workspace_lints_are_found_only_in_their_own_table`).
- **The command line** (`crates/oscope-server/tests/cli.rs`): `--version` and `-V`, and the exit code 2 with the usage, for no arguments, an unknown one, and two.
- **Mutation checks** on the real workspace, each restored afterwards. The workspace test failed every time, with these messages:

  | Broken on purpose | The test's message |
  |---|---|
  | `oscope-core` depends on `oscope-hal` | `oscope-core (L0) depends on oscope-hal (L2), in a higher layer` |
  | `oscope-acquire` depends on `teta-wot` | `oscope-acquire depends on teta-wot, which only oscope-api may depend on` |
  | `oscope-store` has its own `[lints.rust]` | `oscope-store doesn't take the workspace lints, which forbid unsafe code` |

- **The dependencies' age.**
  - crates.io gave the release date of each of the lock's 151 registry crates. The newest is `tokio-util` 0.7.20, of 2026-10-09; none is from after 2026-10-28, two weeks before today, and none is yanked.
  - `teta-wot` v0.2.0 is the owner's own release, which this plan waits for.
  - `cargo-deny` 0.20.2 is a CI tool, not a dependency. It was released on 2026-07-09.
- **Windows** (the working tree, the owner's PC): all 7 CI steps passed on `e7a4a32`. They are toolchain, fmt, clippy, tests, docs, deny and the example. That made 37 test suites and 10 tests: 18 unit-test suites, 2 integration suites, and 17 doc-test suites, which are empty.
- **Ubuntu 24.04.5** (WSL, a fresh clone in the Linux file system, with its own `CARGO_TARGET_DIR`):
  - the clone has 71 files and no CR in any;
  - all 7 steps passed on `e7a4a32`, from an empty target folder;
  - the same 37 suites and 10 tests.
- **GitHub CI** passed on `448cf28`, the workflow's first run: Ubuntu in 1 min 57 s, Windows in 2 min 43 s. Every step ran, with 37 suites and 10 tests on each, `cargo deny` ok in all four checks, and the example's version printed.

## Follow-ups and known gaps

- **P02 adds the CI step** that runs every example's `--smoke`.
- **`examples/vendor/`** needs `exclude` in the root `Cargo.toml` when it arrives.
- **The edges within L1 and L3** are the plan's best reading. P06, P21 and P24–P26 may change them, as ADR-0005 allows.
- **U06's 24-hour soak** ends about 2026-11-12 16:45 PST. Its result goes into the U06 notes in this pull request.

## Clean room

Everything here was written for this phase. The CI workflow has the same job names and action versions as `teta-wot`'s, the owner's own dependency, so the two repositories read alike. Nothing was copied from `microscope` or any other codebase, and no algorithm is involved.
