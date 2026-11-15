# P38: Live stitching: in scans and free-roam

- Status: In review
- Branch / PR: `m1/p38-live-stitching` / [#52](https://github.com/lucaswewa/OScope/pull/52)
- Size: {{SIZE}}

## Summary

A mosaic that builds as frames arrive, from scans or from moving the stage by hand ([spec](../phases.md#p38); plan §5.15, D13; [ADR-0026](../../adr/0026-live-stitching.md)).

- **The canvas:** a pyramid of 256-pixel tiles over a rectangle of the stage, its memory bounded, the rest of its tiles on the scratch disk. The newest frame wins, or its edges are feathered.
- **Each frame is placed where it was taken:** at its exposure midpoint's position, through the calibrated affine. A rolling shutter's rows are each placed where the stage was as they were exposed.
- **Free roaming** draws the live stream's frames when they aren't blurred by the stage's moving, and show a new place, or a place drawn but sharper.
- **A scan's tiles** are each drawn as the engine takes them.
- **The `live_mosaic` Thing** begins, halts and clears it. Its tiles are JPEGs at `/api/v1/mosaic/tiles/{level}/{col}/{row}`, and the tiles changed go with the previews of a client that asks.
- **On the simulator,** frames drawn while the stage moved at 0.4 mm/s, its reports 40 ms late, lie 0.15 µm from the slide's truth: 0.07 of a canvas pixel.
- **`ex-live-mosaic`** shows the live view beside the mosaic in OpenSeadragon, vendored.
- **P37 is marked Done.**

## What was built

- **`oscope_stitch::canvas`:** the `Canvas` (draw, tiles, changes since a generation, clear), its `CanvasGeometry`, `Blend`, `Affine` and `TileKey`.
- **`oscope_acquire::mosaic`:** the `LiveMosaic`, which draws frames on a canvas: roaming's, through its `Gates`, and a scan's, from a queue. Its `MosaicCounts` say how many frames became what.
- **`oscope_acquire::preview`:** `Preview::display`, a preview before its JPEG, which the mosaic draws. The wire's block kind `TILES` (4).
- **`oscope-workflows`:** the scan engine's hook, `ScanRig::tiles` (`TileHook`).
- **`oscope-sim`:** the stage's report latency as a setting, `latency` (5 ms, as before).
- **`oscope-api`:**
  - the `live_mosaic` Thing: `state`, `begin`, `halt` and `clear`;
  - the tile route;
  - the preview stream's `mosaic` parameter;
  - the scan Thing's frames handed to the mosaic;
  - `ApiBuilder::pages_under`, a folder of pages under a prefix.
- **`examples/ex-live-mosaic`** (port 8242), and **`examples/vendor`**: OpenSeadragon 6.1.1 with its licence, which the example helper serves under `/vendor/`.

## How to try it

`cargo run --release -p ex-live-mosaic`, then open http://127.0.0.1:8242/. The [README](../../../examples/ex-live-mosaic/README.md) shows what it prints. On the owner's PC, at real time, it begins a mosaic of 6 × 4 mm in pixels of 1.92 µm, and draws a frame at each of four places the stage stops at, 8 frames blurred on the way. Then it draws each of the 6 tiles of a scan beside them, in some 24 s in all. The arrow keys move the stage, and the mosaic grows where it stops.

## Performance

No budget in §5.20 applies. On the owner's PC, in a release build:

- **Drawing a frame,** the quarter sensor's 1368 × 912 pixels, takes 16.5 ms: drawn as the preview draws it at about the canvas's pixel, its sharpness measured, and drawn on the canvas with each level above.
- **Serving a tile** from memory as a JPEG takes 0.26 ms; one partly covered was 6.3 KB.

## Design notes and deviations from the plan

### The rolling shutter, from the first run

The first run of the placement test drew none of 18 frames: each was judged blurred. A frame's exposure midpoint is its middle row's (P16), and the simulator, like the MIChrome 20, reads its rows out over 60 ms. So the stage's move from the exposure's start to its midpoint included half the readout. Moving at 0.4 mm/s, a frame's last row is exposed 24 µm further along than its first.

- **Blur** is now the stage's move while one row is exposed: its velocity, from the track over the exposure so far, times the exposure.
- **The shear** is taken into the frame's affine. Each row is placed where the stage was as it was exposed, which for a steady velocity is linear in the row.
- **The test was then made to see it.** It compares a region in the frames' upper half, where rows are some 17 ms early. Without the shear in the affine, it is 7.06 µm off the truth; with it, 0.15 µm.

### Sharpness at the same place

Sharpness is compared only between frames of the same place, within a quarter of a frame's shorter side. Against the sharpest frame seen anywhere, glass, less sharp than tissue by its content, would never be drawn after tissue. Where the stage stays still and a frame was judged the same, the next frames are judged the same without being drawn or measured: before that shortcut, roaming drew and measured some 21 frames a second, at a standstill, in a debug build.

### Where the spec's scope moved

- **The canvas is in `oscope-stitch`,** the crate the plan gives blending and pyramids to, which P39 will build on. The spec names `oscope-acquire` and `oscope-api`, which have the rest.
- **`begin`, `halt` and `clear`:** the spec's start and stop are every teta-wot Thing's own (ADR-0034), as the stage's `halt` and the scan's `run` found.
- **The tiles changed** go as a block of the preview's messages (kind 4), as the statistics do (ADR-0035). A teta-wot binary stream can't send text.

### Also

- **Roaming carries on during a scan,** if asked. Where the stage stops for a tile, a frame may be drawn before the scan's own, which is drawn over it: 17 frames for 5 places and 6 tiles in the example. `scans` counts the scan's.
- **The simulator's stage latency** is a setting now, `latency`, so the placement test runs with a USB link's 40 ms rather than the default 5.
- **The example's first walk** went to four places and back, and waited for a fifth frame where the first was drawn, which roaming rightly judged the same. It now goes to four places.
- **Pages under a prefix:** `ApiBuilder::pages_under` serves `examples/vendor` under `/vendor/`, beside an example's page. `examples/vendor` is out of the workspace, as the Python samples are.

### The tests' turns, and two cores

{{TWOCORE}}

### The size

{{SIZEDETAIL}}

## Tests

| Crate | Test | What it checks |
|---|---|---|
| `oscope-stitch` | `canvas` (6) | **A frame lands where its map puts it:** turned 30°, scaled 1.5 and moved by fractions, every pixel checked within 1 of a plane's value, and nothing outside. **The newest wins, or its edges are feathered:** blue over red, the edge a quarter blue, alone whole. **Each level halves the one below:** the sizes, the tiles changed, a pixel the weighted mean of four, an edge half covered. **Memory stays within its budget** of six tiles, of over 40 drawn, and the tiles come back from disk as they were. **Viewers learn what changed,** and all after a clearing. **A flat map or a wrong image is refused** |
| `oscope-acquire` | `mosaic` (2) | **Roaming's gates:** a new place drawn; the same place no sharper, refused, and then sharper, drawn; still, refused unmeasured; 5 µm of blur refused, 0.4 µm drawn; no position, no track, another binning, off the canvas. **A scan's frames** drawn ungated, and from the queue, with the scan's position. **Frames drawn as the stage moves,** 2.5 mm at 0.4 mm/s with its reports 40 ms late, registered against the simulator's own render of the slide: 0.15 µm off |
| `oscope-api` | `live_mosaic` (2) | **Over HTTP:** a mosaic begun, drawn where the stage stops, its tile a JPEG, 404 where nothing is drawn or no level, an edge tile cut to 220 pixels; the changes on the preview stream, all at first, then the tiles a frame changed at every level; halted, drawing no more as frames go by; cleared, empty and all announced. **Each tile a scan takes,** through an ROI, drawn |
| `oscope-api` | `devices` (1 changed) | The `live_mosaic` Thing's TD and the preview's (snapshots) |
| `ex-live-mosaic` | `--smoke` | A frame at each place and each of the scan's tiles drawn, none dropped or failed; a tile, the page and OpenSeadragon served |

**The page** was run under Node against the example, with a stand-in OpenSeadragon and the real preview stream. It made no errors; its tile source maps OpenSeadragon's levels 8–12 onto the mosaic's 4–0; and when the stage moved, it took the 21 tiles changed and fetched them again. Drawing in a browser is for the owner's manual test.

**Where they ran:**

{{RAN}}

**Mutation checks** (each broke the code on purpose, then the tests were run): {{MUTANTS}}

## Follow-ups and known gaps

- **Registration:** frames are placed by the stage alone. P39 registers them against each other.
- **A change of look** doesn't redraw the tiles drawn before it.
- **An accelerating stage** shears what it draws: the rolling shutter's correction assumes a steady velocity over a frame.
- **Evicted tiles are raw,** 256 KB each: a whole slide at 2 µm would take some 2 GB of scratch. Compressing them is left for later.
- **Roaming during a scan** draws frames the scan then draws over. Pausing roaming while a scan runs would save the work.
- **The page** refreshes OpenSeadragon's whole image, at most three times a second, rather than the changed tiles alone.

## Clean room

Nothing was copied from `microscope` or any other codebase.

- **Bilinear sampling, and the affine map's inverse,** are textbook.
- **Brenner's gradient** is P34's measure (Brenner et al., 1976), over the mean squared (Groen, Young and Ligthart, 1985).
- **A rolling shutter's rows,** exposed a line time apart, are the frame model's (P16).
- **The pyramid's levels,** each half the one below rounded up, follow the Deep Zoom format's convention, which OpenSeadragon reads.
- **OpenSeadragon 6.1.1** is vendored unmodified, under its BSD-3-Clause licence, in `examples/vendor`.
