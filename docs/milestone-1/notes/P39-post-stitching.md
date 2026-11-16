# P39: Post-acquisition stitching

- Status: In review
- Branch / PR: `m1/p39-post-stitching` / [#53](https://github.com/lucaswewa/OScope/pull/53)
- Size: {{SIZE}}

## Summary

{{SUMMARY}}

## Behaviour spec

Written before the code, from the sources at its end ([spec](../phases.md#p39); plan §5.15, §5.20).

### What it does

1. **Its input is an acquisition of a scan** (P36):
   - each frame a tile, recorded with the stage's position as the stage reported it;
   - taken through the objective, and at the binning, that its calibration (P32) describes.

   A stepper stage without encoders reports where it was sent, while each frame shows where it was: off by the backlash, the steps it lost and its settling. The stitch places each tile where its image says.
2. **Tiles are placed in the camera's own pixels.** A tile's nominal offset, in pixels, is M⁻¹ (pₖ − p₀): its recorded position less the first tile's, through the inverse of the affine M the scan was planned with. The plan keeps M in its field (P35): its corners on the stage are M times the frame's corners about its centre. A tile is then only moved, never turned or scaled, as M has taken in the camera's turn and pixel. Preibisch et al. (2009) and Chalfoun et al. (2017) place tiles by translations too.
3. **The overlap graph:** two tiles are neighbours if their nominal frames overlap by at least `min_overlap_px` (32) along both axes. In a raster or a snake, that is each tile's neighbours across and down. Diagonal neighbours, overlapping only at a corner, are left out.
4. **Each pair is registered over its overlap,** by P31's phase correlation (Kuglin and Hines, 1975), the pairs in batch:
   - **Over what:** each tile's part is the overlap's rectangle grown by `radius_um` (25 µm), within the frame.
   - **Where:** shifts are searched within that radius of the nominal shift.
   - **Binned:** an overlap longer than 2048 pixels is binned under 2048 first, and its shift scaled back, still sub-pixel (Foroosh et al., 2002).
   - **Raw frames,** a colour camera's Bayer mosaic as recorded, are registered on the sums of their 2 × 2 cells, and their shifts doubled.
   - **Weighted:** a pair's weight is P31's confidence, one minus the ratio of the highest value outside the peak to the peak's. Pairs under `min_confidence` (0.5) are dropped: a featureless overlap, of glass, gives none.
5. **The global solve** takes the offsets xₖ that minimise

   Σ wᵢⱼ ‖xⱼ − xᵢ − dᵢⱼ‖² over the pairs kept, + λ Σ ‖xₖ − oₖ‖² over the tiles,

   for each pair's shift dᵢⱼ and each tile's nominal offset oₖ, with λ = `prior` (0.01) times the pairs' mean weight.
   - **The second sum ties each tile to its nominal place.** It anchors a tile no pair joins, glass say, where the stage put it, and fixes where the mosaic lies. With pairs, it moves a tile a fraction of a pixel.
   - **How it is solved:** the normal equations are the weighted graph Laplacian plus λ I, sparse and positive definite. Conjugate gradients (Hestenes and Stiefel, 1952) solve them, x and y apart, to a residual under 10⁻⁶ pixels.
6. **Outliers by iterative reweighting** (Holland and Welsch, 1977):
   - after each solve, a pair whose residual r exceeds k = 1 pixel has its weight scaled by k / r, Huber's weight (Huber, 1964), and the solve repeats, up to 10 times, until no weight changes by more than 1 %;
   - pairs still over `reject_px` (3 pixels) are then rejected, and counted, and the solve repeats without them.
7. **A flat field, if asked** (`flat_field`), is estimated from the tiles themselves, as CIDRE (Smith et al., 2015) and BaSiC (Peng et al., 2017) do more fully. It is each pixel's median over up to 64 tiles spread over the scan, smoothed by a Gaussian a 32nd of the frame's shorter side wide, and divided by its mean. Each tile is divided by it before blending.
8. **Feathered blending** (Szeliski, 2006):
   - each tile goes at its offset rounded to a whole pixel, or for raw frames to an even one, so that the mosaic keeps their colour pattern;
   - a mosaic pixel is the mean of the tiles over it, each weighted by its distance in pixels to its own nearest edge, plus a half.
9. **Written out of core** as a result of the project: an OME-Zarr v3 image, NGFF 0.5 (Moore et al., 2021; ADR-0021).
   - **Chunk by chunk:** its chunks, 512 pixels square, are made a few at a time in parallel, each from the parts of the tiles over it, read from the acquisition. Then its pyramid is made, a chunk at a time.
   - **Bounded:** what is held at once is the chunks in hand and their tiles' parts, as many chunks as `memory_mb` (256) allows.
   - **Its record:** its provenance names the acquisition and the options, and a file beside its image, `stitch.json`, keeps each tile's offset and the pairs'.
   - **A stack,** a tiled stack's planes, is solved once, on each tile's plane nearest its z, and every plane is written with the same offsets.
10. **The stitch job,** `stitch.run`, a compute job: {`project`, `acquisition`, `options`}.
    - **Progress:** the pairs registered, the solve, then the chunks written.
    - **A cancel** drops the draft.
    - **Its result:** the result's path, and a line: the pairs kept and rejected, the residual's RMS, and the offsets' change from the stage's.
11. **Throughput:** at least 10 tiles of 20 MP a second (§5.20), in a release build on the owner's PC, at bounded memory.

### What the tests show

- **Residuals against the truth:** tiles cut from a known image, their recorded positions off by up to 15 µm, are placed within 0.3 pixels of where they were cut (the RMS), where the recorded positions are pixels off.
- **Featureless tiles:** a scan with blank tiles has the others placed as well. The blank tiles stay at their stage positions, and no pair through them is kept.
- **Outliers:** a pair given a wrong shift is rejected, and the others hold.
- **The memory bound:** the job's working memory stays within its budget over a mosaic many times its size.
- **Throughput:** 20 MP tiles, in a release build: at least 10 a second.
- **`ex-stitch`** scans the simulated slide with the stage's encoders off and steps lost, stitches it, and reports both against the simulator's truth: the stage's positions, and the stitched offsets.

### Sources

- C. D. Kuglin and D. C. Hines, "The phase correlation image alignment method", *Proceedings of the IEEE International Conference on Cybernetics and Society*, 1975, pp. 163–165.
- H. Foroosh, J. B. Zerubia and M. Berthod, "Extension of phase correlation to subpixel registration", *IEEE Transactions on Image Processing* 11(3), 2002, pp. 188–200. doi:10.1109/83.988953
- S. Preibisch, S. Saalfeld and P. Tomancak, "Globally optimal stitching of tiled 3D microscopic image acquisitions", *Bioinformatics* 25(11), 2009, pp. 1463–1465. doi:10.1093/bioinformatics/btp184
- J. Chalfoun, M. Majurski, T. Blattner, K. Bhadriraju, W. Keyrouz, P. Bajcsy and M. Brady, "MIST: accurate and scalable microscopy image stitching tool with stage modeling and error minimization", *Scientific Reports* 7, 2017, 4988. doi:10.1038/s41598-017-04567-y
- M. R. Hestenes and E. Stiefel, "Methods of conjugate gradients for solving linear systems", *Journal of Research of the National Bureau of Standards* 49(6), 1952, pp. 409–436. doi:10.6028/jres.049.044
- P. J. Huber, "Robust estimation of a location parameter", *The Annals of Mathematical Statistics* 35(1), 1964, pp. 73–101. doi:10.1214/aoms/1177703732
- P. W. Holland and R. E. Welsch, "Robust regression using iteratively reweighted least-squares", *Communications in Statistics – Theory and Methods* 6(9), 1977, pp. 813–827. doi:10.1080/03610927708827533
- R. Szeliski, "Image alignment and stitching: a tutorial", *Foundations and Trends in Computer Graphics and Vision* 2(1), 2006, pp. 1–104. doi:10.1561/0600000009
- K. Smith, Y. Li, F. Piccinini, G. Csucs, C. Balazs, A. Bevilacqua and P. Horvath, "CIDRE: an illumination-correction method for optical microscopy", *Nature Methods* 12(5), 2015, pp. 404–406. doi:10.1038/nmeth.3323
- T. Peng, K. Thorn, T. Schroeder, L. Wang, F. J. Theis, C. Marr and N. Navab, "A BaSiC tool for background and shading correction of optical microscopy images", *Nature Communications* 8, 2017, 14836. doi:10.1038/ncomms14836
- J. Moore et al., "OME-NGFF: a next-generation file format for expanding bioimaging data-access strategies", *Nature Methods* 18, 2021, pp. 1496–1498. doi:10.1038/s41592-021-01326-w

## What was built

{{BUILT}}

## How to try it

{{TRY}}

## Performance

{{PERFORMANCE}}

## Design notes and deviations from the plan

{{DESIGN}}

## Tests

{{TESTS}}

## Follow-ups and known gaps

{{FOLLOWUPS}}

## Clean room

Nothing was copied from `microscope` or any other codebase. Each method is written from its sources, as the spec above sets them out.
