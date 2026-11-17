# P53: Performance budgets and nightly soak

- Status: In review
- Branch / PR: `m1/p53-perf` / [#69](https://github.com/lucaswewa/OScope/pull/69)
- Size: M · estimated ~1,050 before it started (`cargo xtask` ~70; `perf`'s benchmarks ~170 and report ~120; the benchmarks' `perf:` lines ~40; the soak ~260 and its detector ~50; tests ~200; the workflow ~35; glue ~20; a tenth on top; plan's change log); one pull request, at the owner's standing choice · measured SIZE

## Summary

Plan §5.20's budgets measured and reported, and a soak that looks for leaks and dropped frames ([spec](../phases.md#p53); plan §5.20, §5.21).

- **`cargo xtask perf`** runs, in release builds, the benchmark each budget's phase left. It writes `target/perf/report.html` and `report.json`, with each budget, what was measured, and its verdict. It fails on a missed budget.
- **`cargo xtask soak`** runs the release server for as long as asked, with:
  - the simulator and its scaled sensor;
  - a preview client;
  - scans and stitches, one after another.

  It samples the server's memory and handles, and fails on a leak or on dropped previews.
- **The leak detector:** Theil–Sen's slope, once warm, against a floor and a fraction of the level.
- **The nightly workflow,** `soak.yml`, four hours on Ubuntu. Its schedule is off, to save CI minutes, until the owner turns it on.
- **What the soak found, and what was fixed:**
  - the job service kept every finished job in memory, for ever;
  - on Windows, deleting a project right after its stitch could fail, once in a while, because another program held one of its new files.

## What was built

- **`xtask`:**
  - `perf`: §5.20's budgets, the benchmarks, how their `perf:` lines are read, the verdicts, and the report as HTML and JSON;
  - a binary, `cargo xtask perf [--quick] [--cuda]` and `cargo xtask soak [--minutes N]`, aliased in `.cargo/config.toml`;
  - its README, which is this phase's example: how to make the report, and what it shows.
- **The benchmarks** print what they measure as `perf:` lines: `the_preview_budget`, `ex-demosaic` (whose `--runs` now counts with `--smoke`), `ex-record`, `ex-sim-bench`, `ex-register`, `ex-stitch` and `ex-deep-zoom`.
- **`oscope-server`'s `soak` test,** ignored but for `cargo xtask soak`:
  - it spawns the server's release build with its own configuration;
  - a preview client acknowledges each frame;
  - the instrument is homed, focused over tissue and exposed, then scans 800 × 600 µm and stitches it, project after project;
  - the server's memory and handles are sampled, on Linux from `/proc`, on Windows through PowerShell;
  - its report is `target/soak/soak.json`.
- **The leak detector,** in the same file, with its own tests, which CI runs.
- **`.github/workflows/soak.yml`:** started by hand, its schedule commented out.
- **Fixes the soak found:**
  - `oscope-jobs`: finished jobs leave memory once `JobsConfig::kept` has passed, 5 minutes by default; their records answer for them;
  - `oscope-store`: deleting a project or a result tries again, on Windows, for up to 2 s while access is denied.

## How to try it

```sh
cargo xtask perf --quick --cuda      # the report: target/perf/report.html
cargo xtask soak --minutes 15        # the soak: target/soak/soak.json
```

The [README](../../../xtask/README.md) says what each runs. On the owner's PC:

PERF

## Performance

PERF_NOTES

## Design notes and deviations from the plan

- **Where the budgets are measured:**
  - §5.20 says "the nightly run gates on them from P53". But they are budgets for the owner's laptop and its GPU, and CI's runners have neither.
  - So `cargo xtask perf` measures them on the owner's machine and fails on a missed one. The nightly gates on what a CI runner can judge: leaks, and dropped previews.
- **The benchmarks are the phases' own,** run as they were, so that the report measures what each phase measured.
- **The recording's budget** is measured by `ex-record --test-sensor`: the recorder alone, at the design rate, as P29 did. It rolls within 20 GB of scratch, deleted after, so the 10-minute run fits this PC's one disk.
- **The soak is a test of the server's,** so that it runs the server's own release build (`CARGO_BIN_EXE`), as users do. `cargo xtask soak` passes it its length and where its report goes.
- **The warm-up:**
  - It is the soak's first quarter, and at least 5 minutes.
  - Caches fill first: the server's memory rose from 100 MB to some 900 MB in the first 2 to 3 minutes, then held.
  - A soak shorter than the warm-up is too short to judge leaks, and says so.
- **Theil–Sen's slope** (Theil 1950; Sen 1968) is the median of the slopes between every two samples, so a spike, or a cache filled and emptied, barely moves it. Ordinary least squares would follow them.
- **The thresholds:**
  - memory leaks if it grows by more than 32 MB and 5 % once warm;
  - handles leak if they grow by more than 32 and 10 %;
  - previews: at most 1 % dropped.
  - Each was chosen so that the soaks here pass, and the injected leaks fail by far.
- **Memory:** on Linux, the resident memory; on Windows, the private bytes, which a leak raises even when Windows trims the working set.

### What the soak found

FOUND

### What the mutants' reading found

MUTANTS

### Two cores

TWOCORE

## Tests

| Suite | Tests | What they show |
|---|---|---|
| `xtask` lib `perf` (new) | 3 | `perf:` lines read, the last of a key counting; each verdict at its edges; passing; each benchmark's arguments, quick or whole, with CUDA where it has GPU measures; the report's verdicts, escaping and runs |
| `oscope-server` `soak` (new) | 2, and the soak | **The detector:** steady series are no leaks, whether noisy, warming up, a sawtooth, spiky, past the floor but not the fraction, or the other way; too short is no verdict; slow growth is a leak, of memory and of handles; the slope of a line. **The soak,** ignored but for `cargo xtask soak` |
| `oscope-jobs` lib `service` (new) | 1 | Finished jobs leave memory once kept long enough, and their records answer; kept longer, they stay |
| `oscope-store` `projects` | 9 | On Windows, a project is deleted though another program holds a file of it for a moment |
| The benchmarks' `--smoke` | — | Unchanged, with their `perf:` lines |

REPLAYS

## Follow-ups and known gaps

- **The nightly soak's schedule** is off until the owner turns it on.
- **Before turning it on, run it once by hand,** for its four hours. At the pace measured here, it may fail on memory's slow growth once warm: some 10 KB a scan and stitch on Windows, and 7 KB on Linux. Its source isn't found yet; the caches checked are bounded.
- **The full report,** `cargo xtask perf --cuda` with the 10-minute recording, wasn't run in this phase: the quick one was. The recording's 10 minutes were measured in P29.
- **Logs and the job history on disk** grow with use, as they should. Logs are pruned by day (P05); the job history isn't pruned yet.

## Clean room

Nothing was copied from `microscope` or any other codebase. The leak detector is Theil–Sen's estimator: H. Theil, 1950, "A rank-invariant method of linear and polynomial regression analysis", Indagationes Mathematicae 12; P. K. Sen, 1968, "Estimates of the regression coefficient based on Kendall's tau", JASA 63(324):1379–1389.
