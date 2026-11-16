# ex-deep-zoom

P40's deep zoom on the stitched slide, through the API as a client would.

1. **A scan and its stitch.** A snake over 4 × 3 mm about the simulated H&E slide's first island goes into a project and is stitched as a job (`stitch.run`): a raw mosaic, in the camera's Bayer pattern.
2. **Deep zoom, as the stitch ends:**
   - the mosaic's colour overview, its 2 × 2 cells as RGB pixels, with levels of its own;
   - its coarse levels' tiles, made in its default look.
3. **The page** opens it in OpenSeadragon, vendored:
   - levels and gamma sliders, each change a look of its own, whose tiles the server makes and keeps;
   - each tile's time from its `Server-Timing`, those made and those kept apart.

## How to try it

```sh
cargo run --release -p ex-deep-zoom
```

It serves at http://127.0.0.1:8244/. On the owner's PC, at real time on the simulator's quarter sensor, it printed:

```text
ex-deep-zoom: a snake over 4.0 × 3.0 mm, stitched in 0.5 s: a raw mosaic of 5060 × 3372 pixels, 14 levels
  its colour overview and coarse levels (17 tiles) ready 1.0 s after the stitch began
  open http://127.0.0.1:8244/ to browse it
```

- **The routes** it uses: `/api/v1/zoom/{project}/results/{id}/image.dzi`, its tiles under `image_files/`, and `status`.
- **In a browser's developer tools,** each tile's `Server-Timing` says whether it was made or kept, and the server's time.

## The bench

```sh
cargo run --release -p ex-deep-zoom -- --bench 200
```

It writes a raw mosaic of about N megapixels, then times it against plan §5.20's budgets:

- cold tiles, one at a time;
- the same tiles, kept;
- full-screen views, 1920 × 1080, of regions never seen, their tiles fetched six at a time as a browser asks for them, without and with a hardware job holding the instrument.

On the owner's PC it printed:

```text
ex-deep-zoom: a raw mosaic of 16328 × 12246 pixels written in 2.3 s
  its colour overview built in 3.8 s
  a cold tile: 4.86 ms of server time, the median, 5.58 the 95th percentile (budget 5)
  a kept tile: 0.08 ms, the median, 0.11 the 95th percentile (budget 1)
  a full-screen view never seen: 45 tiles in 47 ms, the slowest 14.55 ms of server time (budget 150)
  the same while a hardware job holds the instrument: 45 tiles in 100 ms, the slowest 16.54 ms (budget 150)
```

The views are over localhost, not a LAN.

## `--smoke`

```sh
cargo run -p ex-deep-zoom -- --smoke
```

It runs the same at ten times real time over 2 × 1.5 mm, with a temporary data root, and checks:

- the descriptor;
- a coarse tile, made as the stitch ended, coming from the cache;
- a full-size tile made, then kept, with the same bytes;
- the page and OpenSeadragon served.

It ends with `smoke: ok`, or says what failed and exits with code 1.
