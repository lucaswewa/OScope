# U06: Release v0.2.0

- Status: In review
- Branch / PR: `u/u06-release-v0.2.0` in `teta-wot` / [lucaswewa/teta-wot#6](https://github.com/lucaswewa/teta-wot/pull/6), and the README in [lucaswewa/teta-wot#7](https://github.com/lucaswewa/teta-wot/pull/7) · released as [`v0.2.0`](https://github.com/lucaswewa/teta-wot/releases/tag/v0.2.0)
- Size: planned S, estimated ~20 (code ~5, tests ~10, examples 0, glue ~5) · measured **140** hand-written changed lines (+99 −41): code 58, tests 64, examples 8, glue 10. Docs and `Cargo.lock` aren't counted. Most of it is a race fixed in `teta-wot-core`, found while replaying CI, and the problem types' new URL with its test (below).

## Summary

`teta-wot` v0.2.0 is tagged and released, the version OScope pins ([spec](../phases.md#u06)). The release followed `teta-wot`'s `docs/releasing.md`, and the owner gave the word for the tag. The 24-hour soak was still running when it was tagged.

- **The version.** The workspace is `0.2.0`, and the CHANGELOG has its `0.2.0` section. U02–U05 broke two struct literals, so this is a minor release (`0.1.x` → `0.2.0`).
- **Migration notes.** The guide gains a page, *Migrating*: from LabThings in Python, and from 0.1 to 0.2. The tutorial's two links to `../migration.md`, broken on `main`, now lead to it.
- **The soak runs** on the release build: 24 hours, and an hour in the `wot` profile. The hour passed. The 24-hour run ends about 2026-11-12 16:45 PST. If it fails, the fix is released as v0.2.1.
- **The tag,** `v0.2.0`, is on `338336f`, pushed on 2026-11-11 at 20:45 PST. `release.yml` published [the release](https://github.com/lucaswewa/teta-wot/releases/tag/v0.2.0) six minutes later.
- **The README** was rewritten in a follow-up PR ([lucaswewa/teta-wot#7](https://github.com/lucaswewa/teta-wot/pull/7)), which the owner edited. It ships in v0.2.0.
- **The licence review** found the Windows package shipped no licence texts. As the owner chose:
  - the package carries `THIRD_PARTY_LICENSES.txt` again, and the IJG acknowledgement `jpeg-encoder`'s licence asks for;
  - a new `THIRD_PARTY_NOTICES.md` lists the vendored files.
- **Two fixes the owner asked for during review:**
  - **the problem types' URL** moved to `lucaswewa/teta-wot`, with a page that documents them;
  - **the `conformance/` project,** which isn't in the repository, is no longer referenced.

## What was built

All of it is in `teta-wot`.

| Change | Where |
|---|---|
| **The version:** `0.2.0` in `[workspace.package]`, and `Cargo.lock` updated | `Cargo.toml`, `Cargo.lock` |
| **The default OpenAPI version** is the crate's (`env!("CARGO_PKG_VERSION")`), where it was a hard-coded `"0.1.0"` that would have been wrong in every later release | `crates/teta-wot-http/src/lib.rs`, `crates/teta-wot-server/src/lib.rs` (its doc) |
| **`RouteError::ForeignEndpoint`'s message** named the old crate, `wot_http::Endpoint`; it now names `teta_wot::http::Endpoint` | `crates/teta-wot-http/src/lib.rs` |
| **A panicking device call faults the device before its caller is answered.** The device thread used to send `DeviceError::Panicked` first, and only then set `Faulted`, so `Device::state()` could still say `Ready` to a caller that had just been told of the panic. `Call::run` now takes a `faulted` callback, which runs before the reply is sent | `crates/teta-wot-core/src/device.rs` |
| **Problem types under `lucaswewa/teta-wot`,** as the owner asked:<ul><li>`PROBLEM_TYPES_URL` is `https://github.com/lucaswewa/teta-wot/blob/main/docs/problems.md`, where `…/lucaswewa/wot/…` didn't resolve;</li><li>a new `docs/problems.md` describes the six types (`validation-error`, `global-lock-busy`, `cancelled`, `failed`, `not-cancellable`, `no-output`), each under a heading that is its anchor;</li><li>the OpenAPI document's description and the guide's *Wire profiles* link follow;</li><li>`teta-wot`'s `CLAUDE.md` now gives the URL among the identifiers that must not change.</li></ul> | `crates/teta-wot-http/src/problem.rs`, `openapi.rs`, `docs/problems.md`, `docs/guide/src/wire_profiles.md`, `CLAUDE.md` |
| **No `conformance/` project,** as the owner asked:<ul><li>the Python instructions say `pip install labthings-fastapi==0.3.0 httpx`, then `python examples/simulated-microscope/demo.py`;</li><li>two rustdoc comments no longer cite its fixtures;</li><li>`demo.py` and its README no longer say CI runs it.</li></ul> | `tutorial/installing.md`, `examples.md`, `examples/simulated-microscope/README.md` and `demo.py`, `examples/observe-websocket/README.md`, `settings.rs`, `teta-wot-http/src/lib.rs` |
| **The CHANGELOG's `0.2.0` section:**<ul><li>a summary that links the migration page;</li><li>the U01–U05 entries as one list per heading;</li><li>this phase's entries, including a new *Fixed* heading;</li><li>the `[0.2.0]` link.</li></ul>An empty *Unreleased* sits above it. | `CHANGELOG.md` |
| **The guide's *Migrating* page:**<ul><li>*From LabThings in Python:* the tutorial's table, extended with settings, slots, the global lock, arrays, the command line, the configuration file and the client. Every row is one the guide already describes.</li><li>*From 0.1 to 0.2:* the new tag; what is unchanged; the two struct literals; the four behaviour changes to check; what's new, with links.</li></ul> | `docs/guide/src/migration.md`, `SUMMARY.md` |
| **Other guide fixes:**<ul><li>the dependency's tag is `v0.2.0`;</li><li>the features table lists `tls`;</li><li>"depending on `wot`" names `teta-wot`;</li><li>the tutorial's sentences about the migration guide match the page.</li></ul> | `tutorial/installing.md`, `tutorial/index.md`, `tutorial/writing_a_thing.md` |
| **`THIRD_PARTY_NOTICES.md`:** Swagger UI 5.33.0 (Apache-2.0), ReDoc 2.5.4 (MIT) and the W3C TD 1.1 JSON Schema (W3C Software and Document License), with sources and SHA-256 checksums, and the W3C licence's short notice | `THIRD_PARTY_NOTICES.md` |
| **The release package:**<ul><li>`release.yml` packages `THIRD_PARTY_LICENSES.txt` (from `tools/licenses.py`) and `THIRD_PARTY_NOTICES.md` again. The file is back to what it was before `135f9ff` removed them.</li><li>The package README lists them, and states that the software "is based in part on the work of the Independent JPEG Group".</li><li>"unter MIT license" became "under the MIT licence".</li></ul> | `.github/workflows/release.yml`, `tools/release/README.txt` |
| **Small doc fixes:**<ul><li>`releasing.md` names "the wire profiles" (not `labthings`), and the soak's Linux server path;</li><li>`tools/README.md` no longer says CI runs the soak test (it doesn't);</li><li>the `docs-offline` example's README names `teta-wot` and `crates/teta-wot-http/assets/`.</li></ul> | `docs/releasing.md`, `tools/README.md`, `examples/docs-offline/README.md` |

## How to try it

The migration page, in the built guide:

```sh
mdbook build docs/guide        # then open target/guide/migration.html
```

The default API version:

```sh
cargo run -p counter-server -- --serve --port 8211
curl -s http://127.0.0.1:8211/openapi.json    # "info": {"title": "teta-wot", "version": "0.2.0"}
```

The problem types: [`docs/problems.md`](https://github.com/lucaswewa/teta-wot/blob/main/docs/problems.md). A `type` such as `…/docs/problems.md#validation-error` leads to its heading.

The release package was built locally as `release.yml` builds it, without publishing anything: see *Tests*.

## Performance

**The soak** (`releasing.md` step 3), on the release build of `b6da70b`, on the owner's new PC: Windows 11, an Intel Core i9-12900 (16 cores, 24 threads), 64 GB.

| Run | Command | Started | Result |
|---|---|---|---|
| 24 hours | `soak --duration 24h --report soak.csv` (the `tetathing` profile) | 2026-11-11 16:44 PST | running; ends about 2026-11-12 16:45 PST |
| 1 hour | `soak --duration 1h --profile wot` | 2026-11-11 16:44 PST | **PASSED** at 17:45 PST |

- **The binaries** are copies of `target/release/simulated-microscope.exe` and `soak.exe`, in `target/soak-0.2.0/`, so later builds can't touch them. Their SHA-256s:
  - `simulated-microscope.exe`: `83820fd5…28a3c070`;
  - `soak.exe`: `642f0192…a6986aa`.
- **The 1-hour run in numbers:**
  - 439,770 requests, 0 errors, and 0 streams ended early;
  - 75,100 MJPEG frames, 58,023 events and 139,637 WebSocket messages;
  - a p50 latency of 0.2–0.3 ms, with the worst minute's p99 at 82.4 ms;
  - private bytes after the warm-up of 26.6 MB at first and 26.8 MB at the end, against a 49.2 MB limit: steady.
- **The logs and `soak.csv`** are kept beside them.
  - **The release's notes** end with a *Soak* section: the 1-hour summary, and the 24-hour run as still going.
  - **`soak-1h-wot.log`** is attached to the release.
  - **When the 24-hour run ends,** its summary goes into that section, and its log and `soak.csv` are attached.

## Design notes and deviations from the plan

- **The soak ran before the tag, after the merge,** as `releasing.md` has it, but on the pull request's build.
  - **After the 1-hour run passed,** #6 was squash-merged into `main` (`bd2982a`). `main`'s code is the soaked code, and its CI passed.
  - **The owner's choices changed twice.** They chose first to tag then, then to wait for the 24-hour run, and then to tag without waiting for it.
  - **The README** joined the release in the meantime (#7, docs only). It was squash-merged as `338336f`, and `main`'s CI passed on it before the tag.
  - **`v0.2.0` was tagged during the 24-hour run.** That run goes on, on the same binaries. If it fails, the fix is released as v0.2.1; a published tag isn't moved (`releasing.md`).
- **The soak ran before the merge, not after.**
  - **`releasing.md`'s order** is: merge, wait for CI, then soak `main`'s release build.
  - **Here the soak ran on the pull request's code,** so that its results are part of the review. A squash merge of a branch that is up to date with `main` gives `main` the same files, so the soaked build is the released one.
  - **Before tagging,** check that nothing the binaries are built from changed since the soaked commit: `git diff --stat b6da70b main -- crates examples tools Cargo.toml Cargo.lock rust-toolchain.toml` prints nothing.
- **The two soak runs ran side by side,** not one after the other. Each has its own server, its own port (0) and its own temporary folder. Running them together loads the machine more than the checklist asks, and saves an hour.
- **A race fixed in `teta-wot-core`,** beyond the spec.
  - **How it was found.** The first Windows CI replay ran with both soak runs going, and the `device-actor` example failed: after a call panicked, it found the device `Ready` rather than `Faulted`.
  - **The cause.** The device thread answered the caller first, and marked the device faulted after. It has been so since 0.1.0. The integration test had worked around it: a second call waited for the state to change.
  - **Why here.** A release whose CI example can fail by chance shouldn't ship. The fix is small, and the soak had run for only about 15 minutes when it was found.
  - **The soaks were restarted** on the fixed build. The first runs' logs are kept (`target/soak-aborted-356324b`); they had no errors in 18 minutes.
- **The soaks were restarted a second time** for the problem types' URL, which is compiled into the binary. The runs on `bd90a8c` had no errors in 15 minutes (`target/soak-aborted-bd90a8c`).
- **The problem types' URL changed,** at the owner's word.
  - **It is one of the identifiers that must not change** (`teta-wot`'s `CLAUDE.md`), because clients compare problem `type`s. So the change is breaking.
  - **The CHANGELOG and the migration page** give the old and new prefixes, and say clients must use the new one. The names after the `#` didn't change.
  - **The TD `id` namespace** (`https://github.com/lucaswewa/wot#td-id`) wasn't touched. It only seeds the UUIDs and was never meant to resolve; changing it would change every TD `id`.
- **Small fixes beyond the spec,** each found while preparing the release:
  - the default OpenAPI version, which would otherwise have said `0.1.0` in 0.2.0;
  - the stale `wot_http` in an error message, and `wot` in two docs;
  - `tools/README.md`'s claim that CI runs the soak test;
  - the missing `tls` row in the features table.
- **The licence review, and the owner's choices.**
  - **The binaries:** `tools/licenses.py` counts 159 crates, against v0.1.0's 145. The new ones come with TLS and HTTP/2:
    - `h2`, `ring`, `rustls`, `rustls-pki-types`, `rustls-webpki`, `tokio-rustls`, `tokio-util`;
    - `subtle`, `untrusted`, `zeroize`, `fnv`, `try-lock` and `want`.

    Their licences are MIT, Apache-2.0, ISC and BSD-3-Clause, alone or combined. No licence kind is new since v0.1.0.
  - **Two kinds outside the usual list** were already in v0.1.0:
    - `MIT-0` (`borrow-or-share`), more permissive than MIT;
    - IJG, in `jpeg-encoder`'s `(MIT OR Apache-2.0) AND IJG`. For binary-only copies, its documentation must say the software "is based in part on the work of the Independent JPEG Group". The package's README now says so.

    `Unlicense OR MIT` and `Apache-2.0 OR BSL-1.0` are taken as MIT and Apache-2.0.
  - **The package had shipped no licence texts** since `135f9ff`, while `releasing.md` still said it did. The owner chose to restore `THIRD_PARTY_LICENSES.txt`.
  - **`THIRD_PARTY_NOTICES.md` had never existed,** though `releasing.md` and `licenses.py` refer to it. The owner chose to create it. That records the W3C Software and Document License, outside the usual list but in the sources since 0.1, as accepted.
  - **None of the vendored files is in the binaries.** Neither binary enables `docs-offline` or `validation`.
- **The CHANGELOG's date** is 2026-11-11, the day of the tag.
  - **It moved with the owner's choices:**
    - 2026-11-12 in #6;
    - 2026-11-11 in `96ef922`;
    - 2026-11-12 in #7 (`3e2fa8e`);
    - and 2026-11-11 again (`b8a48a1`).
  - **Each change touches only `CHANGELOG.md`,** not the soaked code.
- **The 0.2.0 section was tidied:** one list under each heading, where U01–U05 had each added a group separated by blank lines.

## Tests

- **The device race:**
  - **A unit test** (`device::tests::a_panicking_call_faults_the_device_before_its_caller_is_answered`) runs a panicking call directly. It checks that the device is marked faulted while the caller's reply channel is still empty, so it doesn't depend on timing.
  - **The integration test** `panic_faults_calls_until_reset_recreates_the_driver` asserts `Faulted` right after the `Panicked` reply. It used to make a second call first, to wait for the state.
- **The API version:** `the_default_api_version_is_the_crate_version` (`teta-wot-http/tests/options.rs`) reads the OpenAPI document's `info`.
- **The problem types:** `every_problem_type_is_documented_where_its_url_points` (`problem.rs`) pins `PROBLEM_TYPES_URL`, and finds a `## name` heading in `docs/problems.md` for each of the six types.
- **Mutation checks:**
  - **Answering before faulting,** as before the fix, fails the unit test: the callback saw the answer already sent.
  - **A hard-coded `"0.1.0"`** fails the API-version test.
  - **Renaming the `## no-output` heading** fails the problem-types test.
  - All files were restored.
- **Links.** A script checked every relative link and anchor:
  - **the built guide:** 354 links, none broken. Run against `main`, it finds exactly the two broken `../migration.html` links;
  - **`teta-wot`'s Markdown:** 80 files, 130 links, none broken.
- **The Python instructions** were followed in a fresh virtualenv (`pip install labthings-fastapi==0.3.0 httpx`). `demo.py --spawn target\debug\simulated-microscope.exe` ran end to end: moves, autofocus, a 24 KB capture and two MJPEG frames.
- **GitHub CI,** now that Actions minutes are back, passed on `bd90a8c` (Ubuntu 3 min 27 s, Windows 6 min 11 s). On `b6da70b` it passed too (Ubuntu 3 min 13 s, Windows 5 min 22 s). Locally, `b6da70b` passed fmt, clippy and the tests (78 suites).
- **CI replayed locally on `bd90a8c`,** before the minutes came back:

  | Job | Result |
  |---|---|
  | Ubuntu 24.04.5, a fresh LF clone in WSL (0 CRLF files), with its own `CARGO_TARGET_DIR` | All 46 steps passed (toolchain, fmt, clippy, tests, docs, docs pages, 39 examples, the guide); 80 test suites ok, 248 tests |
  | Windows, the working tree | All 46 steps passed, the guide included; 80 test suites ok, 246 tests |

  The test counts differ by platform: Linux also runs the signal tests and a Unix shutdown test, and Windows a doc test of the Windows service.
- **The first Windows replay** found the race. It ran on `54aaf48`, with both soak runs going: `device-actor` failed its `Faulted` assertion, and the other steps run before it passed.
- **`release.yml`, dry run** on `bd90a8c`, with `GITHUB_REF_NAME=v0.2.0`, never publishing:
  - **The version check** passed, and the release notes are the 70-line `0.2.0` section.
  - **Both binaries built.**
  - **The package holds eight files:**
    - the two binaries;
    - `microscope.json` and `demo.py`;
    - the CHANGELOG and `README.txt`, which ends with the IJG sentence;
    - `THIRD_PARTY_NOTICES.md`;
    - `THIRD_PARTY_LICENSES.txt` (1.4 MB, 159 crates).
  - **The source archives and `SHA256SUMS`** were written.
  - **Not run:** 7-Zip (Python's `zipfile` stood in) and `gh release create`.
- **`release.yml` on the tag** passed in 5 min 28 s, and published:
  - the Windows package;
  - the two source archives;
  - `SHA256SUMS`.

  The downloaded package matches its checksum, and holds the same eight files as the dry run.

## Follow-ups and known gaps

- **The 24-hour soak** ends about 2026-11-12 16:45 PST, after the tag. Its result goes into the release and into these notes. A failure would be fixed in v0.2.1.
- **Three examples mention Python scripts that aren't in the repository:**
  - `observe-websocket`: `observe.py`, in its README, `Cargo.toml` description and code comments;
  - `discovery`: `browse.py`, in a code comment; the guide's *Discovery* page says the example finds Things "from Python";
  - `blob-camera`: `blob_client.py`, in a code comment.

  They probably lived with the `conformance/` project. Only `demo.py` exists.
- **The W3C licence asks for its full text** to travel with copies of the schema. `THIRD_PARTY_NOTICES.md` gives the short notice and links the full text. The text could be vendored beside the schema.
- **On this new PC,** the first CI replay's cargo steps failed at once: the rustup proxy couldn't remove `.cargo/bin/rustup-init.exe` ("Access is denied"), most likely left by rustup updating itself during the toolchain step. The file was gone a minute later, and the next replay ran clean.

Next phase: P01 (Rust workspace, layering test, CI), which pins `v0.2.0`.

## Clean room

The migration page and the notices were written for this phase, from `teta-wot`'s own guide and code. The vendored files' licences come from the files themselves, and the W3C licence's short notice from the [W3C's page](https://www.w3.org/copyright/software-license-2023/). Nothing was copied from `microscope` or any other codebase.
