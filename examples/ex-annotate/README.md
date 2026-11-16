# ex-annotate

P44's annotations and measurements on a stitched slide, through the API as a client would.

1. **A scan and its stitch.** A snake over 2 × 1.5 mm about the simulated H&E slide's first island goes into a project and is stitched as a job: a raw mosaic, in the camera's Bayer pattern, with its pixels' size from the scan's field.
2. **Annotations,** kept with the `projects` Thing's `annotate` action, in µm from the mosaic's top-left corner, in layers: two squares of 300 µm side by side and the rectangle of both (`regions`), a line (`lines`) and a point (`points`).
3. **Their measurements,** from `GET /api/v1/projects/measurements/{project}/results/{id}`: lengths, areas and centres in µm, and each polygon's pixels' statistics by the colours of the camera's filters, read tile by tile from the full-resolution mosaic. Add `?format=csv` for CSV.
4. **The page** opens the mosaic in OpenSeadragon, vendored, and draws the annotations over it. Draw your own, and see them measured.

## How to try it

```sh
cargo run --release -p ex-annotate
```

It serves at http://127.0.0.1:8230/. On the owner's PC, at real time on the simulator's quarter sensor, it printed:

```text
ex-annotate: a mosaic of 2598 × 1730 pixels of 0.960 × 0.960 µm, with 5 annotations in 3 layers
  the rectangle of 600 × 300 µm: 180000.0 µm², 1800.0 µm round, its pixels' means red 41184, green 26617, blue 43453, measured in 21 ms
  open http://127.0.0.1:8230/ to draw polygons, lines and points, and measure them
```

On the page:

- **Draw** with *Draw* ticked: choose a shape, then click to add points. A double-click or Enter ends a polygon or a line, and Esc drops it. A point is kept at one click. Untick *Draw* to click-zoom again; dragging pans either way.
- **Layer** and **Name** say where a new shape goes, and what it's called.
- **The table** lists every annotation's measurements: its length (a polygon's perimeter), its area, its pixels, and their means and standard deviations by colour. *Remove* removes one.
- **Download CSV** gets the same as a file.

The routes it uses:

- `POST /api/v1/projects/annotate` and `/api/v1/projects/remove_annotation`, actions;
- `GET /api/v1/projects/annotations/{project}/{acquisitions|results}/{id}`, a record's layers, each a GeoJSON FeatureCollection;
- `GET /api/v1/projects/measurements/{project}/{acquisitions|results}/{id}`, with `?layer=…`, `&plane=…` and `&format=csv`.

## Its checks

`cargo run -p ex-annotate -- --smoke` scans 1.2 × 0.9 mm at ten times real time, then checks:

- the rectangle's area and perimeter, a square's area and the line's length, as drawn;
- the two squares, which share an edge, have between them the rectangle's pixels, colour by colour, and its means;
- the rectangle holds as many pixels as its area, within 1 %;
- the CSV, a header and a row for each annotation;
- an annotation removed, and the page, its script and OpenSeadragon served.
