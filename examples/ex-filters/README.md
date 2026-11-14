# ex-filters

P23's stock nodes, the general image-processing toolkit, compared on a page: each one's output on a field of the simulated H&E slide, with its time on the CPU and, with `cuda`, the GPU.

| Card | The nodes |
|---|---|
| Gaussian | `filter.gaussian`, σ 2 px |
| Median, on a noisy copy | `filter.median`, radius 2, on the field with 2% of its pixels black or white |
| Bilateral | `filter.bilateral`, σ 3 px in space and 0.08 in value: it smooths the stroma, and keeps the nuclei's edges |
| Unsharp mask | `enhance.unsharp`, σ 2 px, amount 1.5 |
| CLAHE | `enhance.clahe`, 8 × 8 tiles, clip 2.5 |
| Threshold | `convert.format` to grey, then `threshold.manual` at 0.55 |
| Otsu's threshold | grey, `display.lut` to invert it, then `threshold.otsu`: the nuclei, and the level it found |
| Opened | the same, then `morphology.open` with a disk of 2: specks gone |
| The distance transform | the same, then `morphology.distance`: brighter further inside a nucleus |
| Nuclei, labelled | the same, then `morphology.label`: each nucleus a colour of its own, and how many |

The Gaussian, median and bilateral filters and CLAHE have CUDA twins (`oscope-compute`'s `kernels/filters.cu`), which give what the CPU's do: to the bit, but for the bilateral filter's exponentials.

## How to try it

```sh
cargo run --release -p ex-filters
```

It renders a 1024 × 768 field through the 10× objective, runs each card's recipe, writes the images and `filters.json` to `.oscope/ex-filters/data/page/`, and prints each card's times, then `ex-filters: open http://127.0.0.1:8214/`.

Port 8214 is `ex-demosaic`'s too: the range for manual checks is used up. Run one at a time.

**With the GPU,** with the CUDA toolkit: `cargo run --release -p ex-filters --features cuda`. The cards with CUDA twins then show both times. A GPU time includes the copies to the GPU and back, as a recipe on the GPU starts and ends in memory.

**On the owner's PC** (release, 24 threads, an RTX 3070 Ti laptop GPU), at 1024 × 768:

| Card | CPU | GPU, with its copies |
|---|---|---|
| Gaussian | 9.2 ms | 3.2 ms |
| Median | 21.7 ms | 4.9 ms |
| Bilateral | 79.8 ms | 6.5 ms |
| Unsharp mask | 9.4 ms | |
| CLAHE | 5.5 ms | 3.0 ms |
| Threshold | 2.4 ms | |
| Otsu's threshold | 4.3 ms | |
| Opened | 5.8 ms | |
| The distance transform | 11.6 ms | |
| Nuclei, labelled | 8.2 ms: 118 nuclei | |

Nuclei that touch share a label: separating them needs a watershed, later.

## `--smoke`

```sh
cargo run -p ex-filters -- --smoke
```

It runs on a 256 × 192 field with a temporary data root, and checks that the page, the field, the cards and `filters.json` are served, and that nuclei were found. It ends with `smoke: ok`, or says what failed and exits with code 1.
