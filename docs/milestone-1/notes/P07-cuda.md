# P07: CUDA backend

- Status: In review
- Branch / PR: `m1/p07-cuda` / [#15](https://github.com/lucaswewa/OScope/pull/15)
- Size: L · estimated ~1,550 before it started: about 1,240 at the measured density, plus a quarter. That is over L's 900, and the owner chose one pull request · measured **1,209** hand-written lines added (code 632, of which 71 are CUDA C; tests 276; examples 240; glue 61), and 7 removed. Docs and `Cargo.lock` aren't counted. **That is 309 over L's 900, and 341 under the estimate.**

## Summary

CUDA now accelerates the compute layer wherever an NVIDIA GPU is present, and stays out of the way where none is ([spec](../phases.md#p07)). The new [ADR-0011](../../adr/0011-building-loading-and-redistributing-cuda-kernels.md) records the design.

- **The `cuda` feature** of `oscope-compute` loads the CUDA driver, NVRTC and cuFFT when the program runs, through `cudarc` 0.19.10.
- **Without a driver or a GPU,** `Compute` uses the CPU, says why, and warns once.
- **The CUDA twins** of P06's three kernels match their CPU references to the bit.
- **Each lane has a stream,** the live one at the GPU's highest priority. Freed device memory is kept in the device's pool.
- **The frame pool can use pinned memory,** which uploads a 40 MB frame in 1.64 ms against 2.97 ms from ordinary memory.
- **`build.rs` compiles the kernels with `nvcc`** into an embedded fat binary.
- **A weekly CI job** builds and checks the feature with the CUDA compiler installed.
- **`ex-cuda-check`** shows the GPU, the twins' times and agreement, and the upload rates.

**The spec's *Done when*** was met on the owner's PC, which stands in for the laptop the spec names (RTX A2000 12 GB; P02b's host):
- with `OSCOPE_CUDA=1`, the twins match;
- the upload of a pinned 40 MB frame is measured at **1.64 ms** (24.4 GB/s).

## What was built

| Where | What |
|---|---|
| `oscope-compute::cuda` (feature `cuda`) | **`Cuda`:**<ul><li>`open(ordinal)`, which checks for the driver before loading it;</li><li>`info`;</li><li>`to_f32`, `bin2x2` and `downsample` on **`DeviceImage`s**;</li><li>`upload`, `download`, `upload_bytes`, `download_bytes` and `synchronize`;</li><li>`pinned_allocator` and `is_pinned`.</li></ul>**`PinnedAllocator`:** P03's `SlabAllocator`, portable and not write-combined, which falls back to ordinary memory with a warning |
| `oscope-compute` (crate root) | **`Compute`** opens the GPU, or falls back and warns once. **`Compute::cuda()`.** **`ComputeConfig::cuda_device`.** **`ComputeError::Cuda`.** The backends are listed with the one in use first |
| `oscope-compute` (`info`) | **`GpuInfo`:** the ordinal, name, compute capability, memory, the driver's CUDA version, the memory pool, NVRTC and cuFFT. **`ComputeInfo::gpu`** |
| `oscope-compute/kernels/image.cu` | `to_f32_u16`, `to_f32_u8`, `bin2x2` and `downsample`, each adding in the same order as its CPU twin |
| `oscope-compute/build.rs` | With the feature: `nvcc --fatbin -arch=all-major -O3` for each `kernels/*.cu`. On Windows, it finds `cl.exe` with `find-msvc-tools`. It gives a clear error without `nvcc` |
| `oscope-core::FrameView::data` | A view's bytes as one span, from its first pixel to its last, to upload in one copy |
| `.github/workflows/cuda.yml` | *CUDA build*: weekly, by hand, and on pull requests that change the compute crate. It installs `cuda-nvcc-13-4` on Ubuntu, then runs clippy, the tests and the example with the feature |
| `examples/ex-cuda-check` | The example, with its own `cuda` feature, `--runs N` and `--smoke` |
| Docs | ADR-0011 and its index row. The example's README and the examples' index. Plan §7, its change log, and the notes index |

**Dependencies:**
- **New:** `cudarc` 0.19.10 (MIT or Apache-2.0, 2026-09-24), optional, with only the driver, NVRTC and cuFFT, loaded at run time. It brings `libloading` 0.9.0 (ISC, 2025-11-05).
- **Already in the lock:** `find-msvc-tools` 0.1.14, a build dependency on Windows only, and `tracing`.

**On this PC:** the CUDA 13.4 toolkit was installed with `winget` at the owner's request. `nvcc` is V13.4.59, and `version.json` says 13.4.1.

## How to try it

```sh
cargo run --release -p ex-cuda-check --features cuda                 # the README shows the output
OSCOPE_CUDA=1 cargo test -p oscope-compute --features cuda           # the GPU tests too
OSCOPE_CUDA=1 cargo test --workspace --locked --features cuda        # CLAUDE.md's check: 94 tests
cargo run -p ex-cuda-check -- --smoke                                # without the feature
```

**On Windows,** a shell opened before the toolkit's installation needs its new `CUDA_PATH`, with `bin` and `bin\x64` on `PATH`. A new shell has them.

## Performance

These figures are from the owner's PC: release builds, a 5472 × 3648 RGGB16 frame, the live lane, the median of 20 runs.

| Kernel | CPU (6 threads) | GPU, data already there | Speed-up | Host → GPU → host |
|---|---|---|---|---|
| `to_f32` | 6.22 ms | 2.53 ms, with the 40 MB upload | 2.5 × | 17.64 ms |
| `bin2x2` | 3.25 ms | 0.47 ms | 6.9 × | 10.35 ms |
| `downsample` ×4 | 2.98 ms | 0.36 ms | 8.3 × | 7.37 ms |

| Uploading 40 MB | Time | Rate |
|---|---|---|
| From the pool's pinned memory | 1.64 ms | 24.4 GB/s |
| From ordinary memory | 2.97 ms | 13.4 GB/s |

The round trips decided where `Compute`'s host methods run (ADR-0011). The plan's budgets for CUDA start with P08's demosaic (≤ 10 ms).

## Design notes and deviations from the plan

- **The size:** it was estimated at ~1,240, plus a quarter: ~1,550. It measured 1,209, and the quarter wasn't needed this time.
- **A fat binary, not PTX.**
  - **What failed:** the plan's PTX, compiled by CUDA 13.4, was refused by this driver, which supports CUDA 13.2: `CUDA_ERROR_UNSUPPORTED_PTX_VERSION`.
  - **The fix:** `-arch=all-major` gives SASS that any CUDA 13 driver runs, and PTX for GPUs newer than the toolkit.
- **`nvcc` on Windows needs `cl.exe`.** `build.rs` finds it with `find-msvc-tools`, already in the lock through `cc`, and passes it with its environment. A test build found that.
- **Uploads wait for their copy.** `cudarc` doesn't wait after a copy from a borrowed slice, and from pinned memory the copy is asynchronous.
  - **How it showed:** the first timing showed a 40 MB "upload" in 0.00 ms.
  - **Why it matters:** the pool could reuse a slab while the GPU still read it.
  - **The test:** a GPU test now overwrites the slab at once and checks the GPU's copy.
- **Pinned memory isn't write-combined.** That is `cudarc`'s default, but the CPU reads frames too, and reads write-combined memory slowly.
- **`Compute`'s host methods stay on the CPU.**
  - **The measurements:** each round trip took longer than the CPU (above).
  - **The device API:** `Compute::cuda()`, with `DeviceImage`s, keeps data on the GPU.
  - **The ADRs:** this refines ADR-0010, whose words "it runs on the backend in use" now apply to device data. ADR-0011 records it.
- **The twins are compared exactly,** since they add in the same order and CUDA divides as IEEE requires. A mutation that only reordered a sum, a one-ulp difference, failed the test.
- **The streams have priorities:** live highest, batch lowest, as the CPU pools keep the two lanes apart.
- **The memory pool:** stream-ordered allocation from the device's default pool, which keeps freed memory for the next allocation.
- **Fallback:** `cudarc` loads the driver when first used, and panics if it can't. So `Cuda::open` first asks whether the library is present. A missing GPU is reported with how many the driver sees.
- **Device selection** is by the driver's ordinal (`ComputeConfig::cuda_device`), not by a name or a score.
- **The spec's "owner's laptop"** is this PC, since the target host is similar to it (P02b).
- **CLAUDE.md's CUDA command,** `OSCOPE_CUDA=1 cargo test --workspace --locked --features cuda`, works as written: `cuda` is `oscope-compute`'s feature, and the run gives 94 tests.

## Tests

- **`tests/cuda.rs`,** 4 tests, with `OSCOPE_CUDA=1` and the feature (they skip themselves otherwise):
  - **the twins match the CPU to the bit on 200 seeded cases:**
    - six formats (8 and 16 bits, mono, RGB, RGBA, a 12-bit mosaic) at sizes up to 300 × 300;
    - half of them a rectangle inside a frame, so rows are a stride apart;
    - both lanes;
    - `to_f32`, `bin2x2`, and `downsample` with factors 1 to 9;
  - **a full 20 MP frame,** with the same three kernels, exactly;
  - **images go up and down unchanged,** an empty output is right, and the factor and format errors are kept;
  - **the frame pool's memory and the GPU's details:**
    - the pool's slabs are pinned, and ordinary memory isn't;
    - an upload from a pinned slab that is overwritten at once still delivers the original;
    - `GpuInfo` names the GPU, and the backend in use is CUDA.
- **`tests/fallback.rs`,** 1 test, wherever the feature is on: asking for GPU 99, twice, gives the CPU each time. The test checks:
  - the reason;
  - that kernels still run;
  - that there is exactly one warning.
  
  On CI's runners it meets a missing driver instead, which it accepts.
- **`tests/pools.rs`:** P06's `ComputeInfo` test now also holds with a GPU, which comes first in the list.
- **`oscope-core`:** `FrameView::data` for a whole frame and for a rectangle.
- **Mutation checks,** on the GPU, each restored afterwards:

  | Broken on purpose | Failed |
  |---|---|
  | An upload that doesn't wait | `the_frame_pool_gets_pinned_memory_and_the_info_names_the_gpu`: the GPU read the next frame |
  | The pool given ordinary memory | the same: not pinned |
  | A warning at every fallback | `a_missing_gpu_falls_back_to_the_cpu_and_warns_once` |
  | A bin added in another order | `the_twins_match_the_cpu_to_the_bit_on_200_seeded_cases`: 64 of 310 samples off by 2.4e-7 |
  | Rows read a row apart, not a stride apart | the same |
  | Edge blocks divided by a whole block's count | the same |

### CI

- **Windows** (the working tree at `ce75e67`, default features, as CI): all 8 steps passed, with 61 suites and 89 tests. All six examples' `--smoke` passed.
- **Ubuntu 24.04.5** (WSL, a fresh clone of `ce75e67`, 161 files, no CR in any): all 8 steps passed, with the same 61 suites and 89 tests.
- **With the feature and the GPU on Windows:** `OSCOPE_CUDA=1 cargo test --workspace --locked --features cuda` passed 94 tests, and clippy with the feature is clean.
- **GitHub CI** passed: Ubuntu in 1 min 53 s, Windows in 3 min 23 s.
- **The new *CUDA build*** passed on Ubuntu in 1 min 40 s:
  - it installed `cuda-nvcc-13-4` (13.4.92) and built the fat binary;
  - clippy with the feature passed;
  - every test passed, with the GPU tests skipping themselves;
  - the fallback test met a missing driver;
  - `ex-cuda-check --smoke` reported "CUDA: no CUDA driver was found" and fell back to the CPU.

## Follow-ups and known gaps

- **P08** writes the demosaic's CUDA twin, against the ≤ 10 ms budget.
- **The preview (P17)** may want copies that overlap the kernels. That means keeping a frame alive until its copy's event, rather than waiting (ADR-0011).
- **The `system` Thing** can report `GpuInfo` once the server holds a `Compute`.
- **Packaging** decides whether installers bundle NVRTC and cuFFT. The EULA's Attachment A allows it, under its distribution requirements (ADR-0011).
- **The main CI doesn't build the feature.** The weekly job does, so a break shows within a week, or at once on a pull request that touches the compute crate.
- **U06's soak** ends about 16:45 PST today, and P02b's disk test follows. Their results go into this pull request if it is still open.

## Clean room

Written for this phase, from plan §5.6, the `cudarc` documentation, and NVIDIA's documentation of `nvcc` and the driver API. The kernels are the plain definitions of P06's, written in CUDA C. No NVIDIA sample code was used, so no notice is needed under the EULA's §1.1.2. Nothing was copied from `microscope` or any other codebase.
