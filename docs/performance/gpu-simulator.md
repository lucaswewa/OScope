# The simulated camera's image on the GPU

- **Status:** *In review*, in #113 with [the baseline](live-view-baseline.md) and [the live view at 14.5 fps](live-view-fps.md), which follows it. The gate's results are in [the latter's checks](live-view-fps.md#checks).
- **Why:** [the live view's baseline](live-view-baseline.md) found that the simulated camera is the live view's main cost, and that it always renders on the CPU:
  - ~160 ms a still frame, against its 138 ms interval;
  - over 2 s a frame while jogging;
  - 4.5 s for the first frame's tiles;
  - and, beside it, a preview 7–9 times slower than alone.
- **The decision:** [ADR-0062](../adr/0062-the-simulated-camera-s-image-on-the-gpu.md).

## The owner's decisions (2026-11-20)

- **Tolerance:** GPU tiles may agree with the CPU's within a tolerance, not to the bit.
- **Precision:** tiles may be half precision.
- **H&E only for now:** other samples keep rendering their tiles on the CPU.
- **One phase** on its own branch, with all four steps below. The owner tests it thoroughly once it is complete.

## What it does

1. **The server's camera renders on the GPU.**
   - `oscope-api`'s `cuda` feature turns on `oscope-sim`'s.
   - The simulator's configuration gets `gpu`, true by default. With the feature, a GPU and `gpu = true`, the camera renders its frames on the GPU; otherwise on the CPU. The server's log says which, and why.
   - With the camera on the GPU, the frame pool is pinned memory, so that a frame comes back by DMA.
   - **Packing:** the frame's samples are packed on the GPU in the frame's format (16 bits aligned high, or 8 bits) and copied straight into the frame's slab. On the CPU, the exposure writes the slab itself. The separate 40 MB of counts and the packing loop go, on both paths.
   - **A slab is taken before the frame is rendered.** With none free, the frame is dropped without rendering it, and counted as before.
2. **The H&E section's tiles are made on the GPU.** This is when the world's sample is an `HeTissue` (H&E or H-DAB), the light is transmitted, and every layer's blur is at most 64 pixels, the CPU's limit for blurring at the tile's own pixel. A tile is made in two stages, each with a cache in VRAM, in half precision:
   - **The layers:** each layer's stains before any blur, for a tile of 256 pixels. These depend on the pixel's size and the tile's place, not on the focus.
   - **The tile:** the layers' transmittances blurred by the PSF at their distances from focus, multiplied, then the glass and the sensor's colour response: as the CPU's `World::render` and tile cache make it. A tile is keyed as today, by the objective, light, depth bucket and place.
   - **Anything else** is rendered on the CPU as today, converted to half precision on the GPU, and kept in the same cache. That covers other samples, other light, a layer blurred more than 64 pixels, and no slide.
3. **Tiles are made ahead of the stage, and a frame doesn't wait for many.**
   - **Ahead of the stage:** after each frame, while it waits for the next exposure to end, the camera makes the tiles the next frame will need, from the stage's velocity during the frame.
   - **At most a budget of new tiles per frame:** 128 by default. A frame needing more is exposed from a level four times coarser (the pixel × 4), whose few tiles are made first. The rest of the fine tiles are made between frames, so the next frames are sharp.
   - **In stepped time** (tests) the camera finishes every waiting tile before it waits for the clock, so its frames don't depend on how fast the host is.
4. **A focus change only blurs again.**
   - A new depth bucket makes new tiles from the cached layers: blur, multiply, glass. A tile then costs ~0.14 ms on the GPU, rather than ~0.8 ms with its layers.
   - The layers' cache empties only when the slide, its sample or its place changes. The tiles' cache also empties when the focus or tilt changes, as today.
5. **The camera renders on threads of its own,** not on rayon's global pool:
   - the processor's threads less the quarter the live pool takes, 15 of 20 here, in one pool for the process; with 4 threads or fewer, as on CI's runners, all of them, as before;
   - this wasn't in the spec at first. Step 1's CPU path, which writes the slab directly, made the camera busier, and the CPU build's preview, beside it, slowed from 185 to 265 ms. With the camera's own threads the preview takes 54 ms (§Results).

**What stays the same:**

- **The CPU path is the reference and the fallback** (ADR-0010). It stays bit for bit as it is, apart from writing the slab directly.
- **The exposure kernel** (P15) keeps its mounting, rolling shutter, motion and sensor model. It reads half-precision tiles, and packs its output.
- **Frames still reach the frame bus in host memory,** as the real camera's do: nothing downstream changes.

## How it agrees with the CPU

Half precision has 11 significant bits, so a tile's value is rounded to within 2⁻¹² of itself. The GPU also works in single precision where the CPU uses double, apart from positions and the noise lattice, which stay double on both.

- **Tiles:** every sample of a GPU tile within 2⁻⁸ (0.4%) of the CPU's tile, and their mean difference under 2⁻¹² (0.024%). This is checked at 5×, 10×, 20× and 50×; binned 2 × 2; in focus, 10 µm and 40 µm out of it; and for H&E and H-DAB.
- **Frames without noise:** a tile's sample is within 2⁻¹² of itself, so a pixel's count moves by at most 2⁻¹² of its counts above black, and the ADC's rounding tips it a count with at most that chance. So:
  - no pixel differs by more than 2 counts: P15's one, and up to 2⁻¹² of 4095 more;
  - the share of pixels that differ, and the mean difference, are each at most 2⁻¹² of the frame's mean counts above black, plus P15's 0.1%;
  - the mean signed difference is under 0.05 of a count: no bias.

  The spec first said "fewer than 5% differing". That was my mistake: at ~600 counts above black, 2⁻¹² allows 15%, and a frame binned 2 × 2, with four times the counts, allows more. The bound now follows each frame's own signal.
- **Noise:** the same statistics as the CPU's, as now.
- **Determinism:** the same tile from the same GPU, to the bit, whichever batch it was made in. The nuclei are the same on both, since their places come from integer hashes.

## Design

```text
 camera thread (sim clock)                     GPU, the camera's stream
 ─────────────────────────                     ──────────────────────────────────────────────────────────
 frame k's exposure ends
   tiles for its footprint? ─────────────────► the tiles' cache (f16, 257² RGB, LRU)
     missing ≤ budget: make them                 └ missing: the layers' cache (f16, 256² × 2 layers × 2 stains)
     missing > budget: the coarse level              └ missing: he_layers (both layers in one pass)
                                                   gather 3 × 3 neighbours → blur (per layer) → compose
   each row's stage places (≤ 2 MB) ─────────► expose: mounting, Bayer, motion, vignetting, noise, pack
   slab (pinned) ◄──────────────────────────── DMA, 40 MB
   sink → acquisition → frame bus
 until frame k+1's exposure ends:
   make tiles ahead of the motion, and the fine tiles a coarse frame left
```

- **`oscope-compute`** gets the kernels, in `kernels/tissue.cu`:
  - `he_layers`;
  - `gather_layers`, then `blur_line` along rows and columns, the CPU's Gaussian, or from 8 pixels `box_line`, its four box filters;
  - `compose`;
  - `to_half`, for the CPU's tiles.
  - The exposure kernel reads `__half` tiles and packs its output.
- **`oscope-sim`'s `gpu` module** holds both caches, the batches, and the fallback to the CPU's tiles, with the queue of tiles to make between frames.
  - The CPU's tile cache and the GPU's share how a region becomes tiles: the level, the depth bucket and the places.
  - `Sample` gets `gpu_model`, `None` by default, which `HeTissue` answers with its seed, islands and stains.
- **The H&E kernel** makes both layers in one pass, since only the nuclei differ between them.
  - Positions on the slide, up to 75 mm, and the noise's lattice, are doubles.
  - Everything after them is single precision: distances, fades, coverages, stains. The laptop's GPU runs doubles at 1/64 of the float rate. A first version in doubles throughout took 2.4 times as long, and agreed with the CPU no better: half precision's rounding dominates either way.
  - The seed's hash with each salt is made once on the host.
- **The blur** follows `blur.rs`: weights or box radii from the host, edge pixels repeated at the ends of a tile and its margin, read from the layers of the tile's 3 × 3 neighbourhood. A margin of 3σ + 1 ≤ 193 pixels fits within one neighbouring tile.
- **VRAM:** `use_gpu(cuda, slots)` makes `slots` tiles and `slots` layer tiles: 396 KB and 512 KB each.
  - The simulator asks for three frames' worth of its sensor (`gpu_slots`), up to 1024 of each, and at most a quarter of the GPU's free memory. Below one frame's worth, the camera stays on the CPU, and the log says why.
  - The full sensor gets 1024 of each, 0.9 GB, with 3.8 GB free. A full frame at 10× needs 345 tiles and 408 layer tiles.
  - The quarter-size sensor that tests use gets 105, 97 MB.
  - The first version asked for 2048, 1.8 GB. Under the CUDA test suite, servers side by side then took 6 of the GPU's 8 GB.

## Estimate

| Part | Lines |
|---|---:|
| CUDA kernels: `tissue.cu`, and the exposure's changes | 550 |
| `oscope-compute`'s glue: atlases, batches, launches, the DMA | 350 |
| `oscope-sim`: the GPU's caches, batches, fallback, coarse level, queue | 600 |
| `oscope-sim`: the camera writing its slab, prefetching; `Sample::gpu_model`, the shared tile planning | 250 |
| `oscope-api` and the configuration: the feature, `gpu`, the pinned pool | 80 |
| Tests: tiles against the CPU, frames, determinism, refocusing, the coarse level and prefetching, the fallback, the server | 700 |
| `ex-live-view`'s page and benchmark | 40 |
| **In all** | **~2,570** |

## Tests

All with `OSCOPE_CUDA=1` and `--features cuda`, except the CPU path's.

- **The CPU path:**
  - the camera's tests pass unchanged, with the slab written directly;
  - a frame is dropped without rendering when the pool is empty.
- **Tiles:** GPU tiles against the CPU's, within the bounds above, for each case listed; and the same to the bit when made again, in another batch.
- **Frames:** the existing twin tests (`the_gpu_exposes_frames_as_the_cpu_does`, `the_gpu_s_noise_has_the_cpu_s_statistics`), over the H&E section with GPU tiles, within the frames' bounds.
- **Refocusing:** a tile made from the cached layers after a change of depth bucket equals one made from scratch, to the bit. The layers' cache keeps its layers across focus changes, and empties when the slide is unloaded.
- **The coarse level:** a jump of a whole field of view, past the budget, gives a coarse frame, then fine frames once the waiting tiles are made. No frame makes more than the budget.
- **Prefetching:** after a few frames of a jog, the tiles one frame ahead are already in the cache.
- **The fallback:** the test target, reflected light, and no slide all give the CPU's tiles on the GPU.
- **The server:** `[simulator] gpu = false` keeps the camera on the CPU. `ex-live-view --features cuda -- --smoke` passes.
- **Mutation checks:** each test above is shown to fail on purpose once:
  - the H&E kernel without its upper layer;
  - the layers' cache keyed without the pixel size;
  - prefetching that does nothing;
  - a budget that is ignored.

**Measured** with `bench.mjs` against the baseline, CPU and CUDA builds, two runs each. The targets with CUDA:

- a still frame's rendering in under 25 ms;
- end to end under 100 ms;
- at least 5 frames a second while jogging at 500 µm/s;
- a cold field of view in under 0.5 s.

## Results

Measured on 2026-11-21 on the owner's laptop, as the baseline was: `bench.mjs` against `ex-live-view` in release, two runs of each build on fresh data roots. The raw runs are in [`data/`](data/README.md). The ranges below are the two runs' medians.

**The CUDA build:**

| | Baseline | This phase |
|---|---:|---:|
| The camera's rendering, stage still | 158.6–160.0 ms | **25.7–26.5 ms** |
| Frames a second the page draws, still | 5.9–6.0 | **7.23**, the camera mode's own cap |
| The preview, still | 137–144 ms | **37.3–37.4 ms** |
| Server latency, still | 368–370 ms | **64–65 ms** |
| **End to end, still** | **377–381 ms** | **69–71 ms** |
| The camera's rendering, jogging at 500 µm/s | 1,434–1,486 ms | **62–71 ms** |
| Frames a second the page draws, jogging | 0.40–0.43 | **7.24–7.25** |
| **End to end, jogging** | **1,929–1,994 ms** | **92–97 ms** |

- **The camera no longer starts late:** it renders in ~25 ms, against its 138 ms interval, so `camera.wake_late` fell from ~70 ms to under 1.
- **The CPU is free for the preview.** The preview's resize and JPEG took 46 and 31 ms beside the CPU camera; they take 3.8 and 2.7 now, as they do alone.
- **While jogging, the tiles are made between frames:** ~3.5 ms of prefetching each frame, and no frame makes its own. The rendering is the stage's places on the CPU (35–45 ms), and the exposure on the GPU (22–24 ms).
- **Against the spec's targets:**
  - a still frame's rendering under 25 ms: **at the target**, 25.7–26.5 ms median and 23–24 ms on average;
  - end to end under 100 ms: met, 69–71 still and 92–97 jogging;
  - at least 5 fps jogging: met, 7.24;
  - a cold field of view under 0.5 s: met, 288 ms (below).

**The CPU build,** whose camera renders as before, now on threads of its own:

| | Baseline | Before the camera's own threads | This phase |
|---|---:|---:|---:|
| Frames a second the page draws, still | 5.0–5.3 | 3.8–3.9 | **5.5–5.6** |
| The preview, still | 175–185 ms | 265–269 ms | **53–54 ms** |
| **End to end, still** | **481–488 ms** | **573–586 ms** | **312–317 ms** |
| The camera's rendering, still | 157–165 ms | 157–158 ms | 179–182 ms |
| Frames a second the page draws, jogging | 0.42–0.43 | 0.46 | 0.37–0.39 |

- **Without the camera's own threads, step 1 made the CPU build worse.** Writing the slab directly removed the camera's single-threaded packing, the gap in which the preview's threads got the cores. So the preview slowed by half.
- **With the camera on 15 of the 20 threads,** the preview takes 54 ms, and still frames reach the page 35% sooner than at the baseline.
- **Jogging on the CPU is ~10% slower,** where the camera, with fewer threads, sets the pace. It was unusable at the baseline and still is: the CUDA build is the answer to it.

**The simulated camera alone** (`gpu-sim-standalone.txt`): the models benchmark, with the cold frame's tiles made in full:

| | CPU | GPU |
|---|---:|---:|
| A cold field of view, 345 tiles | 5,536 ms | **288 ms** |
| A still frame | 165.6 ms | **7.8 ms** |
| Jogging at 500 µm/s: a frame, and the work between frames | 1,435 ms | **41 + 12.5 ms** |
| 50 µm out of focus, cold | 13,956 ms | 1,307 ms |

- **On the GPU,** a batch of ~18 layer tiles takes 12 ms, their blur and composition 2.5 ms, and the exposure with its DMA ~18 ms.
- **`ex-sim-bench`** gives 8 ms a still frame on the GPU, 129 fps (P15's path: 17 ms), and 24 ms jogging at 1 mm/s (P15's: 236 ms). Its cold frame, 82 ms, is the coarse level's: 345 tiles are past the budget of 128.
- **On the CPU,** with 15 threads, a still frame takes 168 ms: 5.9 fps, inside §5.20's 5.

## What was built

| Where | What |
|---|---|
| `oscope-compute` `kernels/tissue.cu` | The H&E section's layers, the gathering, blurs and composition, and `to_half` |
| `oscope-compute` `kernels/sim.cu` | The exposure reads half-precision tiles, and packs the frame's samples |
| `oscope-compute` `cuda/sim.rs` | `TileAtlas` and `LayerAtlas` in half precision; `he_layers`, `he_tiles`, `upload_tile`, `download_tile`; `expose` into a caller's slab; `half_to_f32` |
| `oscope-compute` `cuda/mod.rs` | The tissue kernels loaded; `Cuda::memory` |
| `oscope-sim` `gpu.rs` | `GpuTiles`: both caches, H&E batches, the CPU's tiles for the rest, the budget and coarse level, the work between frames, `GpuStats` |
| `oscope-sim` `camera.rs` | A slab taken before rendering, and written directly; the GPU's tiles; tiles ahead of the stage; the camera's own threads; `pinned_allocator`, `set_tile_budget`, `gpu_stats` |
| `oscope-sim` `tiles.rs`, `world.rs`, `blur.rs` | `Plan`, the planning the CPU's and the GPU's caches share; `World::blurs`, the glass's numbers |
| `oscope-sim` `sample.rs`, `tissue.rs` | `Sample::gpu_model`, and `HeTissue`'s |
| `oscope-sim` `devices.rs` | `[simulator] gpu`, the GPU's slots sized by the sensor and the free memory, and `on_cpu`, why not |
| `oscope-api` | `cuda` reaches `oscope-sim`; the log says where the camera renders; a pinned frame pool for the GPU's camera |
| `ex-live-view` | The page's steps and the GPU's tiles in `/bench/timings.json` |
| Tests | `oscope-sim`'s `gpu_tiles` (5), 3 more in `gpu`, the frames' twin bounded by half precision, a frame dropped without rendering, and the GPU or not, by configuration and by memory; the models benchmark on the GPU |

**Measured size:** 2,776 hand-written lines added against the baseline's commit (code 2,045; tests 703; examples 28), and 280 removed. Docs aren't counted. That is about 200 over the estimate of ~2,570. The camera's own threads, sizing the GPU's slots by its free memory, and the extra tests weren't in it.

## Checks

- **The whole workspace,** on the final code, each in a target folder of its own with `-j 6`. At cargo's default 20 jobs, this laptop's 16 GB ran out of memory while building, and the errors looked like missing crates:
  - `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` with and without `--features cuda`, and `cargo doc` with `-D warnings`: clean;
  - `cargo test --workspace --no-fail-fast`: 238 suites, none failed;
  - `OSCOPE_CUDA=1 cargo test --workspace --features cuda --no-fail-fast`: 238 suites, none failed;
  - `ex-live-view -- --smoke`, with and without `--features cuda`: ok;
  - `cargo deny check`: no new dependencies.
- **Not run:** the local replays (`scripts/ci-local.sh`), the two-core runs, and Linux under WSL. They are for the owner's tests before a merge.
- **Mutation checks,** each run once on purpose:

| Broken on purpose | Caught by |
|---|---|
| The H&E kernel without its upper layer | `the_gpu_makes_the_cpu_s_tiles_within_half_precision` |
| The layers' cache keyed without the pixel | `each_pixel_s_layers_are_its_own`. At first nothing caught it: each case had its own slide, which emptied the cache, and each pixel size its own tiles' numbers. That test asks for the same numbers at two pixels |
| Prefetching that does nothing | `the_tiles_ahead_of_a_jog_are_made_between_frames` |
| A budget that is ignored | `a_frame_past_its_budget_is_coarse_and_the_next_are_fine` |

- **A false alarm along the way:** my first mutation script restored each file with an older modification time, so cargo kept the mutated build. The next "unbroken" run then failed. The script now updates the time, and every check above was run again from clean.

## Follow-ups

- **The stage's places on the CPU:** 8 ms a still frame, and 35–45 ms jogging, now the largest part of a jogging frame. They could be worked out on the GPU from the stage's moves.
- **The camera and the preview share the GPU.** The exposure takes 18 ms beside the preview but 5.5 ms alone, and the preview's 40 MB upload 18 ms against 4.5 ms. Ordering their work, or keeping the frame on the GPU for the preview, would help. The second is a change to the frame bus.
- **Large blurs:** 50 µm out of focus, a cold field of view takes 1.3 s. The box filters run one thread per line.
- **The H&E kernel** works out each nucleus for every pixel near it. A table of each tile's nuclei would cut its 12 ms per batch.
- **Other samples:** the cells, structures, surface, test target and imported images still render their tiles on the CPU.
- **The CPU build's jogging,** at 0.4 fps: the baseline's CPU-side ideas still apply. Those are one noise draw instead of two, both layers in one pass, and cheaper motion blur.
- **The timing-sensitive status test:** `each_fault_is_reported_by_its_subsystem_and_rolled_up` failed once under the CUDA suite's load. It unplugs the camera, submits a job, and plugs it back in, racing the job. It passed alone three times out of three, and in the next full run.

## How to try it

```sh
cargo run --release -p ex-live-view --features cuda   # the log says "the simulated camera renders on the GPU"
node examples/ex-live-view/bench/bench.mjs --label gpu # in another terminal
OSCOPE_CUDA=1 cargo test -p oscope-sim --features cuda --test gpu --test gpu_tiles
OSCOPE_CUDA=1 cargo test --release -p oscope-sim --features cuda --test models -- --ignored --nocapture --test-threads=1
```

`[simulator] gpu = false` in the server's configuration keeps the camera on the CPU. On the page, the line under the timings says how many tiles the GPU has made, and how many frames were coarse.

## Clean room

Everything here is OScope's own: the GPU kernels are written from `oscope-sim`'s models, ADR-0013's and P10's, which they follow step by step. Half precision is IEEE 754's binary16, through CUDA's `cuda_fp16.h`.
