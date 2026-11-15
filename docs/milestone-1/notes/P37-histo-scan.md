# P37: Histo scan: overview and tissue detection

- Status: In review
- Branch / PR: `m1/p37-histo-scan` / [#51](https://github.com/lucaswewa/OScope/pull/51)
- Size: {{SIZE}}

## Summary

{{SUMMARY}}

## Behaviour spec

Written before the code, as P37 is spec-first (plan §6). Scan only where there's tissue ([spec](../phases.md#p37); plan §5.14: "a fast binned overview pass, tissue detection, then full-resolution tiles over the tissue only").

### What it does

A histological scan takes three steps.

1. **An overview,** a hardware job:
   - frames one field apart, with no overlap, over a region (the coverslip by default), in a snake;
   - in the mode the camera streams in: binned, ideally, as `bin3` has nine times fewer pixels to take and, on the simulator, to render;
   - each frame reduced to colour, its mosaic's 2 × 2 cells to RGB, then averaged into the overview's pixels, 50 µm by default.

   The frames tile the region, so the overview misses none of it. Changing the camera's mode means restarting its stream, so the overview takes the mode it's given, and the scan after it takes its own.
2. **Tissue detection** on the overview:
   - **Saturation.** Each pixel's S = (max − min) / max of its RGB, as HSV's (Smith, 1978). Stained tissue is coloured, and glass, the coverslip and dust are grey. Saturation doesn't change with the light's level.
   - **A threshold,** by Otsu's method (1979) on S's histogram, the split that most separates two classes, but never under 0.05. On an empty slide, S's histogram is the noise alone, and Otsu would split the noise.
   - **Morphology** (Serra, 1982): a closing, then an opening, each by a 3 × 3 square. The closing fills a pixel's gaps and holes; the opening removes lone pixels.
   - **A minimum area.** Connected components, of 8 neighbours (Rosenfeld and Pfaltz, 1966), under 0.05 mm² are dust or debris, and dropped.
   - **A margin.** What's left is grown by 100 µm, two pixels, so that tissue's edges, which the coarse pixels blur, are inside.

   Thresholding colour, cleaned by morphology, is the traditional method that Bándi et al. (2017) compare learned segmentation against. It is enough to choose where to scan, which needs coverage more than a precise edge.
3. **The scan,** through the engine (P36):
   - a snake over the mask (P35), its tiles at full resolution;
   - **a focus map over the tissue first:** autofocus at the tiles nearest a 3 × 3 grid over them (P35's `measure_map`), and a plane fitted, outliers rejected. The tiles then take the map's z.

   The plan's z policy says so (`measure`), and the engine measures the map before its first tile, so a preview has the tiles but not yet their z.

An empty overview has no tissue, and its scan is refused, saying so.

### What the tests show

- **Coverage:** at least 99 % of the simulated tissue, by the sample's own truth every 50 µm, is in some tile's frame, with fewer tiles than a raster over the same region.
- **An empty slide** gives no tissue, so no tiles: the plan is refused, saying so.
- **The pieces:** Otsu's threshold on a histogram of two classes; the floor on noise alone; the closing and the opening; a component under the minimum dropped; the margin.
- **The measured map:** a scan by it puts each tile within a quarter of the depth of field of the truth, on a tilted slide.

### Sources

- N. Otsu, "A threshold selection method from gray-level histograms", *IEEE Transactions on Systems, Man, and Cybernetics* 9(1), 62–66, 1979.
- A. R. Smith, "Color gamut transform pairs", *Computer Graphics* (SIGGRAPH '78) 12(3), 12–19, 1978.
- J. Serra, *Image Analysis and Mathematical Morphology*, Academic Press, 1982.
- A. Rosenfeld and J. L. Pfaltz, "Sequential operations in digital picture processing", *Journal of the ACM* 13(4), 471–494, 1966.
- P. Bándi, R. van de Loo, M. Intezar, D. Geijs, F. Ciompi, B. van Ginneken, J. van der Laak and G. Litjens, "Comparison of different methods for tissue segmentation in histopathological whole-slide images", *IEEE ISBI 2017*, 591–595.

## What was built

{{BUILT}}

## How to try it

{{TRY}}

## Performance

{{PERF}}

## Design notes and deviations from the plan

{{DESIGN}}

## Tests

{{TESTS}}

## Follow-ups and known gaps

{{FOLLOW}}

## Clean room

Nothing was copied from `microscope` or any other codebase. Each method is written from its sources, as the spec above sets them out.
