# P46: Cell detection and counting

- Status: In review
- Branch / PR: `m1/p46-cells` / #62
- Size: L · estimated ~2,750 before it started (in `oscope-imaging`, an adaptive threshold, the distance transform and labelling moved from the pipeline, a seeded watershed, each object's measures and outline, ~550; the pipeline's nodes ~200; in `oscope-workflows`, the counting job over a stored slide, tile by tile with a halo, its table, outlines and annotation layer, ~400; a stub Python extension ~80; tests ~900; `ex-count-cells` and its page ~550; glue and `ex-checkpoint-d`'s wait ~70); one pull request, at the owner's standing choice

## Summary

To come with the code.

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

To come with the code.

## How to try it

To come with the code.

## Design notes and deviations from the plan

To come with the code.

## Tests

To come with the code.

## Follow-ups and known gaps

To come with the code.

## Clean room

To come with the code.
