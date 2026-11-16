# ex-depth-from-focus

P42's depth from focus, through the API as a client would.

1. **A z-scan:** the simulated reflective chip at 50× in reflected light, over a die's dome, 21 planes 0.5 µm apart, from 5 µm under the dome's top to 5 µm over it. The frames are the camera's raw mosaic, stored as a volume (P41).
2. **Depth from focus,** the job `volume.depth_from_focus`, makes three results of the project:
   - **the heights,** in µm, measured on the stack's 2 × 2 cells by the sum-modified-Laplacian (Nayar and Nakagawa, 1994), each at the vertex of the parabola about its peak, their median taken;
   - **their confidence,** from 0 to 1;
   - **the all-in-focus image,** each cell blended from the planes either side of its height.

   It also writes a mesh of the heights, as PLY, in the heights' `exports` folder.
3. **Against the truth:** the simulator gives each cell's true height. The error is printed over the cells that are sure, a confidence of 0.5 or more, and smooth, where the truth varies by no more than a step across the 9 cells a height draws on.
4. **The page** draws the heights with VTK.js, vendored, as a surface coloured by the all-in-focus image, its depth stretched up to 40 times; the cells not sure can be left out.

## How to try it

```sh
cargo run --release -p ex-depth-from-focus
```

It serves at http://127.0.0.1:8247/. On the owner's PC, at real time, it printed:

```text
ex-depth-from-focus: a z-scan of 21 planes, 0.5 µm apart, over a die's dome at 50× in reflected light, in 19.4 s
  depth from focus in 0.4 s: heights of 684 × 456 cells, 73 % of them sure
  against the simulator's truth, over 190070 cells sure and smooth: 0.297 µm rms, 0.054 µm the median (half a step 0.25)
  open http://127.0.0.1:8247/ to see the surface drawn by VTK.js
```

- **On the page,** drag to turn the surface and scroll to zoom. The frame holds the dome in the middle, the pyramid's terraces left and right, the film's pits above and below, and the ridge's slopes at the corners.
- **The median error is 0.05 µm,** a tenth of a step. The RMS, 0.27 to 0.30 µm from run to run, is over half a step because of the pits: small dark squares in a bright film, which take the focus of their bright rims. The dome alone, in the smoke, is 0.10 µm RMS.
- **The truth** is placed where the stage was, as the scan's first frame records it: the simulated stage's play can leave it a few µm from where it was sent.
- **The routes** it uses: the job through `jobs/submit`, and each result as a brick, `/api/v1/volume/{project}/results/{id}/brick`, the heights as `format=f32`.

## `--smoke`

```sh
cargo run -p ex-depth-from-focus -- --smoke
```

It runs the same at ten times real time, over the frame's middle ninth, 456 × 312 pixels, which the dome fills, with a temporary data root. It checks:

- the heights within half a step of the truth, RMS, over the cells sure and smooth, half the cells or more;
- the all-in-focus image's brick: 8 bits, red, green and blue, a pixel for each cell;
- the mesh written;
- the page and VTK.js served.

It ends with `smoke: ok`, or says what failed and exits with code 1.
