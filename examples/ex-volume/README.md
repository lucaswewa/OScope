# ex-volume

P41's volumes, through the API as a client would.

1. **A z-scan.** The simulated fluorescent cells at 50×, lit by the blue line, which excites their membranes' dye, and seen through a confocal's pinhole: 21 planes, 0.5 µm apart, from 4 µm below the cells' focus to 6 µm above it, from under the glass to over the cells.
2. **A volume.** The scan's frames are stored as a volume ([ADR-0029](../../docs/adr/0029-volumes-and-3d-data-on-the-media-plane.md)):
   - its voxel comes from the scan's plan: the frame's pixel at the sample, and the stack's step;
   - its levels halve z too, once their pixels are wider than its planes are apart;
   - it is raw, the camera's Bayer mosaic, so it is served from level 1, each voxel a 2 × 2 cell's mean.
3. **The page** finds the newest volume and fetches a brick of up to 8 million voxels: the finest level that holds them, here level 1, 684 × 456 × 21. VTK.js, vendored, draws it:
   - as a maximum intensity projection, or composited;
   - through a window of its intensities, from black to the dye's green, starting at the brick's median and its 99.9th percentile;
   - its depth stretched up to 8 times, as the stack is 10 µm deep and 263 µm wide.

## How to try it

```sh
cargo run --release -p ex-volume
```

It serves at http://127.0.0.1:8246/. On the owner's PC, at real time, it printed:

```text
ex-volume: a z-scan of 21 planes, 0.5 µm apart, through the fluorescent cells at 50× and a confocal, in 23.8 s
  a raw volume of 1368 × 912 × 21 voxels of 0.192 × 0.192 × 0.5 µm, served from level 1: [684,456,21]
  open http://127.0.0.1:8246/ to see it drawn by VTK.js
```

- **On the page,** drag to turn the volume and scroll to zoom. It opens seen from above the slide and in front of it. The cells are domes on the glass, their membranes bright at their edges.
- **The routes** it uses: `/api/v1/volume/{project}/acquisitions/{id}/meta` and `brick`. The page's brick, 6.6 MB, came in 48 ms over localhost.
- **Slices,** `…/slice?axis=z&index=10`, `axis=y` or `axis=x`, are a header and the voxels, as bricks are; the `volume` module of `oscope-api` describes both.

## `--smoke`

```sh
cargo run -p ex-volume -- --smoke
```

It runs the same at ten times real time, over the frame's middle ninth, 456 × 312 pixels, as the simulator renders each plane afresh, with a temporary data root, and checks:

- the volume's metadata: raw, 21 planes 0.5 µm apart, and voxels the size of the objective's pixel at the sample;
- the page's brick: level 1, the size the metadata gives, and as many voxels as that;
- the pinhole's section: the plane in focus is brighter than those under the glass and over the cells;
- a slice across x, through the middle, holding the brick's own voxels;
- the page and VTK.js served.

It ends with `smoke: ok`, or says what failed and exits with code 1.
