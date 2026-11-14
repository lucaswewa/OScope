# P23: Stock nodes II: filters, enhancement, threshold, morphology

- Status: In review
- Branch / PR: `m1/p23-filters` / [#36](https://github.com/lucaswewa/OScope/pull/36)
- Size: L · estimated ~3,400 before it started (the CPU references of the Gaussian, median and bilateral filters and of CLAHE ~450, their CUDA kernels and launches ~500; twelve stock nodes, with thresholds, morphology, the distance transform and labelling, ~800; tests ~700; `ex-filters` with its page ~550; glue ~50; and a tenth on top); one pull request, at the owner's standing choice for P12–P23 · measured **2,833** hand-written lines added (code 1,610, of which the CUDA kernels 223 and their launches 208; tests 686; examples 537), and 3 removed. **That is 1,933 over L's 900, and about a sixth under the estimate.**

## Summary

The general image-processing toolkit ([spec](../phases.md#p23), plan §5.7).

- **In `oscope-compute`:**
  - the Gaussian, median and bilateral filters and CLAHE on the CPU, as references;
  - their CUDA twins in a new `kernels/filters.cu`. They give the CPU's results to the bit, but for the bilateral filter's exponentials.
- **Twelve stock nodes** in `oscope-pipeline`:
  - `filter.gaussian`, `filter.median`, `filter.bilateral`;
  - `enhance.unsharp`, `enhance.clahe`;
  - `threshold.manual`, `threshold.otsu`;
  - `morphology.erode`, `dilate`, `open`, `close`;
  - `morphology.distance`, the distance transform, and `morphology.label`, connected components with a table of them.
- **`ex-filters`:** a page comparing each node's output on a field of the simulated slide, with its times on the CPU and the GPU, ending with nuclei found and labelled by a recipe of them.

## What was built

| Where | What |
|---|---|
| `oscope-compute` `filters.rs` | `gaussian` and `gaussian_weights`; `median`; `bilateral` and `bilateral_weights`; `clahe`, `clahe_maps` and `Clahe`. Pixels beyond the edges repeat the edge pixels |
| `oscope-compute` `kernels/filters.cu`, `cuda/filters.rs` | `convolve`, `median`, `bilateral`, `clahe_maps` and `clahe_apply`; `Cuda::gaussian`, `median`, `bilateral` and `clahe`, which check their settings with the CPU's code |
| `oscope-compute` | `ComputeError::Setting`, for settings out of range |
| `oscope-pipeline` `stock/filters.rs` | The filter and enhancement nodes, on the CPU or the GPU, each giving its halo for tiles |
| `oscope-pipeline` `stock/threshold.rs` | `threshold.manual`, and `threshold.otsu`, which gives the mask and its level |
| `oscope-pipeline` `stock/morphology.rs` | Erosion, dilation, opening and closing with a square or a disk; the exact distance transform; labelling with 4- or 8-connectivity, and the components' table: number, area, centroid, bounding box |
| `examples/ex-filters` | The renderer and runner, and the comparison page |

## How to try it

```sh
cargo run --release -p ex-filters --features cuda    # or without the feature, on the CPU alone
```

Open `http://127.0.0.1:8214/`: the field, the noisy copy, and the ten cards, each with its times. The [README](../../../examples/ex-filters/README.md) lists the cards and the times measured.

## Performance

No budget of plan §5.20 is P23's. On the owner's PC (release; 24 threads; an RTX 3070 Ti laptop GPU), at 1024 × 768 RGB:

| Node | CPU | GPU, with the copies to it and back |
|---|---|---|
| Gaussian, σ 2 | 9.2 ms | 3.2 ms |
| Median, radius 2 | 21.7 ms | 4.9 ms |
| Bilateral, σ 3 px and 0.08 | 79.8 ms | 6.5 ms |
| CLAHE, 8 × 8 tiles | 5.5 ms | 3.0 ms |
| Unsharp mask, σ 2 | 9.4 ms | |
| Otsu's threshold, after grey and inverting | 4.3 ms | |
| The distance transform, after the threshold and an opening | 11.6 ms | |
| Labelling, the same | 8.2 ms: 118 nuclei | |

## Design notes and deviations from the plan

**Worth your review:**

- **One pull request,** as you decided for P12–P23. It came in about a sixth under the estimate.
- **The twins agree to the bit:** the Gaussian, the median and CLAHE, on 60 seeded images of 1–3 channels and up to 300 × 300 with settings drawn too. The bilateral filter's exponentials (`expf`) round differently on the CPU and the GPU, so it agrees within 1e-5.
- **CLAHE works channel by channel.** On colour images that can shift hues. For contrast alone, take grey first with `convert.format`.
- **Settings have limits,** set by the kernels' fixed sizes and kept on both sides:
  - the median's radius is at most 7, a 15 × 15 window;
  - the bilateral filter's spatial sigma is at most 8 px;
  - the Gaussian's sigma is at most 32 px;
  - the bilateral filter takes up to four channels.
- **Touching nuclei share a label:** separating them needs a watershed, a later node.

**Other choices:**

- **Edges:**
  - The filters repeat the edge pixels beyond the image.
  - Erosion takes the outside as set, and dilation as not, so a mask neither wears nor grows at its edges.
- **Otsu's level** is the upper edge of the last bin of the darker class. Where splits tie, as across a gap between two populations, it takes the first.
- **The distance transform** gives each set pixel's exact Euclidean distance to the nearest pixel not set. With none not set, it gives the diagonal, not infinity, so the image stays finite.
- **Labels** are numbered from 1, in the order components first appear row by row, so the same mask always gives the same numbers.
- **Tiles:** the neighbourhood nodes give their halos, `⌈3σ⌉` for the Gaussian and the unsharp mask, `⌈2σ⌉` for the bilateral filter, the radius for the median, and the radius once a pass for morphology. CLAHE, Otsu's threshold, the distance transform and labelling look at the whole image, and don't run in tiles.

## Tests

| Crate | Test | What it checks |
|---|---|---|
| `oscope-compute` | `filters` (4) | Worked by hand: a point spread into the Gaussian's weights, and a level kept; a 3 × 3 median with the edges repeated, a spike removed; a step kept sharp by the bilateral filter where a Gaussian blurs it, and a wide range giving the Gaussian of the window; CLAHE's clipping and sharing (a level of 0.5 becomes 7/8), an unclipped ramp mapping to itself, two tiles blending between their centres. Settings out of range refused |
| `oscope-compute` | `the_filters_twins_match_the_cpu` (with `OSCOPE_CUDA=1` and `cuda`) | 60 seeded images: the Gaussian, median and CLAHE to the bit, the bilateral filter within 1e-5; settings refused alike |
| `oscope-pipeline` | `filters` (4) | The filter nodes give `oscope-compute`'s results, and refuse settings out of range when the plan is made; a seven-step chain from the Gaussian to a dilation, run in 20 tiles through its summed halo of 17, gives the whole run's mask; Otsu's level between two populations; morphology, the distance transform and labelling worked by hand, with 4- and 8-connectivity and a U joined at its foot |
| `oscope-pipeline` | `filter_nodes_on_the_gpu_match_the_cpu_s` (with `OSCOPE_CUDA=1` and `cuda`) | The four GPU nodes in recipes on the GPU, against the CPU's |
| `ex-filters` | `--smoke` | The page, the field and the ten cards served; nuclei found |

**Where they ran:** the new suites 5 times out of 5 with their binaries pinned to two cores; and on Linux, under WSL. The CUDA tests ran on the owner's GPU.

**Mutation checks** (each broke the code on purpose, then the tests named were run): all twelve caught.

| Broken on purpose | Caught by |
|---|---|
| The Gaussian's weights not summing to 1 | `a_gaussian_spreads_a_point_by_its_weights_and_keeps_a_level` |
| The Gaussian along rows only | the same |
| The median one rank off | `a_median_takes_each_window_s_middle_value` |
| The bilateral filter ignoring differences | `a_bilateral_filter_keeps_edges_a_gaussian_blurs` |
| CLAHE not clipping | `clahe_clips_shares_and_interpolates_as_worked_by_hand` |
| CLAHE not blending between tiles | the same |
| The CUDA CLAHE kernel not sharing the remainder | `the_filters_twins_match_the_cpu` |
| Otsu's method taking the least variance | `otsu_finds_the_level_between_two_populations` |
| Erosion wearing the edges | `morphology_distances_and_labels_are_as_worked_by_hand` |
| An opening's halo counted once | `tiled_runs_through_the_new_halos_give_what_whole_runs_do` |
| Labels never joined | `morphology_distances_and_labels_are_as_worked_by_hand`. The first version missed it, as no pixel in it met two labels: the U was added |
| The distance transform along rows only | `morphology_distances_and_labels_are_as_worked_by_hand` |

## Follow-ups and known gaps

- **A watershed,** to separate touching objects.
- **CUDA twins** for the unsharp mask, the thresholds and morphology, if recipes need them on the GPU.
- **CLAHE on lightness,** to keep hues.

## Clean room

Nothing was copied from `microscope` or any other codebase.

- **The bilateral filter:** C. Tomasi and R. Manduchi, "Bilateral Filtering for Gray and Color Images", ICCV 1998.
- **CLAHE:** K. Zuiderveld, "Contrast Limited Adaptive Histogram Equalization", *Graphics Gems IV*, 1994. The clip, the sharing of the excess and the bilinear interpolation between tiles are written from its description.
- **Otsu's threshold:** N. Otsu, "A Threshold Selection Method from Gray-Level Histograms", *IEEE Transactions on Systems, Man, and Cybernetics* 9(1), 1979.
- **The distance transform:** P. Felzenszwalb and D. Huttenlocher, "Distance Transforms of Sampled Functions", *Theory of Computing* 8, 2012: the lower envelope of parabolas.
- **Labelling:** A. Rosenfeld and J. Pfaltz, "Sequential Operations in Digital Picture Processing", *Journal of the ACM* 13(4), 1966: two passes, with union-find.
- **The median's selection on the GPU** is Hoare's quickselect (*Communications of the ACM* 4(7), 1961).
- **The Gaussian, the unsharp mask and morphology** are textbook operations.
