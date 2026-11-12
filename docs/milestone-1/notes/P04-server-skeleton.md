# P04: Server skeleton, `system` Thing, media plane, example helper

- Status: In review
- Branch / PR: `m1/p04-server-skeleton` / [#12](https://github.com/lucaswewa/OScope/pull/12)
- Size: M · estimated ~1,820 before it started: code ~705, tests ~380, examples ~325, glue ~50, plus a quarter. That is over L's 900, and the owner chose one pull request · measured **1,734** hand-written lines added (code 824, tests 484, examples 370, glue 56), and 29 removed. Docs and `Cargo.lock` aren't counted. **That is 1,134 over M's 600 and 834 over L's 900, and 86 under the estimate.**

## Summary

`oscope-server` now serves OScope's API from a configuration file ([spec](../phases.md#p04)). The new [ADR-0006](../../adr/0006-control-plane-and-media-plane.md) records the design.

- **One server, one origin.** It serves:
  - the control plane's Things under `/api/v1`;
  - the media plane's own routes under `/api/v1/media/`;
  - `/health`;
  - an example's page.

  The media plane and pages share the Things' authentication, CORS and shutdown.
- **The `system` Thing** reports OScope's version and Git commit, the operating system, the processor, the memory, the uptime, and the data and scratch roots with their free space.
- **The command line:** `--config`, `--port`, `--data-root`, `--print-openapi`, `--print-td` and `--version`, with a TOML configuration file.
- **`examples/common`** serves an example's page beside the API, and runs its `--smoke` checks. **`ex-server-hello`** shows the `system` Thing on a page.

## What was built

| Where | What |
|---|---|
| `oscope-api::Api`, `ApiBuilder` | **`Api::builder(&Roots)`** puts the `system` Thing in a `teta-wot` server under `/api/v1`. **`.authenticator()`**, **`.pages(folder)`**, **`.build()`**. **`Api::serve(listener, shutdown)`**, **`openapi()`**, **`thing_descriptions(base)`**, and `into_thing_server()` for tests. `ApiError` |
| `oscope-api::system` | **The `system` Thing,** with 8 read-only properties: `version`, `git_commit`, `os`, `cpu`, `memory`, `uptime` (s), `data_root`, `scratch_root`. **`VERSION`** and **`GIT_COMMIT`**, the latter from a build script |
| `oscope-api` (media) | **`GET /api/v1/media/ping`:** `pong`. As a WebSocket it sends each message back, and it closes with 1001 when the server shuts down |
| `oscope-api` (routes) | **`GET /health`:** `{"status": "ok"}`. **Pages:** each file of a folder at its own path, and `/` for `index.html` |
| `oscope-server` | **`Config`**, from TOML: `[server]`, `[storage]`, `[devices]`. **`Cli`** (clap), **`run`** and the exit codes. It is now a library and a thin `main.rs`, so that tests can call the configuration code |
| `configs/oscope.toml` | A sample configuration, every key at its default except the folders, which go in the repository's `.oscope/` |
| `examples/common` (`ex-common`) | **`Example::from_args(name, port)`:** `--port N` and `--smoke`, with roots under `.oscope/{name}/` or a temporary folder. **`Example::serve(api, page, check)`.** **`get(address, path)`**, a small HTTP client for the smoke checks |
| `examples/ex-server-hello` | The example: the API, a page (`page/index.html`, `page/app.js`) and `--smoke` |
| Docs | ADR-0006 and its index row. The server's README, the examples' READMEs and index. Plan §7, its change log, and the notes index |

**Dependencies:**

- **New packages:** 19, all at least two weeks old, under MIT, Apache-2.0 or Zlib. They are:
  - `sysinfo` 0.39.6, for the system's facts. 0.40 was under two weeks old;
  - `fs4` 1.1.0, for a folder's disk space;
  - `toml` 1.1.8 and its parts, with dependencies pinned back to versions over two weeks old. They are `serde_spanned` 1.1.2, `toml_datetime` 1.1.2, `toml_writer` 1.1.3 and `winnow` 1.0.4;
  - on Windows, `windows` 0.62.2 and `ntapi`, through `sysinfo`.
- **Already in the lock, through `teta-wot`:** `clap`, `anyhow`, `tempfile`, `futures-util` and `tokio-tungstenite`.
- **No package already in the lock changed.**

## How to try it

```sh
cargo run -p ex-server-hello                    # then open http://127.0.0.1:8211/
cargo run -p ex-server-hello -- --smoke
cargo run -p oscope-server -- --config configs/oscope.toml
curl http://127.0.0.1:8210/api/v1/system/os     # in another terminal
cargo run -p oscope-server -- --print-td
cargo test -p oscope-api -p oscope-server       # 18 tests
```

**What the `system` Thing reports** on the owner's PC (`docs/hardware.md`'s host):

| | Windows 11 | Ubuntu 24.04 (WSL) |
|---|---|---|
| `os` | `Windows 11 Pro`, kernel `26300` | `Linux (Ubuntu 24.04)`, kernel `6.18.33.2-microsoft-standard-WSL2` |
| `cpu` | i9-12900, 16 cores, 24 threads | i9-12900, 12 cores, 24 threads |
| `memory` | 68.3 GB | 33.5 GB, the WSL VM's |
| `data_root` | 562 GB available of 1.02 TB | 1.00 TB available of 1.08 TB |

The two systems count the i9-12900's cores differently. It has 8 performance cores with two threads each, and 8 efficiency cores with one. Windows counts 16 cores, and the WSL VM 12. The property reports what the system says.

## Performance

The spec sets no budget. **Shutting down with a media WebSocket open** takes milliseconds: the whole test, with a WebSocket exchange, runs in about 0.01 s. Without the close, the shutdown would wait out the 5 s grace period, and the test fails in that case (see *Tests*).

## Design notes and deviations from the plan

- **The size.**
  - **The estimate** was ~1,460 lines at P02's and P03's measured density, plus a quarter, since P03 came out a quarter over its estimate: ~1,820.
  - **The owner chose one pull request** over a split into the control plane (~1,090) and the media plane with the examples (~730).
  - **It measured 1,734.** The code was 17% over its share and the tests 27%, but the total stayed under the estimate, so adding a quarter worked.
- **The control plane is under `/api/v1`, as the media plane is.** `teta-wot` serves the OpenAPI document and its docs pages at the root (`/openapi.json`, `/docs`, `/redoc`), as FastAPI does. A test found that.
- **Page serving is in `oscope-api`, not `examples/common`.**
  - **Why:** only `oscope-api` may use axum (ADR-0005), and M2's web build can use the same code.
  - **What `examples/common` does:** it chooses the port and roots, starts the server, and runs the smoke checks.
- **`examples/common` is the crate `ex-common`.** A name starting with `oscope-` would need a layer.
- **Pages are read from disk at each request,** with `Cache-Control: no-cache`, so that an edit shows on reload. A page at a path the API uses is refused at start-up.
- **`system`'s properties are functional and read-only.**
  - **Read live:** memory, uptime and the roots' free space, each time they are read.
  - **Read at start-up:** the OS and the processor.
  - **Blocking calls** go to a blocking thread: the memory and the disk space.
- **The roots' space** is `available_bytes`, what this user can still write, and the disk's `total_bytes`, both from `fs4::statvfs`.
  - **A missing folder is an error.** The property checks that the folder exists first: on Windows, `statvfs` answers for a folder that doesn't exist, which a test found.
- **The Git commit** comes from `git rev-parse --short=12 HEAD` in `oscope-api`'s build script, or is `unknown` without Git.
  - **When it updates:** the crate rebuilds when HEAD moves, after a commit, checkout or reset.
  - **What it doesn't show:** uncommitted changes. Watching for them would rebuild on every edit.
- **The configuration.**
  - **Unknown keys are refused,** with their line.
  - **Relative folders are relative to the file,** so that it means the same from anywhere. `--data-root` is relative to where the program runs.
  - **The defaults** are `.oscope/data` and `.oscope/scratch` (plan §13, point 10), and `127.0.0.1:8210`.
  - **`[devices]`** is kept as a table, but not read.
- **The serve loop.**
  - **The folders:** it creates the roots, then prints `oscope-server 0.1.0 serving on http://…/` on stdout. P19's ready line can build on that.
  - **`--port 0`** lets the system choose the port, which the line then gives.
  - **The printing flags:** `--print-openapi` and `--print-td` print without creating anything. `--print-td` prints one JSON object, keyed by Thing name, with the configured address as its base.
- **Exit codes:** 2 for a wrong command line or configuration, and 1 for a failure to serve.
- **`--version`** is unchanged. The Git commit is the `system` Thing's.
- **`/health`** says only that the server answers. The instrument's health will be the `status` Thing's.
- **No logs are shown yet.** `teta-wot` logs through `tracing`, and P05 adds the subscriber.

## Tests

- **`oscope-api/tests/api.rs`,** 7 tests:
  - `/health`;
  - **every `system` property**, with real values:
    - the version and commit;
    - the OS's architecture;
    - cores ≤ threads;
    - available ≤ total memory and space;
    - the roots' absolute paths;
    - the uptime growing (waited for, not slept);
    - the TD: 8 read-only properties, and the uptime's unit;
  - a missing root is an error that names the folder, not a panic;
  - **with a test authenticator:**
    - the media `ping`, a property and `/health` answer 401;
    - the TD and the OpenAPI document stay public;
    - with the token, `ping` answers `pong`;
  - **shutting down with a media WebSocket open,** on a real listener:
    - text and binary messages come back;
    - after the shutdown starts, the socket closes with 1001;
    - `serve` returns, both within 3 s, well inside the 5 s grace period;
  - **pages:**
    - `/`, `/index.html` and `/js/app.js`, with their content types;
    - an edit shows on the next request;
    - the API still answers, and a missing file is 404;
  - pages that would hide the API are refused.
- **`oscope-server/tests/config.rs`,** 6 tests:
  - the defaults, and an empty file;
  - every key, with folders relative to the file;
  - wrong files: an unknown key, a wrong type, a port out of range, an unknown table. Each is refused with the file, the line and the reason, and a missing file with its path;
  - `--port` and `--data-root` over the file;
  - the command line's refusals;
  - the sample `configs/oscope.toml` loads.
- **`oscope-server/tests/cli.rs`,** 5 tests (2 from P01, one of them rewritten), which run the program:
  - `--version`;
  - wrong arguments exit with 2 and point to `--help`;
  - `--print-openapi` and `--print-td`, which create no folder;
  - a wrong configuration file exits with 2, naming the file and the line;
  - **it serves:** `--port 0`, the serving line, `/health`, the data root's path, and both roots created. The test then stops the process it started, by its handle.
- **`ex-common`'s documentation example** compiles.
- **Mutation checks,** each restored afterwards:

  | Broken on purpose | Failed |
  |---|---|
  | A media WebSocket ignoring the shutdown | `shutting_down_closes_an_open_media_websocket_at_once`: not closed within 3 s |
  | The authenticator not passed to `teta-wot` | `the_media_plane_is_behind_the_authenticator_with_the_things` |
  | A missing root not checked | `a_root_that_is_missing_is_an_error_not_a_panic`, on Windows. On Linux, `statvfs` itself fails for a missing folder |
  | Pages allowed to hide the API | `pages_that_would_hide_the_api_are_refused` |
  | A file's folders not made relative to it | `a_file_sets_every_key_and_its_folders_are_relative_to_it` |
  | `--port` not applied | `the_command_line_overrides_the_file` |

- **Windows** (the working tree at `dbe26e4`): all 8 CI steps passed, with 52 suites and 65 tests. All three examples' `--smoke` passed.
- **Ubuntu 24.04.5** (WSL, a fresh clone of `dbe26e4` in the Linux file system, 118 files, no CR in any): all 8 steps passed, with the same 52 suites and 65 tests.
- **GitHub CI:** to come.

## Follow-ups and known gaps

- **Browsers can't yet authenticate a media WebSocket.** Merged routes are checked by header only (ADR-0006). Authentication (ADR-0031) chooses among three ways:
  - an upstream `teta-wot` change;
  - a Thing endpoint;
  - OScope's authenticator and the handler.
- **With an authenticator, `/health` and pages need credentials.** OScope's authenticator can let them through by path.
- **The `system` Thing** doesn't report the GPU yet (P07). §5.4's disks and licences come with the `status` Thing and the release work.
- **Logging** (P05): the serve loop prints only its serving line.
- **`teta-wot`'s own lock** still has tokio 1.53.1 (P03's notes).
- **U06's 24-hour soak** ends about 2026-11-12 16:45 PST, followed by P02b's disk-write test. Their results go into the U06 notes and P02b's notes and `hardware.md`, in this pull request if it is still open.

## Clean room

Written for this phase, from plan §5.4 and `teta-wot`'s guide. Nothing was copied from `microscope` or any other codebase. No algorithm needs a citation.
