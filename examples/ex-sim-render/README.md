# ex-sim-render

The simulated H&E slide, seen through each objective at several distances from focus (P10).

**The slide** carries procedural H&E tissue from a seed, in islands under the coverslip, with glass between them. The section is 4 µm thick, in two half-layers, so at high magnification different nuclei come into focus at different depths.

**What the example renders:**

- **an overview of the slide,** 3000 × 1000 pixels at 25 µm each. It shows the islands, their holes, and the coverslip's edges;
- **fields through each objective** (Mitutoyo's 5×, 10×, 20× and 50×), 768 × 512 pixels of the MIChrome 20's 2.4 µm:
  - all centred on the nucleus nearest an island's centre, taken from the tissue's ground truth;
  - each at five focus positions, a depth of field apart, from below the section's middle to above it.

For each field it records the rendering time and a sharpness, the mean step in green between pixels about 0.3 µm apart. It writes every image as a PNG, and serves a page that shows them.

## How to try it

```sh
cargo run --release -p ex-sim-render
```

Use `--release`, since a debug build is many times slower. On the owner's PC it prints this, then `ex-sim-render: open http://127.0.0.1:8215/`:

```text
the slide, 3000 × 1000 at 25 µm a pixel: 202 ms
fields of 768 × 512 at (45369, 34052) µm on the stage, seed 1, on a nucleus in the section's lower half:
          pixel    depth        dz      time sharpness
  5x   0.480 µm  31.5 µm -31.49 µm     70 ms    0.0072
  5x   0.480 µm  31.5 µm  +0.00 µm     63 ms    0.0141
  …
  50x  0.048 µm   1.9 µm  -0.95 µm     82 ms    0.0144
  50x  0.048 µm   1.9 µm  +0.00 µm     75 ms    0.0144
  50x  0.048 µm   1.9 µm  +1.91 µm     93 ms    0.0124
```

**Open the address in a browser.** You should see:

- **the slide,** with red boxes where the 5× and 10× fields are;
- **a row of fields for each objective,** sharpest at or next to the middle column, and softer towards the ends:
  - at 5× the stroma's fibres and the nuclei;
  - at 50× a few nuclei, a few micrometres across, with their chromatin and a paler ring of cytoplasm.

**At 50× the sharpest fields are a half-section either side of the middle,** not at it. The section's two half-layers lie 1 µm below and above its middle, and the depth of field is 1.9 µm. So focusing down brings the centred nucleus (in the lower half) into focus, and blurs those of the upper half. The sharpness column shows it.

Ctrl-C stops the server.

**The files** are written to `.oscope/ex-sim-render/data/page/`:

- `overview.png`;
- `field-{objective}-{0…4}.png`;
- `results.json`.

**Options:**

- `--seed N`: other tissue;
- `--width N`: wider fields, two thirds as tall;
- `--port N`: another port.

**`--smoke`** renders 192 × 128 fields and a small overview, on a port the system picks, with a temporary data root. Then it checks:

- that the page, its script, the overview, a field and `results.json` are served;
- that there are 20 fields, and each objective is sharpest within half a depth of field of the section's middle, softer a depth of field away;
- that a field rendered twice is the same to the bit.

```sh
cargo run -p ex-sim-render -- --smoke
```

It ends with `smoke: ok`, or says what failed and exits with code 1.
