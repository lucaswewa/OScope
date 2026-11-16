# P42: Depth from focus and extended depth of field

- Status: Done
- Branch / PR: `m1/p42-depth-from-focus` / [#57](https://github.com/lucaswewa/OScope/pull/57)
- Size: L · estimated ~2,650 before it started (in `oscope-compute`, the sum-modified-Laplacian focus measure and its CUDA twin, to the bit, ~170; in `oscope-volume`, the focus measures streamed plane by plane, the height from the peak and its parabola, the confidence, the median and the all-in-focus image ~300, and the PLY, STL and OBJ meshes ~200; the pipeline's nodes for each step ~240; in `oscope-workflows`, the depth-from-focus job, from a volume acquisition to its results and mesh, ~250; in `oscope-api`, the job's wiring and float bricks for the heightfield ~50; tests ~700; `ex-depth-from-focus` and its VTK.js page ~450; glue ~50; and about a tenth on top); one pull request, at the owner's standing choice · measured **3,039** hand-written lines added (code 1,453: `oscope-volume` 612, `oscope-workflows` 373, `oscope-pipeline` 266, `oscope-compute` 184, `oscope-api` 18; tests 967; examples 516; glue 103), and 30 removed. **That is 2,139 over L's 900, and 15 % over the estimate.** VTK.js, vendored in P41, isn't counted.

## Summary

A z-stack's heights, their confidence and its all-in-focus image, by depth from focus ([spec](../phases.md#p42); plan §5.16).

- **The focus measure,** the sum-modified-Laplacian (Nayar and Nakagawa, 1994), has a CUDA twin that agrees with the CPU to the bit. `Compute::focus_sml` takes the GPU when there is one.
- **`oscope-volume`** streams a stack a plane at a time:
  - **the heights:** each pixel's from its peak and the parabola through it, with its confidence, their median taken;
  - **the all-in-focus image:** each pixel blended from the planes either side of its height;
  - **meshes** of the heights, as PLY, STL or OBJ with its texture.
- **The pipeline** has a node for each step: `volume.focus`, `volume.height`, `volume.all_in_focus` and `export.mesh`.
- **The job `volume.depth_from_focus`** takes a volume acquisition to three results of its project: the heights, their confidence and the all-in-focus image. It writes a mesh if asked. A raw stack is measured and coloured on its 2 × 2 cells.
- **Float bricks:** the volume routes' `format=f32` serves the heights as stored, the surface as a heightfield brick.
- **`ex-depth-from-focus`** scans a die's dome on the simulated reflective chip at 50×, checks the heights against the simulator's truth, and draws them with VTK.js as a surface textured with the all-in-focus image.
- **On the simulator,** over the dome, the heights lie 0.13 µm RMS from the truth where sure, a quarter of a step. Over the whole frame the median error is 0.07 µm, and the RMS 0.29 µm: the film's pits take their bright rims' focus.
- **P41 is marked Done.**

## Behaviour spec

Written before the code, from the sources at its end ([spec](../phases.md#p42); plan §5.16).

### What it does

1. **Its input is a z-stack:** N planes of one size, N ≥ 3, taken at heights zₖ = z₀ + kΔ, from the lowest, a step Δ > 0 apart. It is a volume acquisition (P41), such as a z-scan's. Heights are the stage's z, in µm. Each plane gives two images:
   - **a grey one, to measure focus:** a raw stack's cells' means, its pyramid's level 1 (P41); an RGB stack's channels' mean; a grey stack's own plane;
   - **a colour one,** for the all-in-focus image, of the same size: a raw stack's cells as RGB pixels, their red, their greens' mean and their blue (P08's superpixel); otherwise the plane itself.

   A raw stack's heights and all-in-focus image are therefore at level 1's size and pixel.
2. **The focus measure,** at each pixel of each plane, is the sum-modified-Laplacian (Nayar and Nakagawa, 1994):
   - **the modified Laplacian,** with a step of s pixels (default 1): ML(x, y) = |2I(x, y) − I(x − s, y) − I(x + s, y)| + |2I(x, y) − I(x, y − s) − I(x, y + s)|, where all four neighbours are inside the image, and 0 elsewhere;
   - **its sum** over the (2r + 1)² window about the pixel, within the image (default r = 2, 5 × 5): F(x, y);
   - **in a fixed order:** along rows first, then down columns. The CUDA twin computes ML and both sums with the same arithmetic, in the same order, with fused multiply-adds off, so that it agrees with the CPU to the bit (ADR-0010).

   Pertuz et al. (2013a) found the Laplacian-based measures, the modified Laplacian among them, among the most accurate for shape from focus. The measure isn't divided by each plane's brightness: a stack's planes share their exposure, and dividing would move a peak wherever defocus changes the mean.
3. **Streamed:** planes are measured one at a time. For each pixel only the running peak, the measures either side of it, and the measures' sum are kept, never the whole volume of measures: memory is a few images of a plane's size, whatever N.
4. **The height** at a pixel comes from k\*, the plane where F is largest (the lowest, if several tie):
   - **inside the stack** (0 < k\* < N − 1): the vertex of the parabola through the measures at k\* − 1, k\* and k\* + 1. That is zₖ\* + Δ(Fₖ\*₋₁ − Fₖ\*₊₁) / (2(Fₖ\*₋₁ − 2Fₖ\* + Fₖ\*₊₁)), within half a step of zₖ\* since Fₖ\* is the largest; or zₖ\* where the three are equal;
   - **at the first or last plane:** that plane's z, as the peak may lie beyond the stack.

   Nayar and Nakagawa (1994) fit a Gaussian through the same three measures; the parabola is the simpler fit. Subbarao and Choi (1995) refine further, along a focused image surface.
5. **The confidence,** from 0 to 1, is (F_max − F_mean) / F_max, F_mean the measure's mean over the planes. It is 0 where F_max is 0, or where the peak is at the stack's first or last plane.
   - **A sharp peak** above a low curve nears 1.
   - **A curve with no peak,** as over featureless glass where only noise measures, nears 0.

   Pertuz et al. (2013b) judge a focus curve's peak against its spread in the same spirit, through a fitted Gaussian; this ratio is the simplest such measure.
6. **The median:** the height map is then filtered by the median of each (2m + 1)² window, default m = 1, with oscope-compute's median (P23). It removes single wrong pixels at steps and on glass; m = 0 leaves the map as it is.
7. **The all-in-focus image:** at each pixel, the colour planes either side of its height, blended linearly. With u = (z − z₀) / Δ, k = ⌊u⌋ and t = u − k, the pixel is (1 − t) Cₖ + t Cₖ₊₁, or C_N₋₁ alone at the top.
   - **Of the methods Valdecasas et al. (2001) compare,** this is the simplest: each pixel from its sharpest plane. Blending the two nearest planes by the sub-step height avoids the seams of choosing one.
   - **Forster et al. (2004)** fuse in complex wavelets, sharper at edges: a follow-up.
8. **Meshes** of the height map:
   - **Vertices:** one at every `stride`-th pixel's centre (default 1). x runs to the right and y up the image, from its bottom-left pixel's centre, at the pixel size; z is the height. All are in µm, right-handed: the surface is seen from above as the image shows it.
   - **Faces:** two triangles for each square of four vertices, split along the same diagonal and wound counter-clockwise seen from above, so their normals point up. A square with a corner whose confidence is under `min_confidence` (default 0) is left out.
   - **PLY** (Turk, 1994): binary, little-endian; each vertex's x, y and z as `float`, and its red, green and blue as `uchar`, from the all-in-focus image; each face a `uchar` count of 3 and three `int` indices.
   - **STL** (3D Systems, 1989): binary; an 80-byte header, the triangles' count as `uint32`, then each triangle's normal and three vertices as `float32` and a `uint16` of 0.
   - **OBJ** (Wavefront): text. `v x y z` for each vertex, `vt u v` for its place in the all-in-focus image, and faces `f a/a b/b c/c`, numbered from 1. A material file beside it, `.mtl`, names the image, written as a PNG (`map_Kd`).
9. **The pipeline's nodes** (P21), a step each:
   - **`volume.focus`:** a stack of grey planes → the stack of their focus measures; parameters `step` and `radius`; on the CPU or the GPU;
   - **`volume.height`:** the stack of focus measures, and the planes' `z0_um` and `step_um` → the height in µm and the confidence; parameter `median`;
   - **`volume.all_in_focus`:** a stack of planes, grey or colour, the height, and `z0_um` and `step_um` → the image;
   - **`export.mesh`:** the height, its confidence, and the all-in-focus image if given → a mesh file in a folder, as `export.dzi` writes. Parameters `format` (`ply`, `stl` or `obj`), `pixel_um`, `stride` and `min_confidence`. In batch runs only, and never over a file already there.
10. **The job `volume.depth_from_focus`,** a compute job: {`project`, `acquisition`, `options`}.
    - **Its results,** two of the project's:
      - the height map, `float32` in µm, with its confidence as a second channel;
      - the all-in-focus image, in the acquisition's samples, RGB for a colour camera.

      Each names the acquisition and the options in its provenance.
    - **A mesh, if asked** (`mesh`: `ply`, `stl` or `obj`): in the height map's exports folder.
    - **On the GPU** when there is one. Its progress is counted in planes, and it is cancelled between planes.
    - **Refused:** an acquisition that isn't a volume, or of fewer than 3 planes.
11. **The surface over HTTP:** the height map's result is served as a heightfield brick, `…/volume/{project}/results/{id}/brick?format=f32`: its samples as stored, `float32`, the header's samples byte 4 (ADR-0029's header gains `f32`). The all-in-focus image is a brick in 8-bit RGB.
12. **`ex-depth-from-focus`** (port 8247):
    - **a z-scan** of the simulated reflective chip at 50× in reflected light, 21 planes 0.5 µm apart about its focus;
    - **the job,** then the heights' error against the simulator's truth, printed;
    - **the page** draws the heightfield with VTK.js, as a surface textured with the all-in-focus image.

### What the tests show

- **Height:**
  - **on a synthetic stack:** a textured surface of smoothly varying height, each plane blurred by its distance from focus. The RMS error is below half a step where the confidence is 0.5 or more;
  - **on the simulator's reflective chip,** in the example's smoke: below half a step where the confidence is 0.5 or more and the surface is continuous across the measure's window.
- **Confidence:** its median is under 0.4 over a featureless region, where only noise measures, and over 0.6 over a textured one.
- **The parabola:** measures sampled from a known parabola give its vertex; a peak at the first or last plane gives that plane and a confidence of 0.
- **CPU and CUDA,** with `OSCOPE_CUDA=1`: the focus measure is the same to the bit on random images, steps and windows, and the job gives the same heights on the GPU as on the CPU.
- **The all-in-focus image,** on the synthetic stack, is nearer the in-focus texture than any one plane is.
- **Meshes,** read back in each format: their vertices, faces and normals; a square under the confidence floor left out.
- **The nodes** give what the library gives, and `export.mesh` won't write over a file.
- **The job,** over a z-scan of the simulator's chip: its results recorded with their provenance; a flat acquisition refused.
- **The heightfield brick's** samples are the stored floats.

### Sources

- Nayar, S. K. and Nakagawa, Y. (1994). Shape from focus. *IEEE Transactions on Pattern Analysis and Machine Intelligence* 16(8), 824–831.
- Pertuz, S., Puig, D. and Garcia, M. A. (2013a). Analysis of focus measure operators for shape-from-focus. *Pattern Recognition* 46(5), 1415–1432.
- Pertuz, S., Puig, D. and Garcia, M. A. (2013b). Reliability measure for shape-from-focus. *Image and Vision Computing* 31(10), 725–734.
- Subbarao, M. and Choi, T. (1995). Accurate recovery of three-dimensional shape from image focus. *IEEE Transactions on Pattern Analysis and Machine Intelligence* 17(3), 266–274.
- Valdecasas, A. G., Marshall, D., Becerra, J. M. and Terrero, J. J. (2001). On the extended depth of focus algorithms for bright field microscopy. *Micron* 32(6), 559–569.
- Forster, B., Van De Ville, D., Berent, J., Sage, D. and Unser, M. (2004). Complex wavelets for extended depth-of-field: a new method for the fusion of multichannel microscopy images. *Microscopy Research and Technique* 65(1–2), 33–42.
- Turk, G. (1994). The PLY polygon file format. Stanford University.
- 3D Systems (1989). *StereoLithography Interface Specification.*
- Wavefront Technologies. *Object Files (.obj)* and *Material Library Files (.mtl)*, appendices to the Advanced Visualizer manual.

## What was built

- **`oscope-compute`:**
  - `focus::sml`, the sum-modified-Laplacian, and its limits (`SML_RADIUS`);
  - its CUDA twin, `Cuda::focus_sml`, two kernels in `kernels/focus.cu`;
  - `Compute::focus_sml`.
- **`oscope-volume`,** until now a placeholder:
  - `depth`: `FocusOptions`, `Planes`, `Peaks`, `median`, `depth_from_focus`, `AllInFocus` and `all_in_focus`;
  - `mesh`: `Mesh`, `MeshFormat`, `MeshOptions` and `write_mesh`;
  - its `cuda` feature, for its GPU test.
- **`oscope-pipeline`:** the nodes `volume.focus`, `volume.height`, `volume.all_in_focus` and `export.mesh`, and stacks as inputs (`Inputs::stack`, `Value::stack`).
- **`oscope-workflows`:** the job, in `depth` (`DEPTH`, `DepthOptions`, `depth_from_focus`, `depth_jobs`).
- **`oscope-api`:** the job, registered by the projects Thing; `format=f32` in the volume routes.
- **Examples:** `examples/ex-depth-from-focus` (port 8247) and its page.

## How to try it

`cargo run --release -p ex-depth-from-focus`, then open http://127.0.0.1:8247/. The [README](../../../examples/ex-depth-from-focus/README.md) shows what it prints.

- **On the owner's PC, at real time,** the z-scan of 21 planes took 21.5 s, and depth from focus 0.5 s, over 684 × 456 cells of 0.384 µm.
- **The page** draws the surface, coloured by the all-in-focus image. Its depth can be stretched up to 40 times, and the cells not sure left out.

## Performance

No §5.20 budget covers depth from focus.

- **The job,** on the CPU in a release build, took 0.5 s over a stack of 21 planes of 1368 × 912 pixels: 684 × 456 cells.
- **The z-scan** took 21.5 s at real time, the simulator rendering each plane.

## Design notes and deviations from the plan

### Where the spec moved

- **The focus measure's defaults are a step of 2 and a window of 7** (r = 3), not 1 and 5.
  - **Why:** on the simulated camera's stack of the dome, a step of 1 measured the camera's noise about as much as the texture. Over 150 cells of the dome, against its sphere, a step of 1 gave 0.95 µm RMS and a median confidence of 0.36.
  - **What else was tried:** a step of 2 gave 0.11 to 0.15 µm and 0.55; a step of 3, 0.13 µm; smoothing the planes first, 0.12 to 0.16 µm. A step tuned to the texture is what Nayar and Nakagawa (1994) advise.
  - **On the synthetic stack,** a step of 2 measures better too: 0.059 µm RMS, every textured pixel sure.
- **Three results, not two:** the confidence is a result of its own, not a second channel of the heights'. The thumbnails and deep zoom show one quantity well; heights in µm and confidence from 0 to 1 in one image would show as red and green, stretched together.
- **The confidence weighs the peak against the whole curve,** so a stack that reaches little beyond the depth of field is less sure. On the job's test, 13 planes, 4 µm under the dome and 2 µm over it, left 36 % of the cells sure; 21 planes, 5 µm either way, left all of them sure. The example and the test take 21.
- **Smooth across the window a height draws on:** the measure's 7 cells and the median's cell either side, 9 in all.
- **The pits:** over the whole frame, the film's pits, small dark squares in bright aluminium, take the focus of their rims. Their cells are sure, but up to a µm high. The full frame's RMS, 0.29 µm, is over half a step; its median, 0.07 µm, isn't. The smoke's check covers the dome, as the spec's does.

### What the mutants' reading found

Each mutant broke the code on purpose, then the tests were run: forty-four of forty-four caught, two of them in the CUDA kernel, run on the GPU.

- **Reading the tests against the list first** found these gaps, now covered:
  - the heights' median, never compared with the filter's;
  - the focus node, run with its defaults only, so ignoring its parameters would pass;
  - the mesh node, never given a confidence;
  - the job's refusal, tested on one plane, which fails both its conditions at once.
- **The first run missed two,** and one didn't compile:
  - a tie keeping the later plane. The test's tie was of two neighbours, and the parabola through two equal neighbours has its vertex midway, whichever is taken. The tie is now of two planes apart.
  - OBJ numbered from 0. The face line the test looked for was another face's, numbered from 0. It now checks the first face.
  - a cancel ignored, which needed its type written out. It was then caught.

| Mutant | Caught by |
|---|---|
| The modified Laplacian's far edges unchecked; the larger of the two; across not made positive; the window one short; down columns, the same pixel; a step of 0 taken; the widest window refused | the sum-modified-Laplacian's definition |
| The kernel's modified Laplacian across alone; its window one short | the twin, on the GPU |
| The CPU's measure of no window | the nodes |
| A tie keeping the later plane; no measure before or after the peak; no sum; a peak at the last plane fitted; the vertex the wrong way; the confidence over the mean | the parabola's vertex |
| No median | the synthetic surface |
| Heights past the top not held to it; the blend's weights the same; a stack finished short | the blend |
| y down the image; triangles facing down; a stride's last vertex lost; the texture's v down; the confidence floor ignored | the mesh |
| PLY faces of 4; STL normals of 0; OBJ numbered from 0; files written over | each format read back |
| The focus node's radius, the height node's median, or the mesh node's confidence ignored | the nodes |
| Not a volume taken; two planes taken; a cancel ignored | the job's refusals and its cancel |
| Heights from 0; cells the pixels' size; raw planes demosaiced bilinearly; the confidence saved as the heights | the job over the dome |
| Floats clamped; integers to floats unscaled; floats of 2 bytes | float bricks |
| The job registered under another name | `ex-depth-from-focus --smoke` |

### The page

- **The surface** is a point at each cell and two triangles to each square, coloured by the all-in-focus image: `vtkPolyData`, `vtkMapper` with its scalars as colours, and `vtkActor`, its z stretched by the depth slider.
- **Seen in a browser?** Not here. Under Node, against the running example, the page ran with VTK.js's real objects, all but the WebGL render window, without errors. It built 311,904 points and 621,530 triangles, and 393,342 triangles once only the sure were kept; the colours were direct, the depth stretched 10 times, then 20. Drawing it is for the owner's manual test.

### Two cores

- **Five times each, side by side,** with `scripts/two-core.sh` (ADR-0038), every suite P42 adds or changes passed 5 times in 5, each a run in:

  | Suite | Seconds |
  |---|---|
  | `oscope-compute`'s unit tests | 0.4–0.5 |
  | `oscope-volume`'s `depth` | 0.4–0.7 |
  | `oscope-volume`'s `mesh` | 0.3–0.5 |
  | `oscope-pipeline`'s `stock` | 1.0–1.1 |
  | `oscope-workflows`'s `depth` | 13.4–14.2 |
  | `oscope-api`'s `volume` | 6.3–7.2 |
  | `ex-depth-from-focus --smoke` | 44.1–50.0 |

- **They found no fault.**

### The size

L's budget is 900; P42 measured 3,039, 15 % over its estimate of ~2,650.

- **One pull request,** under the owner's standing choice: the measure, its twin, the stack's heights, the job and the example are one path.
- **Against the estimate:**
  - code took 1,453 lines against ~1,210;
  - tests 967 against ~700;
  - the example 516 against ~450;
  - glue 103 against ~50.
- **What the estimate didn't foresee:**
  - the test of the job over the simulator, with the camera's noise;
  - the float bricks' test;
  - the nodes' stack inputs;
  - `oscope-volume`'s crate-level glue.

## Tests

| Crate | Test | What it checks |
|---|---|---|
| `oscope-compute` | `focus` (1 new) | **The sum-modified-Laplacian** is its definition: a bright pixel's, its neighbours', at the edges and two pixels away; summed over windows within the image; refusals |
| `oscope-compute` | `cuda` (1 new) | **The twin,** with `OSCOPE_CUDA=1`: the same to the bit as the CPU on 60 random images, steps and windows, and through `Compute`; colour refused |
| `oscope-volume` | `depth` (3) | **The parabola's vertex** is the height; a peak at an end has its plane and no confidence; a tie keeps the lower plane. **A synthetic surface,** each plane blurred by its distance from it, with noise: heights within 0.059 µm RMS where sure, every textured pixel sure; confidence 0.78 on texture, 0.12 on glass; the median the filter's; the all-in-focus image nearer the texture than the best plane. **The blend** of the planes either side; refusals |
| `oscope-volume` | `mesh` (2) | **Meshes:** vertices y up, normals up, colours, texture places, a confidence floor, strides; PLY, STL and OBJ read back; never written over |
| `oscope-volume` | `cuda` (1) | **On the GPU,** with `OSCOPE_CUDA=1`: the CPU's heights and confidence, to the bit |
| `oscope-pipeline` | `stock` (1 new) | **The nodes,** with parameters of their own, give what the library gives; `export.mesh` with a confidence floor; refusals |
| `oscope-workflows` | `depth` (2) | **The job** over a z-stack of the simulator's dome at 50×, a raw volume with the camera's noise: 99.98 % of the cells sure, 0.037 µm RMS from the truth; three results with their provenance; the image in colour; the mesh. A cancel; three planes that aren't a volume, and a volume of two, refused |
| `oscope-api` | `volume` (1 new, 1 changed) | **A height map** as a float brick, its floats as stored; integers as floats from 0 to 1 |
| `ex-depth-from-focus` | `--smoke` | Over the dome: heights within half a step of the truth, RMS, where sure and smooth, half the cells or more; the all-in-focus image's brick; the mesh; the page and VTK.js |

**Mutation checks:** forty-four of forty-four caught; see what the mutants' reading found, above.

**Where they ran:**

- **On Windows,** `scripts/ci-local.sh --cuda`, run alone on `fc22a46`, in 16 minutes: 58 of 58 steps, 534 tests in 199 suites.
  - **The CUDA tests ran on the GPU,** as P42 adds a CUDA kernel. The twin matched the CPU to the bit, and depth from focus gave the CPU's heights.
  - **The rest:** the Python tests ran for real, and every example's `--smoke`, `ex-depth-from-focus`'s among them. Each suite and smoke run had two CPUs of its own, eight at a time (ADR-0038).
  - The commit since adds only these notes.
- **On Linux,** `scripts/ci-clone.sh` under WSL, run alone on a fresh clone of `fc22a46`, in 6 minutes: 57 of 57 steps, 534 tests in 199 suites, the Python tests real; 737 files, none with a CR. WSL has no CUDA toolkit.
- **On two cores,** five times each, side by side: the suites in the table above.

## Follow-ups and known gaps

- **Small dark features in bright surroundings,** such as the film's pits, take their surroundings' focus. A measure normalised by the local brightness, or the reliability measure of Pertuz et al. (2013b), might tell them apart.
- **The confidence depends on the stack's reach** beyond the depth of field.
- **The all-in-focus image** blends whole planes: Forster et al. (2004)'s wavelets would keep edges sharper.
- **The mesh** keeps every cell, up to a million vertices, with no decimation.
- **The job measures on the GPU** through the compute layer when the server is built with CUDA. The example's builds run on the CPU.

## Clean room

Nothing was copied from `microscope` or any other codebase.

- **The sum-modified-Laplacian and the peak** follow Nayar and Nakagawa (1994); the parabola's vertex is elementary. The confidence is OScope's own ratio, in the spirit of Pertuz et al. (2013b).
- **The all-in-focus image** is the selection by the focus measure that Valdecasas et al. (2001) describe, blended about each height.
- **PLY, STL and OBJ** are written from their published descriptions: Turk (1994), 3D Systems (1989) and Wavefront's.
- **VTK.js 38.0.4** is P41's vendored copy. The page uses its documented classes.
