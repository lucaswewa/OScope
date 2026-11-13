# P15: Simulator at full resolution

- Status: In review
- Branch / PR: `m1/p15-sim-full-resolution` / [#28](https://github.com/lucaswewa/OScope/pull/28)
- Size: M · estimated ~1,500 before it started (code ~850, of it a CUDA kernel and its wrapper ~300; tests ~400; `ex-sim-bench` ~250); one pull request, at the owner's standing choice for P12–P23 · measured **1,680** hand-written lines added (code 906, the kernel and the CUDA workflow's lines among them; tests 537; examples 237), and 56 removed. Docs and `Cargo.lock` aren't counted. **That is 1,080 over M's 600, and 180 over the estimate.**

## Summary

The simulated camera keeps up with a 20 MP frame ([spec](../phases.md#p15), §5.20). **Measured first:** at full resolution, rendering the world took 3.2 s of a frame's 3.4 s. So P15 renders each piece of the world once, not once a frame.

- **A cache of rendered tiles:** 256 pixels square, for each objective, light and depth bucket. A frame samples the tiles it needs; only those missing are rendered, in parallel.
- **The frame itself:** sampling through the mounting, over the stage's motion, then the sensor's noise and the mosaic. It runs on rayon's threads, or on the GPU, where a CUDA kernel samples tiles held in GPU memory.
- **§5.20's budgets, at full resolution with the stage still, are met:**
  - 10.1 frames a second on the CPU, against 5;
  - 62.1 with CUDA, against 14.5.

  Jogging at 1 mm/s, neither backend keeps up: see Performance.
- **A scaled-down sensor,** `SimCameraConfig::quarter()`: 1368 × 912 pixels four times as large, the same field.
- **`ex-sim-bench`** times both backends: cold, still, and jogging.

## What was built

| Where | What |
|---|---|
| `oscope_sim::tiles` | **`TileCache`:** tiles keyed by the optics, the light and the depth bucket, the least recently used dropped beyond a budget, all dropped when the world changes. `field(world, optics, light, z, region)`, `stats`, `bucket_depth`. **`TiledField`:** `sample`, `grid`, `tiles`. **`Tile`**, **`CacheStats`**, **`TILE`** |
| `oscope_sim::camera` | Frames from the cache. **`SimCameraConfig::cache`** (2 GiB; 256 MiB for `small`), **`quarter()`**. **`SimCamera::use_gpu`**, **`on_gpu`**, **`cache_stats`**. The sensor's pixel is the camera's own |
| `oscope_sim::gpu` (with `cuda`) | Which tile is in which slot of the GPU's atlas; a frame's numbers for the kernel |
| `oscope-compute`: `kernels/sim.cu`, `cuda::sim` | **The `expose` kernel**, the CPU path's twin. **`TileAtlas`**, **`Exposure`**; **`Cuda::tile_atlas`**, **`upload_tile`**, **`expose`** |
| `examples/ex-sim-bench` | Cold and warm frames, still and jogging, on each backend, against the budgets |
| CI | The CUDA workflow also checks `oscope-sim` and `ex-sim-bench` with the feature |
| Docs | P14 marked *Done*; P15's estimate in the change log |

## How to try it

```sh
cargo run --release -p ex-sim-bench
cargo run --release -p ex-sim-bench --features cuda
cargo test --release -p oscope-sim --test bench -- --ignored
OSCOPE_CUDA=1 cargo test -p oscope-sim --features cuda --test gpu
```

The example's [README](../../../examples/ex-sim-bench/README.md) shows what it prints.

## Performance

**On the owner's PC:** i9-12900, 24 threads; RTX A2000 12 GB. §5.20's budgets are for the owner's laptop (RTX 3070 Ti); this PC stands in for it, at the owner's choice.

The simulated MIChrome 20 at 5472 × 3648, mode `design`, over H&E tissue at 10×. The median of 8 frames, from `ex-sim-bench`:

| | Cold | Still | Jog 0.2 mm/s | Jog 1 mm/s |
|---|---|---|---|---|
| CPU | 3,099 ms | **99 ms, 10.1/s** | 119 ms, 8.4/s | 323 ms, 3.1/s |
| GPU | 3,105 ms | **16 ms, 62.1/s** | 18 ms, 54.7/s | 162 ms, 6.2/s |

**The ignored benchmark tests** measured 9.9 frames a second on the CPU and 61.7 on the GPU, still.

**Met, still and at slow jogs.**

- **The CPU** meets its budget at twice it. The laptop has fewer cores than this PC's 24 threads, so its margin there is smaller.
- **The GPU** meets its budget at four times it, on a GPU slower than the laptop's.

**Not met jogging fast.** At 1 mm/s each frame brings about 70 µm of new world into view. Its tiles render on the CPU, at about 6.5 million pixels a second for the procedural tissue, whichever backend samples them: 3.1 and 6.2 frames a second.

- A CUDA blur, which the spec mentions, wouldn't help, since the blur isn't the cost. Rendering the samples themselves on the GPU would.
- That is recorded as a follow-up, at the owner's standing choice for a budget not met.

**A cold frame** costs what every frame cost before: 3.1 s at 20 MP. It comes with a new slide, objective, light or depth bucket. So at full resolution an autofocus sweep renders afresh at each step.

## Design notes and deviations from the plan

- **Tiles:**
  - Each is 256 pixels square, plus one row and column of overlap with the next, so bilinear sampling never needs two tiles.
  - They sit on a grid fixed to the stage at each pixel size, so a tile equals the same pixels of any larger rendering: to 1e-5 in the test.
  - Each tile is rendered alone, as a field of its own, and the world adds the margins its blur needs.
- **A tile's key:** the optics' numerical aperture, magnification, sensor pixel, wavelength and medium; the light; and the depth bucket. The intensity isn't part of it: the sensor applies that.
  - **Depth buckets** are a sixteenth of the depth of field, centred so that the world's focus is a bucket's middle. A tile renders at its bucket's middle, at most a thirty-second of the depth of field from the stage. Through the 50×, the difference from rendering at the stage's own depth stayed under 0.01 of full light.
- **The world's identity:** the cache keeps the slide's sample, its place, the coverslip and the focus, and drops every tile when any of them changes. Keeping the sample alive means a new one can't reuse its address and be taken for it.
- **The budget:** 2 GiB by default, enough for several full frames at 10×. The least recently used tiles go first, but never the field in hand.
- **The camera's sensor pixel is now its own.** Before, it took the pixel from the optics it was given, and its sensor's size went unused. They were the same 2.4 µm in every earlier test; `quarter()` needs them to differ.
- **The GPU path is the per-frame step:** sampling, motion, noise and the mosaic. The spec mentions CUDA for defocus too.
  - Defocus is the tiles' blur, done once per tile on the CPU, and cached.
  - Fast jogs are limited by rendering the samples, not by the blur, so a CUDA blur wouldn't change them.
- **The twins:**
  - **Without noise:** the GPU's frames agree with the CPU's within one count, in fewer than 0.1% of pixels; measured, under 0.02%. The kernel works in floats, with positions relative to the tiles' corner; the CPU in doubles.
  - **With noise:** they agree in distribution. Each pixel on the GPU draws from a generator of its own: SplitMix64 over the seed, frame and pixel, then Box–Muller and Knuth's Poisson, as on the CPU.
- **The GPU's tiles:**
  - an atlas of slots in GPU memory, 512 in the example, about 400 MB;
  - only tiles new to it are uploaded;
  - the least recently used are replaced;
  - a frame needing more tiles than there are slots goes to the CPU.

  If the GPU fails, the camera goes back to the CPU for good, and `on_gpu` says so.
- **What building it found:**
  - **The sensor pixel,** above: `quarter()` made no difference until the camera used its own pixel.
  - **Four gaps in the first tests, found by breaking the code:**
    - tiles keyed without their light, or their objective;
    - the GPU's read noise dropped;
    - the mounting's terms transposed.

    The tests now cover each:
    - the same place in two lights;
    - two objectives' tiles at the same grid positions and depth bucket;
    - a dark frame at full gain;
    - a turned mounting without a mirror. A mirror makes the cross terms equal.
  - **Two mistakes in my tests:**
    - a jog too slow to bring a new tile into a binned frame;
    - a field off the tiles' pixel grid.

## Tests

**94 tests in `oscope-sim` without CUDA, 6 of them new; 2 more with it, and 2 benchmarks, ignored:**

| File | Tests |
|---|---|
| `tests/tiles.rs` (6) | Tiles are the world rendered directly. Cached rendering matches uncached within a bucket, through the 50×. Frames reuse tiles, and a jog renders only the new ones (the scaled-down sensor). A new slide is never seen through old tiles. A tile is kept for its light and objective. The cache keeps to its budget |
| `tests/gpu.rs` (2, with `cuda` and `OSCOPE_CUDA=1`) | **The GPU exposes frames as the CPU does,** within a count: still at full resolution; turned, binned 2 × 2 with an ROI, jogging; turned the other way and mirrored, binned 3 × 3. **The GPU's noise has the CPU's statistics,** and the model's: in light, and dark at full gain |
| `tests/bench.rs` (2, ignored) | Full resolution keeps up: at least 5 frames a second on the CPU, 14.5 on the GPU |

**All of the camera's earlier tests pass on the cache,** its blur, shear and motion through the mounting among them.

**Breaking the code on purpose:** 11 changes, each made alone.

- 7 were caught the first time.
- 4 were caught only once the tests were strengthened, as above.

| Change | Caught by |
|---|---|
| A changed world not noticed | `unloading_leaves_only_background_and_noise` |
| Nothing evicted | `the_cache_keeps_to_its_budget` |
| Tiles keyed without their light | `a_tile_is_kept_for_its_light_and_objective` (added) |
| Tiles keyed without their objective | `a_tile_is_kept_for_its_light_and_objective` (added) |
| The tiles' grid half a pixel off | `tiles_are_the_world_rendered_directly` |
| Depth buckets not centred on focus | `tiles_are_the_world_rendered_directly` |
| The camera's own pixel ignored | `frames_reuse_tiles_and_a_jog_renders_only_new_ones` |
| The GPU's mosaic shifted | `the_gpu_exposes_frames_as_the_cpu_does` |
| The GPU's read noise dropped | `the_gpu_s_noise_has_the_cpu_s_statistics` (strengthened) |
| The GPU's motion averaged over one place | `the_gpu_exposes_frames_as_the_cpu_does` |
| The GPU's mounting transposed | `the_gpu_exposes_frames_as_the_cpu_does` (strengthened) |

**The workspace,** with every step CI runs: REPLAYS

## Follow-ups and known gaps

- **Rendering the samples on the GPU,** so that fast jogs keep up at full resolution. Or rendering ahead of the stage in the direction it moves, on spare cores.
- **P16:** the camera Thing turns on `use_gpu` when the compute backend has CUDA, and shows `cache_stats`.
- **P34 and P42:** an autofocus or z-stack at full resolution renders a new depth bucket at each step. Binned modes make those steps cheaper.
- **The fluorescent cells** have 39 layers, so their cold frames cost several times the tissue's.

## Clean room

Written for this phase from:

- plan §5.10, §5.20 and ADR-0013;
- **the twins' noise:** SplitMix64 (Steele, Lea and Flood, 2014); the Box–Muller transform (Box and Muller, 1958); Knuth's Poisson method (*The Art of Computer Programming*, vol. 2);
- **the cache:** least-recently-used eviction, a textbook policy.

Nothing was copied from `microscope` or any other codebase.
