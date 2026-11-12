# P08: Demosaicing on the CPU

- Status: In review
- Branch / PR: `m1/p08-bayer-colour` / [#16](https://github.com/lucaswewa/OScope/pull/16)
- Size: L · P08 was estimated at ~2,500 before it started, and the owner agreed to split it into three: this phase, P08b and P08c. This phase's share was ~900 · measured **819** hand-written lines added (code 401, tests and benchmarks 268, examples 133, glue 17), and 1 removed. Docs and `Cargo.lock` aren't counted. **Within L's 900.**

## Summary

Raw Bayer mosaics now become RGB images on the CPU, with three methods ([spec](../phases.md#p08)). P08c's CUDA twins will be compared with these.

- **`oscope-imaging::color::demosaic`** offers three methods:
  - **superpixel:** half size, nothing interpolated;
  - **bilinear;**
  - **Malvar–He–Cutler:** bilinear, corrected by the gradient of the pixel's own colour.

  Each works for every Bayer pattern, on f32 mosaics, so that P08b's black level and white balance can run on the mosaic first.
- **`demosaic_frame`** goes from a raw 8- or 16-bit frame to RGB, in one of `Compute`'s lanes.
- **To measure them:**
  - `chart`, a test chart of known colours: patches, a gradient and a zone plate;
  - `mosaic`, what a sensor would see of it;
  - `psnr`.
- **The quality:** every method meets its PSNR mark on the chart, at 512 × 384 for each pattern and at 20 MP. Malvar–He–Cutler beats bilinear by 7.7 dB, 10.8 dB on the fine rings.
- **The speed:** §5.20's CPU budget, ≤ 80 ms for 20 MP on 8 threads, is met by all three, with Malvar–He–Cutler at 29.5 ms.
- **This pull request also records:**
  - the P08 split in the plan;
  - U06's soak, stopped at 19 h 26 min by an editor restart, with no failure;
  - P02b's disk measurement.

## What was built

| Where | What |
|---|---|
| `oscope-imaging::color` (`demosaic`) | **`Demosaic`** (`Superpixel`, `Bilinear`, `MalvarHeCutler`, and `ALL`), and **`demosaic(mosaic, pattern, method)`**. The full-size methods mirror at the edges about the outermost pixel (… 2 1 0 1 2 …), which keeps each pixel's colour; a side of 1 or 2 pixels keeps the parity. Malvar–He–Cutler's output is clamped to 0–1 |
| `oscope-imaging::color` (`chart`) | **`chart(width, height)`:** 24 hue patches, bright and dark, a gradient of the three colours, and a grey zone plate up to a quarter of a cycle per pixel. **`mosaic(rgb, pattern)`.** **`psnr(reference, image, border)`** |
| `oscope-imaging::color` | **`demosaic_frame(compute, lane, view, method)`** and **`ColorError`** |
| `oscope-imaging/benches/demosaic.rs` | Each method on a 20 MP RGGB16 frame, from the raw frame to RGB, on a live pool of 8 threads |
| `examples/ex-demosaic` | The example: each method's time, against the budget, and its PSNR, with `--runs`, `--threads` and `--smoke` |
| Docs | The split (phases, plan §7, the change log and the phase count). U06's soak. P02b's disk. P07 marked Done |

**Dependencies:** none new. `oscope-imaging` now uses `oscope-compute` (allowed within L1 by ADR-0005), `oscope-core`, `rayon`, `serde` and `thiserror`, and `criterion` and `rand` for tests.

## How to try it

```sh
cargo run --release -p ex-demosaic                  # the README shows the output
cargo run -p ex-demosaic -- --smoke
cargo bench -p oscope-imaging --bench demosaic
cargo test -p oscope-imaging                        # 6 tests
```

## Performance

These figures are from the owner's PC, with release builds, on a 5472 × 3648 RGGB16 frame, from the raw frame to RGB, including its conversion to floats.

| Method | 8 threads (criterion) | 24 threads, after conversion | §5.20 (≤ 80 ms on 8 threads) |
|---|---|---|---|
| Superpixel | 12.8 ms | 5 ms | met |
| Bilinear | 27.3 ms | 14 ms | met |
| Malvar–He–Cutler | 29.5 ms | 15 ms | met |

| PSNR on the chart | Superpixel | Bilinear | Malvar–He–Cutler |
|---|---|---|---|
| 512 × 384, each pattern | 22.4 dB | 22.2 dB | 29.9 dB |
| 20 MP, RGGB | 21.8 dB | 21.3 dB | 29.9 dB |

**By region at 512 × 384, for bilinear and Malvar–He–Cutler:**
- **the patches:** 31.7 and 30.8 dB. Malvar–He–Cutler overshoots a little at their edges;
- **the gradient:** 163 and 149 dB. Both rebuild a linear gradient;
- **the rings:** 16.7 and 27.5 dB.

**Superpixel is measured against the chart averaged over 2 × 2 cells.** The rings alias for every method; they are there to separate the methods, not to be rebuilt.

## Design notes and deviations from the plan

- **The split.** P08 was estimated at about 2,500 lines, nearly three times L. The owner agreed to split it into three phases, each within L:
  - **P08:** demosaicing, its tests, the benchmarks and the CPU budget;
  - **P08b:** the colour steps, the display transform, crop and binning, and the writers;
  - **P08c:** the CUDA twins, the CUDA budget, and the example's page.

  P17 and P22 now depend on P08c.
- **The example has no page yet.** Its page, comparing the methods' images side by side, needs P08b's PNG writer, and comes with P08c.
- **Demosaicing takes f32 mosaics**, converted by P06's `to_f32`, not raw bytes. That lets black level and white balance (P08b) work on the mosaic first, as cameras do, and gives the CUDA twins one input type.
- **Edges are mirrored about the outermost pixel.** That keeps every pixel's Bayer colour, so a flat colour comes back exactly at every size from 2 × 2 and every pattern. Repeating the edge pixel instead would put red where blue belongs, and a mutation check showed it.
- **Malvar–He–Cutler is clamped to 0–1,** since its gradient term overshoots at sharp edges.
- **Superpixel's size** is rounded down, and an odd last row or column is dropped, as with binning.
- **The CUDA twins will live in `oscope-compute`,** since `oscope-imaging` may not use unsafe code and kernel launches are unsafe (`lib.rs` says so). `oscope-imaging` will call them through `Compute::cuda()`.
- **The chart's zone plate** reaches a quarter of a cycle per pixel at its edge. Red and blue, sampled every other pixel, alias there, so it separates the methods.

## Tests

- **`oscope-imaging/tests/demosaic.rs`,** 6 tests:
  - **a flat colour comes back exactly** (within 1e-6), for each of the four patterns, every method, and seven sizes from 2 × 2 to 33 × 18 (seeded colours);
  - **the PSNR marks on the chart,** for each pattern:
    - superpixel and bilinear above 21 dB, Malvar–He–Cutler above 28.5 dB;
    - Malvar–He–Cutler at least 5 dB above bilinear, about the paper's 5.5 dB on photographs;
    - by region: both above 60 dB on the gradient, and Malvar–He–Cutler above 25 dB and 8 dB above bilinear on the rings;
    - Malvar–He–Cutler's output inside 0–1;
  - **a full 20 MP chart** meets the marks;
  - **frames through the lanes:**
    - `demosaic_frame` from 16-bit GRBG and 8-bit BGGR frames, in both lanes, equals converting and then demosaicing;
    - other formats and RGB input are refused;
  - **tiny and empty mosaics** give finite output;
  - **`mosaic` and `psnr`:**
    - each pattern's sampling;
    - equal images give infinite PSNR, and an error of 0.1 gives 20 dB;
    - the border is left out;
    - an empty interior and different shapes are errors.
- **Mutation checks,** each restored afterwards:

  | Broken on purpose | Failed |
  |---|---|
  | Malvar's green without its gradient term | `a_flat_colour_comes_back_exactly_at_every_size_and_pattern` |
  | Edges mirrored with the edge pixel repeated | the same |
  | Bilinear's green and blue swapped at red | the same |
  | Superpixel's red and blue swapped | the same |
  | Malvar's overshoot not clamped | `each_method_meets_its_psnr_on_the_chart_for_every_pattern` (the clamp assertion was added for this) |
  | Blue sampled from green | `mosaics_and_psnr_do_what_they_say` |
  | The border measured anyway | the same |
  | Every frame taken as RGGB | `frames_are_demosaiced_in_the_lanes_from_8_and_16_bit_mosaics` |

### CI

To come.

## Follow-ups and known gaps

- **P08b:**
  - black level, white balance, the colour matrix and the display transform;
  - crop and binning with demosaicing;
  - the TIFF and PNG writers.
- **P08c:**
  - the CUDA twins, against ≤ 10 ms;
  - `ex-demosaic`'s comparison page.
- **No SIMD by hand:** the methods are already well inside the budget.

## Clean room

- **The sources:** demosaicing is written from the published methods.
  - **Malvar–He–Cutler:** H. S. Malvar, L.-W. He and R. Cutler, "High-quality linear interpolation for demosaicing of Bayer-patterned color images", *Proc. IEEE ICASSP 2004*, vol. 3, pp. 485–488. The filters are those of its figure 2: α = 1/2, β = 5/8 and γ = 3/4, in eighths.
  - **Bilinear and superpixel:** the textbook methods, such as those in R. Ramanath et al., "Demosaicking methods for Bayer color arrays", *Journal of Electronic Imaging* 11(3), 2002.
- **The chart and PSNR** are written for this phase.
- **Nothing was copied** from `microscope` or any other codebase.
