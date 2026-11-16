# P40: Deep-zoom serving, tile cache, DZI export

- Status: In review
- Branch / PR: `m1/p40-deep-zoom` / [#55](https://github.com/lucaswewa/OScope/pull/55)
- Size: M · estimated ~2,750 before it started (in `oscope-imaging`, the DZI geometry and tiles rendered from a pyramid's levels through a display transform, raw frames demosaiced, ~320; in `oscope-store`, tiles read through a cache of decompressed chunks, a colour overview for raw images, the disk tile cache on scratch and the DZI export, ~440; the pipeline's DZI node ~70; in `oscope-api`, the routes with `Server-Timing`, the tile pool capped while a hardware job runs, the cache warmed when a stitch ends and the export format, ~320; tests ~650; `ex-deep-zoom` and its bench ~410; `ex-checkpoint-d` ~250; glue ~50; and about a tenth on top); one pull request, at the owner's standing choice · measured <<SIZE>>

## Summary

Any image of a project can be browsed at any zoom, its tiles made on demand from its pyramid ([spec](../phases.md#p40); plan §5.15, D33; [ADR-0028](../../adr/0028-deep-zoom-serving.md)). Checkpoint D runs from end to end.

- **Deep Zoom (DZI) tiles** of 256 pixels are made from an image's OME-Zarr pyramid through the look in the query, as JPEG: `/api/v1/zoom/{project}/{kind}/{id}/image.dzi` and its `image_files/`.
- **Raw images show in colour,** at the owner's choice. At full size each tile is demosaiced. Halved, tiles come from a colour overview, an OME-Zarr image of the mosaic's 2 × 2 cells, built when a stitch ends or when first asked for.
- **What is kept:**
  - decompressed chunks, in memory;
  - tiles, on the scratch disk, by image, look, level and tile, those used least recently deleted first;
  - each look's display table.
- **Fast while a scan runs:** tiles are made on a pool of their own, 8 threads, and 1 while a hardware job holds the instrument. Every tile carries `Server-Timing`.
- **When a stitch ends,** its colour overview and coarse levels are made, so a slide opens at once.
- **Exports:** a static DZI folder, from `projects.export` with the format `dzi`, or from the pipeline's `export.dzi` node.
- **§5.20, on this PC, over localhost:**
  - a cold tile: 4.8–4.9 ms of server time, the median, though its 95th percentile, 5.6–5.8 ms, is over the 5;
  - a kept tile: 0.08–0.09 ms;
  - a full-screen view never seen: 46–49 ms, and 95–100 ms while a hardware job holds the instrument.
- **`ex-deep-zoom`** browses a stitched slide in OpenSeadragon with levels and gamma. **`ex-checkpoint-d`** is Checkpoint D: a histo-scan with the live mosaic, stitched, then browsed in deep zoom.
- **P39 is marked Done.**

## What was built

- **`oscope_imaging::zoom`:**
  - `Dzi`: the levels, sizes, tiles and descriptor;
  - the `Pyramid` trait;
  - `tile` and `tile_rgb`, `write_dzi`, and `InMemory`.

  Also `color::DisplayTable`, a display transform's table made once.
- **`oscope_store::zoom`:**
  - `StoredImage`, `ChunkCache` and `StoredPyramid`, which shows raw mosaics in colour;
  - `build_overview` and `default_display`;
  - `TileCache`, `TileKey`, `look_key` and `image_key`;
  - `record_image` and `export_dzi`.

  Also `Samples::from_unit`.
- **`oscope-pipeline`:** the stock node `export.dzi`.
- **`oscope-api`:**
  - the zoom routes, the tile pool, and the warm-up as a stitch ends (`zoom.rs`);
  - the export format `dzi`.
- **Examples:**
  - `examples/ex-deep-zoom` (port 8244), its page and its bench;
  - `examples/ex-checkpoint-d` (port 8245), on `ex-deep-zoom`'s page;
  - `ex-common`'s `fetch`, which keeps an answer's head and bytes.

## How to try it

`cargo run --release -p ex-deep-zoom`, then open http://127.0.0.1:8244/. The [README](../../../examples/ex-deep-zoom/README.md) shows what it prints. On the owner's PC, at real time, it scans a snake over 4 × 3 mm and stitches it into a raw mosaic of 5060 × 3372 pixels. The mosaic's colour overview and 17 coarse tiles are ready a second after the stitch began. The page opens it in OpenSeadragon, with sliders for black, white and gamma, and each tile's server time.

`cargo run --release -p ex-checkpoint-d`, then open http://127.0.0.1:8245/: Checkpoint D. The [README](../../../examples/ex-checkpoint-d/README.md) shows what it prints. On the owner's PC, at real time, it found 23.8 mm² of tissue in an overview of 6 × 4 mm. Its 25 tiles were each drawn on the live mosaic as they were taken, then stitched, 40 pairs kept of 40, into a mosaic of 6290 × 4194 pixels to browse, in 146 s in all.

`cargo run --release -p ex-deep-zoom -- --bench 200` times deep zoom against §5.20.

## Performance

§5.20 asks for a cold tile in 5 ms of server time or less, a kept one in 1 ms or less, and a full-screen view of a region never seen in 150 ms or less on a LAN. On the owner's PC, in a release build, `ex-deep-zoom --bench 200` wrote a raw mosaic of 16328 × 12246 pixels and served it over localhost. The LAN was measured over localhost, as the owner chose. Three runs:

| Measure | Budget | Measured |
|---|---|---|
| A cold tile at full size, one at a time, 60 spread over the mosaic: the median | 5 ms | 4.82–4.86 ms |
| The same: the 95th percentile | | 5.55–5.80 ms |
| The same tiles, kept: the median | 1 ms | 0.08–0.09 ms |
| A full-screen view, 1920 × 1080, never seen: 45 tiles, six at a time | 150 ms | 46–49 ms |
| The same while a hardware job holds the instrument | 150 ms | 95–100 ms |

- **The colour overview** of the 200 MP mosaic took 3.8–3.9 s to build.
- **A cold raw tile at full size** reads up to four chunks: the tile's own, and the ones beside it for the 2 pixels its demosaicing needs. A full-screen view shares them between its tiles, through the chunk cache.
- **While a hardware job runs,** tiles are made on one thread, so a view takes about twice as long.

## Design notes and deviations from the plan

### Where the spec moved

- **Raw images,** which the spec didn't foresee, get a colour overview, at the owner's choice. The other choices were grey overviews, or stitching into RGB at three times the size. The overview is three quarters of the raw mosaic's samples, in the tile cache, not the project.
- **The routes' names:** a route can't take part of a path's segment, so the descriptor is `image.dzi` beside `image_files/`, where OpenSeadragon looks for its tiles.
- **"Low priority"** is the pool's cap of one thread while a hardware job runs. It isn't an OS thread priority.
- **The exporter isn't ADR-0037's.** Its exporters take an image's planes whole, gigabytes for a mosaic. `projects.export` runs `dzi` as a format of its own, from the pyramid.
- **The node** writes an image in memory. A node must give something, so it gives where it wrote and how many tiles. It runs in batch only, and never over a folder already there.
- **The budgets are the bench's,** as P39's throughput was. The tests check a view's tiles, not its time, as CI's runners can't time it.

### A table for each look, from the bench

The bench's first run measured a cold tile at 5.74 ms, the median. Each tile rebuilt its display table, 65,536 steps of the curve, a power for sRGB's or a gamma, more than the tile's own samples. Made once for each look, and kept, a cold tile took 4.86 ms.

### The smoke's warm-up

The smoke at first waited for the colour overview and a first coarse tile, then checked a tile of level 8 as kept. The warm-up was still making the coarse levels: 12 of 17 tiles were there. The status now says when the coarse levels are all made (`coarse`), and the examples wait for that.

### Two cores

<<TWO_CORE>>

### The size

<<SIZE_DISCUSSION>>

## Tests

| Crate | Test | What it checks |
|---|---|---|
| `oscope-imaging` | `zoom` (3) | **Deep Zoom's levels and tiles** for five sizes, from a pixel to 5472 × 3648: each level's size, its tiles, the last at its edges, none past them, and the descriptor. **Tiles are the levels,** cut, and averaged past the coarsest, at every level and corner, within a level of 8 bits. **A DZI folder** holds every tile of every level, its descriptor the same, its corner tile 88 × 44; a grey image grey in each channel |
| `oscope-store` | `zoom` (5) | **Reads through the chunk cache** are the image's own, exactly, across chunks and edges, at every level, plane 1 of 2, grey and RGB; a read again comes from the cache. **The chunk cache** lets the chunk used least recently go. **A raw mosaic in colour:** at full size as the whole mosaic demosaiced then cut, grey halved before its overview, the overview each cell's red, greens' mean and blue exactly, and halved tiles from its levels. **The tile cache** keeps the newest within its budget, a tile used again kept, a tile deleted behind its back forgotten, its layout, and its files taken in when opened again. **An export** of a raw acquisition is what is served, every tile byte for byte, and leaves nothing beside it |
| `oscope-api` | `zoom` (4) | **Over HTTP:** the descriptor and status; a tile made, then kept, the same bytes, by `Server-Timing`; in the cache's layout; another look's tiles its own; every level's last tile at its edge, none past it; in colour; refusals (404, 400). **A DZI export** by `projects.export` is what is served, every tile. **A raw image** in colour at full size at once, grey halved until its overview is built, which asking starts, not kept, then in colour and kept. **One thread** while a hardware job holds the instrument, then all again |
| `oscope-api` | `capture` (1 changed) | The formats: `dzi` among the exporters |
| `oscope-pipeline` | `stock` (1 new) | **`export.dzi`:** every tile, the same as deep zoom's of the image; refused written over, for a folder not given whole, a name that leaves it, and levels the wrong way round |
| `ex-deep-zoom` | `--smoke` | A stitched raw mosaic, its overview and coarse levels made as the stitch ended; the descriptor; a coarse tile kept; a full-size tile made, then kept; the page and OpenSeadragon |
| `ex-checkpoint-d` | `--smoke` | Checkpoint D: every tile of the tissue's scan drawn on the live mosaic, none dropped; the stitch's pairs kept; deep zoom's descriptor, a coarse tile kept, a full-size tile |

**The page** was run under Node against the example, with stand-ins for OpenSeadragon and the browser's PerformanceObserver. It made no errors:

- it found the stitched mosaic and set its sliders from its default look;
- its tile source named the tiles it served;
- a gamma changed the look in place;
- "As made" went back to the default look;
- it showed the made and kept tiles' times.

Drawing in a browser is for the owner's manual test.

**Where they ran:**

<<WHERE_RAN>>

**Mutation checks:** <<MUTANTS>>

## Follow-ups and known gaps

- **A cold raw tile's 95th percentile** is over 5 ms, as its margins read up to four chunks. Reflecting the mosaic at a chunk's edge would read one, but leave faint seams every 512 pixels.
- **HTTP/2** for remote browsers is U03's, over TLS. Over plain HTTP, browsers fetch six tiles at a time.
- **No OS thread priority:** the cap alone keeps tiles from crowding a hardware job.
- **The caches' sizes** are fixed: 512 MB of chunks, 4 GB of tiles. They are no settings yet.
- **A raw acquisition's overview,** not a stitch's, is built when first asked for: seconds for a large mosaic, its halved tiles grey meanwhile.
- **The page** shows plane 0, with levels and gamma for all channels together.
- **The bench's scan** is a hardware job holding the instrument, which caps the pool. It doesn't load the CPU as a simulated scan would.

## Clean room

Nothing was copied from `microscope` or any other codebase.

- **Deep Zoom's levels, tiles and descriptor** follow Microsoft's published description of the format, which OpenSeadragon reads.
- **The superpixel and bilinear demosaics** are P08's, and the JPEG encoder P17's.
- **OpenSeadragon 6.1.1** is the copy P38 vendored, unmodified.
