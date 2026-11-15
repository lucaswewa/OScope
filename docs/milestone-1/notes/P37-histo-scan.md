# P37: Histo scan: overview and tissue detection

- Status: In review
- Branch / PR: `m1/p37-histo-scan` / [#51](https://github.com/lucaswewa/OScope/pull/51)
- Size: M · estimated ~2,000 before it started (the overview ~220; tissue detection ~300; the engine's measured map ~80; the `scan` Thing's `overview` and `tissue` pattern ~150; the simulator's truth ~20; tests ~500; `ex-histo-scan` ~450; glue ~30; and about a tenth on top); one pull request, at the owner's standing choice · measured **1,558** hand-written lines added (code 728: `oscope-workflows` 554, `oscope-api` 168, `oscope-sim` 6; tests 427; examples 382; glue 21), and 22 removed; the `scan` Thing's TD snapshot is generated. **That is 958 over M's 600, and 22 % under the estimate.**

## Summary

Scan only where there's tissue ([spec](../phases.md#p37); plan §5.14), as the behaviour spec below sets out, written before the code.

- **The overview,** a hardware job: frames a field apart over a region, in the camera's mode (binned, ideally), reduced to colour in pixels of 50 µm.
- **Tissue detection:** saturation over Otsu's threshold, never under 0.05; a closing and an opening; pieces under 0.05 mm² dropped; a margin of 100 µm.
- **The `tissue` pattern:** a snake over the tissue found, its z by a focus map the engine measures before the first tile (the new `measure` policy).
- **On the simulator,** 99.89 % of the tissue in 16 × 12 mm about the first island, by its own truth, is covered by 176 tiles where a raster takes 224; across the island's edge, 99.86 % by 38 where a raster takes 56. An empty slide finds no tissue.
- **`ex-histo-scan`** walks it through the API, and its page shows the overview, the tissue and the tiles as they arrive.
- **P36 is marked Done.**

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

- **`oscope_workflows::scan`:**
  - **`overview`** and its job, `scan.overview` (`overview_jobs`, `LastOverview`): an `Overview` of a region's colour, from frames a field apart.
  - **`detect`,** with `TissueOptions`: the `Tissue` in an overview, a `Mask` of it, its threshold, area and pieces. **`otsu`:** the threshold.
  - **`ZPolicy::Measure`** (`columns`, `rows`, `surface`), which the engine turns into a measured map before its first tile.
  - `autofocus`'s helper for a frame's colours, `colour_cells`, shared with white balance.
- **`oscope-sim`:** `HeTissue::is_tissue`, the truth the tests hold coverage to.
- **The `scan` Thing:** the `overview` action (a job), the `last_overview` property (the image in base64, the tissue as runs of pixels), and the `tissue` pattern.
- **`examples/ex-histo-scan`** (port 8241).

## How to try it

`cargo run --release -p ex-histo-scan`, then open http://127.0.0.1:8241/. The [README](../../../examples/ex-histo-scan/README.md) shows what it prints: on the owner's PC, an overview of 49 frames in 16.1 s, 25.5 mm² of tissue in 3 pieces, 38 tiles where a raster takes 56, 99.86 % of the truth covered, and the scan in 123.6 s.

## Performance

No budget in §5.20 applies. On the simulator at real time, the overview of 8 × 6 mm, 49 frames binned 3 × 3, took 16.1 s, and detection a few milliseconds.

## Design notes and deviations from the plan

### The spec's revision, from the tests' first run

**The exposure must suit the mode.** The first coverage test found no tissue: every pixel of the overview was white. Binned 3 × 3, a pixel gathers nine times the light, and the exposure set for full resolution saturated it. The overview takes the camera as it is given, so its caller sets the exposure for the binned mode, as P34's auto exposure does: the tests and `ex-histo-scan` run it after each change of mode. The spec's first step says the mode is the caller's; the exposure is too.

### The overview

- **Frames a field apart** are a snake with no overlap, from P35's planner, at the stage's z. Their cells tile the region, so every overview pixel gets samples.
- **Each cell is placed** through the matrix the field's corners give, so a turned camera's cells land where they are on the stage.
- **The camera's mode is the caller's.** Changing it means stopping and starting the stream, which the `camera` Thing does. So an overview takes the mode it finds, and the scan after it its own.

### Tissue detection

- **Saturation** is (max − min) / max, so the light's level doesn't change it: the tests find the same tissue in light a tenth as bright.
- **Otsu's threshold** splits a 256-bin histogram where the variance between the classes is greatest. On glass alone, it would split the noise, so it is never under 0.05.
- **The closing and the opening** use a 3 × 3 square, eroding only by neighbours within the image, so that tissue at the region's edge isn't eaten away.
- **The margin,** 100 µm, two pixels, grows the tissue square by square. It also fills holes smaller than it, which a scan would cross anyway.

### The `measure` policy

- **The engine measures the map first,** before it starts its recording: autofocus at the tiles nearest the middles of a grid over them, a tile counted once if two middles are nearest it. The fit's floor is a quarter of the depth of field.
- **The plan then becomes a plan by the map:** each tile at the map's z, and its policy `map`. So the recording, the acquisition and the progress keep the measured map, not the request to measure one.
- **A cancel or failure while measuring** returns the stage to the start, with nothing recorded.

### The `scan` Thing

- **`overview`** runs the job with the camera's field now, with no overlap. **`last_overview`** gives the image in base64 and the tissue as runs of pixels, so a page draws them without a mask of 1 and 0 for every pixel.
- **The `tissue` pattern** is a snake over the last overview's tissue, its z by a 3 × 3 measured map unless asked otherwise. Without an overview, or with no tissue in it, it is refused, saying which.

### Coverage, against the truth

The tests hold coverage to the simulated tissue's own truth, `HeTissue::is_tissue`, every 50 µm, not to the detection. A point counts if it is in some tile's frame. Holes in the tissue are glass, and don't count.

### The tests' turns, and two cores

{{TWOCORE}}

### The size

M's budget is 600; P37 measured 1,558, 22 % under its estimate of ~2,000.

- **One pull request,** under the owner's standing choice: the overview, the detection and the tissue's scan are one path, and the coverage test needs all three.
- **Against the estimate,** the code took 728 lines against ~770, the tests 427 against ~500, and the example 382 against ~450.

## Tests

| Crate | Test | What it checks |
|---|---|---|
| `oscope-workflows` | `scan::histo` (4, unit) | **Otsu** splits two classes between them, and gives 0 for values all alike. **Tissue is found, cleaned, kept by area and grown:** a hole filled, a lone pixel removed, a speck of 0.04 mm² dropped, the margin of two pixels. **The same tissue in dim light.** **Grey glass alone,** with noise, is no tissue, at the floor |
| `oscope-workflows` | `histo` (2) | **Coverage:** an overview binned 3 × 3 of 16 × 12 mm about the first island; its mask holding 99 % of the truth's pixels of tissue; its tissue covered by 176 tiles where a raster takes 224, 99.89 % of the truth's points of tissue in a tile. **An empty slide** has no tissue, so a snake over it is refused |
| `oscope-workflows` | `scan_engine` (1 more) | **A map measured before the first tile** puts each tile at its z, the plan's policy then `map`, every tile within a quarter of the depth of field of the truth on the tilted slide |
| `oscope-api` | `scan` (1 more), and a unit test | **Over HTTP:** the `tissue` pattern refused before an overview; an overview in `bin3`, 140 frames, its image and tissue; the tissue's tiles fewer than a raster's, their z a measured map. **Base64** as RFC 4648's examples |
| `oscope-api` | `devices` (1 changed) | The `scan` Thing's TD (a snapshot) |
| `ex-histo-scan` | `--smoke` | 99 % of the truth covered, fewer tiles than a raster, a frame for each tile |

**Where they ran:**

{{RAN}}

**Mutation checks** (each broke the code on purpose, then the tests were run): fourteen of fourteen caught.

- **They covered:** saturation's division, the floor under Otsu's threshold, Otsu's split, the closing, the opening, the least area, the margin, the overview's placing of its cells and its frames, the tiles at the map's z and the plan's policy then, the `tissue` pattern's measured map, the overview's frames a field apart, and base64's padding.
- **Reading the tests against the list first** found four gaps, now covered: saturation not divided by the brightest channel, which dim light shows; the tiles left at the last z, which the slide's small tilt over 600 µm hid, so each tile is held to the map's z exactly; the overview's frames overlapping, so their count is checked; and base64, which no test read.
- **The first run missed two more,** now covered: the overview's cells placed half a mm off, and every other frame skipped. The tiles still covered 99 % of the tissue, as a tile is far larger than either fault, so the mask itself is now held to the truth, pixel by pixel.

| Mutant | Caught by |
|---|---|
| Saturation not divided by the brightest | the same tissue in dim light |
| No floor under Otsu's threshold; Otsu's split moved | grey glass alone |
| No closing; no opening; every piece kept; no margin | tissue found, cleaned, kept by area and grown |
| The overview's cells half a mm off; every other frame skipped | coverage, once the mask was held to the truth |
| Tiles not at the map's z; the plan's policy left to measure | a map measured before the first tile |
| A tissue scan's z the stage's; the overview's frames overlapping | the overview over HTTP |
| Base64 without its padding | base64 as RFC 4648's examples |

## Follow-ups and known gaps

- **The whole coverslip's overview** takes some 1,000 frames binned at the 10×. A lower objective would take it in tens, but the objectives are swapped by hand, so the overview takes the scan's.
- **Islands one by one:** the snake crosses the gaps between pieces of tissue. An order by piece may save travel (P35's follow-up).
- **Learned segmentation,** as Bándi et al. (2017) compare, for tissue that a threshold of colour misses, such as unstained or faint tissue.
- **The truth's coverage of the mask** is held at 99 %; a check that glass far from tissue stays out of it would catch a mask too generous.
- **Tissue at the overview's region's edge** is cut there. A region with a margin of its own takes it in.
- **The focus map's points** are the tiles nearest a grid, which on a sparse mask may cluster.

## Clean room

Nothing was copied from `microscope` or any other codebase. Each method is written from its sources, as the spec above sets them out.
