# ex-live-overlays

The live view's overlays, drawn from live statistics of the raw frames, which come with the previews on the `preview` Thing's stream (P18, ADR-0035).

## How to try it

```sh
cargo run -p ex-live-overlays
```

It starts the simulated microscope on the scaled-down sensor, homes the stage, places it over the H&E slide's tissue, turns the light on and starts streaming. Then it prints `ex-live-overlays: open http://127.0.0.1:8219/`.

**On the image:**

- **Drag** to draw an ROI. Its statistics appear on the right, and the focus value is measured over it.
- **Shift-drag** to draw a line. Its profile appears on the right: each channel's raw values, cell by cell along it.
- **Scroll** to zoom, as in `ex-live-view`. **Whole frame** zooms out; **Clear ROI and profile** removes them.
- **Crosshair** marks the image's centre.
- **False colour** maps brightness through a lookup table in WebGL: black, violet, red, orange, then pale yellow.
- **Saturation** marks in pink every preview pixel with a raw sample at full scale under it.

**Beside it:**

- **The histogram** of each channel of the raw mosaic, in 256 bins from 0 to 4095, on a log scale or not.
- **Its percentiles** (1 %, median, 99 %) and the share of each channel at full scale.
- **The ROI's** size and place on the sensor, and each channel's mean, standard deviation, minimum and maximum.
- **The profile,** and **the focus value:** the normalised variance of the green cells. It peaks in focus.

**To see them change:**

- **Saturation:** move the light to the right, then raise the exposure to 25 ms. Pink appears where the slide is brightest, and the histograms pile up at 4095.
- **Focus:** hold z − or z + to move out of focus, and watch the focus value fall and the profile flatten.

**The statistics, with your own client:** ask the stream for them with `stats` beside the other parameters, as the README of `ex-live-view` describes:

```json
{"width": 800, "height": 600, "ack": 0,
 "stats": {"roi": {"x": 600, "y": 400, "width": 120, "height": 80},
           "profile": {"from": [0, 456], "to": [1367, 456]},
           "bins": 256, "mask": true}}
```

- At most 10 a second, the messages carry blocks after their JPEG: the statistics as JSON, and the mask while something is saturated.
- The header's 4 bytes at 72 give the JPEG's length; ADR-0035 gives the blocks' layout.

**Things to know:**

- **Statistics are of the raw frame,** in the sensor's own units: 0 to 4095 for its 12 bits. The image is the 8-bit preview.
- **Histograms are counted from a subsample** of about a million samples, by whole cells of the mosaic. ROI statistics count every sample.
- **The simulated camera gives about 7 frames a second,** so each frame comes with its statistics.

**`--smoke`** serves on a port the system picks, with a temporary data root, then checks:

- that the page is served;
- that statistics arrive with 64 bins, the ROI's, and a profile of 685 samples across the sensor;
- that at full light and 25 ms, a saturation mask arrives.

```sh
cargo run -p ex-live-overlays -- --smoke
```

It ends with `smoke: ok`, or says what failed and exits with code 1.
