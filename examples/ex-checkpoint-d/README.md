# ex-checkpoint-d

Checkpoint D (plan stage 8): the simulated H&E slide histo-scanned while the live mosaic draws it, stitched, then browsed in deep zoom. It is a script over the API, as a client would run it.

1. **The overview** (P37): 6 × 4 mm across the first island's edge, binned 3 × 3, and the tissue found in it.
2. **The live mosaic** (P38), begun over the region and drawing the scan's tiles alone, as the engine takes them.
3. **The tissue's scan** (P36, P37), at full resolution, its focus map measured first.
4. **The stitch** (P39), as a job.
5. **Deep zoom** (P40): the colour overview and coarse levels are made as the stitch ends. The mosaic is browsed on `ex-deep-zoom`'s page.

## How to try it

```sh
cargo run --release -p ex-checkpoint-d
```

It serves at http://127.0.0.1:8245/. On the owner's PC, at real time on the simulator's quarter sensor, it printed:

```text
ex-checkpoint-d: an overview of 6 × 4 mm across the first island's edge: 23.8 mm² of tissue
  the tissue scanned: 25 tiles, 25 drawn on the live mosaic as they were taken
  stitched: 40 pairs kept of 40, missing by 0.00 px rms; a mosaic of 6290 × 4194 pixels in 14 deep zoom levels
  in 146 s; open http://127.0.0.1:8245/ to browse it
```

- **The page** opens the newest stitched mosaic in OpenSeadragon, with levels and gamma, as `ex-deep-zoom`'s does.
- **The stage** has its encoders here, so the stitch has nothing to correct: the tiles were where the stage said. `ex-stitch` shows a blind stage's.

## `--smoke`

```sh
cargo run -p ex-checkpoint-d -- --smoke
```

It runs the same quickly, with a temporary data root:

- ten times real time;
- 3 × 2 mm;
- binned 3 × 3 throughout;
- a focus map of 2 × 2.

It checks:

- every tile of the scan drawn on the live mosaic, none dropped;
- the stitch keeping pairs;
- deep zoom's descriptor;
- a coarse tile, made as the stitch ended, coming from the cache;
- a full-size tile;
- the page.

It ends with `smoke: ok`, or says what failed and exits with code 1.
