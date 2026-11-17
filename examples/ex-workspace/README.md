# ex-workspace

P48's Workspace through the API, as a client would: analyses on a stitched slide, kept with their provenance, made again and compared.

1. **The slide.** As in [`ex-count-cells`](../ex-count-cells/README.md), a snake over 1 × 0.75 mm across the edge of the simulated H&E slide's first island is scanned at 20× and stitched. P46's `cells.count` counts the whole mosaic, and finds the glass's white.
2. **An analysis,** by the `workspace` Thing's action `run`: the cell-counting recipe on a region of 200 × 150 µm of tissue, left of the mosaic's middle, so a smoke run reads the same tissue. The recipe:
   - each stain's amount (`stain.deconvolve`, with the white);
   - the hematoxylin's (`convert.format`, its first channel);
   - Otsu's level (`threshold.otsu`);
   - the nuclei split (`cells.split`) and measured (`cells.measure`).

   It runs as a job, `workspace.run`. Its result holds:
   - the hematoxylin's amounts as its image;
   - `objects.csv`, `outlines.geojson` and `nuclei.png`;
   - `summary.json`.
3. **Its provenance:**
   - the analysis, recipe and region;
   - each node's version, and its extension's if it has one;
   - `sha256:` over the samples it read;
   - the software.
4. **Made again** with `rerun`:
   - as it was, its table the same, byte for byte;
   - with nuclei of 25 µm² at least, fewer of them. `GET difference/…` names that one change.
5. **The page** lists the project's results, each with its nodes, its input's hash, its files, and what differs from the first analysis.

## How to try it

```sh
cargo run --release -p ex-workspace
```

It serves at http://127.0.0.1:8216/. On the owner's PC, at real time on the simulator's quarter sensor, it took 27 s and printed:

```text
ex-workspace: a snake over 1.00 × 0.75 mm at 20×, stitched; the counting recipe on 200 × 150 µm of it: 153 nuclei, 5100 per mm² (P46's count of the slide: 4582)
  made again: the same table; with nuclei of 25 µm² at least: 96, the difference [{"path":"/recipe/workspace.run/recipe/nodes/measure/params/min_area","before":10,"after":25}]
  open http://127.0.0.1:8216/ to see the results and their provenance
```

**The routes,** on the `workspace` Thing:

- `POST /api/v1/workspace/run` with `{"project", "analysis": {"recipe", "input": {"kind", "id"}, "roi"?, "tile"?, "name"?}}`: a job, `workspace.run`.
  - The recipe takes one input, the image `image`: grey, or RGB, a raw image's 2 × 2 cells in colour, in linear light from 0 to 1.
  - `roi` is `{"x", "y", "width", "height"}` in µm from the image's top-left corner, as annotations are. A run reads 25 megapixels at most, so a larger image needs one.
  - `tile` runs it tile by tile, if each of its nodes can.
- `POST /api/v1/workspace/rerun` with `{"project", "result", "changes"?}`: made again, `changes` by the node's ID in the recipe, each parameter's new value, or `null` for its default.
- `GET /api/v1/workspace/results/{project}`: the results, with their provenance and files.
- `GET /api/v1/workspace/difference/{project}/{a}/{b}`: what differs, each a JSON Pointer and the two values.
- `POST /api/v1/workspace/delete_result`: refused while another result was made from it.
- **Annotations** moved here from the `projects` Thing: `annotate`, `remove_annotation`, `GET annotations/…` and `GET measurements/…`, as [`ex-annotate`](../ex-annotate/README.md) uses them.

**Each output** is a file of the result:

| Output | File |
|---|---|
| The first image | The result's own image |
| A later image | `<name>.ome.zarr` |
| A mask | `<name>.png` |
| A table | `<name>.csv` |
| Shapes | `<name>.geojson` |
| Other JSON | `<name>.json` |

## Its checks

`cargo run -p ex-workspace -- --smoke` scans 0.6 × 0.45 mm at ten times real time, then checks that:

- the recipe finds more than 50 nuclei in the region, within 25 % of the density P46's count finds over the slide (the region's own varies: 12 % above it here);
- made again, its table is the same, byte for byte;
- with nuclei of 25 µm² at least, it finds fewer, and the difference is that one parameter, from 10 to 25;
- all three analyses carry the mosaic's hash;
- the page and its script are served.
