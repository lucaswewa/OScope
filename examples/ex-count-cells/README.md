# ex-count-cells

P46's cell counting on a stitched slide, through the API as a client would.

1. **A scan at 20×.** A snake over 1 × 0.75 mm across the edge of the simulated H&E slide's first island is scanned and stitched: a raw mosaic, with glass for the white.
2. **Its nuclei counted** by the job `cells.count`:
   - on the hematoxylin channel, by colour deconvolution (P45), at Otsu's level;
   - holes filled, and touching nuclei split by a seeded watershed on the distance transform;
   - each measured: area, perimeter, circularity, centroid, and the channel's mean.

   It works tile by tile with a halo, keeping each nucleus in the tile that holds its centroid. It makes a result of `cells.csv`, `cells.geojson` and `summary.json`, and keeps the outlines as the mosaic's annotation layer `cells` (P44).
3. **Matched to the simulator's nuclei,** where its first frame says the mosaic lies. The mosaic's small offset from that is found first, as the median of each nucleus's offset to its nearest within 10 µm; then nuclei pair up within 3 µm.
4. **The page** draws the outlines over the mosaic in OpenSeadragon, vendored, with the count's summary and the table to download.

## How to try it

```sh
cargo run --release -p ex-count-cells
```

It serves at http://127.0.0.1:8232/. On the owner's PC, at real time on the simulator's quarter sensor, it took 26 s and printed:

```text
ex-count-cells: a snake over 1.00 × 0.75 mm at 20× across the island's edge, stitched; 4441 nuclei counted, 4586 per mm² of tissue
  against the simulator's 3216 nuclei, the mosaic -0.3, -0.0 µm from where its first frame says: recall 0.995, precision 0.994, the median miss 0.28 µm
  open http://127.0.0.1:8232/ to see their outlines
```

- **More nuclei are counted than the region holds:** the mosaic's frames reach beyond the region scanned. Recall and precision are taken 30 µm inside the region.
- **The table,** a row a nucleus: `/api/v1/data/<project>/results/<count>/cells.csv`.
- **The outlines** are the mosaic's layer `cells`, which P44's routes measure: `/api/v1/workspace/measurements/<project>/results/<mosaic>?layer=cells`.

**The job,** for any stored image:

```sh
curl -X POST http://127.0.0.1:8232/api/v1/jobs/submit -H 'content-type: application/json' \
  -d '{"kind": "cells.count", "params": {"project": "<project>", "input": {"kind": "result", "id": "<mosaic>"}, "options": {}}}'
```

- `channel` is `hematoxylin` (brightfield) or `grey` (fluorescence).
- `threshold` is `otsu`, `{"level": 0.3}` or `{"adaptive": {"radius": 10, "offset": 0.05}}`.
- `seed_radius` and `min_distance` set the watershed's seeds; `min_area`, `max_area` and `min_circularity` the filters.
- `dab: true` classes each nucleus of an H-DAB slide by its mean DAB, in P45's bins.

A learned segmenter, cellpose or StarDist, plugs in as a Python extension: see [`ext-python-cells`](../ext-python-cells/README.md).

## Its checks

`cargo run -p ex-count-cells -- --smoke` scans 0.6 × 0.45 mm at ten times real time, then checks that:

- more than 50 nuclei are counted;
- recall and precision against the simulator's nuclei are 0.85 or more, the mosaic's offset under 10 µm;
- the annotation layer holds an outline for each nucleus;
- the page and its script are served.
