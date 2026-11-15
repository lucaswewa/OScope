# ex-plan

P35's scan planners and focus maps, on the simulator's H&E slide, tilted.

1. **A focus map.** It autofocuses at a 3 × 3 grid of points over 4 × 4 mm of the first tissue island, on a slide that rises 1 µm a mm along x and falls 0.5 along y. It prints each point's focus against the simulator's truth. Then it fits a plane and a thin-plate spline to them, and says how close each comes to the truth between the points.
2. **The plans,** each with its estimates, over the MIChrome 20's whole sensor through the 10×, a tenth of each frame shared with its neighbours:
   - a raster and a snake over the whole coverslip;
   - a snake over the tissue only, from a mask of where it is;
   - a snake over the island's 4 × 4 mm, each tile at the map's z;
   - a tiled z-stack over the same, 11 planes 2 µm apart at each tile;
   - a z-scan at the island's middle;
   - the islands' middles, as a list of positions.

## How to try it

```sh
cargo run --release -p ex-plan
```

It serves at http://127.0.0.1:8239/. Its page draws each plan: the region in green, each tile's frame, and the path the stage takes from the dot. Each frame is coloured by its place in the order, from blue to red, or for the plan by the map, by its z. On the owner's PC, at real time through a 1024 × 768 region of the 20 MP sensor's middle, it printed:

```text
the 10× on the MIChrome 20: frames of 1313.3 × 875.5 µm, a tenth shared with each neighbour; turned 0.5°, cells of 1296.4 × 864.2 µm

the focus map: autofocus at 9 points over 4 × 4 mm of the first island, the slide tilted 1 µm a mm along x and -0.5 along y
  point, mm          found µm   truth µm    miss
  (34.02, 12.73)      5029.818   5029.654  +0.164
  (35.35, 12.73)      5030.528   5030.987  -0.459
  (36.68, 12.73)      5032.531   5032.320  +0.211
  (36.68, 14.06)      5031.726   5031.654  +0.073
  (35.35, 14.06)      5030.493   5030.320  +0.172
  (34.02, 14.06)      5028.925   5028.987  -0.062
  (34.02, 15.39)      5028.161   5028.320  -0.159
  (35.35, 15.39)      5029.523   5029.654  -0.131
  (36.68, 15.39)      5030.808   5030.987  -0.179
  a plane: 5030.28 µm at the middle, rising +1.02 µm a mm along x and -0.55 along y
  between the points, against the truth: the plane within 0.12 µm, a thin-plate spline within 0.31; a quarter of the depth of field is 1.97 µm
  each autofocus took 10.8 s, on average

plan                              tiles      grid planes  frames      data    travel        time
raster over the coverslip          1333   43 × 31      1    1333   53.2 GB 3028.3 mm 33 min 23 s
snake over the coverslip           1333   43 × 31      1    1333   53.2 GB 1562.5 mm 30 min 56 s
snake over the tissue               370   37 × 25      1     370   14.8 GB  672.4 mm  8 min 57 s
snake over 4 × 4 mm, by the map      20     4 × 5      1      20    0.8 GB   20.9 mm         6 s
tiled z-stack over 4 × 4 mm          20     4 × 5     11     220    8.8 GB   20.9 mm  4 min 20 s
z-scan at the first island            1     1 × 1     11      11    0.4 GB    0.0 mm        13 s
the islands' middles                  4     4 × 1      1       4    0.2 GB   65.3 mm        51 s
  at 10 mm/s, 50 ms settling and 138 ms a frame, and 10.8 s an autofocus, as on the simulator above
```

- **The cells.** A grid tiles cells: the largest rectangles, square with the stage, that each frame holds. Square with the stage, a frame is its cell. Turned 0.5°, as a calibration might find the camera, the cell is 1.3 % smaller each way, so that the frames still cover what the cells do.
- **Raster or snake.** Both take the same tiles. The raster runs each row from its left, so the stage runs back along every row; the snake runs each row the other way from the last, and travels about half as far.
- **The tissue's mask** is the coverslip rendered at 100 µm a pixel, where it is darker than clear glass in green: a stand-in for P37's tissue detection. A snake over it takes 370 tiles, where the whole coverslip takes 1,333.
- **The z policies.** The plans without the map autofocus every 10 tiles over the coverslip and the tissue, and at every tile elsewhere; the estimates count each autofocus at what one took on the simulator. The plan by the map knows each tile's z before the stage moves, from 5027.6 to 5033.0 µm.
- **The data** is each frame's raw mosaic in 16 bits, before compression: 40 MB a frame.
- **The time** counts the moves in straight lines at the stage's default speed, 50 ms of settling and a frame at the MIChrome 20's 7.24 frames a second for each plane, and each autofocus. Acceleration isn't counted, nor writing the frames.

## `--smoke`

```sh
cargo run -p ex-plan -- --smoke
```

It runs the same on the quarter sensor at ten times real time, through a 256 × 192 region of the sensor's middle, with a temporary data root, and measures the map at 2 × 2 points. It holds the map within a quarter of the depth of field of the truth between the points, the coverslip's snake to its whole grid, and every plan to at least a tile. It checks that the server gives the page and the plans. It ends with `smoke: ok`, or says what failed and exits with code 1.
