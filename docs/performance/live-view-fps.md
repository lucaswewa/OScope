# The live view at 14.5 fps

- **Status:** *In review*, in #113, on the branch `perf/live-view-fps` with [the baseline](live-view-baseline.md) and [the GPU simulator](gpu-simulator.md), rebased on `main` at W6.4.
- **The goal:** the live view at 12 frames a second or more, at 1000 × 667 of the 5472 × 3648 sensor, built with CUDA. The owner had found the GPU and its memory only moderately busy at [the GPU simulator](gpu-simulator.md)'s 7.2 fps.
- **The result:** 14.5 fps, still and jogging at up to 2 mm/s, with a frame on the page 29 ms after its exposure with the stage still, and 42 ms jogging at 500 µm/s.

## Why it was 7.2 fps

**The camera's mode, not the chain.** The simulated camera starts in mode `full`, 16 bits, whose rate is the MIChrome 20's measured 7.24 fps ([`SimCameraConfig::capabilities`](../../crates/oscope-sim/src/camera.rs)). In [the GPU simulator's runs](data/gpu-sim-gpu1.json) every step ran exactly 7.24 times a second, and the camera slept between frames:

- **still:** the camera's render 26 ms and the preview 37 ms of each 138 ms frame;
- **jogging:** 71 ms and 22 ms.

**The faster modes:**

- **`design`:** full resolution in 16 bits at 14.5 fps, P29's design rate. It sends the same 40 MB frames as `full`, so the comparison with the baseline stays like for like.
- **`full` in 8 bits:** 14.49 fps, the MIChrome 20's measured rate.

**In mode `design`, the code as it was** already ran at 14.5 fps, still and jogging ([`fps-before.json`](data/fps-before.json)). But a jogging frame then took 43 ms of the camera's time, 24 ms of it in `camera.motion`. That left little headroom.

## What changed

1. **The example streams in mode `design`.** It sets the mode through `camera/set_mode` before it starts streaming. Its README says how to go back to `full`.
2. **The camera finds the stage's positions without the stage's lock.**
   - **Before:** for each row of a frame, `camera.motion` asked the stage where it was during the row's exposure:
     - with the stage still, 3 times;
     - jogging at 500 µm/s with a 15 ms exposure, about 35 times, a pixel's motion apart.
   
     Each call took the stage's mutex. That is 11,000 calls a still frame, and 128,000 jogging, from the camera's 15 threads at once, all queueing for one lock.
   - **Now:** the camera takes the stage's **true path** once a frame, `SimStage::true_path(start)`. It's a copy of the stage's phases from the frame's start on: its motions, backlash, lost steps and settling. Its threads then find positions on that copy, with no lock.
   - **The same positions:** a position on the path is what `true_position` gives, to the bit. Both use the same function on the same phase. A test checks it through a move, a jog, its stop and a reversal.
3. **The benchmark records the camera's settings** (`camera` in its JSON), since the mode now sets the most frames a second.

## Results

On the owner's laptop, built with `--features cuda`, at 1000 × 667, in mode `design`. Medians of 30 s windows, two runs each ([`fps-path1.json`](data/fps-path1.json), [`fps-path2.json`](data/fps-path2.json)), against the code before this change in the same mode ([`fps-before.json`](data/fps-before.json)), and against [the GPU simulator](gpu-simulator.md) in mode `full`:

| | `full`, before | `design`, before | `design`, now |
|---|---:|---:|---:|
| **Still:** page fps | 7.23 | 14.5 | 14.5 |
| end to end | 69–71 ms | 36 ms | 29–30 ms |
| end to end, p95 | 75 ms | 66 ms | 62 ms |
| the camera's render | 26 ms | 14 ms | 7.7 ms |
| `camera.motion` | 8.0 ms | 6.0 ms | 0.3 ms |
| **Jogging at 500 µm/s:** page fps | 7.24 | 14.5 | 14.5 |
| end to end | 92–97 ms | 63 ms | 42 ms |
| end to end, p95 | 125 ms | 69 ms | 47–48 ms |
| the camera's render | 62–71 ms | 43 ms | 20 ms |
| `camera.motion` | 35–45 ms | 24 ms | 0.6–0.7 ms |

**Faster jogs,** one run each, every frame of the window jogging ([`fps-jog1000.json`](data/fps-jog1000.json), [`fps-jog2000.json`](data/fps-jog2000.json)):

| Jogging at | page fps | end to end | p95 | the camera's render |
|---|---:|---:|---:|---:|
| 500 µm/s | 14.5 | 42 ms | 48 ms | 20 ms |
| 1 mm/s | 14.5 | 55 ms | 66 ms | 32 ms |
| 2 mm/s | 14.5 | 56 ms | 81 ms | 32 ms |

From 1 mm/s, the 15 ms exposure crosses 62 pixels or more, so each row is exposed at the most places, 64. The exposure then takes 30 ms on the GPU. The tiles keep up: no frame was coarse once the first field of view was made.

**The CPU build** in mode `design` ([`fps-cpu1.json`](data/fps-cpu1.json), [`fps-cpu2.json`](data/fps-cpu2.json)) is still bound by the camera's rendering:

- **still:** 5.6 fps, as before, and a frame on the page in 278–283 ms instead of 312–317 ms. Its `camera.motion` fell from 3.2–3.5 ms to 0.2 ms;
- **jogging:** 0.34–0.36 fps, from 0.37–0.39. That's within the runs' spread: each jogging window holds only about 11 frames.

## Why the GPU stays moderately busy

The camera sets the pace, so the GPU finishes a frame's work in part of each 69 ms and then waits. `nvidia-smi`, sampled every 20 ms while the page streamed ([`fps-gpu-clocks.csv`](data/fps-gpu-clocks.csv)):

| | still | jogging at 500 µm/s |
|---|---:|---:|
| the GPU busy | 15–30% | 32–44% |
| its graphics clock | 375–960 MHz, in P0, P3 and P5 | 1,155–1,500 MHz, in P0 |
| its power | 27–29 W | 42–51 W |
| its memory used | 1,443 of 8,192 MiB | 1,475 of 8,192 MiB |

- **With the stage still, the driver clocks the GPU down** between frames, to as little as a quarter of its 1,635 MHz. A frame whose work lands on a low clock would take about 2.5 times as long. That is most likely why roughly one still frame in eight is slow (`camera.expose_gpu` 7 ms median, 18 ms at p95, and the preview's `to_f32` 4 ms and 18 ms), and end to end has a p95 of 62 ms against a 29 ms median.
  - In mode `full`, with twice the time between frames, the camera's render took 26 ms instead of 14: presumably the clock fell further. Its clocks weren't sampled.
  - To test this, NVIDIA's control panel can set *Prefer maximum performance* for the example. That's a setting of the owner's machine, not of the code, and it wasn't tried.
- **The memory** is mostly [the GPU simulator's tiles](../adr/0062-the-simulated-camera-s-image-on-the-gpu.md): at most 1,024 slots, 0.94 GB, capped at a quarter of the free memory. More slots wouldn't raise the rate: a jog keeps needing new tiles, and the budget already holds about three fields of view. They would help a stage that goes back over where it has been.

## Tests

- **`a_path_gives_the_true_positions_the_stage_gives`** (`crates/oscope-sim/tests/stage.rs`), on the default stage, with its backlash, acceleration and settling:
  - the history: a move, a jog, its stop, and a reversal on two axes;
  - paths taken every 25 ms through all of that;
  - positions at every half millisecond, from each path's start to 100 ms past now;
  - each must equal `true_position`'s, to the bit.
- **Broken on purpose,** with a path that keeps only the stage's current phase: the test fails. It passes again restored.
- **The camera's own tests,** of the rolling shutter, motion blur and the GPU's frames, run unchanged, and pass.

## Measured size

149 lines added and 28 removed, outside the docs:

- **code:** 83 added, 24 removed, in `oscope-sim`'s stage, camera and `lib.rs`;
- **tests:** 43;
- **the example:** 12 of code, 2 of the benchmark, and 11 of README.

## Checks

All pass, on the owner's laptop, with `-j 6`:

- `cargo fmt --all --check`;
- `cargo clippy --workspace --all-targets --locked -- -D warnings`, without and with `--features cuda`;
- `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked`;
- `cargo test --workspace --locked`: 238 suites, none failed;
- `OSCOPE_CUDA=1 cargo test --workspace --locked --features cuda`: 238 suites, none failed;
- `ex-live-view --smoke`, without and with `--features cuda`, now in mode `design`;
- `cargo deny check`.

**The gate, on `main` at W6.1** (`94b4b2b`), before the rebase, on the owner's laptop:

- **The Windows replay,** `scripts/ci-local.sh --changed main --cuda`. `oscope-core`'s timing reaches nearly everything: 77 Rust packages, 58 smoke runs and every end-to-end spec.
  - **The first run** passed every step but one: `ex-live-view`'s smoke run. Pinned to two CPUs, the CPU build needs over a minute for the full sensor's first tiles, and no preview came within 60 s. `--smoke` now takes the quarter sensor, as before the baseline: 11–12 s on two CPUs.
  - **Two more runs** on the fix each failed some tests that passed in the other runs, none in what this PR changes:
    - end-to-end specs: three under load, then one visual test 1 pixel off the baselines, which are the owner's desktop's;
    - the status test's unplug race, in P50's test since #66: the job must start before the camera is plugged back in, moments later;
    - `ex-live-recipe`'s smoke budget, 205–222 ms against 200. On two CPUs here it failed 6 times in 10 on this branch, and 4 in 10 on `main`.
- **Two cores, five times:** the suites and smoke run the PR touches, 45 of 45 passed. They were `oscope-sim`'s stage, camera and devices, `oscope-acquire`'s and `oscope-api`'s preview, `oscope-api`'s devices, the libraries of `oscope-core` and `oscope-compute`, and `ex-live-view`.
- **Linux under WSL wasn't run.** This laptop's WSL has no Node, can't read `/mnt/c`, and has 7 GB of memory. The owner chose to merge on the Windows replay.
- **After the rebase on W6.4,** the replay runs again on the commit merged.

## Follow-ups

- **More than 14.5 fps** needs a camera mode faster than the MIChrome 20's design rate. That's a change to the camera's model, for the owner to decide. Jogging at 500 µm/s, the GPU's share of a frame is about 35 ms: the exposure, the tiles and the preview's steps. That would allow about twice the rate.
- **The slow still frames** most likely come from the GPU's clock, not the code. If their p95 matters, try *Prefer maximum performance* first.
- **The CPU build** stays bound by its tiles and exposure, about 177 ms a frame. [The GPU simulator's follow-ups](gpu-simulator.md#follow-ups) still apply.
- **`ahead`** still asks the stage twice under its lock after each frame, for the next frame's tiles. That's negligible.

## How to try it

```sh
cargo run --release -p ex-live-view --features cuda
```

- **Open** `http://127.0.0.1:8218/`. The page shows about 14.5 fps.
- **Jog** with the buttons: it stays at 14.5.
- **The benchmark:** `node examples/ex-live-view/bench/bench.mjs --label gpu`. `--speed 2000` jogs at 2 mm/s.
- **The camera in mode `full`,** for the old 7.24 fps: see [the example's README](../../examples/ex-live-view/README.md).

## Clean room

The true path is our own code, over the stage's own phases. Nothing was copied from elsewhere.
