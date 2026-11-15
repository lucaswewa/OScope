# ex-histo-scan

P37's histological scan, on the simulator's H&E slide, through the API as a client would: scan only where there's tissue.

1. **The overview.** It sets the camera to `bin3` and the exposure for it, as a binned pixel gathers nine times the light. Then it runs the `scan` Thing's `overview` over 8 × 6 mm across the first island's edge: frames a field apart, reduced to colour in pixels of 50 µm.
2. **The tissue found:** its saturation's threshold, by Otsu's method, its area, and its pieces. It checks the tissue against the simulator's own truth, every 50 µm, and counts the tiles of the `tissue` pattern against a raster's over the region.
3. **The scan.** Back at full resolution, with the exposure set again, it runs the `tissue` pattern: a snake over the tissue, its focus map measured at 3 × 3 of its tiles before the first. The frames go into a project.

## How to try it

```sh
cargo run --release -p ex-histo-scan
```

It serves at http://127.0.0.1:8241/. Its page shows the latest overview, the tissue found in it shaded green, and the scan's tiles over it: outlined as planned, filled as their frames are taken. On the owner's PC, at real time on the simulator's quarter sensor, it printed:

```text
ex-histo-scan: an overview of 8 × 6 mm across the first island's edge: 49 frames binned 3 × 3, 160 × 120 pixels of 50 µm, in 16.1 s
  tissue at saturation over 0.234: 25.5 mm² in 3 pieces, 1 dropped as dust
  38 tiles over the tissue, where a raster over the region takes 56; the simulator's truth: 10007 of 10021 points of tissue, every 50 µm, in a tile (99.86 %)
  scanned in 123.6 s: a focus map of 9 points, 0 rejected, missing 0.27 µm rms; 38 frames into the project
  open http://127.0.0.1:8241/ to see the overview, the tissue and its tiles
```

- **The quarter sensor,** 1368 × 912 pixels, as the simulator renders each new place of a 20 MP frame for seconds. Its field through the 10× is the MIChrome 20's.
- **Binned for the overview:** `bin3` has nine times fewer pixels to take and render, with the same field.
- **Tissue by saturation:** stained tissue is coloured, and glass is grey. Otsu's threshold divides the two, and is never under 0.05, so an empty slide finds none. A closing and an opening clean the edges; a piece under 0.05 mm² is dust; and a margin of 100 µm takes in the edges the coarse pixels blur.
- **A third fewer tiles** than a raster here, half the region being glass. Over a whole coverslip, where tissue lies in islands, the saving is larger: P35's `ex-plan` found 370 tiles over the tissue against 1,333.
- **The focus map** is measured by autofocus at the 9 tiles nearest a 3 × 3 grid over the tissue, and its plane gives each tile's z before the first is taken.

## `--smoke`

```sh
cargo run -p ex-histo-scan -- --smoke
```

It runs the same at ten times real time, with a temporary data root, but quicker to render in a debug build: it scans the tissue binned 3 × 3 too, and measures the focus map at 2 × 2 tiles, not 3 × 3, as each autofocus renders a sweep of frames. It holds the tissue to 99 % of the truth covered, its tiles to fewer than a raster's, and the project to a frame for each tile. It ends with `smoke: ok`, or says what failed and exits with code 1.
