# P42: Depth from focus and extended depth of field

- Status: In review
- Branch / PR: `m1/p42-depth-from-focus` / [#57](https://github.com/lucaswewa/OScope/pull/57)
- Size: L · estimated ~2,650 before it started (in `oscope-compute`, the sum-modified-Laplacian focus measure and its CUDA twin, to the bit, ~170; in `oscope-volume`, the focus measures streamed plane by plane, the height from the peak and its parabola, the confidence, the median and the all-in-focus image ~300, and the PLY, STL and OBJ meshes ~200; the pipeline's nodes for each step ~240; in `oscope-workflows`, the depth-from-focus job, from a volume acquisition to its results and mesh, ~250; in `oscope-api`, the job's wiring and float bricks for the heightfield ~50; tests ~700; `ex-depth-from-focus` and its VTK.js page ~450; glue ~50; and about a tenth on top); one pull request, at the owner's standing choice

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
