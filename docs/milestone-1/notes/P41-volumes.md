# P41: Z-stacks, volume store, slice and brick routes

- Status: In review
- Branch / PR: `m1/p41-volumes` / [#56](https://github.com/lucaswewa/OScope/pull/56)
- Size: L · estimated ~1,760 before it started (in `oscope-store`, a volume's pyramid halving z as well as x and y, as its voxels near a cube, its levels' planes and the OME metadata's z scale, ~220; a z-scan's voxel size from its plan at ingest ~30; in `oscope-api`, the volume routes: metadata, orthogonal slices and bricks with a binary header, uint8 or uint16, at a level chosen to fit, a raw image from its first level, ~320; tests ~550; `ex-volume` and its VTK.js page ~440; glue ~40; and about a tenth on top); one pull request, at the owner's standing choice · measured **1,567** hand-written lines added (code 525: `oscope-api` 402, `oscope-store` 118, `oscope-workflows` 5; tests 574; examples 447; glue 21), and 39 removed. **That is 667 over L's 900, and 11 % under the estimate.** VTK.js, vendored, isn't counted.

## Summary

A z-stack is stored as a volume, with its real voxel size, and served for VTK.js to draw ([spec](../phases.md#p41); plan §5.16, D8; [ADR-0029](../../adr/0029-volumes-and-3d-data-on-the-media-plane.md)).

- **Volumes in the store:** an image whose planes are depths, flagged in its OME metadata.
  - **Its pyramid halves z** as well as x and y once its voxels near a cube, each voxel then the mean of the 2 × 2 × 2 below it.
  - **The OME metadata** gives each level's z scale and translation.
  - **Reads** outside a level's planes or pixels are refused.
- **A z-scan's frames are a volume,** its voxel taken from the scan's plan: the frame's pixel at the sample, and the stack's step.
- **The volume routes,** under `/api/v1/volume/{project}/{kind}/{id}/`:
  - `meta`;
  - `slice`, across z, y or x;
  - `brick`, a box at a level, or at the finest level that holds `max` voxels.

  Slices and bricks are a 64-byte header, then the voxels, in 8 or 16 bits. A raw volume is served from level 1.
- **VTK.js 38.0.4** is vendored. **`ex-volume`** takes a z-scan of the simulated fluorescent cells at 50× through a confocal. Its page draws the scan as a maximum intensity projection or composited.
- **Two places that volumes broke, now mended:** deep zoom's default look read a volume's plane at its coarsest level without mapping it; and the stitch would have taken a z-scan's volume, which it now refuses.
- **P40 is marked Done.**

## What was built

- **`oscope-store`:**
  - `ImageInfo::volume`, `z_halvings` and `level_planes`;
  - the writer's levels made from the planes below them, two where z halves (`halve_stack`);
  - the OME metadata's `"volume": true`, and each level's z scale and translation;
  - the reader's refusal of reads outside a level;
  - a raw image's level 1, its cells' means, however small the image;
  - the ingest's voxel for a z-scan;
  - thumbnails and deep zoom reading a volume's plane where its level holds it.
- **`oscope-workflows`:** the stitch refuses a volume.
- **`oscope-api`:** the volume routes (`volume.rs`).
- **Examples:** `examples/ex-volume` (port 8246) and its page. VTK.js 38.0.4 is in `examples/vendor/`, with its licence and the hashes of its files.

## How to try it

`cargo run --release -p ex-volume`, then open http://127.0.0.1:8246/. The [README](../../../examples/ex-volume/README.md) shows what it prints.

- **The scan:** on the owner's PC, at real time, it took 21 planes, 0.5 µm apart, through the simulated cells' membranes in 23.8 s.
- **The volume:** a raw volume of 1368 × 912 × 21 voxels of 0.192 × 0.192 × 0.5 µm, served from level 1, 684 × 456 × 21.
- **The page** fetches that level as one brick and draws it with VTK.js:
  - as a maximum intensity projection or composited;
  - through a window of its intensities, which starts at the brick's median and its 99.9th percentile;
  - its depth stretched up to 8 times.

## Performance

No §5.20 budget covers volumes.

- **The page's brick,** 6.6 MB, came in 48–49 ms over localhost, three times, in a release build.
- **The z-scan** of 21 planes took 23.8 s at real time, the simulator rendering each plane through the pinhole.

## Design notes and deviations from the plan

### Where the spec moved

- **"Volumes downsampled in xy and z":** z halves only as the voxels near a cube, not at every level (ADR-0029). A thin stack of fine pixels keeps its planes in its first levels: `ex-volume`'s keeps all 21 at level 1, of 0.38 µm pixels, and halves them at level 2, of 0.77 µm.
- **A raw volume is grey,** its level 1's cells' means. The MIChrome 20 is a colour camera, so its stacks are Bayer mosaics. Colour volumes would be demosaiced plane by plane; none is needed yet.
- **Through a confocal:** the example takes its stack through the simulator's pinhole, so each plane is a section. Widefield, every plane would carry the others' blur. The smoke checks the section: the plane in focus is brighter than those under the glass and over the cells.
- **Isosurfaces,** which §5.16 lists, aren't in P41's spec. P43 draws a surface.
- **A raw image always has level 1 now.** `ex-volume`'s smoke takes the frame's middle ninth, 456 × 312, which fits one chunk. Until now such a pyramid had only level 0, the mosaic, and the routes would have served the pattern. A raw image's pyramid now always has its cells' means, however small. Raw images written before keep their one level, and are served from it.

### The smoke's ninth

On two CPUs, `ex-volume --smoke` over the whole frame took 200–225 s, as the simulator renders each plane afresh through the pinhole: the replays' longest run. As `ex-autofocus`'s smoke does, it now takes a region of the sensor's middle, the frame's ninth, with the same 21 planes, and took 65–77 s on two CPUs, beside eight other runs.

### What the mutants' reading found

Each mutant broke the code on purpose, then the tests were run: forty-three of forty-three caught.

- **Reading the tests against the list first** found these gaps, now covered:
  - z halving again once one plane is left;
  - reads past a level's edge;
  - a record from before volumes, without the flag, and the project's records keeping it;
  - a brick's level at its bounds: `max` at a level's own count, and none that fits;
  - 8-bit and float volumes, in either format, deep enough that the thumbnail reads a level that halves z;
  - level 2's array holding its planes, no more;
  - deep zoom through a level that halves z.
- **Two places that volumes broke:** the first found by the deep-zoom test, the second while looking for other reads of a plane at a coarse level.
  - **Deep zoom's default look** read the full-size plane at the coarsest level: plane 12 of a level of 7. In the default look, a tile of any plane past the coarsest level's count was refused (400). The look now maps the plane, as the tiles do.
  - **The stitch** reads, for its flat field, each tile's plane nearest focus at a coarse level, which a volume's halved z may not hold. A z-scan is one tile, with nothing to stitch, so the stitch now refuses a volume.
- **The first run missed three:**
  - every level's planes made, all 13 at level 2. That wrote stray chunks past its array's 7, which readers never see. The test now counts its chunks' planes.
  - a slice past the last plane, which the reader refuses anyway, with the same 400. The test now checks the route's own reason.
  - a box past the last plane, z = 12 and d = 2, at level 2. Rounded, it fits that level's 7 planes, so the reader would serve it. The test now asks for it, and the route refuses it.

| Mutant | Caught by |
|---|---|
| z halved while its step is over the pixel, or with one plane left; every image a volume; unknown voxels as deep ones; levels' planes halved down | a volume's levels; unknown voxels |
| One plane below where z halves; the last odd plane read past the end; the planes below not paired | a volume's levels: the means |
| z's scale not halved; z's centre not moved; the flag not written, or not read back | the OME metadata |
| Each level's array of all the planes; every level's planes made | level 2's array, and its chunks |
| Reads past a level's planes, or its edge | the reader's refusals |
| Old records refused | a record from before volumes |
| A small raw image of one level | a raw image's level 1, however small |
| The thumbnail of the full stack's middle plane | 8-bit and float volumes, as they are committed |
| Deep zoom's chunks, or its default look, of a full-size plane | deep zoom through a level that halves z |
| Any scan a volume; the pixel from the frame's left edge; z the pixel's; a z-scan not marked a volume | a z-scan's frames become a volume |
| A volume stitched | a z-scan's volume refused |
| A raw volume from level 0; the metadata's levels from 0; any level sliced | a raw volume |
| The box's origin rounded up; its end rounded down; z halved with x and y | a box in full-size voxels |
| The level must hold fewer than `max`; the finest level when none fits | the brick's level at its bounds |
| 8 bits from 16 truncated; 16 from 8 shifted; floats to 8 bits truncated, or to 16 at 8's scale; the header's samples always 8 bits | slices and bricks in 8 and 16 bits |
| The header's voxel at full size | the headers |
| A slice past the last plane; a box past the edge | the refusals and their reasons |
| Reports served | the refusals (404) |

### The page

- **The window starts from the brick,** at its median, the glass between the cells, and its 99.9th percentile. A fixed window suited this stack but wouldn't suit others.
- **The camera** starts above the slide and in front of it, the frame's top row at the top: the image's y runs down, and z up from the glass.
- **Seen in a browser?** Not here. Under Node, against the running example, the page ran with VTK.js's real objects, all but the WebGL render window. VTK.js held the brick as the page meant: 684 × 456 × 21 voxels of 0.384 × 0.384 × 0.5 µm, from 8 to 189. The page started as a maximum intensity projection, then composited with an opacity ramp, with no errors. Drawing it is for the owner's manual test.

### Two cores

- **Five times each, side by side,** with `scripts/two-core.sh` (ADR-0038), every suite P41 adds or changes, and those over the code it changed, passed 5 times in 5, each a run in:

  | Suite | Seconds |
  |---|---|
  | `oscope-store`'s `volume` | 4.9 |
  | `oscope-store`'s `ingest` | 8.0–8.8 |
  | `oscope-store`'s `images` | 10.8–12.0 |
  | `oscope-store`'s `zoom` | 2.9–3.9 |
  | `oscope-api`'s `volume` | 6.8–7.9 |
  | `oscope-api`'s `zoom` | 2.8–4.4 |
  | `oscope-workflows`'s `stitch` | 22.4–23.9 |
  | `ex-volume --smoke` | 65.5–77.0 |
  | `ex-deep-zoom --smoke` | 33.9–51.2 |

- **They found no fault.** A first round, with the smoke over the whole frame, passed too, but `ex-volume` took 200–225 s: see the smoke's ninth, above.

### The size

L's budget is 900; P41 measured 1,567, 11 % under its estimate of ~1,760.

- **One pull request,** under the owner's standing choice: the store's volumes, the routes and the example are one path.
- **Against the estimate:**
  - code took 525 lines against ~570;
  - tests 574 against ~550;
  - the example 447 against ~440;
  - glue 21 against ~40.
- **The estimate didn't foresee** the stitch's refusal, the deep-zoom mend or a raw image's level 1, but over-counted the store's part.

## Tests

| Crate | Test | What it checks |
|---|---|---|
| `oscope-store` | `volume` (2) | **A volume's levels** halve z as its voxels near a cube: 1100 × 900 × 13 of 0.3 × 0.3 × 1 µm halves at level 2 alone, to 7 planes. Each voxel there is the mean of the 2 × 2 × 2 below it, or 2 × 2 × 1 at the end. The OME metadata's z scale and translation per level, and its flag; level 2's array of 7 planes, and its chunks no more; reads past a level's planes or edges refused. **Unknown voxels** halve z at every level, 9 → 5 → 3, and no more once one plane is left. **A flat image** keeps its planes. **A raw image** of 456 × 312 has level 1, its cells' means; a grey one of that size has one level. **A record from before volumes,** without the flag, isn't one |
| `oscope-store` | `images` (1 changed) | **A volume's metadata is NGFF 0.5's,** by its schemas, its level 2 halving z |
| `oscope-store` | `ingest` (1 new) | **A z-scan's frames become a volume** of its voxels, 0.5 × 0.5 × 2 µm, from the field's top edge and the stack's step; a tiled stack's don't; the project's records keep the flag |
| `oscope-api` | `volume` (3) | **Over HTTP:** the metadata and levels. Slices across z, y and x, and bricks, are the store's own voxels, in 8 and 16 bits, with their headers. A brick's level is the finest that fits, at its bounds, or the coarsest when none fits. A box in full-size voxels maps to a level. Deep zoom works through a level that halves z. Refusals: 400, each with its reason, among them a box past the last plane that a halved level would hold rounded; and 404. **A raw volume** starts at level 1. **8-bit and float volumes** come in either format, scaled; deep enough that the thumbnail reads a level that halves z |
| `oscope-workflows` | `stitch` (1 new) | **A z-scan's volume** is refused: one place, nothing to stitch |
| `ex-volume` | `--smoke` | A z-scan through the cells, over the frame's middle ninth: a raw volume of 21 planes 0.5 µm apart, of the objective's pixel; the page's brick of level 1, of the metadata's size; the pinhole's section; a slice across x, the brick's own voxels; the page and VTK.js |

**Mutation checks:** forty-three of forty-three caught; see what the mutants' reading found, above.

**Where they ran:**

- **On Windows,** `scripts/ci-local.sh`, run alone on `ea20315`, in 8 minutes: 56 of 56 steps, 524 tests in 194 suites. The Python tests ran for real, and every example's `--smoke`, `ex-volume`'s among them, in 55 s. Each suite and smoke run had two CPUs of its own, eight at a time (ADR-0038). The commit since adds only these notes.
- **On Linux,** `scripts/ci-clone.sh` under WSL, run alone on a fresh clone of `ea20315`, in 6 minutes: 56 of 56 steps, 524 tests in 194 suites, the Python tests real; 722 files, none with a CR.
- **The CUDA tests** weren't run: P41 touches no CUDA path.
- **On two cores,** five times each, side by side: the suites in the table above.

## Follow-ups and known gaps

- **A raw volume is grey:** its cells' means mix the camera's colours.
- **A brick is made whole in memory,** up to 512³ voxels, 128 MiB in 8 bits, unless the client asks for fewer.
- **Only a z-scan's frames are a volume.** A tiled stack's are each tile's stack, and nothing yet stitches them into one volume.
- **No result is a volume yet:** the routes serve results too, for P42 and P43.
- **The page draws one brick,** the finest that fits 8 million voxels. It doesn't stream finer bricks as the view zooms in.
- **No isosurfaces:** P43 draws a surface.

## Clean room

Nothing was copied from `microscope` or any other codebase.

- **The pyramid's z** follows NGFF 0.5's multiscales: each level's scale and translation, as ADR-0021 gives them for x and y. The rule for halving z, and the means, are OScope's own.
- **The header** of slices and bricks is OScope's own.
- **VTK.js 38.0.4** is vendored unmodified, BSD-3-Clause. The page uses its documented classes: `vtkGenericRenderWindow`, `vtkImageData`, `vtkVolumeMapper` and the transfer functions.
- **The simulator's confocal section** is P13's.
