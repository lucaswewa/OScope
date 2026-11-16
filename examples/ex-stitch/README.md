# ex-stitch

P39's post-acquisition stitching, on the simulator's H&E slide, through the API as a client would.

1. **A blind stage's scan.** The stage has no encoders, so it reports where it was sent, and half its moves lose 6 µm, which add up. A snake of 9 tiles over 3 × 2 mm about the first island goes into a project.
2. **The stitch,** as a job (`stitch.run` through the `jobs` Thing):
   - its tiles' overlaps registered and the global solve run;
   - the mosaic written as a result of the project, with `stitch.json` beside it.
3. **Both against the simulator's truth:** where the stage said each tile was, and where the stitch put it.

## How to try it

```sh
cargo run --release -p ex-stitch
```

It serves at http://127.0.0.1:8243/. Its page shows the newest stitch's mosaic, a thumbnail, and its numbers. On the owner's PC, at real time on the simulator's quarter sensor, it printed:

```text
ex-stitch: a snake of 9 tiles over 3 × 2 mm, the stage stepping blind: where it said it was, 5.48 µm rms off the truth
  stitched in 0.3 s: 12 pairs kept of 12, 0 rejected, missing by 0.05 px rms; the tiles 0.05 µm rms off the truth
  the mosaic: 3822 × 2544 pixels, result … of the project
  open http://127.0.0.1:8243/ to see it
```

- **The tiles are raw,** the camera's Bayer mosaic. They are registered on their 2 × 2 cells, from the acquisition's first level, and placed at even offsets, so the mosaic keeps their colour pattern.
- **The truth** is the simulator's: its stage's true position at each frame's exposure midpoint.

## The throughput bench

```sh
cargo run --release -p ex-stitch -- --bench 40
```

It stitches N synthetic tiles of 20 MP, 16-bit, a tenth overlapping, in rows of five, recorded up to 10 µm off, and times the stitch alone. Plan §5.20 asks for 10 tiles a second. On the owner's PC it printed:

```text
ex-stitch: 40 tiles of 20 MP written in 5.6 s
  registered by 0.5 s, solved by 0.5 s, written by 3.5 s, pyramid by 3.5 s
  stitched in 3.9 s: 10.3 tiles a second; 67 pairs kept of 67, missing by 0.05 px rms; at most 405 MB of parts at once
```

## `--smoke`

```sh
cargo run -p ex-stitch -- --smoke
```

It runs the same at ten times real time, with a temporary data root. It holds the stage to have been over 2 µm off, so that there was something to correct, and the stitched tiles to be within 1 µm of the truth. It also checks that the mosaic's thumbnail and the page are served. It ends with `smoke: ok`, or says what failed and exits with code 1.
