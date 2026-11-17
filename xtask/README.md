# xtask

The workspace's own tasks:

- **the layering rules** of [ADR-0005](../docs/adr/0005-layered-crates-and-their-dependency-rules.md), checked by its tests;
- **`cargo xtask perf`,** plan §5.20's performance budgets, measured and reported (P53);
- **`cargo xtask soak`,** the soak, which looks for leaks and dropped frames (P53).

`cargo xtask` is an alias, in [`.cargo/config.toml`](../.cargo/config.toml), for `cargo run --package xtask --`.

## The performance report

```sh
cargo xtask perf --quick --cuda      # about half an hour, builds included
cargo xtask perf --cuda              # the budgets' own lengths: the recording runs for 10 minutes
```

It runs, in release builds, the benchmark each budget's phase left:

| Budget | Benchmark |
|---|---|
| Preview (P17) | `oscope-acquire`'s `the_preview_budget` |
| Demosaic (P08, P08c) | `ex-demosaic`, on 8 threads |
| Scratch recording (P29) | `ex-record --test-sensor`, rolling within 20 GB of scratch, deleted after |
| Simulated camera (P15) | `ex-sim-bench` |
| Phase correlation (P31) | `ex-register --bench` |
| Post-stitching (P39) | `ex-stitch --bench` |
| Deep zoom (P40) | `ex-deep-zoom --bench` |

- **What it reads:** each benchmark prints what it measured as `perf: {key} {value}` lines.
- **What it writes:**
  - `target/perf/report.html`: each budget, what was measured, and its verdict: met, missed, or not measured, as the GPU's are without `--cuda`;
  - `target/perf/report.json`, the same as data.
- **`--quick`** makes the long ones short, such as 30 s of recording for 10 minutes. The report says so.
- **It fails** if a budget is missed or a benchmark fails.
- **Where:** §5.20's budgets are the owner's laptop's (an RTX 3070 Ti); the owner's PC stands in for it.

## The soak

```sh
cargo xtask soak --minutes 60
```

It runs `oscope-server`'s `soak` test, ignored otherwise, against the server's release build:

- **The setup:**
  - the simulator, with its scaled sensor;
  - a preview client, acknowledging each frame;
  - scans of 800 × 600 µm over tissue and their stitches, one after another, each project deleted after.
- **Sampling:** the server's memory and handles, every 2 s, or 360 times in all for longer soaks. On Linux these are its resident memory and open files; on Windows, its private bytes and handles.
- **It fails if:**
  - memory grows by more than 32 MB and 5 % once warm;
  - handles grow by more than 32 and 10 % once warm;
  - more than 1 % of the previews are dropped.
- **The warm-up** is the first quarter, and at least 5 minutes, while caches fill. A shorter soak is too short to judge leaks, and says so.
- **Growth** is Theil–Sen's slope of the series: the median of the slopes between every two samples. Spikes and caches' sawteeth barely move it.
- **Its report:** `target/soak/soak.json` holds each sample, the rates once warm, the scans, the previews, and the verdicts.

**The nightly soak,** [`.github/workflows/soak.yml`](../.github/workflows/soak.yml), runs it for four hours on Ubuntu, on the CPU. Its schedule is off, to save CI minutes, until the owner turns it on; until then it runs when started by hand.
