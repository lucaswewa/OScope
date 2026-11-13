# ex-sim-3d

The simulator's 3D samples, stepped through in z, beside their ground truth (P13).

**The reflective surface** is a 5 × 5 mm silicon chip of 200 µm dies. Each die has four quadrants:

- a stepped pyramid, with terraces 1 µm apart;
- a dome 3 µm high;
- a ridge of two slopes;
- an aluminium film with pits etched through it.

The chip is tilted slightly, and its height map is known everywhere: the ground truth for depth from focus.

**The fluorescent cells** are a culture in three dyes:

- nuclei in a DAPI-like blue;
- membranes in a green like Alexa Fluor 488;
- organelles in an orange like Alexa Fluor 568.

Their true volume, each dye's density at every point, is known too. The instrument has no fluorescence; the simulator does, in widefield and through a confocal.

**What the example does:**

- **The surface,** at 20×: 17 planes from 0 to 8 µm above the slide in reflected light, over a die's middle, where its four quadrants meet. Then depth from focus, for each pixel: the plane where the modified Laplacian is greatest (Nayar and Nakagawa, 1994), to a parabola. It prints how close that comes to the true height map.
- **The cells,** through a simulated 60× oil objective of NA 1.4: 13 planes from 0 to 6 µm, in widefield and confocal, each image the violet, blue and green lines' added. Beside each, a slice of the true volume. It prints each plane's mean light in both modes.

It writes every image as a PNG, and the truths as raw files, and serves a page that steps through z.

## How to try it

```sh
cargo run --release -p ex-sim-3d
```

Use `--release`, since a debug build is many times slower. On the owner's PC it prints this, then `ex-sim-3d: open http://127.0.0.1:8216/`:

```text
the surface at 20×, 768 × 512 at 0.120 µm, over the die at (32500, 10500) µm: 17 planes, 225 ms each
  heights 1.73 to 6.75 µm; depth from focus: median error 0.007 µm, 86% of pixels within 0.25 µm
the cells through a 60× oil objective of NA 1.4, 640 × 480 at 0.080 µm, on the cell at (45007, 9979) µm: 13 planes, 770 ms an image of three lines
  the confocal's optical section: 0.85 µm here; 4.05 µm through the instrument's dry 50×
       z  widefield   confocal
  0.0 µm     0.0148     0.0060
  0.5 µm     0.0147     0.0066
  …
  3.0 µm     0.0148     0.0025
  …
  6.0 µm     0.0161     0.0002
```

**Open the address in a browser.** You should see:

- **the surface:**
  - a slider through z. At each z, the parts of the die at that height are sharp: the terraces one by one, a ring of the dome, a band of the ridge, then the film, whose pits come into focus 1.5 µm lower;
  - the true height map, from blue to yellow;
  - what depth from focus found, on the same scale. It agrees almost everywhere. It misses at the steps and the pits' edges, where a bright edge's blur misleads the focus measure;
- **the cells:**
  - a slider through z;
  - widefield, a haze of every depth's light;
  - the confocal, a clean section: the membrane's ring, the nucleus, a few organelles;
  - the true volume at that z, which matches the confocal's picture.

**What the numbers show:**

- **Depth from focus finds the height** to within 0.007 µm for half the pixels. The slabs the surface is rendered in are 0.5 µm apart; a pixel's light is shared between the two either side of its height, so its blur follows its height closely.
- **Widefield keeps all the light** at every z, about 0.015: out of focus it is blurred, not lost.
- **The confocal passes only the light near focus:** about a sixth of widefield's through the cells, and an eightieth above them, at 6 µm. Its optical section through this objective is 0.85 µm. Through the instrument's dry 50× it would be 4.05 µm, as thick as the cells.

Ctrl-C stops the server.

**The files** are written to `.oscope/ex-sim-3d/data/page/`:

- `surface-{0…16}.png`, `surface-truth.png`, `surface-found.png`;
- `cells-wide-{i}.png`, `cells-confocal-{i}.png`, `cells-truth-{i}.png`;
- `surface-height.f32`: the true height map, 512 rows of 768, in µm, little-endian;
- `cells-truth.f32`: the true volume, 13 planes of 480 rows of 640 pixels of the three dyes' densities, little-endian;
- `results.json`, which describes them.

**Options:**

- `--seed N`: another chip's grain and another culture;
- `--port N`: another port.

**`--smoke`** renders a 192 × 128 surface and 160 × 120 cells at five planes, on a port the system picks, with a temporary data root. Then it checks:

- that the page, the images and the truth are served;
- that depth from focus has a median error under 0.25 µm;
- that above the cells, widefield keeps over 70% of the light at their middle, and the confocal under 80% of what widefield keeps;
- that a plane renders the same twice.

```sh
cargo run -p ex-sim-3d -- --smoke
```

It ends with `smoke: ok`, or says what failed and exits with code 1.
