# ADR-0028: Deep-zoom serving

- Status: Accepted
- Date: 2026-11-16
- Phase: P40

## Context

- **Plan §5.15 and D33 want deep zoom on demand:** tiles made from an image's pyramid with a display transform applied, kept on the scratch disk, fast while a scan runs, and exported for sharing.
- **§5.20's budgets:** a cold tile in 5 ms of server time or less, a kept one in 1 ms or less, and a full-screen view of a region never seen in 150 ms or less on a LAN.
- **What already exists:**
  - ADR-0021's store writes OME-Zarr v3 in chunks of 512 pixels, and a pyramid of halvings down to one chunk.
  - OpenSeadragon, vendored in P38, reads Deep Zoom (DZI) images.
  - ADR-0037's exporters write an image a plane at a time.
- **A colour camera's frames are kept raw,** its Bayer mosaic, one channel, and so is P39's mosaic of them. Their pyramid averages each 2 × 2 cell of red, green and blue into grey.

## Decision

**1. Tiles are Deep Zoom's:** 256 pixels square, no overlap, JPEG at quality 85 (`oscope_imaging::zoom`).

- **Levels:** DZI level `n` of an image is the size of its OME-Zarr level `N − n`, `N` the last. Past the pyramid's coarsest level, a tile is the coarsest, box-averaged further.
- **Chunks hold tiles:** a 512-pixel chunk holds 2 × 2 of them.
- **Any pyramid:** the tiles are cut from a `Pyramid`, which the store's images, raw images in colour, and images in memory are.

**2. The routes,** under `/api/v1/zoom/{project}/{kind}/{id}/`, `kind` being `acquisitions` or `results`:

- `image.dzi`, the descriptor;
- `image_files/{level}/{col}_{row}.jpg`, a tile;
- `status`, its size, levels and default look, a raw image's colour overview, and the caches.

**The look** is in the query: `plane`, and `black`, `white` from 0 to 1 of the samples' range, and `gamma`. Unset, an image shows as its thumbnail does.

**3. Raw images show in colour,** at the owner's choice:

- **At full size,** each tile is demosaiced bilinearly, with the 2 pixels around it that the filter needs, read from the chunks beside it.
- **Halved,** from a colour overview. That is an OME-Zarr image in the tile cache whose pixels are the mosaic's 2 × 2 cells, its red, its greens' mean and its blue, with its own pyramid.
- **The overview is built** when a stitch ends, or when a halved tile is first asked for. Until then, halved tiles are the mosaic's grey levels, and they aren't kept.

**4. What is kept:**

- **Decompressed chunks,** in memory, 512 MB, shared by every image, those used least recently let go first.
- **Tiles,** on the scratch disk, 4 GB, those used least recently deleted first. A restart takes in the tiles already there. The layout, which the desktop app can read straight from disk (§5.18): `<scratch>/zoom/<image>/<look>/<level>/<col>_<row>.jpg`, and `<scratch>/zoom/<image>/overview.ome.zarr`.
- **Each look's display table,** 65,536 entries, made once rather than for each tile.

**5. Tiles are made on a pool of their own,** of 8 threads at most, and 1 while a hardware job holds the instrument, so that viewing never starves acquisition.

**6. `Server-Timing`** on every tile: `tile;desc="hit"` or `"miss"`, and the server's time in ms. A browser's developer tools show it.

**7. When a stitch ends,** its mosaic's colour overview, if it is raw, and its coarse levels, those no wider or higher than 1,024 pixels, are made in its default look. A slide opens at once.

**8. Exporting:**

- **A static DZI folder,** `projects.export` with the format `dzi`: `exports/<id>_dzi/` in the record's folder, `image.dzi` and `image_files/`, made from the pyramid as the routes make them. A raw image's overview is built for it, and deleted after.
- **The pipeline's `export.dzi` node** writes an image in memory as a folder, in batch runs alone, and never over one that is there.

## Consequences

- **The budgets,** on the owner's PC, over localhost, as the owner chose, on a raw mosaic of 200 MP:

  | Measure | Budget | Measured |
  |---|---|---|
  | A cold tile, at full size, the median | 5 ms | 4.8–4.9 ms |
  | A cold tile, the 95th percentile | | 5.6–5.8 ms |
  | A kept tile | 1 ms | 0.08–0.09 ms |
  | A full-screen view never seen, 45 tiles six at a time | 150 ms | 46–49 ms |
  | The same, while a hardware job holds the instrument | 150 ms | 95–100 ms |

- **A raw tile at full size** reads up to four chunks, for its margin; most of a cold tile's time is there.
- **The colour overview** is three quarters of the raw mosaic's samples, and took 3.8 s for 200 MP.
- **A look's tiles stay under its key:** changing the look makes tiles cold, and the old ones are kept until the budget needs their room.
- **Over plain HTTP,** browsers fetch six tiles at a time. HTTP/2 over TLS is U03's.
- **The pool's cap** isn't an OS thread priority: a thread at most while a hardware job runs.

## Alternatives considered

- **Grey overviews for raw images,** in colour only at full size: simpler, but an H&E slide's overview grey. The owner declined it.
- **Stitching raw tiles into RGB:** colour without a special case, but three times the mosaic's size and a slower stitch, reopening ADR-0027.
- **Tiles with an overlap of a pixel,** smoother at fractional zoom in some viewers: each tile would straddle chunks.
- **Demosaicing a raw tile from its own chunk,** reflected at the chunk's edges: faster cold tiles, but faint seams every 512 pixels at full size.
- **A DZI exporter through ADR-0037's trait:** it gives planes whole, gigabytes for a mosaic, where tiles need only the pyramid's chunks.
