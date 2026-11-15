# ex-live-mosaic

P38's live mosaic, on the simulator's H&E slide, through the API as a client would: a mosaic that builds as frames arrive.

1. **Begun** over 6 × 4 mm about the first island, in pixels of twice the frames' (1.92 µm through the 10×), after the exposure is set.
2. **Roaming:** the stage moves to four places. At each, once it stops, a frame is drawn where the slide is. On the way, frames blurred by the moving stage aren't drawn, nor frames that show a place already drawn and aren't sharper.
3. **Scanning:** a snake of 6 tiles beside them, into a project. Each tile's frame is drawn as the scan takes it.

## How to try it

```sh
cargo run --release -p ex-live-mosaic
```

It serves at http://127.0.0.1:8242/. Its page shows the live view beside the mosaic, which grows as you move the stage with the arrow keys (a quarter of a millimetre; with Shift, a millimetre) or press *Scan 2 × 2 mm here*. *Begin* starts a new mosaic over 10 × 8 mm about the stage, *Halt* stops drawing, and *Clear* empties it. On the owner's PC, at real time on the simulator's quarter sensor, it printed:

```text
ex-live-mosaic: a mosaic of 3125 × 2084 pixels of 1.92 µm over 6 × 4 mm, in 5 levels
  roamed to 4 places: 5 frames drawn, 1 blurred on the way, 9 showing a place drawn
  scanned 6 tiles beside them, each drawn as it was taken: 19 frames drawn in all, in 82 tiles at every level
  open http://127.0.0.1:8242/ to see the live view beside the mosaic; the arrow keys move the stage
```

- **The mosaic** is OpenSeadragon 6.1.1, vendored in [`examples/vendor`](../vendor/README.md), over the `live_mosaic` Thing's tiles at `/api/v1/mosaic/tiles/{level}/{col}/{row}`.
- **Its changes** come with the live view: each preview message carries the tiles changed since the last, and the page fetches those again.
- **Where each frame goes:** the stage's position at the middle of its exposure, through the camera's calibration or, without one, the nominal pixel. The rolling shutter's rows are each drawn where the stage was as they were exposed.
- **Roaming draws during a scan too:** where the stage stops for a tile, a frame may be drawn before the scan's own, which is drawn over it.

## `--smoke`

```sh
cargo run -p ex-live-mosaic -- --smoke
```

It runs the same at ten times real time, with a temporary data root. It holds each place and each of the scan's tiles to a frame drawn, no frame to be dropped or fail, and the tile under the island, the page and OpenSeadragon to be served. It ends with `smoke: ok`, or says what failed and exits with code 1.
