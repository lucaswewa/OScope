# ex-scan

P36's scan engine and `scan` Thing, on the simulator, through the API as a client would.

It homes the stage, moves over the first island of the simulated H&E slide at its focus, starts the camera, and makes a project. The slide is tilted, 2 µm higher a mm along x and 1 lower along y. Then:

1. it asks the `scan` Thing's `plan` for a snake over 1 × 1 mm of the island, autofocusing every tenth tile, and prints the preview: the tiles, and the time and data estimated;
2. it runs the scan with `run`, a job, and prints each tile as it arrives, from the Thing's `progress`;
3. it prints the acquisition the scan made: a frame for each tile, each where the stage was when it was taken, with the plan.

## How to try it

```sh
cargo run --release -p ex-scan
```

It serves at http://127.0.0.1:8240/. Its page draws the latest scan's tiles as they arrive: each tile of the plan outlined, filled once its frame is taken, coloured by the z it was focused at, and a dot where the stage is. On the owner's PC, at real time through a 1024 × 768 region of the 20 MP sensor's middle, it printed:

```text
ex-scan: a snake over 1 × 1 mm of the first island: 30 tiles, 5 × 6, of 244.8 × 184.3 µm; about 36 s and 47.0 MB, autofocusing every tenth tile
  tile  1 at (34.911, 13.646) mm, focused at z 5058.46 µm
  tile  2 at (35.129, 13.646) mm, focused at z 5058.46 µm
  …
  tile 10 at (34.911, 13.810) mm, focused at z 5058.46 µm
  tile 11 at (34.911, 13.976) mm, focused at z 5057.68 µm
  …
  tile 21 at (34.911, 14.308) mm, focused at z 5057.55 µm
  …
  tile 30 at (34.911, 14.474) mm, focused at z 5057.55 µm
  scanned in 43.6 s, against 36 estimated
  the acquisition "snake over 1 × 1 mm": 30 frames, each within 1.41 µm of its tile; the plan with it. Open http://127.0.0.1:8240/ to see the tiles.
```

- **The snake** runs each row the other way from the last. The tiles are the region's frames, 1020 × 768 pixels (the camera keeps a region's width a multiple of 12) of 0.24 µm, a tenth of each shared with its neighbours.
- **Autofocus every tenth tile:** tiles 1, 11 and 21 are focused, and the tiles between keep the last z found. Over 1 mm the tilt moves the focus less than a quarter of the depth of field, 1.97 µm.
- **Each frame's position** is where the stage said it was, once settled: within 1.41 µm of the tile's centre, the simulated stage's backlash as rows reverse.
- **Every scan ends where it started:** the stage returns to the island's middle.
- **The estimate** counts the moves at the stage's default speed, 50 ms of settling and a frame for each tile, and 10 s an autofocus. The simulator's autofocus took a little longer.
- **A Python workflow:** with uv installed, the sample [`ext-python-workflow`](../ext-python-workflow/README.md) plans a spiral the same engine runs.

## `--smoke`

```sh
cargo run -p ex-scan -- --smoke
```

It runs the same on the quarter sensor at ten times real time, through a 256 × 192 region of the sensor's middle, with a temporary data root. It holds the acquisition to a frame for each tile, each within 2.5 µm of it, with the plan, and checks the page is served. It ends with `smoke: ok`, or says what failed and exits with code 1.
