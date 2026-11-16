# P45: Stain analysis

- Status: In review
- Branch / PR: `m1/p45-stain` / [#61](https://github.com/lucaswewa/OScope/pull/61)
- Size: M · estimated ~2,500 before it started (in `oscope-imaging`, optical density, stain vectors and colour deconvolution, Macenko's estimate with a 3 × 3 eigensolver, and the DAB bins, ~350; in `oscope-sim`, the H-DAB section and the stains' amounts as truth, ~120; the pipeline's four nodes ~220; in `oscope-workflows`, the job over a stored slide, tile by tile, ~320; tests ~850; `ex-stain` and its page ~600; glue ~40); one pull request, at the owner's standing choice · measured **2,410** hand-written lines added (code 1,120: `oscope-workflows` 426, `oscope-imaging` 378, `oscope-pipeline` 226, `oscope-sim` 86, `oscope-api` 4; tests 892; examples 373; glue 25), and 20 removed. **That is 1,810 over M's 600, and 4 % under the estimate.**

## Summary

Brightfield stains separated, and DAB quantified, on whole slides ([spec](../phases.md#p45); plan §5.17, D25).

- **Colour deconvolution** (Ruifrok and Johnston, 2001) gives each pixel's amount of each stain: H&E, H-DAB, or vectors given. Macenko's method (2009) estimates two stains' vectors from the image if asked.
- **DAB is binned** as negative, weak, moderate or strong, giving the positive percentage of the tissue and the H-score.
- **Four pipeline nodes,** two of which run tile by tile.
- **A job, `stain.separate`,** works a stored slide tile by tile. It makes three results: the stains' amounts, and each stain's image. It also writes a summary.
- **The simulator** has an H-DAB section beside its H&E one, and gives both sections' stain amounts as truth.
- **`ex-stain`** separates a stitched H&E slide's stains, by the standard vectors and by Macenko's. It then quantifies an H-DAB slide's DAB.

## Behaviour spec

Written before the code, from the sources at its end ([spec](../phases.md#p45); plan §5.17, D25).

### What it does

1. **Its input is a brightfield image in colour:** red, green and blue, the light each pixel passed. A raw image, a camera's Bayer mosaic, is taken on its 2 × 2 cells, each one RGB pixel, at the cells' size.
2. **Optical density,** channel by channel, against the white, the light through glass: OD = −log₁₀(I / W).
   - I is held at W/1024 or more, so OD is 3.01 at most.
   - **The white** is given, or else estimated: each channel's 99.5th percentile over the image's pixels.
3. **Stain vectors** are a stain's optical densities in red, green and blue for one unit of it, normalised to length 1 (Ruifrok and Johnston, 2001):
   - **hematoxylin:** (0.650, 0.704, 0.286);
   - **eosin:** (0.072, 0.990, 0.105);
   - **DAB:** (0.268, 0.570, 0.776);
   - the user's own, two or three.

   **H&E** is hematoxylin and eosin; **H-DAB** is hematoxylin and DAB. With two stains, the third vector is the cross product of the two, normalised. It's a residual: what neither stain explains.
4. **Colour deconvolution** (Ruifrok and Johnston, 2001):
   - By Beer and Lambert, a pixel's OD is its stains' amounts times their vectors: OD = a · M, the rows of M being the vectors.
   - So its amounts are a = OD · M⁻¹, each in units of its stain's vector.
   - **Refused:** vectors that aren't independent (|det M| under 10⁻³), or that have a component that is negative or not finite. The residual's components may be negative.
5. **Macenko's estimate** of two stains' vectors from the image (Macenko et al., 2009):
   - The pixels used are those whose optical densities, as a vector, have a length of at least β = 0.15: glass and the faintest stain are left out. (Revised while writing the tests: requiring β in every channel would leave out eosin alone, whose red and blue densities are small, so its vector couldn't be found.)
   - Their OD vectors' second moments, about the origin, have three eigenvectors (Jacobi's method). The two with the largest eigenvalues span the plane of the stains.
   - Each pixel's OD is projected on that plane, at angle φ. The stains' vectors are at φ's 1st and 99th percentiles, so outliers don't decide them.
   - Each vector is turned so its components sum positive. The one with more red OD comes first: hematoxylin, then eosin or DAB.
   - **Refused** if fewer than 100 pixels are dense enough.
6. **DAB quantification,** from an H-DAB deconvolution:
   - **Tissue** is a pixel whose OD, summed over its channels, is at least 0.05; glass isn't counted.
   - **Each tissue pixel is binned** by its DAB amount: negative under 0.15, weak from 0.15, moderate from 0.35, strong from 0.60.
   - **What it gives:** the counts of tissue pixels in each bin; the percentage of tissue that is positive (weak, moderate or strong); and the H-score, 1 × weak % + 2 × moderate % + 3 × strong %, from 0 to 300 (McCarty et al., 1985).
   - The thresholds can be given.
7. **Each stain's image,** the stain alone as it would look: W × 10^(−a·v) for its amount a and vector v, as 8-bit RGB.
8. **The pipeline's nodes:**
   - **`stain.deconvolve`:** an RGB image → its stains' amounts, an image of three channels (the first stain, the second, the residual). Its parameters are `stains` (`he`, `hdab` or `vectors`) and `white`, [1, 1, 1] by default. It works pixel by pixel, so it runs tile by tile.
   - **`stain.estimate`:** an RGB image → JSON of Macenko's two vectors, with `white`, `beta` and `percentile`. It needs the whole image.
   - **`stain.dab`:** an RGB image → a mask of classes, with `white`, `stains` and the thresholds. The classes are 0 for glass, 1 negative, 2 weak, 3 moderate and 4 strong. It works pixel by pixel, so it runs tile by tile.
   - **`stain.dab_score`:** a mask of classes → a table of the counts, the positive percentage and the H-score. A count, cheap over a whole slide.
9. **The job `stain.separate`,** a compute job, takes {`project`, `kind`, `id`, `options`}, over a stored image read tile by tile in its chunks.
   - **Options:** `stains`, `vectors`, `estimate` (Macenko's), `white` and the DAB thresholds.
   - **A first pass** keeps every 8th pixel each way, for the white and Macenko's estimate where they're asked for.
   - **A second pass** deconvolves each tile into three results of the project:
     - the amounts, 32-bit floats, its channels named by stain;
     - each stain's image, 8-bit RGB;
     - in the amounts' folder, `summary.json`: the white, the vectors, each stain's mean amount over tissue, the residual's RMS, and for H-DAB the DAB counts, percentage and H-score.
   - Progress counts tiles, and the job is cancelled between them.
   - **Refused:** an image that isn't RGB or raw, and stains it doesn't know.
10. **The simulator's H-DAB section,** "H-DAB tissue", is the H&E section's islands and nuclei, stained as an immunohistochemical slide:
    - hematoxylin as in H&E, no eosin;
    - DAB in 40 % of the nuclei, at three levels, weak, moderate and strong, in equal numbers, which land in the bins above;
    - DAB's vector as above.

    Both sections give each stain's amount at any point, summed over the section's depth, as the tests' truth.
11. **`ex-stain`** (port 8231):
    - **H&E:** the H&E slide scanned and stitched, separated with the H&E vectors and with Macenko's estimate. It prints the white, the vectors and their angles from the H&E ones, the stains' mean amounts, and the residual's RMS.
    - **H-DAB:** the H-DAB slide loaded, scanned, stitched and quantified. It prints the bins' counts, the positive percentage and the H-score.
    - **The page** shows a mosaic or one stain's image in OpenSeadragon, chosen from a list, beside each summary.

### What the tests show

- **Deconvolution:**
  - **Synthetic pixels** made from known amounts by Beer–Lambert give their amounts back within 10⁻⁵, for H&E, H-DAB and three vectors given, and within 10⁻² once quantised to 8 bits.
  - **The simulator's H&E and H-DAB sections,** rendered at known places, give back its amounts of each stain within 10⁻⁴, pixel by pixel. (Tightened from 10⁻³ while checking the tests: 10⁻³ couldn't tell the vectors' lengths, which differ from 1 by up to 0.2 %.) DAB lies only in the positive nuclei.
- **Macenko's estimate:**
  - from synthetic two-stain images with spread amounts, within 2° of the vectors;
  - from the simulator's H&E section, within 7°. (Revised while writing the tests from 5°: the section's nuclei hold eosin and its stroma hematoxylin, so no pixel is one stain alone, and each vector leans towards the other, 5.7° and 5.3°.)
  - from the simulator's H-DAB section, hematoxylin within 1°, and DAB within 20°. (Added while writing the tests: DAB lies only in nuclei that hold hematoxylin too, so its vector leans 16.5° towards hematoxylin. So the job uses the stains' own vectors unless asked to estimate.)
- **DAB:** synthetic patches with known fractions of each bin give their counts, percentage and H-score exactly, and pixels at the thresholds fall in the bins above them.
- **The nodes** give what the library gives, and `stain.deconvolve` and `stain.dab` give the same tiled or whole.
- **The job:**
  - over a stored RGB image and a raw one, its results recorded, with the amounts the library gives;
  - its summary, and a cancel;
  - refusals.
- **The example's smoke:**
  - the H&E slide's residual is under a tenth of its stains' mean amounts, and under half of what H-DAB's vectors leave there (made definite while writing the example);
  - Macenko's vectors are within 7° of H&E's;
  - the H-DAB slide's positive percentage is between 1 % and 40 %, and its H-score between one and three times it (made definite while writing the example: 40 % of the nuclei are positive, and they cover only part of the tissue).

### Sources

- Ruifrok, A. C. and Johnston, D. A. (2001). Quantification of histochemical staining by color deconvolution. *Analytical and Quantitative Cytology and Histology* 23(4), 291–299.
- Macenko, M., Niethammer, M., Marron, J. S., Borland, D., Woosley, J. T., Guan, X., Schmitt, C. and Thomas, N. E. (2009). A method for normalizing histology slides for quantitative analysis. *Proceedings of the IEEE International Symposium on Biomedical Imaging*, 1107–1110.
- McCarty, K. S. Jr., Miller, L. S., Cox, E. B., Konrath, J. and McCarty, K. S. Sr. (1985). Estrogen receptor analyses: correlation of biochemical and immunohistochemical methods using monoclonal antireceptor antibodies. *Archives of Pathology and Laboratory Medicine* 109(8), 716–721: the H-score.
- Golub, G. H. and Van Loan, C. F. (2013). *Matrix Computations*, 4th edition, §8.5: Jacobi's method for a symmetric matrix's eigenvalues.

## What was built

- **`oscope-imaging::stain`:**
  - `optical_density`;
  - `Stains`: the vectors, a residual made for two, the inverse, `deconvolve` and `alone`;
  - `StainChoice` (`he`, `hdab`, `vectors`);
  - `macenko`, with Jacobi's method for the 3 × 3 second moments;
  - `DabThresholds`, `dab_class`, `dab_classes` and `DabCounts`, with its positive percentage and H-score.
- **`oscope-sim`:**
  - `HeTissue::ihc`, the H-DAB section, a sample kind `ihc-tissue` named "H-DAB tissue";
  - each `Nucleus`'s `dab`;
  - `HeTissue::stain_amounts`, the truth.

  The H&E section renders as before, to the bit: the second stain is chosen, its density's expression unchanged.
- **The pipeline:** `stain.deconvolve` and `stain.dab` run pixel by pixel, and so by tiles; `stain.estimate` and `stain.dab_score` need the whole image.
- **`oscope-workflows::stain`:** `separate` and the job factory `stain_jobs` (`stain.separate`), registered by the `projects` Thing. A raw image is read on its 2 × 2 cells, a chunk's worth at a time.
- **`ex-stain`** (port 8231), its page and README. `ex-sim-samples` and two tests now count seven built-in samples.

## How to try it

```sh
cargo run --release -p ex-stain
```

Then open http://127.0.0.1:8231/. It takes about 34 s at real time, and prints:

```text
ex-stain: H&E, a snake over 2.0 × 1.5 mm across the island's edge, stitched; the white 0.752, 0.751, 0.753
  H&E's vectors: hematoxylin 0.159, eosin 0.225 over the tissue; the residual's RMS 0.0292
  Macenko's: hematoxylin 1.5° and eosin 6.0° from H&E's
ex-stain: H-DAB, the same place: of 1091331 tissue pixels, 1003531 negative, 46287 weak, 24845 moderate, 16668 strong
  DAB positive 8.0 % of the tissue, H-score 13
  open http://127.0.0.1:8231/ to see the mosaics and their stains
```

The page lists the mosaics and each stain's image to show in OpenSeadragon, with the three separations' summaries. The [README](../../../examples/ex-stain/README.md) gives the job's parameters.

## Performance

§5.20 has no budget for stains. On this PC, with a release build, the job separates the example's mosaic in 0.78–0.82 s over three runs. The mosaic is 2598 × 1730 raw pixels, so 1.1 megapixels of cells. That time covers both passes and three results written with their pyramids. A 200-megapixel raw slide would take about a minute.

## Design notes and deviations from the plan

### Where the spec moved

All were revised while writing the tests or the example, and the spec above says where:

- **Macenko's pixels** are those whose densities, as a vector, are at least β long. Requiring β in every channel would leave out eosin alone: the simulator's stroma has a red density of about 0.07.
- **Macenko on the simulator** comes within 7° of H&E's vectors, not 5°: 5.7° and 5.3°. Each vector leans towards the other, as no pixel is one stain alone.
- **Macenko on the H-DAB section** finds hematoxylin exactly, but DAB only within 20° (16.5°): DAB lies only in nuclei that hold hematoxylin too. So the job uses a stain set's own vectors unless asked to estimate.
- **The simulator's truth** is held to 10⁻⁴, not 10⁻³, as 10⁻³ couldn't tell the vectors' lengths, which differ from 1 by up to 0.2 %.
- **The 8-bit test** keeps its densities under 1, as the spec says: green, the densest, at 0.93.
- **The example's smoke** has definite bounds:
  - the residual is under a tenth of the stains' amounts, and under half of what H-DAB's vectors leave on the H&E slide;
  - the positive percentage is between 1 % and 40 %, and the H-score between one and three times it.

### Choices

- **The job's input** is `{"kind": "result", "id": …}` or an acquisition's, the form its provenance records.
- **Results' names** are the slide's, with "(Macenko)" for an estimate; the example names its scans "H&E slide" and "H-DAB slide".
- **The white,** if not given, is each channel's 99.5th percentile. So a slide needs some glass, which the example's scan across the island's edge has. Without glass, the brightest tissue would read as clear.
- **The residual's RMS** measures the fit. On the camera's mosaic it is 0.029, about 7 % of the stains' amounts: camera noise, and the 2 × 2 cells mixing neighbouring sites' colours. H-DAB's vectors leave more.
- **DAB's percentage is of the tissue's area,** not of nuclei: 8 % here, where 40 % of the nuclei are positive. Counting positive nuclei is P46's.

### What the mutants' reading found

Fifty-four mutants, listed in advance from the code: of the densities, the vectors and their inverse, Jacobi's rotations, Macenko's steps, DAB's bins, the simulator's DAB, the nodes' defaults, the job's cells, passes, sums, images and cancels, and the job's registration. Reading the tests against the list found gaps, now covered:

- **Three vectors chosen** weren't tested through `StainChoice`.
- **Macenko's percentile** gave what the extremes give on clean data. Twenty outlying pixels now bend the extremes over 5°, and the percentile not at all.
- **DAB beyond the nuclei** wouldn't fail the pixel test, as the render and the truth share their code. A grid of points now finds DAB only within positive nuclei.
- **The truth's 10⁻³** couldn't see the vectors' lengths; it is 10⁻⁴ now.
- **The nodes' default white,** H&E unbinned, both stains' images, the means, and the "(Macenko)" names weren't tested.

The run then missed four:

- **Tissue from any density, and no cancel in the first pass,** now caught. A faint band under tissue's density, and a check that a first-pass cancel tells nothing of the second, were added.
- **A raw tile a chunk of cells, not half:** equivalent. The tiles are clipped to the image, so only their alignment to the chunks, and so the speed, changes.
- **The final turn of a vector to sum positive:** not exercised. Jacobi's rotations of under 45° leave the dominant eigenvector positive on every set of densities tried, so its vectors come out positive already. The turn stays, as the spec says it.

So **52 of 54 are caught,** one is equivalent, and one guards a sign never met.

### The page

The page wasn't seen in a browser here. Under Node, against the running example, with a stand-in DOM and OpenSeadragon, it:

- listed the eight images, the two mosaics and the six stain images;
- showed the three summaries, the H-DAB one with its 8.1 % and H-score;
- opened the first mosaic at 2598 × 1730;
- on another choice, replaced it in place, the new image's tile answering 200.

### Two cores

`scripts/two-core.sh`, five times each, side by side, on two CPUs each:

| Target | Passed | Time |
|---|---|---|
| `oscope-imaging:stain` | 5 of 5 | 0.4 s |
| `oscope-sim:stain` | 5 of 5 | 0.7–0.8 s |
| `oscope-sim:loader` | 5 of 5 | 0.3–0.6 s |
| `oscope-pipeline:stock` | 5 of 5 | 1.3–1.4 s |
| `oscope-workflows:stain` | 5 of 5 | 5.8–6.6 s |
| `oscope-api:devices` | 5 of 5 | 18.5–19.9 s |
| `ex-stain --smoke` | 5 of 5 | 65.1–77.5 s |
| `ex-sim-samples --smoke` | 5 of 5 | 1.6–1.7 s |

## Tests

| Suite | Tests | What they show |
|---|---|---|
| `oscope-imaging` `stain` (new) | 5 | Amounts made by Beer–Lambert given back, for H&E, H-DAB and three vectors, and through 8 bits; the residual square to the stains; the floor; refusals; each stain alone; Macenko's vectors from synthetic mixtures, robust to outliers, which the extremes aren't; the stains chosen; DAB's bins at their thresholds, a patch's counts, percentage and H-score |
| `oscope-sim` `stain` (new) | 3 | Both sections' amounts given back pixel by pixel within 10⁻⁴; Macenko on both sections; DAB in 40 % of the nuclei, at three levels in equal shares, only within positive nuclei, each level's median in its bin |
| `oscope-sim` `loader` (changed) | 3 | Seven built-in samples |
| `oscope-pipeline` `stock` (one new) | 9 | The nodes give the library's results, with their defaults, and the same tiled or whole; refusals |
| `oscope-workflows` `stain` (new) | 4 | An RGB slide's results are the library's, with both stains' images, the means, DAB's counts and the summary, and progress tile by tile; H&E isn't binned; a raw slide on its cells, the white found; Macenko's on the job's sample; cancels in either pass leave nothing; refusals; the job's parameters |
| `oscope-api` `devices` (changed) | 9 | Seven built-in samples, eight with an import |
| `ex-stain --smoke` | — | The H&E slide's residual, under H-DAB's; Macenko's angles; the H-DAB slide's percentage and H-score; DAB scored on H-DAB alone; the page |
| `ex-sim-samples --smoke` (changed) | — | Nine samples, seven built in |

**The local replays,** the merge gate (ADR-0038), each run alone on `999cb3d`. The commit since adds only these lines.

- **Windows:** `scripts/ci-local.sh`, in 5 minutes: 61 of 61 steps, 566 tests in 210 suites, the Python tests real, and every example's `--smoke`. No CUDA path changed, so the CUDA tests didn't run.
- **Linux:** `scripts/ci-clone.sh` on a fresh WSL clone, in 7 minutes: 61 of 61 steps, 566 tests in 210 suites; 778 files, none with a CR.
- **Two runs before them failed on Windows,** each one step:
  - **`ex-http-walkthrough`'s smoke** counted six built-in samples; it counts seven now (`999cb3d`). The other places that count them had been found by searching for the number written as a length, which this one isn't.
  - **`ex-checkpoint-d`'s smoke,** on `999cb3d`, read the live mosaic's state once as the scan ended, and found 8 of its 9 tiles drawn: the last was still being drawn under the replay's load. P45 doesn't touch scanning or the live mosaic. The smoke passed 5 of 5 on two cores and in the next replay. Its check should wait for the last tile; that fix goes into P46, whose replays run anyway.

## Follow-ups and known gaps

- **`ex-checkpoint-d`'s smoke races its last tile:** it reads the live mosaic's state once, as the scan ends. P46 makes it wait for every tile, with a deadline.
- **Positive nuclei,** counted rather than area: P46's cell detection on the hematoxylin and DAB channels.
- **The white from a blank field,** a calibration's, rather than the slide's brightest pixels.
- **Display of the amounts** themselves, a channel at a time; the page shows each stain's image instead.
- **Stain normalisation:** mapping one slide's stains onto another's, the use Macenko's paper was written for.
- **A DAB vector estimated** where DAB is only nuclear needs pixels of DAB alone; Macenko's method leans towards hematoxylin there.

## Clean room

Nothing was copied from `microscope` or any other codebase. The methods are the published ones in the spec's sources, written from their descriptions:

- colour deconvolution and the stains' vectors (Ruifrok and Johnston, 2001);
- Macenko et al.'s estimate (2009);
- the H-score (McCarty et al., 1985);
- Jacobi's method (Golub and Van Loan, 2013).

The vectors are the published table's numbers.
