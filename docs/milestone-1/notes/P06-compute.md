# P06: Compute abstraction, CPU backend, benchmark harness

- Status: Done
- Branch / PR: `m1/p06-compute` / [#14](https://github.com/lucaswewa/OScope/pull/14)
- Size: M · estimated ~1,480 before it started: code ~620, tests and benchmarks ~400, examples ~145, glue ~30, plus a quarter. That is over L's 900, and the owner chose one pull request · measured **1,413** hand-written lines added (code 713, tests and benchmarks 528, examples 153, glue 19), and 4 removed. Docs and `Cargo.lock` aren't counted. **That is 813 over M's 600 and 513 over L's 900, and 67 under the estimate.**

## Summary

The compute layer that every algorithm targets now exists, with the CPU as its reference ([spec](../phases.md#p06)). The new [ADR-0010](../../adr/0010-compute-backends-the-cpu-reference-and-cuda.md) records the design.

- **`Compute`** runs kernels on the backend in use, in the lane it is given.
  - **The backends:** the CPU is the only one so far. CUDA is listed as not in this build until P07.
  - **`ComputeInfo`** gives the backends, the processor's threads, its SIMD level and the pools' sizes.
- **Live and batch rayon pools,** each with threads and a queue of its own, so that batch jobs can't hold up the preview.
- **The first CPU kernels:** u16 → f32, 2 × 2 binning, and box downsampling, on an f32 `Image`.
- **The equivalence helper** compares two outputs within a tolerance. It reports the largest error, where it is, and the two values. The CUDA twins' tests will use it.
- **Criterion benchmarks** in `benches/`, at 20 MP.
- **`ex-compute-info`** lists the backends and runs microbenchmarks in both pools.

## What was built

| Where | What |
|---|---|
| `oscope-compute` (crate root) | **`Compute`:** `new(&ComputeConfig)`, `info`, `pools`, and the kernels `to_f32`, `bin2x2` and `downsample`, each with a `Lane`. **`ComputeConfig`** sets each pool's threads. **`ComputeError`** |
| `oscope-compute` (`info`) | **`Backend`** (`Cpu`, `Cuda`), **`BackendInfo`**, **`ComputeInfo`** (serialisable, for a later `system` property) and **`SimdLevel`**, detected at run time |
| `oscope-compute` (`pools`) | **`Lane`** (`Live`, `Batch`), and `Lane::current()` from the thread's name. **`Pools`:** `pool`, `install` and `spawn` |
| `oscope-compute` (`image`) | **`Image`:** f32, interleaved, row by row. **`Shape`** |
| `oscope-compute::cpu` | **The reference kernels.** `to_f32` takes a `FrameView` of one plane of 8- or 16-bit samples, whatever its stride. Also `bin2x2`, `downsample` and `converts` |
| `oscope-compute::equivalence` | **`Tolerance`** (`EXACT`, `abs`, `rel`), **`compare`** (any samples that convert to f64), **`compare_images`**, **`Comparison`** with its report, **`Position`** and **`assert_equivalent`** |
| `oscope-compute/benches/kernels.rs` | The three kernels on a 20 MP RGGB16 frame, in the live lane, with throughput in input bytes |
| `examples/ex-compute-info` | The example, with `--runs N` and `--smoke` |
| Docs | ADR-0010 and its index row. The example's README and the examples' index. Plan §7, its change log, and the notes index |

**Dependencies:**

- **New:** `rayon` 1.12.0, and `criterion` 0.8.2 as a development dependency, without its plots. With them came 21 packages, all at least two weeks old, under MIT, Apache-2.0 or the Unlicense. Most are criterion's.
- **Already in the lock:** `serde`, `thiserror`, `rand` and `serde_json`.

## How to try it

```sh
cargo run --release -p ex-compute-info               # the README shows the output
cargo run -p ex-compute-info -- --smoke
cargo bench -p oscope-compute --bench kernels        # criterion
cargo test -p oscope-compute                         # 14 tests
```

## Performance

Measured on the owner's PC (i9-12900, 24 threads, AVX2) with release builds, on a 5472 × 3648 RGGB16 frame in the live lane (6 threads):

| Kernel | Criterion | Throughput |
|---|---|---|
| `to_f32` (40 MB in, 80 MB out) | 6.57 ms | 5.66 GiB/s of input |
| `bin2x2` | 2.83 ms | 26.3 GiB/s |
| `downsample` by 4 | 2.84 ms | 26.2 GiB/s |

`ex-compute-info` gives the same in both pools: `to_f32` takes about 6.2 ms with 6 threads or with 18, since memory limits it. The plan's budgets (§5.20) start with P08's demosaic.

## Design notes and deviations from the plan

- **The size.** It was ~1,190 at the measured density, plus a quarter: ~1,480. It measured 1,413, 5% under.
- **The backend abstraction is `Compute`, with a `match` on the backend** rather than a trait with a method for each kernel (ADR-0010). P07 adds the CUDA arm.
- **`Backend::Cuda` exists already,** so that `ComputeInfo` can list it as unavailable, as the example shows.
- **The pools' default sizes:**
  - **live:** a quarter of the threads, at least 2;
  - **batch:** the rest, at least 1;
  - **asking for 0** gives 1.
- **The pools keep queues apart. They don't set OS priorities.** Live work never waits behind batch work, but under heavy load the operating system shares the cores between them (ADR-0010).
- **The kernels' conventions** (ADR-0010):
  - **`to_f32`** divides by 65,535 or 255. 16-bit data is aligned high, so a 12-bit sensor's maximum is 0.99977;
  - **binning** sums the blocks and drops an odd edge;
  - **downsampling** averages the blocks, including the smaller edge blocks.
- **The kernels accumulate in f32,** as their CUDA twins will. The tests compare them with a plain reference written in f64.
- **The equivalence report** gives the position as (x, y, channel), from the sample's index and the shape. It works on any samples that convert to f64, so u16 outputs can be compared as well as f32.
- **SIMD detection** is in a `cfg(target_arch = "x86_64")` block, with a scalar fallback for other targets. The kernels don't use explicit SIMD yet, and the compiler vectorises them.
- **Criterion's arguments** go to every benchmark target, the library's included. So run `--bench kernels` to pass criterion options such as `--measurement-time`.

## Tests

- **`tests/kernels.rs`,** 4 tests:
  - **known values:**
    - `to_f32` for Mono16, Rgb8 and a 12-bit mosaic aligned high;
    - a rectangle of a larger frame (its stride);
    - PlanarF32 refused;
    - binning and downsampling on 3 × 3 and RGB inputs, including the edge blocks, factors 1 and 50, and 0 refused;
  - **300 seeded cases** over seven formats and random sizes up to 40 × 40:
    - `to_f32` against the view's own samples, exactly;
    - `bin2x2` and `downsample` (factors 1 to 7) against a plain f64 reference, within 1e-6 + 1e-5 × |expected|;
  - **a full 20 MP frame,** through `Compute`:
    - the last sample;
    - binning against four times downsampling by 2 over all 4,990,464 outputs, within 1e-6;
    - a downsample by 4 in the batch lane.
- **`tests/pools.rs`,** 3 tests:
  - **a full batch pool doesn't hold up live work.** Six batch jobs are given two threads: two wait at a gate, and four queue. Live work, including a kernel, then runs on live threads within 10 s, while no further batch job has started. Opening the gate lets all six finish, all on batch threads;
  - **the pools' sizes and names:**
    - each pool has the threads asked for;
    - `Lane::current` inside each pool, and outside both;
    - a parallel loop inside the live pool stays on live threads;
    - asking for 0 threads gives 1;
  - **`ComputeInfo`:**
    - the default division of the threads;
    - the backends' order and availability;
    - the SIMD level;
    - the JSON form.
- **`tests/equivalence.rs`,** 7 tests:
  - equal images pass;
  - the largest error's position and values, and the report's text;
  - absolute, relative and combined tolerances at their limits;
  - NaN matches only NaN, which a test found: NaN's relative term had failed even an exact match;
  - u16 buffers;
  - shape and size errors;
  - `assert_equivalent`'s panic message.
- **Mutation checks,** each restored afterwards:

  | Broken on purpose | Failed |
  |---|---|
  | Live work sent to the batch pool | `a_full_batch_pool_does_not_hold_up_live_work`: the live work timed out |
  | The worst sample's column counted from its index | `the_largest_error_is_reported_with_its_position_and_values`. **It first passed:** the test's worst sample was the last, where both counts agree. It now sits where they differ |
  | The relative tolerance ignored | `tolerances_are_absolute_plus_relative` |
  | NaN never matching NaN | `nan_matches_only_nan` |
  | A bin missing a corner | `the_kernels_match_a_plain_reference_on_300_seeded_cases` |
  | Edge blocks divided by a whole block's count | the same |
  | 16-bit samples divided by 65,536 | `to_f32_scales_8_and_16_bit_samples_from_0_to_1` |

### CI

- **Windows** (the working tree at `d432bc4`): all 8 CI steps passed, with 58 suites and 88 tests. All five examples' `--smoke` passed.
- **Ubuntu 24.04.5** (WSL, a fresh clone of `d432bc4` in the Linux file system, 150 files, no CR in any): all 8 steps passed, with the same 58 suites and 88 tests.
- **GitHub CI** passed: Ubuntu in 2 min 56 s, Windows in 4 min 16 s. All five examples printed `smoke: ok` on both, and the `cargo deny` step passed.

## Follow-ups and known gaps

- **P07** adds the CUDA backend and the CUDA twins of these kernels, compared with `equivalence`. It also adds pinned memory as the frame pool's allocator, and records the toolkit (ADR-0011).
- **The `system` Thing** can report `ComputeInfo` once a server holds a `Compute` (P16 or P17).
- **Explicit SIMD** waits for a kernel that needs it. These three are already limited by memory.
- **U06's soak** ends about 16:45 PST today, and P02b's disk test follows. Their results go into this pull request if it is still open.

## Clean room

Written for this phase, from plan §5.6 and the `rayon` and `criterion` documentation. Nothing was copied from `microscope` or any other codebase. The kernels are the plain definitions of binning and box averaging, and need no citation.
