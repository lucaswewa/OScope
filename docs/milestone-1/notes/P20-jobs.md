# P20: Job service and the `jobs` Thing

- Status: Done
- Branch / PR: `m1/p20-jobs` / [#33](https://github.com/lucaswewa/OScope/pull/33)
- Size: L · estimated ~2,600 before it started (`oscope-jobs`: the model, the SQLite store, the hardware queue, the compute pool and the job's context ~1,000; the `jobs` Thing, the hardware lock in the device Things and the server ~380; tests ~700; `ex-jobs` with its page ~550); one pull request, at the owner's standing choice for P12–P23, rather than the spec's split of persistence into P20b · measured **2,561** hand-written lines added (code 1,379; tests 698; examples 484), and 14 removed. Docs, `Cargo.lock` and the generated snapshots aren't counted. **That is 1,661 over L's 900, and about 40 under the estimate.**

## Summary

Every long piece of work is a job: queued, followed, controlled and recorded ([spec](../phases.md#p20), D28, plan §5.11).

- **`oscope-jobs`:**
  - the job model;
  - **the hardware queue:** one job at a time, in an order that can change, holding the hardware lock;
  - **the compute pool:** workers, and a GPU memory budget;
  - **a job's context:** progress, checkpoints for cancelling and pausing, results, its own log;
  - **the history in SQLite:** jobs a stop cut short are marked interrupted.
- **The `jobs` Thing:** the queue, the kinds and the lock's holder; `list` and `job/{id}`; `submit`, `cancel`, `pause`, `resume` and `reorder`; `changed` events. Actions that start jobs answer with a `JobRef`.
- **The stage refuses manual moves** while a hardware job runs, as busy (409), naming the job. `halt` and `clear_alarm` never are.
- **`ex-jobs`:** stage tours and crunching, on a page with the queue and progress live.
- **ADR-0016** records it.

## What was built

| Where | What |
|---|---|
| `oscope-jobs` `model.rs` | `JobState`, `Resource`, `Progress`, `JobSpec`, `JobInfo`, `JobFilter`, `JobError`, and `Stop` (how a job's code ends early) |
| `oscope-jobs` `store.rs` | `JobStore`, on `rusqlite` with its bundled SQLite. One row a job (ID, kind, state, submission time, its state as JSON); numbered migrations by `PRAGMA user_version`; write-ahead logging; `interrupt` |
| `oscope-jobs` `service.rs` | `JobService`: `open` (which marks interrupted jobs), `submit`, `register` and `submit_kind`, `get`, `list`, `queue`, `subscribe`, `hardware`, `cancel`, `pause`, `resume`, `reorder`. The hardware dispatcher, the compute dispatcher, and running a job in its `job` span |
| `oscope-jobs` `ctx.rs` | `JobCtx`: `progress`, `count`, `add_result`, `checkpoint`, `cancelled`, `is_cancelled` |
| `oscope-api` `jobs.rs` | The `jobs` Thing, `JobRef`, and the forwarding of changes as events, coalesced |
| `oscope-api` `devices` | The device link finds the hardware lock; `manual()` refuses while a job holds it; the stage's moves, jogs and homing use it |
| `oscope-server` | The job service, its history in `{data_root}/index.sqlite`, and `[jobs]` (`workers`, `gpu_mib`); for `--print-td`, a history in memory |
| `examples/ex-jobs` | `demo.tour` and `demo.crunch`; the page; `--smoke` |

## How to try it

```sh
cargo run -p ex-jobs
```

Open `http://127.0.0.1:8211/`:

- Submit a few tours: they run one after the other. Move the last to the front with **First**.
- Pause one, resume it, cancel another.
- Two crunches of 1500 MiB each run one after the other: the budget is 2048.
- Hold **Jog the stage by hand** while a tour runs: refused, naming the tour.

The [README](../../../examples/ex-jobs/README.md) has the same with `curl`, and how to see an interrupted job.

## Design notes and deviations from the plan

**Worth your review:**

- **One pull request,** not the spec's suggested split into P20 and P20b (persistence), as you decided for P12–P23. It came in at about the estimate.
- **Job events are coalesced:** a job's change of state is sent at once, its progress at most every 100 ms. `teta-wot` gives each event subscriber a buffer of 5, and drops what doesn't fit. In the first test run a job's burst of progress pushed its *completed* out. Clients should still treat events as hints, and re-read a job after reconnecting. A larger buffer in `teta-wot` is a follow-up.
- **`ex-jobs` uses port 8211,** `ex-server-hello`'s: the manual-check range, 8211–8219, is used up. CLAUDE.md's port ranges may want widening for M2's examples.
- **`sqlite-wasm-rs` is pinned to 0.6.2:** `rusqlite`'s WebAssembly dependency resolved to 0.6.3, released 3 days ago. It never builds here, but the lock holds it, and the two-week rule applies.

**Other choices:**

- **Cancelling is cooperative,** at checkpoints, not by dropping the job's future: a stage move or a file write is never cut half-way. A job that never reaches a checkpoint can't be stopped, short of stopping the server.
- **What a job holds is freed before its end is announced:** the lock, a worker, its GPU memory. The first version freed the lock just after announcing the end, and the ordering test caught a client told "completed" that would still have been refused.
- **A stopping server starts no more jobs.** When the runtime stops, a running job's task is cancelled, and the service records it interrupted. At first that read as a panic, recording it failed, and the dispatcher then started the next queued job only to cut it short too. The restart test caught both.
- **The compute queue is first come first served:** a job waiting for GPU memory holds up smaller ones behind it, which keeps large jobs from starving.
- **The history's rows hold the job as JSON,** beside the columns lists filter and sort by, so the model can grow without a migration for every field. `index.sqlite` will gain the projects', acquisitions' and results' tables later, each as a migration step.
- **A job's log** is P05's: the records of its `job` span, through the `logs` Thing's `?job=`. No new route.
- **The stage refuses moves, jogs and homing,** but not `halt` or `clear_alarm`: stopping must always work. The other device Things aren't locked yet. Only the stage moves.

## Tests

| Crate | Test | What it checks |
|---|---|---|
| `oscope-jobs` | `jobs` (7) | **Hardware:** in their queue's order, never two at once, each holding the lock, reordered, the running one not movable. **Compute:** at most 2 at once, and at most one at a time when two can't share the GPU budget; more than the budget refused. **Cancelling:** a running job at its checkpoint, before its end; a queued one never starts. **Pausing:** no progress while paused, even when told to finish, then on to the end; refused for a job that can't, and resume for one that isn't paused. **Reporting:** progress, ETA, counters and results in its events; a failure and a panic as failed. **Restart:** a run on its own runtime, stopped; then a completed job kept, the running and queued ones interrupted, each saying what it was doing; lists by state, kind and limit, newest first. **Kinds:** made from parameters, with the account; bad parameters and unknown kinds refused |
| `oscope-api` | `jobs` (3) | **Over HTTP:** a job submitted, its `JobRef` followed to completion; its events, read only after it ended, still reaching *completed*; list filters; 400, 404 and refusals. **The lock:** while a hardware job runs, the stage's `move_to`, `move_by`, `jog` and `home` are refused as busy (409), naming it, and `halt` isn't; reordering; pausing a job that can't; cancelled, the stage moves again. **The Thing's description,** snapshotted |
| `oscope-api` | `devices` | The stage's description, with the lock in its text, snapshotted again |
| `ex-jobs` | `--smoke` | A tour holding the hardware, a jog refused during it, two crunches of 1500 MiB run one after the other, all listed as completed |

Both job suites passed 5 times out of 5 with their binaries pinned to two cores, as GitHub's runners have, and on Linux.

**Mutation checks** (each broke the code on purpose, then the test named was run): all nine caught.

| Broken on purpose | Caught by |
|---|---|
| The hardware lock not held | `hardware_jobs_run_one_at_a_time_in_their_queue_s_order` |
| Reordering ignored | `hardware_jobs_run_one_at_a_time_in_their_queue_s_order` |
| A checkpoint ignores cancelling | `a_cancelled_job_stops_at_its_checkpoint_and_a_queued_one_never_starts` |
| A checkpoint ignores pausing | `a_paused_job_waits_at_its_checkpoint_until_resumed` |
| No interrupted marking at start-up | `the_history_survives_a_restart_and_unfinished_jobs_are_interrupted` |
| GPU memory not held | `compute_jobs_share_the_workers_and_the_gpu_budget` |
| Workers not limited | `compute_jobs_share_the_workers_and_the_gpu_budget` |
| Manual moves not refused | `manual_moves_are_refused_while_a_hardware_job_runs` |
| Progress events not coalesced | `a_job_is_submitted_followed_and_listed`: its *completed* event is lost |

## Follow-ups and known gaps

- **`teta-wot`:** a larger, or configurable, event subscriber buffer.
- **Resuming interrupted scans:** later, with the scans (plan §5.11).
- **The control lease** (D32) will gate `submit` and the job controls, with the device Things.
- **Ports** for M2's examples: the manual-check range is used up.

## Clean room

Nothing was copied from `microscope` or any other codebase.

- **The job model** is plan §5.11's.
- **The scheduling** is first come first served, with counting semaphores for the workers and the GPU memory (Dijkstra's semaphores, through `tokio`).
- **The schema migrations** follow SQLite's documented `user_version` pragma.
