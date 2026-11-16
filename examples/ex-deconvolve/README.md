# ex-deconvolve

P43's deconvolution, through the API as a client would.

1. **A widefield z-scan:** the simulated fluorescent cells at 50×, lit by the blue line, which excites their membranes' dye, and seen without a pinhole. It takes 21 planes 0.5 µm apart, from 5 µm under the cells' focus to 5 µm over it. The frames are the camera's raw mosaic, stored as a volume (P41).
2. **Deconvolution,** the job `volume.deconvolve`, makes a result of the project: the stack's 2 × 2 cells, its background taken off, deconvolved by Richardson–Lucy over 20 iterations. The PSF is Born and Wolf's for a widefield microscope at the objective's NA. The result is a volume of floats.
3. **Against the truth:** the simulator gives the cells' membranes, averaged through each voxel's depth. The NRMSE of the stack and of its deconvolution against them is printed, each standardised, so that units and offsets don't count.
4. **The page** draws either, the stack or its deconvolution, with VTK.js, vendored:
   - as a maximum intensity projection;
   - composited through a transfer function;
   - as an isosurface, by marching cubes;

   with its level and its depth on sliders.

## How to try it

```sh
cargo run --release -p ex-deconvolve
```

It serves at http://127.0.0.1:8248/. On the owner's PC, at real time, it printed:

```text
ex-deconvolve: a widefield z-scan of 21 planes, 0.5 µm apart, through the fluorescent cells at 50×, in 19.9 s
  deconvolved in 11.0 s: 684 × 456 × 21 voxels, Richardson–Lucy by Born and Wolf's PSF at NA 0.55
  against the cells' true membranes: NRMSE 1.171 the stack, 1.149 deconvolved
  open http://127.0.0.1:8248/ to see them drawn by VTK.js
```

- **On the page,** switch between the stack and its deconvolution, and between the three ways to draw them; drag to turn the volume and scroll to zoom.
- **The NRMSE moves little,** as widefield at NA 0.55 resolves about 3 µm in depth, and the membranes are skins 0.3 µm thick. The simulator's own blur is a Gaussian, not Born and Wolf's PSF. Deconvolution gathers light back towards the cells, which shows best as an isosurface or a projection from the side, with the depth stretched.
- **The routes** it uses: the job through `jobs/submit`, and each volume as a float brick, `/api/v1/volume/{project}/{kind}/{id}/brick?format=f32`.

## `--smoke`

```sh
cargo run -p ex-deconvolve -- --smoke
```

It runs the same at ten times real time, over the frame's middle ninth, 456 × 312 pixels, with a temporary data root. It checks:

- the deconvolution nearer the cells' truth than the stack, by the NRMSE;
- the result's voxels, one for each of the stack's cells;
- the page and VTK.js served.

It ends with `smoke: ok`, or says what failed and exits with code 1.
