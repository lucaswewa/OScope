# ADR-0016: The job service

- Status: Accepted
- Date: 2026-11-13
- Phase: P20

## Context

- **D28 and plan §5.11:** every long piece of work is a job.
  - Hardware jobs run one at a time, from a queue that can be reordered, and hold the hardware lock, so manual moves are refused meanwhile.
  - Compute jobs run on a pool of workers that shares the GPU.
  - Every job has progress, a log and cancel, and its history is kept in SQLite.
  - Jobs a restart cut short are marked interrupted.
- **Later phases submit jobs:** autofocus, scans, pipelines, analyses. They need one way to describe a job, to run its code, and to tell clients where to follow it.
- **`teta-wot` actions answer 201 at once,** with an invocation the client follows (ADR-0034). **Its events give each subscriber a buffer of 5,** and drop what doesn't fit.
- **P05's logs** already give each record the ID of the `job` span it was made in (ADR-0009).

## Decision

**1. A job** has:
- an ID (a version 7 UUID), a kind, a title and the account that submitted it;
- its parameters, as JSON;
- its resource class, hardware or compute;
- whether it can pause, and the GPU memory it holds, in MiB;
- a state: queued, running, paused, completed, failed, cancelled or interrupted;
- progress: a fraction, a stage text, counters, and an ETA from the fraction and the time so far;
- references to its results, and why it failed;
- when it was submitted, started and finished;
- its place in its queue while it waits.

**2. Scheduling,** in `oscope-jobs`:
- **Hardware jobs** run one at a time, in their queue's order, which `reorder` changes. The running job holds the hardware lock.
- **Compute jobs** run first come first served, each when a worker (2 by default) and its GPU memory are free. The GPU budget is configured, `[jobs] gpu_mib`; without one, GPU memory isn't counted. A job needing more than the whole budget is refused.
- **What a job holds** (the lock, a worker, GPU memory) is freed before its end is announced, so a client told it has ended can act at once.

**3. A job's code** is given a `JobCtx`:
- `progress`, `count` and `add_result` to report;
- `checkpoint().await?` where it may stop or pause. It returns `Cancelled` once the job is cancelled, and waits there, *paused*, while a pause is asked for;
- `cancelled()` for long waits, in a `select!`.

It returns `Ok`, `Stop::Cancelled` or `Stop::Failed`. A panic fails the job. It runs in a `job` span with its ID, so its records are its log: the `logs` Thing's records `?job={id}`.

**4. Cancelling and pausing:**
- **A queued job** is cancelled at once, and never starts.
- **A running job** stops at its next checkpoint.
- **Only a running job that can pause** pauses. `resume` lets it go on.

**5. The history,** in SQLite: `{data_root}/index.sqlite`, the index plan §5.12 has, which later phases add tables to.
- **One row a job:** its ID, kind, state and submission time, which lists filter and sort by, and its latest state as JSON.
- **Migrations** are numbered steps, counted by `PRAGMA user_version`; a released step is never changed.
- **Write-ahead logging,** with `synchronous = NORMAL`.
- **When it is written:** a change of state at once; progress at most every 2 s.
- **Interrupted jobs:**
  - **At start-up,** every job that hadn't ended is marked interrupted, with what it was doing.
  - **As the server stops,** a running job cut short is recorded interrupted, and no queued job starts. The next start marks the queued ones.

**6. The `jobs` Thing:**
- **Properties:** `queue` (the jobs not yet ended), `kinds`, and `hardware` (the lock's holder).
- **GET endpoints:** `list?state=…&kind=…&limit=…`, the newest first, and `job/{id}`.
- **Actions:** `submit` (a kind and its parameters), `cancel`, `pause`, `resume` and `reorder`. Their refusals carry ADR-0034's kind and status: unknown (404), conflict (409), refused (400).
- **The `changed` event,** coalesced: each change of state at once, and a job's progress at most every 100 ms, the latest. A job's own burst then fits `teta-wot`'s buffer of 5. Clients still treat events as hints, and re-read `job/{id}` or `queue` after reconnecting.

**7. The `JobRef` pattern:** an action that starts a job answers at once with `{"id": …, "href": "/api/v1/jobs/job/{id}"}`, where to follow it. `submit` does; later phases' actions that start jobs will too.

**8. Kinds:** `JobService::register(kind, factory)`, where the factory makes a job from its parameters. Later phases register theirs. `submit_kind` fills in the kind, the parameters and the account.

**9. The device Things refuse manual moves** while a hardware job holds the lock: the stage's `move_to`, `move_by`, `jog` and `home` answer busy (409), naming the job. `halt` and `clear_alarm` never do. Jobs drive the devices directly, through `Devices`, not through the Things.

## Consequences

- **Gains:**
  - One model for every long piece of work, from P20 on: queued, followed, controlled and recorded the same way.
  - The instrument can't be moved by hand under a running scan.
  - The GPU's memory is shared without jobs running out of it.
  - The history survives restarts, and says what a stop cut short.
- **Costs:**
  - **Cancelling and pausing are cooperative:** code that never reaches a checkpoint can't be stopped, short of stopping the server.
  - **The compute queue is first come first served:** a job waiting for GPU memory holds up smaller ones behind it.
  - **Events can still be missed** by a subscriber that falls 5 behind across many jobs: hence hints, and re-reading.
  - **Interrupted jobs aren't resumed;** resuming scans is later.
- **To watch:**
  - `teta-wot`'s per-subscriber buffer: a larger or configurable one, upstream, would make events more reliable.
  - `index.sqlite` gains the projects', acquisitions' and results' tables (P28 onwards), each as a new migration step.

## Alternatives considered

- **Jobs as `teta-wot` action invocations:** invocations expire, aren't kept across restarts, and have no queue, resources, progress fields or pausing.
- **A table with a column for each field:** the JSON beside a few indexed columns lets the model grow without a migration for each field.
- **Preemptive cancelling** (dropping the job's future): a stage move or a file write could be cut half-way. Checkpoints put the stops where the job's code says it is safe.
- **Sending every change as an event:** `teta-wot`'s buffer of 5 drops a job's end behind its progress, as the first test run showed.
- **The lock as a mutex the device Things wait on:** a manual move would hang until the scan ended; refusing it at once, as busy, says why.
