# ex-core-tour

A tour of `oscope-core` (P02), in three parts:

- **Units:** lengths in micrometres and durations in milliseconds, converted from millimetres, nanometres and `std::time::Duration`.
- **A camera-to-stage map:**
  - an `Affine2` fitted by least squares to 24 noisy point pairs over the 20 MP sensor, under the 10× objective;
  - the pixel size, angle and flip read back from the fit;
  - its residual, and its error at the far corner of the sensor.
- **The clock:** a `SystemClock` mapped to UTC, and a `FakeClock` that times an exposure and finds its midpoint.

## How to try it

```sh
cargo run -p ex-core-tour
```

It prints something like this, with today's date and times:

```text
Units
  25 mm of travel is 25000 µm: 104167 pixels of 0.24 µm, the 10x objective's
  an exposure of 2.5 ms is 2.5ms

A camera-to-stage map, fitted to noisy point pairs
  pixel size 0.24000 µm, turned 0.502°, flipped: true
  residual 0.037 µm rms; the far corner 0.032 µm from the truth
  the sensor's corner, at stage (12500, 8000) µm, maps back to pixel (0.01, -0.13)

The clock
  started at 2026-11-12 06:04:31.658361 UTC; now is 0.000007100 s later, 2026-11-12 06:04:31.658368100 UTC
  an exposure from 0.069000000 s to 0.071500000 s has its midpoint at 0.070250000 s
```

The fit's numbers are the same on every run: its noise comes from a fixed seed.

- **The map** is 0.24 µm per pixel, with the image's y flipped against the stage's and turned by 0.5°.
- **The fit** finds all three from the noisy points. The noise is ±0.05 µm on each axis, and the fit is within 0.04 µm of the truth across the sensor.
- **The corner** maps back to a pixel near `(0, 0)`, off by the noise.

`--smoke` also checks the conversions, the fit and the clock. It ends with `smoke: ok`, or lists what failed and exits with code 1.
