# P05: Logging and the `logs` Thing

- Status: In review
- Branch / PR: `m1/p05-logging` / [#13](https://github.com/lucaswewa/OScope/pull/13)
- Size: M · estimated ~1,760 before it started: code ~740, tests ~380, examples ~266, glue ~25, plus a quarter. That is over L's 900, and the owner chose one pull request · measured **1,928** hand-written lines added (code 936, tests 551, examples 426, glue 15), and 37 removed. Docs and `Cargo.lock` aren't counted. **That is 1,328 over M's 600 and 1,028 over L's 900, and 168 (10%) over the estimate.**

## Summary

OScope now keeps logs for the Logging tab and for diagnosis ([spec](../phases.md#p05)). The new [ADR-0009](../../adr/0009-logging.md) records the design.

- **Everything logs through `tracing`.** The server records `info` and above by default. Records go to:
  - the console;
  - a ring of the newest 50,000 records;
  - daily JSON Lines files, kept 30 days, in `<data_root>/logs`.
- **The levels** run from `trace` to `critical`. Critical is an error marked `critical = true`.
- **A job's records** carry its id when it runs in a `job` span. The store can capture them all, for P20.
- **The `logs` Thing:**
  - queries the ring by level, time, target, text and job, a page at a time;
  - lists and downloads the daily files;
  - writes a note.
- **The live tail** is a media-plane WebSocket, at `/api/v1/media/logs`.
- **`ex-logs`** shows the tail on a page, with filters, a button for each level, and the files.

## What was built

| Where | What |
|---|---|
| `oscope-api::logs` (`record`) | **`Level`** (`trace`…`critical`), **`Record`**, **`Query`**, with `matches` and `page_size`, and **`Page`** |
| `oscope-api::logs` (`layer`) | **`LogLayer`,** a `tracing_subscriber` layer. It handles the fields, `critical`, and the innermost `job` span's id (**`JOB_SPAN`**) |
| `oscope-api::logs` (`store`) | **`LogStore`**, with **`LogConfig`** (the ring, the folder and the days to keep). Its methods: `query`, `follow` for the tail, `capture`, `files` and `file`. **`Capture`** |
| `oscope-api::logs` (`files`) | **The daily files:** one append per record. **`list`**, and **`prune(folder, today, keep_days)`**, which runs at start-up and at each new day. **`LogFile`** |
| `oscope-api::logs` | **`install(store, filter)`:** the global subscriber, with an `EnvFilter`, the console on stderr, and the store. **`LogError`** |
| `oscope-api::logs::Logs` | **The `logs` Thing:** `files`, `emit(level, message)`, and the endpoints `records?…` and `files/{name}` |
| `oscope-api` (media) | **`GET /api/v1/media/logs`,** the live tail, as ADR-0009 describes. `ping` now shares the shutdown close with it |
| `oscope-api::ApiBuilder` | **`.logs(store)`.** Every server now has the `logs` Thing, over a store of its own unless it is given one |
| `oscope-server` | **`[logging]`:** `level`, `ring` and `keep_days`. Serving starts the logs, in `<data_root>/logs`. A wrong level exits with 2 |
| `configs/oscope.toml` | The `[logging]` table |
| `examples/common` | **Logs for every example:** `info`, plus OScope's own targets at every level (`LOG_LEVEL`). Also **`post`**, beside `get` |
| `examples/ex-logs` | The example: a page, a record every 2 s with a job every fifth, and `--smoke` |
| Docs | ADR-0009 and its index row. The READMEs, plan §7, the change log and the notes index |

**Dependencies:** `tracing` and `tracing-subscriber` are already in the lock through `teta-wot`, used with the same features, and `chrono` was there already. **The lock gained no third-party package.**

## How to try it

```sh
cargo run -p ex-logs                      # then open http://127.0.0.1:8213/
cargo run -p ex-logs -- --smoke
cargo run -p oscope-server                # in another terminal:
curl "http://127.0.0.1:8210/api/v1/logs/records?level=info&limit=5"
cargo test -p oscope-api --test logs      # 8 tests
```

**On the page:**
- **The tail** fills with the server's records, and a tick every 2 s. Every tenth second, the tick is a warning inside a job.
- **The buttons** each write a note at their level, which appears in the tail.
- **The filter** clears the table, then shows only the records that match.
- **Pause** freezes the table, and resuming reloads the newest records.
- **The daily file** is listed with a download link.

## Performance

The spec sets no budget. **A record costs 0.54 µs in memory only, and 2.61 µs with its file**, about 380,000 records a second. This was measured on the owner's PC, with a release build, over 200,000 records, each with two fields. A test that was then deleted took the measurement.

## Design notes and deviations from the plan

- **The size.**
  - **The estimate** was ~1,410 at the measured density, plus a quarter: ~1,760.
  - **It measured 1,928,** 10% over. The tests (+45%) and the example (+60%) grew most.
  - **Why the tests grew:** they cover the tail's backlog, its live records, a follower that falls behind, a change of query, and the close.
  - **Why the example grew:** its page has a filter, pause, buttons and the files.
- **The logging code is in `oscope-api`,** not a crate of its own. Only the API and the server use the store, and P20's job service needs only the `job` span, which `tracing` provides (ADR-0009).
- **Queries cover the ring only,** the owner's choice. The files hold 30 days to download.
- **The tail's protocol has two marks the spec didn't ask for:** `{"query": …}` before a backlog, and `{"caught_up": n}` after it.
  - **The first** lets a client that changes its filter drop what it shows: every record after it matches the new filter.
  - **The second** says the backlog is over. The page and the tests both rely on these marks.
- **`emit` writes through `tracing`,** so the console, the files and the tail all get the note, and the filter applies to it.
  - **In the server,** at `info`, a `trace` or `debug` note isn't kept.
  - **In the examples,** OScope's own targets are kept at every level, so that every button shows.
- **Files are named by UTC day,** and each record is appended in one write, so a file is whole up to its last record.
  - **On Windows,** a folder entry's size lags while its file is open, so the listing reads each file's own size. The manual check found that.
- **The ring holds `Arc`s,** shared with the tail and the captures. A query copies only the page it returns.
- **The server records from when it starts serving.** The print flags don't start the logs.
- **The console is stderr,** so that stdout keeps only the serving line, which P19's ready line will build on.

## Tests

- **`oscope-api/tests/logs.rs`,** 8 tests:
  - **critical:** an error with `critical = true` is `critical`, and drops the field. A warning with it stays a warning, and an error with `critical = false` stays an error. Also the levels' order and names;
  - **fields, target and jobs:**
    - numbers, booleans, floats and strings keep their types;
    - records in a job, and in a span inside it, carry its id;
    - another job's records carry theirs, and others carry none;
    - the numbers run from 1, and times don't go back;
    - a capture holds exactly the job's records;
  - **a capture survives the ring:** a ring of 2 with 20 records, half in the job, and the capture holds all 10;
  - **queries** over 300 records, at five levels and two targets:
    - the default page, and `next`;
    - paging by 64 visits all 300 exactly once, newest first;
    - level, target, text in a field's value and in the message, a time window, a combination, and an unknown job. Each count is checked against filtering all the records directly;
    - the page size's limits;
  - **the ring** keeps the newest 50 of 120;
  - **daily files and retention:**
    - files from 40, 31 and 30 days ago are deleted at start-up;
    - those from 29 days ago and yesterday stay, and other files are left alone;
    - three records come back from today's file, line by line, equal to the store's;
    - only listed names can be fetched;
    - `prune` two days on deletes the file from 29 days ago, and with 1 day it keeps only today's;
  - **the `logs` Thing over HTTP:**
    - `emit` writes a critical note, which the query finds;
    - paging with `before`;
    - wrong parameters answer 400;
    - the file list, and the download, with its type, name and content;
    - unknown names and `..` answer 404;
    - the TD lists the files and the action;
  - **the live tail** on a real server:
    - the backlog, oldest first, then `caught_up`;
    - a query with its `query` mark and an empty backlog;
    - 30 live records, of which exactly the matching 10 arrive, in order;
    - a flood of 2,000: the follower is told how many it skipped, and what arrives stays in order up to the last;
    - a wrong query's error;
    - the close with 1001 at shutdown.
- **`oscope-server`:**
  - the `[logging]` defaults and values, and the sample;
  - a wrong level exits with 2;
  - the served server's own records come back through `records`, and are in today's file.
- **Mutation checks,** each restored afterwards:

  | Broken on purpose | Failed |
  |---|---|
  | Critical not mapped | `critical_is_an_error_marked_critical` |
  | One day too many kept | `daily_files_hold_json_lines_and_are_kept_30_days` |
  | The tail not filtering live records | `the_live_tail_sends_the_backlog_then_new_records_in_order` |
  | A follower that falls behind not told | the same, which waited for the skipped count |
  | A job span's id not looked up | `records_carry_their_fields_target_and_job` |
  | A page's `next` one too high | `queries_filter_by_level_time_target_text_and_job_newest_first_in_pages` |
  | The ring one record too long | `the_ring_keeps_the_newest_records` |
  | A capture taking every job's records | `a_capture_keeps_a_jobs_records_however_small_the_ring` |
  | Downloads of any name | `daily_files_hold_json_lines_and_are_kept_30_days` |
  | The server not recording into its store | `it_serves_from_its_configuration_and_creates_its_folders` |

- **The page** was driven in Node against a live `ex-logs`, with a stand-in DOM:
  - the backlog and `caught_up`;
  - the six buttons, each record coming back over the tail;
  - a filter that left only `error` and `critical`;
  - the file list.
- **Windows and Ubuntu:** see *CI* below.

### CI

- **Windows** (the working tree at `9ac299e`): all 8 CI steps passed, with 54 suites and 74 tests. All four examples' `--smoke` passed.
- **Ubuntu 24.04.5** (WSL, a fresh clone of `9ac299e` in the Linux file system, 136 files, no CR in any): all 8 steps passed, with the same 54 suites and 74 tests.
- **GitHub CI:** to come.

## Follow-ups and known gaps

- **P20** runs each job in a `job` span, and keeps its `capture`.
- **Records don't name their account,** and `emit` will need the control lease (ADR-0031).
- **Older history** can be downloaded, but not queried.
- **The tail and the query endpoint aren't in the OpenAPI document** (ADR-0009).
- **The media WebSockets' authentication in browsers** is still open (ADR-0006).
- **U06's soak** ends about 16:45 PST today, and P02b's disk test follows. Their results go into this pull request if it is still open.

## Clean room

Written for this phase, from plan §5.19 and the `tracing` and `tracing-subscriber` documentation. Nothing was copied from `microscope` or any other codebase. No algorithm needs a citation.
