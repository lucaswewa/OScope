# The live view's measurements: raw results

These are the measurements behind [the live view's baseline](../live-view-baseline.md), taken on 2026-11-20, and [the GPU simulator](../gpu-simulator.md)'s, on 2026-11-21, all on the owner's laptop.

| File | What it is |
|---|---|
| `live-view-cpu.json`, `live-view-cpu2.json` | Two runs of `bench.mjs` against `cargo run --release -p ex-live-view` |
| `live-view-gpu.json`, `live-view-gpu2.json` | Two runs against the same, built with `--features cuda` |
| `standalone.txt` | The output of the benchmarks run alone: the preview, at 1000 × 667 and §5.20's 1920 px; `ex-sim-bench`; and the simulated camera's models, twice |
| `gpu-sim-cpu1.json`, `gpu-sim-cpu2.json` | [The GPU simulator](../gpu-simulator.md)'s CPU build, two runs of `bench.mjs` (2026-11-21) |
| `gpu-sim-gpu1.json`, `gpu-sim-gpu2.json` | Its CUDA build, two runs |
| `gpu-sim-cpu-before-pool.json` | Its CPU build before the camera had threads of its own: the regression that change fixed |
| `gpu-sim-standalone.txt` | Its models benchmark on the CPU and the GPU, and `ex-sim-bench` |

**Each JSON file** holds:

- `machine` and `browser`;
- `windows`: `start`, `still` and `jog`, each with:
  - `steps`: every step, the server's and the page's, over the window's 30 s: `count`, `per_s`, and `mean_ms`, `p50_ms`, `p95_ms` and `max_ms`;
  - `server`: `/bench/timings.json` as the page last read it. It has the camera's tile cache, the frames published, and the offset from the camera's clock to the epoch;
  - `sessions`: the `preview` Thing's sessions;
  - `webgl`: the renderer that drew;
  - and, for `jog`, `stage`: where the jog went from and to, and its speed.
