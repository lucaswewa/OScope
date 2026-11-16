# ADR-0027: Post-acquisition stitching

- Status: Accepted
- Date: 2026-11-16
- Phase: P39

## Context

- **Plan §5.15** stitches a scan after it is taken. The steps:
  - pairwise phase correlation over the overlaps, searched near the expected offset;
  - a global weighted least-squares solve, rejecting outliers by iterative reweighting;
  - an optional flat field, then feathered blending;
  - a multiscale OME-Zarr mosaic, written out of core.

  Z-stacks reuse the per-tile transforms.
- **§5.20 asks for at least 10 tiles of 20 MP a second,** at bounded memory.
- **What already exists:**
  - P31 registers pairs, sub-pixel, with a confidence.
  - P36's scans record each frame with the stage's position, and keep the plan, its field built from M (P35).
  - ADR-0021's store writes OME-Zarr v3 in 512-pixel chunks, and makes the pyramid.
- **A stepper stage without encoders** reports where it was sent, while each frame shows where it was: off by the backlash, the steps it lost and its settling.
- **A colour camera's frames are kept raw,** its Bayer mosaic, one channel.

## Decision

**1. Tiles are placed in the camera's pixels, by translations.**

- A tile's nominal offset is M⁻¹ times its recorded position less the first tile's. M comes from the plan's field corners.
- The solve moves tiles only: M has taken in the camera's turn and pixel.

**2. Pairs are the tiles side by side.**

- **Which pairs:** their nominal frames overlap by `min_overlap_px` (32) along both axes, and by half a frame along one. Neighbours corner to corner are left out.
- **What is registered:** each pair's parts are the nominal overlap, binned to at most 2048 pixels a side.
  - Binned by an even factor, they are read from the acquisition's level 1, and cut at even pixels so that it lines up.
  - Raw tiles are always binned by 2 at least: level 1 is their 2 × 2 cells.
- **How:** the pairs are registered in parallel, each within `radius_um` (25) of the stage's shift.
- **Kept:** a registration under `min_confidence` (0.5) is dropped: glass gives none.

**3. The solve** takes the offsets that minimise the weighted misses of the pairs' shifts, plus λ = 0.01 × their mean weight times each tile's squared distance from its nominal offset.

- **The prior** anchors tiles no pair joins, such as glass, and fixes where the mosaic lies; elsewhere it moves a tile a fraction of a pixel.
- **The method:** conjugate gradients with Jacobi's preconditioner, x and y apart, to a residual under 10⁻⁶.
- **Outliers:** Huber's weights (k = 1 pixel) are applied for up to 10 rounds. Then, while a pair misses by more than `reject_px` (3), the worst one is rejected and the solve repeats.

**4. A flat field,** if asked:

- **Estimated** as each pixel's median over up to 64 tiles at the coarsest level 64 pixels or more on its shorter side, smoothed by a Gaussian a 32nd of that side, over its channel's mean.
- **Raw tiles:** their coarse levels mix a cell's colours, so the flat corrects shading, not colour.

**5. Blending:**

- **Placement:** each tile at its offset rounded to a whole pixel, an even one for raw tiles.
- **Feathering:** each mosaic pixel the mean of the tiles over it, weighted by each one's distance to its own edge, plus a half.

**6. The mosaic is a result** of the project:

- **Its image** is OME-Zarr in the store's chunks. Its provenance names the acquisition, the options and the node `stitch.mosaic`. `stitch.json` beside it keeps the offsets, the pairs and the misses.
- **Level 0** is made chunk by chunk, each chunk from its tiles' parts, which are read through a cache of the acquisition's decoded chunks.
- **Level 1** is made as level 0 is written: each chunk's half-size quarter is kept until its level-1 chunk has all its quarters, then written whole.
  - The store's `ImageWriter::write_level` and `made` let it, and `halve_samples` halves as `finish` does.
  - The store's pyramid then starts at level 2, and makes each level's rows of chunks in parallel.

**7. Memory** is bounded by `memory_mb` (512):

- **Level 1's waiting quarters:** room for at most two rows of chunks of them is set aside first.
- **Decoded chunks:** half of what is left, those used least recently let go first.
- **Chunks being made:** the other half. A pool of writers works on as many chunks as that holds, at six chunks' worth each: four tiles' parts and the sums, where tiles overlap.

**8. A stack** is solved once, on each tile's plane nearest its z, and every plane is written with the same offsets.

**9. The job:** `stitch.run`, a compute job, takes {`project`, `acquisition`, `options`}.

- **Progress:** the pairs, the solve, the chunks.
- **A cancel** drops the draft.
- **Its results:** the result's path, and the pairs kept and rejected, the miss and the tiles' moves.

## Consequences

- **P40 serves the mosaic's levels as they are.** The thumbnail and the data route show it now.
- **Throughput:** on the owner's PC, 10.1–10.3 tiles of 20 MP a second over 40 and 60 tiles, and 9.1 over 20, where the pyramid's top and the thumbnail weigh more.
  - **The rest is the store's:** with a file for each chunk, Windows' file handling costs about 9 ms a chunk.
  - **Ingest gains too:** making the pyramid's rows in parallel speeds every acquisition's.
- **Raw tiles land within a pixel,** not half of one, as they are placed at even offsets. The solve's offsets in `stitch.json` keep their sub-pixel parts.
- **Seams between tiles of different brightness** are feathered, not evened out. The flat field corrects the shading they share; exposure differences between tiles aren't corrected.
- **The prior biases a tile toward the stage** by about λ over its links' weight, a hundredth of its stage error. That is far below the registration's own error.

## Alternatives considered

- **MIST's stage model** (Chalfoun et al., 2017), estimating the stage's repeatability to bound the search: the prior and the search radius do the same work, more simply.
- **Rotation and scale in the solve:** M, from calibration, already takes them in. Translations keep the solve linear.
- **Registering at full resolution:** the 2048-pixel binning keeps the sub-pixel estimate, at a quarter of the work or less.
- **Making level 1 after level 0,** as the store's `finish` did: it read all of level 0 back, a second of 4 for 40 tiles of 20 MP.
- **Sharded Zarr,** to write fewer files: a change to ADR-0021's store format, left for later.
