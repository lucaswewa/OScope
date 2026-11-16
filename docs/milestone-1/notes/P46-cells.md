# P46: Cell detection and counting

- Status: Done
- Branch / PR: `m1/p46-cells` / [#62](https://github.com/lucaswewa/OScope/pull/62)
- Size: L · estimated ~2,750 before it started (in `oscope-imaging`, an adaptive threshold, the distance transform and labelling moved from the pipeline, a seeded watershed, each object's measures and outline, ~550; the pipeline's nodes ~200; in `oscope-workflows`, the counting job over a stored slide, tile by tile with a halo, its table, outlines and annotation layer, ~400; a stub Python extension ~80; tests ~900; `ex-count-cells` and its page ~550; glue and `ex-checkpoint-d`'s wait ~70); one pull request, at the owner's standing choice · measured **2,728** hand-written lines added (code 1,341: `oscope-workflows` 531, `oscope-imaging` 520, `oscope-pipeline` 258, `oscope-store` 28, `oscope-api` 4; tests 888; examples 473, the stub's Python and manifest among them; glue 26), and 173 removed, most of them the code moved out of the pipeline. **That is 1,828 over L's 900, and 1 % under the estimate.**

## Summary

Nuclei found, split where they touch, measured and counted, on whole slides ([spec](../phases.md#p46); plan §5.17, D25).

- **Segmentation:**
  - on the hematoxylin channel (P45's colour deconvolution) or on grey;
  - by Otsu's level, a level given, or an adaptive threshold;
  - holes filled, and touching nuclei split by a seeded watershed on the distance transform.
- **Each object's measures:** area, perimeter, circularity, centroid in µm, and channels' means. Each outline is GeoJSON in µm.
- **Three pipeline nodes,** and **a job, `cells.count`,** which works a stored slide tile by tile with a halo. It counts each nucleus once, in the tile that holds its centroid. It makes a table, the outlines and a summary, and keeps the outlines as an annotation layer on the slide (P44). On an H-DAB slide it classes each nucleus by its DAB (P45).
- **A stub Python extension** shows where cellpose or StarDist plugs in.
- **`ex-count-cells`** counts the nuclei of a slide stitched at 20×. Against the simulator's nuclei: recall 0.995 and precision 0.994, a median miss of 0.28 µm.

## Behaviour spec

Written before the code, from the sources at its end ([spec](../phases.md#p46); plan §5.17, D25).

### What it does

1. **Its input is a channel to segment,** one value a pixel, where nuclei are high:
   - **brightfield,** the hematoxylin amount by colour deconvolution (P45), the white given or found as P45 finds it;
   - **fluorescence,** the image's grey, its channels' mean.

   A raw image is taken on its 2 × 2 cells, as P45 takes it.
2. **A mask, by a threshold:**
   - **global:** a level given, or Otsu's (1979) over the channel's values;
   - **adaptive:** a pixel is set when it exceeds the mean of the square window about it, of side 2r + 1, by an offset. The mean comes from a summed-area table (Crow, 1984), and the window is cut at the image's edges.

   Holes in an object are filled, so a nucleus with a pale middle is one object.
3. **The distance transform** of the mask, Euclidean and exact (Felzenszwalb and Huttenlocher, 2012). P23's, moved into `oscope-imaging`.
4. **The seeded watershed** (Beucher and Meyer, 1993; Meyer's flooding, 1994) splits touching nuclei:
   - **Seeds:** the distance transform's regional maxima, at least `min_distance` from the background. A pixel is a peak when no pixel within `seed_radius` is higher. A plateau's peaks are one seed.
   - **Flooding:** from the seeds, through the mask, the deepest pixels first, by a priority queue on the distance; ties go first come, first served. Each pixel takes the label of the neighbour that reached it, and nothing leaves the mask.
   - **A component without a seed,** too thin to have one, is one object.
5. **Each object's measures:**
   - **area:** its pixels, in µm²;
   - **perimeter:** the length of the pixel edges between it and what isn't it, times π/4, an unbiased estimate for boundaries at random angles (Kulpa, 1977). In µm;
   - **circularity:** 4π · area / perimeter², 1 for a disc;
   - **centroid:** its pixels' mean, in µm from the image's top-left corner;
   - **intensity:** each channel's mean over it, and the segmented channel's.
6. **Filters:** objects outside `min_area`–`max_area` (µm²), or under `min_circularity`, are dropped. The defaults are 10 µm², 200 µm² and 0.4.
7. **Outlines:** each object's outer boundary along its pixels' edges, as a GeoJSON polygon in µm (P44's coordinates). Straight runs are merged, and holes are left out.
8. **The pipeline's nodes:**
   - **`threshold.adaptive`:** an image → a mask. It works pixel by pixel within its window, so it runs tile by tile with a halo of r.
   - **`cells.split`:** a mask → labels, an image of object numbers. Its parameters are `seed_radius` and `min_distance`.
   - **`cells.measure`:** labels and an image → a table of the objects' measures, and their outlines as GeoJSON. Its parameters are `pixel_um` and the filters.
9. **The job `cells.count`,** a compute job, takes {`project`, `input`, `options`}, over a stored image read in tiles with a halo of 32 cells.
   - **Options:** `channel` (`hematoxylin` or `grey`), the threshold, the watershed's parameters, the filters, and `dab`.
   - **A first pass** keeps every 8th pixel each way. It finds the white, and Otsu's level for a global threshold.
   - **A second pass** segments each tile with its halo. It keeps the objects whose centroid lies in the tile's core, so none is counted twice and none left out.
   - **With `dab`,** each nucleus is classed by its mean DAB amount, in P45's bins.
   - **Its result,** of the project, has no image. It holds `cells.csv`, a row per object; `cells.geojson`; and `summary.json`: the count, the density per mm² of tissue, the mean area and circularity, and the DAB classes' counts.
   - **The outlines** are also kept as an annotation layer, `cells`, on the image counted (P44), where its measurements and the example's page find them.
   - Progress counts tiles, and the job is cancelled between them.
10. **A stub Python extension,** `examples/ext-python-cells`, shows where a learned segmenter plugs in. It has a node, `example.cells_segment`, whose code calls cellpose or StarDist when either is installed. Otherwise it gives a stand-in: a mask of the pixels above the image's mean.
11. **`ex-count-cells`** (port 8232):
    - the H&E slide scanned at 20× and stitched;
    - its nuclei counted by the job, on the hematoxylin channel;
    - the count, its density, and the match to the simulator's nuclei printed. The match is greedy, nearest first, within 3 µm, after the mosaic's offset from the stage is found by the best shift within 10 µm;
    - **the page** draws the outlines over the mosaic in OpenSeadragon, with the counts.

### What the tests show

- **Thresholds:** Otsu's level between two populations; the adaptive threshold finds spots on a sloping background that a global one can't; holes filled.
- **The watershed:**
  - two overlapping discs give two objects, each near its own disc's area;
  - a dumbbell of two discs on a thin neck is split at the neck;
  - a single disc stays one object, however its plateau ties;
  - nothing leaves the mask.
- **Measures:**
  - discs of known radius: area within 2 %, perimeter within 5 %, circularity within 0.1 of 1;
  - a square's centroid exact;
  - channel means;
  - the filters drop what they should.
- **Outlines:** a square's outline is its four corners in µm, and a disc's encloses its pixels' area.
- **Against the simulator's H&E truth** (P10), rendered at known places at 20×'s cells and segmented on the hematoxylin channel:
  - recall and precision of 0.9 or more, matching within 3 µm;
  - a matched nucleus's centroid within 1 µm of its centre, as the median;
  - touching pairs of nuclei in the truth, centres closer than their long axes, are split as often as not.
- **The nodes** give what the library gives; `threshold.adaptive` gives the same tiled or whole.
- **The job:**
  - a stored image, the same count whatever its tiles, with no object counted twice across their seams;
  - its table, outlines, annotation layer and summary;
  - H-DAB classes;
  - a cancel;
  - refusals.
- **The example's smoke:** recall and precision of 0.85 or more against the truth, and the page.

### Sources

- Otsu, N. (1979). A threshold selection method from gray-level histograms. *IEEE Transactions on Systems, Man, and Cybernetics* 9(1), 62–66.
- Crow, F. C. (1984). Summed-area tables for texture mapping. *Computer Graphics* 18(3), 207–212.
- Felzenszwalb, P. F. and Huttenlocher, D. P. (2012). Distance transforms of sampled functions. *Theory of Computing* 8, 415–428.
- Beucher, S. and Meyer, F. (1993). The morphological approach to segmentation: the watershed transformation. In *Mathematical Morphology in Image Processing*, 433–481. Marcel Dekker.
- Meyer, F. (1994). Topographic distance and watershed lines. *Signal Processing* 38(1), 113–125.
- Kulpa, Z. (1977). Area and perimeter measurement of blobs in discrete binary pictures. *Computer Graphics and Image Processing* 6(5), 434–451.
- Rosenfeld, A. and Pfaltz, J. L. (1966). Sequential operations in digital picture processing. *Journal of the ACM* 13(4), 471–494: the labelling, P23's.

## What was built

- **`oscope-imaging::cells`:**
  - `otsu`, over any range of values;
  - `adaptive`, by a summed-area table;
  - `fill_holes`;
  - `distances` and `components`, moved from the pipeline's morphology, which now calls them;
  - `watershed`, its seeds the distance's peaks and a plateau's peaks one seed, flooded deepest first through a binary heap;
  - `segment`, the chain, with `Threshold`;
  - `measure` and `Object`;
  - `Filters`;
  - `outline`, along pixels' edges, its walk bounded.
- **The pipeline:** `threshold.adaptive` (its halo its radius), `cells.split` and `cells.measure` (a table and GeoJSON).
- **`oscope-store`:** `Store::replace_layer`, a whole annotation layer at once, for thousands of outlines.
- **`oscope-workflows::cells`:** `count` and the job factory `count_jobs` (`cells.count`), registered by the `projects` Thing. P45's colour reader, `Colour`, is shared, and reads grey images too.
- **`examples/ext-python-cells`,** the stub, excluded from the workspace like the other Python samples. It has a test, gated by `OSCOPE_PYTHON=1`, that packs, installs and runs it.
- **`ex-count-cells`** (port 8232), its page and README.
- **`ex-checkpoint-d`** waits for its last tile to be drawn, as P45's notes promised.

## How to try it

```sh
cargo run --release -p ex-count-cells
```

Then open http://127.0.0.1:8232/. It takes about 26 s at real time, and prints:

```text
ex-count-cells: a snake over 1.00 × 0.75 mm at 20× across the island's edge, stitched; 4441 nuclei counted, 4586 per mm² of tissue
  against the simulator's 3216 nuclei, the mosaic -0.3, -0.0 µm from where its first frame says: recall 0.995, precision 0.994, the median miss 0.28 µm
  open http://127.0.0.1:8232/ to see their outlines
```

The page draws the 4,441 outlines over the mosaic, with the summary and the table to download. The [README](../../../examples/ex-count-cells/README.md) gives the job's options.

## Performance

§5.20 has no budget for counting. On this PC, with a release build, the job counted the example's 4,441 nuclei in 0.29 s. The mosaic is 2596 × 1728 raw pixels, 1.1 megapixels of cells, and the time covers both passes, the files and the layer. A 200-megapixel raw slide would take some 15 s.

## Design notes and deviations from the plan

- **The job counts on 2 × 2 cells** of a raw slide, as P45 does: at 20×, 0.96 µm cells, so nuclei of 5.6 to 9.2 µm across are 6 to 10 cells. Recall and precision are 0.99 against the truth even so.
- **The example matches through an offset found first:** the stitch doesn't record where its mosaic lies on the stage. Placed by its first frame's recorded position, the mosaic was 0.3 µm off, under the 10 µm the search allows.
- **A tile's core is 256 cells, its halo 32** (31 µm at 20×), wider than any nucleus. So a nucleus whose centroid lies in a core lies whole in its tile. The job's test puts nuclei on the seams, and counts each once.
- **The outlines are kept as a layer of the slide,** `cells`, replaced at each count, so P44's routes measure them and pages draw them. Keeping them one by one, through `annotate`, would rewrite the file for each.
- **DAB's classes come from each nucleus's mean DAB,** P45's bins applied to the nucleus. P45's percentage is of tissue area; this is of nuclei.
- **The stub's learned paths are called through each library's documented interface,** and neither library is a dependency. The test runs the stand-in, as the CI has neither.
- **The spec held:** no revisions were needed while writing the tests, beyond test data. A sanity bound of 300 nuclei needed a larger region in the truth test, and the job test's ground was set to count as tissue.

### What the mutants' reading found

Thirty-three mutants, listed in advance from the code, covering:

- the distance transform and labelling, Otsu's level, the adaptive window, hole filling;
- the watershed's seeds, flooding and unseeded parts;
- the measures, the filters and the outline's turns;
- the nodes' halo, colour and filling;
- the job's halo, seams, white, tissue, DAB classes, layer, refusals and cancels;
- the layer's IDs.

Reading the tests against the list first found gaps, now covered:

- Otsu's first split across a gap;
- a background open only at the edge, which isn't a hole;
- plateau-peaked squares, which only plateau seeds can split;
- unequal discs, split at their waist;
- `segment` itself;
- the adaptive threshold in colour, and a hollow disc for `cells.split`;
- the job's white found, its density, grey's tissue and a first-pass cancel.

The outline's walk got a bound, so that a wrong start couldn't loop for ever.

The run then missed nine:

- **Fixed in the tests (eight):**
  - an anti-diagonal line, to keep eight-connected labelling's up-right neighbour;
  - a square's border missing one corner pixel, whose inside meets the outside only at that corner;
  - the unequal discs' split located at their waist, x = 36.5, where breadth-first flooding would put it half-way;
  - a filter by the largest area;
  - the adaptive node on tiles of 8 pixels, smaller than its window, so that only its halo makes them agree;
  - channels whose mean is flat, where the first channel's isn't;
  - glass in the job's picture, brighter than its ground, so the white found is the glass's and not the median;
  - white glass in its grey picture, which only grey's rule counts as tissue.
- **Equivalent (one):** popping a parabola that ties the envelope's last bound, or keeping it, gives the same distances.

One mutant was too weak and was redone: flooding by the parent's depth rather than one's own is still deepest first, a step behind, so it splits at the same waist. True breadth-first flooding is caught.

So **32 of 33 are caught,** and one is equivalent.

### The page

The page wasn't seen in a browser here. Under Node, against the running example, with a stand-in DOM and OpenSeadragon, it:

- found the mosaic and the count;
- drew all 4,440 outlines in that run;
- cleared them when *Outlines* was unticked;
- gave the summary in its heading;
- linked the table, which answered 200 with 4,440 rows.

### Two cores

`scripts/two-core.sh`, five times each, side by side, on two CPUs each. P45's suites and example are among them, for the shared reader:

| Target | Passed | Time |
|---|---|---|
| `oscope-imaging:cells` | 5 of 5 | 0.6 s |
| `oscope-sim:nuclei` | 5 of 5 | 0.5–0.7 s |
| `oscope-pipeline:stock` | 5 of 5 | 1.2–1.3 s |
| `oscope-pipeline:filters` | 5 of 5 | 0.5–1.1 s |
| `oscope-store:annotations` | 5 of 5 | 7.3–7.7 s |
| `oscope-workflows:cells` | 5 of 5 | 9.9–11.5 s |
| `oscope-workflows:stain` | 5 of 5 | 6.7–7.3 s |
| `ex-count-cells --smoke` | 5 of 5 | 25.5–28.6 s |
| `ex-checkpoint-d --smoke` | 5 of 5 | 59.1–66.8 s |
| `ex-stain --smoke` | 5 of 5 | 59.0–77.5 s |

## Tests

| Suite | Tests | What they show |
|---|---|---|
| `oscope-imaging` `cells` (new) | 5 | Otsu's first split across a gap; the adaptive threshold finds spots on a slope, which a global level can't; holes filled, but not a cup open at the edge; the watershed splits overlapping discs, a dumbbell, plateau-peaked squares and unequal discs at their waist; a disc, a bar and a thin line stay one; nothing leaves the mask; `segment` with each threshold; discs' areas, perimeters and circularity; a rectangle's centroid, means and crack; the filters; outlines along the edges, enclosing their pixels exactly |
| `oscope-sim` `nuclei` (new) | 1 | The H&E section rendered at 20×'s cells, segmented on its hematoxylin: 495 nuclei, recall 0.994, precision 0.980, a median miss of 0.11 µm, 47 of 50 touching pairs split |
| `oscope-pipeline` `stock` (one new) | 10 | The nodes give the library's results, in grey and colour; the adaptive threshold the same tiled or whole; a hollow disc filled; the filters' defaults |
| `oscope-store` `annotations` (one new) | 9 | A layer replaced at once, with new IDs; one bad shape refuses them all |
| `oscope-workflows` `cells` (new) | 2 | A slide wider than a tile, nuclei on its seams: each counted once, as the library counts the whole; the table, outlines, layer, summary and tissue; the white found; DAB classes; grey, its tissue the image; refusals; cancels in either pass; the job's parameters |
| `oscope-ext` `python` (one new) | 8 | With `OSCOPE_PYTHON=1`: the stub packed, installed and run, its stand-in's mask |
| `ex-count-cells --smoke` | — | Over 50 nuclei; recall and precision of 0.85 or more against the truth, the offset under 10 µm; an outline each; the page |

**The local replays,** the merge gate (ADR-0038), each run alone on `cca3bcb`. The commit since adds only these lines.

- **Windows:** `scripts/ci-local.sh`, in 10 minutes: 62 of 62 steps, 577 tests in 214 suites, the Python tests real (the stub's among them), and every example's `--smoke`. No CUDA path changed, so the CUDA tests didn't run.
- **Linux:** `scripts/ci-clone.sh` on a fresh WSL clone, in 7 minutes: 62 of 62 steps, 577 tests in 214 suites; 793 files, none with a CR.

## Follow-ups and known gaps

- **Full-resolution counting,** demosaiced rather than on 2 × 2 cells, for smaller nuclei or lower magnifications.
- **Cytoplasm and membranes:** cells grown from their nuclei, for whole-cell measures.
- **A learned segmenter in earnest:** the stub's paths, run on a machine with cellpose or StarDist.
- **The mosaic's place on the stage,** recorded by the stitch, would spare the example its offset search.

## Clean room

Nothing was copied from `microscope` or any other codebase. The methods are the published ones in the spec's sources, written from their descriptions: Otsu's level, summed-area tables, Felzenszwalb and Huttenlocher's distance transform (P23's), Rosenfeld and Pfaltz's labelling (P23's), the watershed after Beucher and Meyer and Meyer's flooding, and Kulpa's perimeter correction. The stub calls cellpose and StarDist through their own published interfaces; neither is copied or distributed.
