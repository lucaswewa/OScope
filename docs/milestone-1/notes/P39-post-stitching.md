# P39: Post-acquisition stitching

- Status: In review
- Branch / PR: `m1/p39-post-stitching` / [#53](https://github.com/lucaswewa/OScope/pull/53)
- Size: L · estimated ~2,500 before it started (the overlap graph and the pairs' parts ~120; their registration in batch, binned where long, ~120; the global solve by conjugate gradients with reweighting and rejection ~200; the flat field ~100; the blend and the out-of-core writer ~220; the stitch job, from the acquisition to the result, ~220; the simulator's encoders and lost steps as settings ~25; the job's wiring ~20; tests ~600; `ex-stitch`, its page and its throughput bench ~450; glue ~40; and about a tenth on top); one pull request, at the owner's standing choice · measured **2,880** hand-written lines added (code 1,546: `oscope-workflows` 834, `oscope-stitch` 620, `oscope-store` 64, `oscope-sim` 24, `oscope-api` 4; tests 789; examples 505; glue 40), and 18 removed. **That is 1,980 over L's 900, and 15 % over the estimate.**

## Summary

A scan's tiles registered against each other, solved together and written as one mosaic, a result of its project ([spec](../phases.md#p39); plan §5.15, §5.20; [ADR-0027](../../adr/0027-post-acquisition-stitching.md)).

- **Pairs** are the tiles side by side. Their overlaps, binned under 2048 pixels, are registered in parallel by P31's phase correlation, near the stage's shift. Those under a confidence of 0.5, glass say, are dropped.
- **The global solve** is weighted least squares by conjugate gradients, each tile tied weakly to where the stage put it. Huber's weights weigh down the pairs that miss, and the worst pair over 3 pixels is then rejected, one at a time.
- **A flat field,** if asked, is estimated from the tiles' own median. The tiles are then feathered into each other.
- **The mosaic** is written out of core, chunk by chunk, as an OME-Zarr result of the project, within a memory budget (256 MB). Level 1 is made as level 0 is written. `stitch.json` beside it keeps the offsets and the pairs.
- **The job `stitch.run`** runs it through the `jobs` Thing, with its progress and a cancel.
- **The simulator's stage can step blind:** without `encoders` it reports where it was sent, and `lost_steps` makes it lose steps.
- **On the simulator,** a blind stage's 9 tiles lay 5.48 µm rms off the truth; stitched, 0.05 µm.
- **Throughput:** about §5.20's 10 tiles of 20 MP a second: 10.0–10.3 over 60 tiles, and 9.5–11.9 over 40, 10.3 the median, holding at most 203 MB of the 256.
- **P38 is marked Done.**

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

- **`oscope_stitch::tiles`:** `Tiles`, a scan's frames at their nominal offsets: its `pairs` side by side, and each pair's `Crops`, the parts registered. `Placed`, the tiles on whole pixels, and those `over` a chunk of the mosaic. `PixelBox`.
- **`oscope_stitch::solve`:** `solve`, from the nominal offsets and the pairs' `Link`s, under `SolveOptions`, to a `Solution`: the offsets, the links kept and rejected, and their miss.
- **`oscope_stitch::blend`:** `blend`, a chunk of the mosaic from its tiles' `Part`s, feathered; `flat_field`, a `Flat` from the tiles.
- **`oscope_workflows::stitch`:** `stitch`, from an acquisition to a result, under `StitchOptions`, with its `StitchReport`; the job `stitch.run` (`stitch_jobs`), and `result_image`.
- **`oscope-store`:** `ImageWriter::write_level` and `made`, so that a level above 0 can be written as it is made; `halve_samples`, as `finish` halves; and each level's rows of chunks made in parallel.
- **`oscope-sim`:** the stage's `encoders` (on, as before) and `lost_steps` (none) as settings.
- **`oscope-api`:** the projects Thing registers the stitch job.
- **`examples/ex-stitch`** (port 8243): a blind stage's scan stitched through the API, against the simulator's truth; its page; and the throughput bench.

## How to try it

`cargo run --release -p ex-stitch`, then open http://127.0.0.1:8243/. The [README](../../../examples/ex-stitch/README.md) shows what it prints. On the owner's PC, at real time, the stage steps blind over a snake of 9 tiles across 3 × 2 mm, half its moves losing 6 µm, and the stitch takes 0.3 s. The page shows the mosaic's thumbnail and its numbers. `cargo run --release -p ex-stitch -- --bench 40` times the stitch of 40 tiles of 20 MP.

## Performance

§5.20 asks for at least 10 tiles of 20 MP a second, at bounded memory. On the owner's PC, in a release build, `ex-stitch --bench N` stitched N synthetic 16-bit tiles of 5472 × 3648 pixels, a tenth overlapping, in rows of five, recorded up to 10 µm off, under the default budget of 256 MB:

| Tiles | Runs | Tiles a second | Median | Held at most |
|---|---|---|---|---|
| 20 | 6 | 8.6–11.7 | 9.6 | 200 MB |
| 40 | 9 | 9.5–11.9 | 10.3 | 203 MB |
| 60 | 6 | 10.0–10.3 | 10.1 | 202 MB |

- **Where the time goes,** for 40 tiles in 3.4–4.2 s: the pairs are registered and solved by 0.5–0.6 s, and levels 0 and 1 written by 3.0–3.6 s. The pyramid's upper levels and the thumbnail take the rest.
- **Every pair is kept,** missing by 0.04–0.05 pixels rms.
- **The store's files bound it:** each chunk is a file, which costs Windows some 9 ms to write, and 40 tiles make some 2,600 chunks at level 0.
- **The runs vary** by up to a quarter, on a PC in use, with VS Code and rust-analyzer running.
- **At 20 tiles,** the pyramid's top and the thumbnail, the same for any scan, weigh more: 2 runs of 6 reached 10 a second.

## Design notes and deviations from the plan

### Where the spec moved

- **The job and its writer are in `oscope-workflows`,** in layer 4: they read and write the store and run as a job, which `oscope-stitch`, in layer 1, can't depend on ([ADR-0005](../../adr/0005-layered-crates-and-their-dependency-rules.md)). `oscope-stitch` keeps the methods: the pairs, the solve, the blend and the flat field.
- **The parts registered are the nominal overlap itself,** not grown by the search radius. Phase correlation compares two parts of one size, and grown, each would hold more of what the other doesn't. The radius bounds the search instead.
- **Pairs are the tiles side by side.** At a fifth's overlap, a raster's neighbours corner to corner overlap by more than 32 pixels along both axes too. A pair now also overlaps by half a frame along one axis.
- **Binned parts are read from the acquisition's level 1,** where the binning is even: its pixels are means of 2 × 2, so the parts are cut at even pixels. Raw tiles are binned by 2 at least, on their cells.
- **Rejection is one pair at a time,** the worst first, rather than all those over 3 pixels at once: a wrong pair drags its neighbours' links off too, until it is gone.
- **The flat field is estimated at a coarse level,** the coarsest at least 64 pixels on its shorter side, and applied at its pixels there. Reading 64 tiles whole would cost more than the stitch.
- **Level 1 is made as level 0 is written,** a quarter of a chunk at a time, and the store's pyramid then starts at level 2. Made after level 0, as the spec had it, it read all of level 0 back: a second of the 4 that 40 tiles took.
- **Throughput is the bench's,** not a test's: CI's shared runners can't time it.

### From 3.7 tiles a second to 10

The first whole run of the bench stitched 20 tiles in 5.3 s, 3.7 a second. What brought it to §5.20's 10:

- **each level's rows of chunks made in parallel,** in the store, which speeds every acquisition's pyramid too;
- **a cache of the acquisition's decoded chunks:** a chunk of the mosaic takes parts of up to four tiles, and each of their chunks is decoded once;
- **the pairs registered in parallel,** a batch at a time;
- **a pool of writers,** as many as the budget holds;
- **level 1 made by the writers;**
- **the parts read from level 1** where they are binned by 2.

The bench's texture was first a sum of sines, which repeat: 8 of 31 pairs were registered a period off, and dropped. Value noise doesn't repeat; all 31 are kept.

### The memory bound, from the bench

The budget was raised to 512 MB while the throughput work went on. Measured again at the spec's 256 MB, the stitch was as fast, but 60 tiles held 261–275 MB at once, over the budget.

- **Why:** rayon split the chunks among its writers, and each worked down a far part of the mosaic. Each part had a band of level 1's quarters waiting for their others, while their room held one band's. At 512 MB, the writers' unused share hid it. The memory test, whose budget holds one writer, couldn't see it.
- **The fix:** the chunks go a band of two rows at a time, a row of level 1's, the band's chunks in parallel. No more than a band's quarters wait. 20 to 60 tiles then held 192–203 MB, at the same speed.
- **The default is the spec's 256 MB again.**
- **The bench fails** if the stitch held more than its budget. The tests' mosaics are too small for several writers' quarters to outgrow their room.

### The tests' turns

- **Precision and memory apart.** One test checked both at first, and once found the tiles 0.334 pixels off the truth, over its 0.3. They are two tests now. The precision test stitches 30 tiles, 10.4 pixels off before and 0.17 after. The memory test stitches 110, holding at most 8.9 MB of its 10, the tiles 0.21 pixels off, within its 0.5: the miss grows with the raster, as the links' errors add up between far tiles.
- **The mosaic is the slide** only where each tile lands on its own whole pixel. The solve keeps the stage's mean offset, through its prior, so the test's errors sum to none, the first tile's none.
- **Narrow overlaps:** in the test's raster some overlaps fall under the confidence and are dropped. The test expects some, and under a quarter.

### Two cores

- **Five times each.** Pinned to two cores, every suite P39 adds or changes passed 5 times in 5, each a run in:

  | Suite | Seconds |
  |---|---|
  | `oscope-stitch`'s `stitch` | 3.6–4.1 |
  | `oscope-stitch`'s `canvas` | 2.0–2.1 |
  | `oscope-workflows`' `stitch`, five at once | 30.8–31.3 |
  | `oscope-store`'s `images` | 10.8–11.7 |
  | `oscope-store`'s `ingest` | 6.9–7.4 |
  | `oscope-api`'s `projects` | 0.3–0.8 |
  | `oscope-api`'s `jobs` | 0.4–1.0 |
  | `ex-stitch --smoke`, five at once | 172–177 |

- **They found no fault.** The job's suite and the smoke ran again once the chunks went in bands.
- **Five at once:** at the owner's asking, the last runs went five at a time, each on its own P-core's two threads. The smoke's five took 3 minutes, not some 15.
- **Left out:** the scan engine's suite, some 270 s a run on two cores. P39 doesn't change it: the simulator's new settings default to the stage's old values.

### The size

L's budget is 900; P39 measured 2,880, 15 % over its estimate of ~2,500.

- **One pull request,** under the owner's standing choice: the methods, the job and the example are one path, and the job's tests need the methods.
- **Against the estimate,** the code took 1,546 lines against ~1,025, the tests 789 against ~600, the example 505 against ~450, and the glue 40 against ~40.
- **What the estimate didn't foresee:** the throughput work (the cache of decoded chunks, the writers' pool and its bands, level 1 made by the writers, the store's levels in parallel), the raw tiles' path through level 1, and the tests the mutants' reading added.

## Tests

| Crate | Test | What it checks |
|---|---|---|
| `oscope-stitch` | `stitch` (7) | **Tiles go back where they were cut:** 12 tiles of 240 × 200 pixels cut from a known texture, recorded up to 15 pixels off, over 5 pixels rms; 17 pairs across and down, registered and solved to within 0.3 pixels, none rejected. **Blank tiles stay where the stage put them,** within a pixel, no pair through them kept, and the others placed as well. **A wrong link,** 25 pixels off, rejected, the others holding. **A link a little wrong,** 4 pixels, which plain least squares spreads under the 3-pixel rejection, weighed down by Huber's weights until it is rejected. **Placing and blending:** offsets rounded to whole pixels, even ones for raw tiles; two tiles of one value meet without a seam; where they differ, each pixel leans to the tile it is deeper in. **Pairs** need their least overlap; a long one is binned by 2 under 2048, cut at even pixels, and the stage's shift comes back from its expected one. **The flat field** is within 5 % of the shading 64 noisy tiles share |
| `oscope-workflows` | `stitch` (5) | **The job on an acquisition:** a raster's tiles cut from a known slide where the plan put them, recorded up to 12 µm off; each placed within 0.3 pixels, some narrow overlaps dropped and none rejected; the mosaic the slide itself at every pixel sampled; level 1, level 0 halved; its provenance and `stitch.json`. **Memory:** over a mosaic more than twice its budget of 10 MB, at most 10 MB held, the tiles within 0.5 pixels. **Raw tiles** of 640 × 480, registered from level 1, within 0.3 pixels. **A stack** of three planes solved on its middle one, each plane written at the same offsets. **A cancel** while registering, once solved and while writing, leaving no result and no draft |
| `ex-stitch` | `--smoke` | A blind stage's scan, over 2 µm off the truth, stitched through `jobs/submit` to within 1 µm; the thumbnail and the page served |

**Where they ran:**

- **On Windows,** `scripts/ci-local.sh`, run alone on `b46e7a2`: 49 of 50 steps, 504 tests in 186 suites, the Python tests real, and every example's `--smoke`, `ex-stitch`'s new among them. The docs step failed on two links in `oscope-stitch`'s crate doc: `solve` and `blend` are both modules and functions. `6667a49` names the modules, and there the docs, fmt and clippy steps passed.
- **On Linux,** `scripts/ci-clone.sh` under WSL, on a fresh clone of `6667a49`, run alone: 50 of 50 steps, 504 tests in 186 suites, the Python tests real; 690 files, none with a CR.
- **The CUDA tests** weren't run: P39 touches no CUDA path. Its pairs are registered by P31's batch on the CPU.
- **On two cores,** pinned, 5 times out of 5 each: `oscope-stitch`'s `stitch` and `canvas`; `oscope-workflows`' `stitch`; `oscope-store`'s `images` and `ingest`; `oscope-api`'s `projects` and `jobs`; and `ex-stitch --smoke`.

**Mutation checks** (each broke the code on purpose, then the tests were run): twenty-seven of twenty-seven caught, twenty-six by the tests and the smoke, one by the bench.

- **Reading the tests against the list first** found six gaps, now covered: the least overlap; long overlaps binned and cut at even pixels; feathering where tiles differ, which two tiles of one value hid; raw tiles read from level 1; a stack registered on its middle plane; and the confidence filter.
- **The first run missed one,** and one didn't build:
  - **No reweighting:** each wrong link in the tests was rejected with or without Huber's weights. A link 4 pixels wrong, which plain least squares spreads under the rejection's 3 pixels (its worst miss 2.50), now needs them (its miss 3.70).
  - **The binning removed** left an untyped `1`; typed, the test of long overlaps caught it.
- **Writing these notes** found the spec's cancel untested. A test now cancels in each part, and the three mutants that ignore a cancel are caught. The simulator's two settings, ignored, are caught by `ex-stitch --smoke`.
- **The bands:** all the chunks in one band, as before the fix, is caught by the bench alone. Within three runs of 60 tiles, one held 268 MB of its 256. No test's mosaic is large enough to show it.

| Mutant | Caught by |
|---|---|
| Neighbours corner to corner paired | tiles go back where they were cut |
| Any overlap paired; long overlaps not binned; the stage's shift not binned | pairs need their least overlap |
| A registration's shift taken the wrong way | tiles go back where they were cut; blank tiles; a wrong link; pairs need their least overlap |
| Binned parts cut at odd pixels | raw tiles from level 1 |
| Raw tiles at odd pixels; no feathering | placing and blending |
| No prior | blank tiles stay where the stage put them |
| No reweighting | a link a little wrong |
| No rejection | a wrong link is rejected |
| The flat not divided by its mean; the flat not smoothed | the flat field |
| Every registration kept | the job on an acquisition; memory |
| A stack registered on its farthest plane | a stack |
| A cached chunk read at a whole chunk's width; level 1's quarters swapped | the job on an acquisition; memory; raw tiles; a stack |
| A level written at level 0 | the job on an acquisition; a stack |
| A writer counted as one chunk's worth; decoded chunks never let go | memory |
| A cancel ignored while registering, once solved, or while writing | a cancel |
| The stitch job not registered; the stage's encoders or lost steps setting ignored | `ex-stitch --smoke` |
| All the chunks in one band | the bench, `--bench 60`, within three runs |

## Follow-ups and known gaps

- **A turned camera is untested:** the tests' and the example's M are diagonal. A turn goes through the same M⁻¹, but no test turns it.
- **Exposure differences between tiles** aren't evened out: the blend feathers them. The flat field corrects only the shading the tiles share.
- **The flat field** is applied at its coarse pixels, not interpolated between them.
- **Small scans fall short of 10 tiles a second:** 20 tiles at 9.6, the median, and 40 under 10 in 3 runs of 9. The store writes a file for each chunk, which costs Windows some 9 ms; sharded Zarr would write fewer, a change to ADR-0021's format.
- **The memory bound with many writers** is the bench's to check, not a test's.
- **The pairs are registered on the CPU,** some 0.5 s of 40 tiles' 4. P31's batch on the GPU would need the compute service's lane.
- **The prior** biases a tile toward the stage by about λ over its links' weight, a hundredth of its stage error.
- **Raw tiles land within a pixel,** at even offsets; `stitch.json` keeps their sub-pixel parts.
- **A stack is registered on one plane:** a tile featureless there, though not on its other planes, stays where the stage put it.
- **The simulator's settings' refusals,** of a latency or lost steps out of range, aren't tested.
- **P40 serves the mosaic for deep zoom;** the page shows its thumbnail now.

## Clean room

Nothing was copied from `microscope` or any other codebase. Each method is written from its sources, as the spec above sets them out.
