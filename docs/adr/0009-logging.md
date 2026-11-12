# ADR-0009: Logging

- Status: Accepted
- Date: 2026-11-12
- Phase: P05

## Context

OScope's logs serve three readers (plan §5.19):

- **The Logging tab** (M2, W4): the recent records, filtered by level, time, source and text, and a live tail.
- **Diagnosis:** days of history, kept on disk and downloaded.
- **The job service** (P20): every job keeps its own log.

`teta-wot` and most Rust libraries log through `tracing`, which has five levels. The plan adds a sixth, *critical*, for failures that need attention now. The control plane can't serve a live stream well, so the tail belongs on the media plane (ADR-0006).

## Decision

1. **Everything logs through `tracing`.** The process installs one subscriber, with `oscope_api::logs::install`. It has three parts:
   - a filter, from `[logging] level` in `tracing`'s `EnvFilter` syntax, `info` by default;
   - the console, on stderr, since stdout carries the server's serving line;
   - the log store's layer.
2. **The levels** run `trace` < `debug` < `info` < `warn` < `error` < `critical`.
   - **A critical record** is an error with the field `critical = true`: `tracing::error!(critical = true, "…")`. The record drops the field.
   - **On any other level,** `critical` is an ordinary field.
3. **A record** has these fields:
   - a number, from 1, in the order records are made;
   - its time, in UTC;
   - its level, target, message and other fields, as JSON values;
   - the job it was made in, if any.
4. **The store** (`LogStore`) keeps the newest records in a ring: 50,000 by default, `[logging] ring`.
   - **Queries** filter by lowest level, a time range, a target prefix, text and job. Text means the message or a field's value, ignoring case.
   - **Pages** are newest first, at most 1,000 records each. `before` gives the next page.
   - **Queries cover the ring only,** the owner's choice. Older history is in the files.
5. **The files** are `<data_root>/logs/oscope-YYYY-MM-DD.jsonl`, one per UTC day, with one record per line as JSON.
   - **Writing:** each record is appended in a single write.
   - **Keeping:** 30 days, today's included (`[logging] keep_days`). Older files are deleted at start-up and at each new day. Other files in the folder are left alone.
6. **Jobs** run in a span named `job`, with a `job_id` field.
   - **Records made inside it,** at any depth, carry the innermost job's id.
   - **`LogStore::capture(job)`** collects a job's records as they are made, however many the ring drops.
7. **The `logs` Thing** has:
   - **the property `files`;**
   - **the endpoint `GET records?…`,** with the query's fields as parameters: a page;
   - **the endpoint `GET files/{name}`,** which downloads a listed file only;
   - **the action `emit(level, message)`.** It writes a note through `tracing`, with the target `oscope::note`, so the filter applies to it as to any record.
8. **The live tail** is the media-plane WebSocket `/api/v1/media/logs`.
   - **The client** sends a query whenever it likes.
   - **The server's replies:** `{"query": …}`, then the newest matching records up to the query's `limit`, oldest first, then `{"caught_up": n}`, then each new matching record.
   - **Other messages:** `{"skipped": n}` when the client is over 1,024 records behind, and `{"error": …}` for a wrong query.
   - **At shutdown,** it closes with 1001.
9. **No gap and no duplicate.** One lock numbers each record, stores it and sends it to the tail. The tail subscribes and takes its backlog under the same lock.

## Consequences

- **The logging path takes one lock, and one write per record.**
  - **The cost** on the owner's PC: 0.5 µs a record in memory, and 2.6 µs with the file, about 380,000 records a second. That is ample for M1.
  - **If a hot path ever logs at `trace`,** file writing can move to a thread of its own.
- **If the files fail,** for example because the disk is full, they stop with one message on stderr. The ring and the tail go on.
- **Older history can't be queried,** only downloaded. A query over the files can be added later, since they parse back into records.
- **File names follow UTC days,** not local midnight.
- **`emit` changes state,** so it will need the control lease (D32). Records don't name their account yet; authentication (ADR-0031) can add the principal.
- **The query endpoint, the downloads and the tail aren't in the OpenAPI document,** since `teta-wot` doesn't describe custom endpoints. M2 writes those calls by hand. The `files` property and the `emit` action are described.
- **Libraries that log through the `log` crate** aren't recorded, since there is no `tracing-log` bridge. None of OScope's dependencies need it so far.

## Alternatives considered

- **`tracing-appender`'s rolling files.** It keeps a number of files, not a number of days, and its clock can't be set for tests. OScope's writer is about 100 lines, and its retention is a function of a date.
- **Text files.** They are easier to read, but can't be parsed back reliably. JSON Lines are still readable, and any tool can load them.
- **Queries over the files as well as the ring.** It would cost about 125 lines more, and the owner chose the ring only.
- **A crate of its own for the logs.** Only the API and the server use the store. The job service (L3) needs only the `job` span, for which `tracing` is enough. The store can move to a lower crate if a lower layer ever needs it.
- **Server-sent events for the tail.** A WebSocket can change its query without reconnecting, and sits with the media plane's other streams.
