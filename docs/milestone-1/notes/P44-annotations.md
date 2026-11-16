# P44: Annotations and measurements

- Status: In review
- Branch / PR: `m1/p44-annotations` / [#60](https://github.com/lucaswewa/OScope/pull/60)
- Size: M · estimated ~2,000 before it started (in `oscope-imaging`, shapes' lengths, areas and centres, and the pixels a polygon holds, row by row, ~170; in `oscope-store`, annotations in layers with their validation and their keeping, ~260, and measurements read tile by tile, with the CSV, ~230; the `projects` Thing's routes ~130; tests ~560; `ex-annotate` and its OpenSeadragon drawing page ~490; `ex-common`'s helpers ~60; glue ~30); one pull request, at the owner's standing choice · measured **2,439** hand-written lines added (code 881: `oscope-store` 617, `oscope-imaging` 141, `oscope-api` 123; tests 913; examples 614; glue 31), and 20 removed. **That is 1,839 over M's 600, and 22 % over the estimate.** OpenSeadragon, vendored in P38, isn't counted.

## Summary

Shapes drawn on images in µm, kept in layers, and measured ([spec](../phases.md#p44); plan §5.17).

- **Annotations** are points, lines and polygons as GeoJSON, in µm, in layers on an acquisition or a result. They're kept in the record's folder, in its project, and the `projects` Thing keeps, lists and removes them.
- **Measurements** are a line's length, and a polygon's area, perimeter and centre. A polygon's pixels' statistics are also measured, channel by channel: their count, mean, standard deviation, least and greatest. A raw image's are by the colours of its filters. They're read tile by tile from the full resolution, and are the same whatever the tiles. They're served as JSON or CSV.
- **A scan's images now carry their pixel size,** from its plan's field, so a stitched mosaic is measured in µm.
- **`ex-annotate`** stitches a scan of the simulated H&E slide, and its page draws on it in OpenSeadragon and lists what each shape measures.

## What was built

- **`oscope-imaging::shapes`:**
  - the length and centre of a path;
  - a polygon's area, perimeter and centre, holes and all, by the shoelace formula about its first point;
  - the pixels a polygon holds, a row at a time: runs of columns whose centres are inside by the even-odd rule.
- **`oscope-store::annotations`:**
  - `Geometry` (`Point`, `LineString`, `Polygon`), `Feature`, `Layer` (a FeatureCollection) and `Annotations`;
  - `Store::annotations`, `annotate` and `remove_annotation`, under the store's lock.

  A record's annotations are `annotations.json` in its folder: a versioned document (`oscope.annotations`, version 1) whose layers are GeoJSON FeatureCollections.
- **`oscope-store::measure`:**
  - `measure`, a record's annotations measured, one layer's or all;
  - `polygon_stats`, a polygon's pixels' statistics, tile by tile;
  - `measurements_csv`, the measurements as CSV (RFC 4180).
- **Ingest** records every scan's pixel size from its plan's field, where P41 recorded only a z-scan's. A tiled scan's planes still aren't a volume, and their z scale is 1.
- **The `projects` Thing:**
  - the actions `annotate` and `remove_annotation`;
  - `GET annotations/{project}/{acquisitions|results}/{id}`, a record's layers;
  - `GET measurements/{project}/{acquisitions|results}/{id}`, with `?layer=…`, `&plane=…` and `&format=csv`.
- **`ex-common`:** `invoke`, `job` and `made`, which most examples had copied, for the examples to come.
- **`ex-annotate`** (port 8230), its page, and its README.

## How to try it

```sh
cargo run --release -p ex-annotate
```

Then open http://127.0.0.1:8230/. It scans a snake over 2 × 1.5 mm about the first island, stitches it, and keeps five annotations in three layers. It prints the mosaic and the rectangle's measurements:

```text
ex-annotate: a mosaic of 2598 × 1730 pixels of 0.960 × 0.960 µm, with 5 annotations in 3 layers
  the rectangle of 600 × 300 µm: 180000.0 µm², 1800.0 µm round, its pixels' means red 41184, green 26617, blue 43453, measured in 21 ms
  open http://127.0.0.1:8230/ to draw polygons, lines and points, and measure them
```

On the page:

- **Drawing:** tick *Draw*, choose a shape, and click to add points. A double-click or Enter keeps a polygon or a line; a point is kept at one click.
- **The table** lists every annotation's measurements; *Remove* removes one.
- **Download CSV** gets them as a file.

The [README](../../../examples/ex-annotate/README.md) lists the routes.

## Performance

§5.20 has no budget for measurements. On this PC, with a release build, through the API:

- **The example's five annotations**, the three polygons' 195,000-odd pixels among them, measure in 21 ms.
- **A polygon over the whole mosaic,** 4,494,540 pixels of 2598 × 1730, measures in 27–30 ms over five runs, about 160 megapixels a second.

So a region of a 200-megapixel slide would take about 1.3 s.

## Design notes and deviations from the plan

- **Coordinates are µm from the image's top-left corner,** x across and y down: an image's pixel size scales its pixels to them. They aren't the stage's: an image doesn't record where on the stage it is, and a mosaic's place is its frames'. On an image of unknown pixel size, a µm is a pixel, and the measurements say `px`.
- **A pixel is a polygon's when its centre is inside,** by the even-odd rule. A centre on a left or top edge is inside, and one on a right or bottom edge isn't. So two polygons that share an edge share no pixel, and leave none out between them; the example checks this on the stitched slide.
- **One file per record, not per layer.** Files named by layers would make a layer's name a file's name, with Windows' reserved names and case to avoid. The routes give each layer as plain GeoJSON. A feature's ID is unique in its record and never given twice (`last_id`).
- **Validation follows RFC 7946:**
  - positions are 2 or 3 finite numbers;
  - a line has two points or more;
  - a polygon's rings are closed, of four positions or more, and enclose something.

  A z is kept but not measured; a stack's plane is chosen with `plane`.
- **The statistics are a population's,** in the image's own units:
  - always at full resolution, since a coarser level of a raw mosaic would mix its colours;
  - each row's sums run on from tile to tile, left to right, and rows are taken in order by Chan, Golub and LeVeque's pairwise update, so the result is the same to the bit whatever the tiles.

  The test asserts equality, not closeness, for 16-bit and float images.
- **A line or a point measures its shape only:** no profile along a line, no value under a point. Both are follow-ups.
- **Measuring is an endpoint, not a job.** It runs on a blocking thread; a polygon over a 4.5-megapixel mosaic takes about 28 ms. Recipes on an ROI run as jobs with P48's `workspace` Thing, which also consolidates these routes, as its spec says.
- **The scans' pixel size** is a change to ingest: without it, a stitched mosaic had no pixel size, and could be measured only in pixels.

### What the mutants' reading found

Sixty-four mutants, listed in advance from the code: of the geometry, the validation, the keeping, the statistics, the tiles, the CSV, ingest and the routes. Reading the tests against the list found gaps, now covered:

- **The centre of a ring that encloses nothing,** its first point, wasn't tested.
- **Two guards could never change a result:** the pixel bounds of no points, and the floor at 0 before a saturating cast. They're removed.
- **Annotations kept at once from several threads:** without a test, the store's lock could be dropped. Eight threads now keep 25 each, and every ID from 1 to 200 is there.
- **A polygon partly off the image,** and one wholly off it, whose statistics are all 0 rather than infinite.
- **The CSV's `px` header,** on an image of unknown pixel size.
- **The routes' `layer` and `plane`,** and a removal that kept the emptied layer.

The run then missed one: **NaN in a point or a line** was kept, since the only NaN tested was a polygon's, which its area refused anyway. A NaN point and an infinite line are now refused. One mutant's test binary failed to link, locked by Windows (LNK1104); it was caught when run again. So **64 of 64 are caught.**

### The page

The page wasn't seen in a browser here. Under Node, against the running example, with a stand-in DOM and a stand-in OpenSeadragon whose viewport maps image pixels to the screen at a quarter, it:

- found the newest mosaic, and drew the five annotations with their labels;
- kept a polygon from three clicks and a double-click, the double-click's two clicks not zooming. Its area, 22,118.4 µm², and its 24,000 pixels are what its corners give;
- kept a named point at one click, and a line ended by Enter, of 192 µm;
- showed a drawing in progress, and dropped it on Esc;
- removed an annotation from its row's button;
- set the CSV link and the units, without an error.

Drawing it in OpenSeadragon is for the manual test.

### Two cores

`scripts/two-core.sh`, five times each, side by side, on two CPUs each:

| Target | Passed | Time |
|---|---|---|
| `oscope-imaging:shapes` | 5 of 5 | 0.4–0.5 s |
| `oscope-store:annotations` | 5 of 5 | 7.8–8.4 s |
| `oscope-store:ingest` | 5 of 5 | 7.6–9.7 s |
| `oscope-api:projects` | 5 of 5 | 0.6–1.9 s |
| `ex-annotate --smoke` | 5 of 5 | 17.8–20.9 s |

The store's statistics test took 7.3 s at first: tiles of 37 pixels each decoded a whole chunk. Tiles of 100 are off the chunks' grid too, so the 37s went, and the test takes 3.5 s.

### The size

The estimate was about 2,000; the measure is 2,439, 22 % over:

- **Tests: 913, against ~560.** The statistics are checked against every pixel and against Franklin's test, and the refusals are many small cases, which rustfmt spreads over many lines.
- **Code: 881, against ~790.**
- **Examples: 614, against ~550.**

The largest files are the store's tests (614), `measure.rs` (349), `ex-annotate`'s `main.rs` (299), `annotations.rs` (252) and the page's script (207).

## Tests

| Suite | Tests | What they show |
|---|---|---|
| `oscope-imaging` `shapes` (new) | 4 | Rectangles, a triangle and a square with a hole: areas, perimeters and centres as drawn, whichever way they turn, closed or not, and a slide's width from the origin; a path's length and centre; pixels by their centres, at edges on pixels' edges and through their centres; two polygons sharing an edge share no pixel; holes; clipping; every pixel of a concave star with a hole against Franklin's crossing test |
| `oscope-store` `annotations` (new) | 8 | Kept in layers, with IDs; replaced by ID; removed, a feature or a layer, and IDs not given twice; kept on disk as GeoJSON in a versioned document; kept at once by eight threads; shapes, layer names and records refused; areas, lengths, centres and pixels in µm against every pixel; the same statistics in tiles of 512, 100 or the whole, for 16-bit and float images, and against every pixel; pixels of unknown size; a stack's planes; polygons off the image; a raw image by its filters' colours; the CSV |
| `oscope-store` `ingest` (changed) | 5 | A tiled stack's pixels have their size now, and it still isn't a volume |
| `oscope-api` `projects` (one new, one changed) | 3 | Annotations kept, listed, measured as JSON and CSV, one layer's and one plane's, refused and removed over HTTP; the Thing's description with the new actions |
| `ex-annotate --smoke` | — | The rectangle's area and perimeter, a square's area, the line's length; the two squares' pixels, colour by colour, are the rectangle's, and so are their means; the rectangle's pixels match its area within 1 %; the CSV; a removal; the page and OpenSeadragon served |

⟨REPLAYS⟩

## Follow-ups and known gaps

- **A line's profile, and the value under a point.**
- **Measuring on a coarser level** for speed on whole slides, which needs a raw mosaic's levels kept by colour.
- **Annotations in the stage's coordinates,** once images record where they are.
- **More of GeoJSON:** MultiPolygon and the other multi-geometries, if P46's objects or imports need them.
- **The examples' copies of `invoke`, `job` and `made`:** 18 examples still have their own; they can move to `ex-common` when next touched.

## Clean room

Nothing was copied from `microscope` or any other codebase. The methods are published ones, written from their descriptions:

- **Areas and centres:** the shoelace formula, and the centroid of a polygon by its signed area (Bourke, *Calculating the area and centroid of a polygon*, 1988).
- **The pixels a polygon holds:** scan-line filling by the even-odd rule, with an edge's lower end counted and its upper end not (Foley, van Dam, Feiner and Hughes, *Computer Graphics: Principles and Practice*, 2nd ed., 1990, §3.6).
- **The statistics:** the pairwise update of a mean and a sum of squares (Chan, Golub and LeVeque, *Updating formulae and a pairwise algorithm for computing sample variances*, Stanford STAN-CS-79-773, 1979; and *Algorithms for computing the sample variance*, The American Statistician 37(3), 1983).
- **The tests' independent check** is W. R. Franklin's crossing test (PNPOLY), in the tests only.
- **Formats:** GeoJSON (RFC 7946), CSV (RFC 4180).
