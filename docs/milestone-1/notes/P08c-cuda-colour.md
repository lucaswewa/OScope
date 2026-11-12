# P08c: CUDA twins of the colour pipeline, the budgets, the comparison page

- Status: In review
- Branch / PR: `m1/p08c-cuda-colour` / [#18](https://github.com/lucaswewa/OScope/pull/18)
- Size: L · estimated ~1,100 before it started, over L's 900; the owner chose one pull request · measured **1,072** hand-written lines added (code 485, of which 155 are CUDA C; tests 186; examples 386; glue 15), and 144 removed, mostly in `ex-demosaic`'s rewrite. Docs and `Cargo.lock` aren't counted. **That is 172 over L, and 28 under the estimate.**

## Summary

The colour pipeline now runs on the GPU too, with every step equal to its CPU reference to the bit ([spec](../phases.md#p08c)). This completes P08's three phases.

- **The CUDA twins** are in `oscope-compute`, which may use unsafe code:
  - superpixel, bilinear and Malvar–He–Cutler demosaicing;
  - the black level;
  - white balance and the colour matrix;
  - the display table.

  The kernels are compiled without fused multiply-adds, so each product and sum rounds as the CPU's does.
- **`oscope_imaging::color::gpu`** puts the same types in front of them as the CPU's functions, with images kept on the GPU between steps.
- **§5.20's CUDA budget** is met on the owner's PC. A 20 MP raw frame in pinned memory goes to RGB on the GPU in 3.3–6.5 ms against ≤ 10 ms, with Malvar–He–Cutler at 5.7 ms.
- **`ex-demosaic`** now serves a page comparing the methods side by side, with:
  - their CPU and GPU times against the budgets;
  - their PSNR;
  - whether each twin matched;
  - their images, with the 16-bit TIFFs to download.

## What was built

| Where | What |
|---|---|
| `oscope-compute/kernels/color.cu` | `superpixel`; `demosaic` (bilinear and Malvar–He–Cutler, with the CPU's mirroring); `subtract_black`; `correct`; `display`. Each follows its twin's order of operations |
| `oscope-compute::cuda::color` | **`DemosaicKernel`**, and on `Cuda`: **`demosaic`**, **`subtract_black`**, **`correct`** and **`display(image, table)`**. Each checks its image's channels (**`ComputeError::Channels`**) before launching, which the kernels' safety rests on |
| `oscope-compute` | `cuda.rs` became `cuda/mod.rs`, with the `color` submodule. `build.rs` adds `--fmad=false` |
| `oscope-imaging::color::gpu` (feature `cuda`) | **`demosaic`**, **`demosaic_frame`**, **`subtract_black`**, **`correct`** and **`to_display`**, with the CPU's settings and checks. The display table and the black-level check are shared with the CPU |
| `examples/ex-demosaic` | Rewritten as a served example (port 8214), on `ex-common`:<ul><li>its frame comes from the frame pool, in pinned memory when there is a GPU;</li><li>it times both paths and checks the twins;</li><li>it writes `results.json` and the images beside the page (`page/index.html`, `page/app.js`).</li></ul>Its own feature `cuda`, and `--runs` and `--threads` |
| `examples/common` | **`Example::from_args_with(name, port, options)`** and **`number(option)`**, for an example's own options. `get` and `post` read binary answers, such as images |
| `oscope-api` pages | `.tif` and `.tiff` are served as `image/tiff` |
| `.github/workflows/cuda.yml` | The weekly CUDA build also checks `oscope-imaging` and `ex-demosaic` with the feature |

**Dependencies:** none new. `ex-demosaic` now uses `ex-common`, `oscope-api`, `serde_json` and `tokio`, all already in the workspace.

## How to try it

```sh
cargo run --release -p ex-demosaic --features cuda       # then open http://127.0.0.1:8214/
cargo run -p ex-demosaic -- --smoke                      # without the GPU
OSCOPE_CUDA=1 cargo test -p oscope-imaging --features cuda --test gpu
```

## Performance

These figures are from the owner's PC (RTX A2000 12 GB, i9-12900), with release builds, on a 5472 × 3648 RGGB16 frame in the frame pool's pinned memory, from the raw frame to RGB. Each is the median of 10 runs.

| Method | CPU, 8 threads | GPU | §5.20 | GPU against CPU |
|---|---|---|---|---|
| Superpixel | 11.9 ms | 3.3 ms | ≤ 80 / ≤ 10 ms: met | the same, to the bit |
| Bilinear | 27.5 ms | 5.2 ms | met | the same, to the bit |
| Malvar–He–Cutler | 29.8 ms | 5.7 ms | met | the same, to the bit |

The GPU's times include uploading the 40 MB frame (about 1.6 ms from pinned memory, P07) and converting it to floats. The GPU test, in a debug build, measured the same: 3.30, 5.24 and 5.74 ms.

## Design notes and deviations from the plan

- **No fused multiply-adds.** `nvcc` fuses `a × b + c` into one instruction by default, which rounds once where the CPU rounds twice.
  - **The flag:** `build.rs` passes `--fmad=false`. A mutation check that removed it made the demosaic twins differ.
  - **The cost:** a little speed, still well inside the budget.
  - **P07's kernels** don't multiply and add, so they are unaffected.
- **The twins follow their reference's order of operations,** sum by sum.
  - **Mirroring:** the kernels mirror the edges with the same function as the CPU.
  - **Rounding:** the display table is indexed with `roundf`, as Rust's `round`.
  - **The result:** every step matches to the bit, at every pattern and size from 1 × 1.
- **The kernels are in `oscope-compute`, the API in `oscope-imaging`,** since only the former may use unsafe code. `oscope-compute` takes a `DemosaicKernel` of its own, not `oscope-imaging`'s `Demosaic`, since the dependency runs from imaging to compute.
- **Every launch checks its image's channels first.** A kernel given the wrong shape would read or write outside its buffer, so the checks are part of the launches' safety, not only of their errors. A mutation check that removed one was caught.
- **The display table is built on the CPU** by the same code as `to_display`, and uploaded for each call. It is 64 KB, small next to an image.
- **`ex-demosaic` became a served example,** so that its page can show the images.
  - **Its files** go into a page folder in its data root, which `ex-common` serves.
  - **Its time from the raw frame** uses the frame pool, pinned when there is a GPU, as the camera's frames will be.
- **`ex-common` changes:**
  - **Its own options:** `ex-common` now takes an example's own numeric options (`from_args_with`). Before, it refused any option but its own.
  - **Binary answers:** its HTTP helper now reads binary answers. It had read text only, which failed on an image.
- **The region (`demosaic_region`) has no GPU twin.** The spec names the three methods and the colour steps, and a region on the GPU is a crop of the device image, which P17 can add.

## Tests

- **`oscope-imaging/tests/gpu.rs`,** 3 tests, with `OSCOPE_CUDA=1` and the feature:
  - **the demosaic twins:**
    - 45 sizes, from 1 × 1, 2 × 2, 3 × 2 and 5 × 7 up to 160 × 120, seeded;
    - every pattern, and each method;
    - random samples and the chart;
    - each equals the CPU exactly;
  - **the colour-step twins:**
    - black level 0.03;
    - gains and a full matrix with negative terms;
    - the display under each curve, with black and white points, byte for byte;
    - the same refusals as the CPU's: an RGB step on a mosaic, a demosaic of RGB, and a black level of 1.5;
  - **a 20 MP frame from pinned memory:** each method equals the CPU, and its time is printed against 10 ms.
- **Without the feature or the GPU,** the tests skip themselves, as P07's do.
- **The page's script** was driven in Node against a live `ex-demosaic`:
  - its summary named the GPU;
  - its table gave the times, PSNR and "the same, to the bit" for each method;
  - all six images were served.
- **Mutation checks,** on the GPU, each restored afterwards:

  | Broken on purpose | Failed |
  |---|---|
  | Fused multiply-adds allowed | `the_demosaic_twins_match_the_cpu_to_the_bit` |
  | Edges mirrored with the edge pixel repeated | the same |
  | Malvar's overshoot not clamped | the same |
  | The pattern's cell read by columns | the same |
  | The display's level truncated, not rounded | `the_colour_step_twins_match_the_cpu_to_the_bit` |
  | The matrix's first row read as a column | the same |
  | RGB not required for the correction | the same |

### CI

- **Windows** (the working tree at `8a93e6d`, default features, as CI): all 8 steps passed, with 65 suites and 100 tests. All seven examples' `--smoke` passed.
- **Ubuntu 24.04.5** (WSL, a fresh clone of `8a93e6d`, 182 files, no CR in any): all 8 steps passed, with the same 65 suites and 100 tests.
- **With the feature and the GPU on Windows:**
  - `OSCOPE_CUDA=1 cargo test --workspace --locked --features cuda` passed 108 tests;
  - clippy with the feature is clean on the four crates the weekly job checks;
  - `ex-demosaic --features cuda -- --smoke` passed, in release.
- **GitHub CI, and the *CUDA build* on Ubuntu with the feature and no GPU:** to come.

## Follow-ups and known gaps

- **The preview (P17)** will chain these on the GPU from the frame to JPEG. It can add a region's crop on the device, and keep the display table uploaded.
- **The colour matrix and white balance for the MIChrome 20** need calibration with a real chart (M3).
- **Pipeline nodes (P22)** wrap these functions.

## Clean room

The kernels follow their CPU references from P08 and P08b: Malvar, He and Cutler (ICASSP 2004), the textbook bilinear and superpixel methods, and IEC 61966-2-1's sRGB curve, as P08's and P08b's notes cite them. Nothing was copied from `microscope` or any other codebase, and no NVIDIA sample code was used.
