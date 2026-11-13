# ex-sim-camera

The simulated MIChrome 20 camera, streaming over the simulated slide (P12).

**The camera** sees the simulated world through the optics as the stage truly moves, on the simulation clock:

- **its modes:** `full`, `bin2` and `bin3` at the MIChrome 20's measured rates for 8 and 16 bits, and `design`, 16 bits at 14.5 fps;
- **a rolling shutter:** each row is exposed a line time after the one above, so a moving stage shears the frame, and blurs it over the exposure;
- **its sensor:** shot, dark and read noise, a 12-bit ADC with a black level, binned pixels summed, and 16-bit frames aligned high;
- **keeping up:** if rendering is slower than the frame interval, the frames that end meanwhile are dropped, and counted in the next frame's `dropped_before`. The stage moves on regardless.

**The example** places the stage over an island of the H&E tissue, jogs it slowly in x, and streams for 2 simulated seconds in each mode and format. For each it prints:

- the frames delivered and dropped, and the rate they come to. Delivered and dropped together match the mode's rate;
- the jitter of the frames' host times;
- the largest error of the position acquisition would record: the stage's position track at each exposure's midpoint, against where the stage truly was.

## How to try it

```sh
cargo run --release -p ex-sim-camera
```

It runs on a quarter-size sensor, 1368 × 912, in real time. On the owner's PC it prints:

```text
the simulated MIChrome 20 on a 1368 × 912 sensor, over H&E tissue, jogging at 0.2 mm/s; 2 s in each mode:
  mode    bits       frame    rated  frames dropped     rate    jitter   position
  full       8  1368 × 912 14.49 fps     10      18 14.49 fps    3.5 ms   0.000 µm
  full      16  1368 × 912 7.24 fps      9       4 7.24 fps    3.3 ms   0.000 µm
  bin2       8   684 × 456 53.14 fps     34      70 53.14 fps    2.7 ms   0.000 µm
  bin2      16   684 × 456 26.57 fps     35      18 26.57 fps    3.1 ms   0.000 µm
  bin3       8   456 × 304 67.26 fps     83      49 67.26 fps    1.3 ms   0.000 µm
  bin3      16   456 × 304 33.87 fps     67       0 33.87 fps    5.7 ms   0.000 µm
  design    16  1368 × 912 14.50 fps     10      17 14.50 fps    2.7 ms   0.000 µm
```

**What the numbers show:**

- **The CPU renderer doesn't keep up** in most modes: a 1368 × 912 frame takes about 0.2 s. So frames are dropped, and each one delivered is still true to its moment. P15 makes the simulator fast enough for full resolution.
- **The rate** counts frames delivered and dropped, and matches the mode's.
- **The position error is zero** during a steady jog: the track's reports lie on the stage's straight path, so interpolating between them is exact. A frame during a change of speed would show an error, at most a report interval's worth of the change.

**Options:**

- `--full`: the whole 5472 × 3648 sensor. Rendering takes a few seconds a frame, so most frames are dropped.

**`--smoke`** runs a 384 × 264 sensor for 0.6 s in each mode, then checks:

- that each mode delivered at least two frames;
- that delivered and dropped frames together come to the mode's rate, within 2%;
- that the recorded positions are within 0.5 µm of the truth.

```sh
cargo run -p ex-sim-camera -- --smoke
```

It ends with `smoke: ok`, or says what failed and exits with code 1.
