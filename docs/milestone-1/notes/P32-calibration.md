# P32: Calibration I: pixel size, camera-to-stage affine, backlash, store

- Status: In review
- Branch / PR: `m1/p32-calibration` / [#46](https://github.com/lucaswewa/OScope/pull/46)
- Size: L · estimated ~3,000 before it started (the calibration sets and their store ~400; the fit and its decomposition ~150; the procedures: frames to grey, moves, registration, the pattern, backlash, the objective check, ~450; the `calibration` Thing and its jobs ~350; the simulator's truth ~60; tests ~920; `ex-calibrate` ~280; glue ~60; and about a tenth on top); one pull request, at the owner's standing choice for P24–P34 · measured {{SIZE}}

## Summary

{{SUMMARY}}

## Behaviour spec

Written before the code, as a spec-first phase's must be ([spec](../phases.md#p32); D18; plan §5.13; [ADR-0023](../../adr/0023-the-calibration-model.md)).

### What calibration answers

How the camera sits over the stage, and how the stage moves:

- **Geometry,** for each objective and binning: the size of a pixel at the sample, and the matrix that turns a displacement in the image into a stage move. It holds the camera's turn on the stage, whether its image is mirrored, and any difference in scale between its axes.
- **The stage,** for the instrument: the backlash of x and y, the soft limits, and where homing leaves it.

Each is a **calibration set**, kept, versioned and chosen in the calibration store. P33 adds the photometric sets (dark, flat, colour) beside them.

### Pixels and the nominal pixel size

- **A pixel** is a pixel of the frame, at the camera's binning: binned 2 × 2, a pixel is two of the sensor's a side.
- **The nominal pixel size,** at the sample, is the sensor's pixel × the binning ÷ the magnification on the instrument. The magnification is the objective's, times the instrument's tube lens over the one it was designed for (P09's catalogue): at 10× on the MIChrome 20's 2.4 µm pixels behind a 200 mm tube lens, 0.24 µm.

### The camera-to-stage matrix

- **The definition.** M is the 2 × 2 matrix, in µm per pixel, that turns a displacement d in the image (pixels: x to the right, y down) into the stage move that brings the content d away from the image's centre to its centre: **s = M · d**.
- **Equivalently,** a stage move s carries the image's content by **d = −M⁻¹ · s**, as the content moves across the sensor against the stage.
- **The decomposition.** M = R(θ) · U · F, with:
  - **R(θ),** the turn by θ;
  - **U = [[s_x, k], [0, s_y]],** with s_x and s_y positive;
  - **F = diag(m, 1),** with m = −1 when the image is mirrored, its x reversed, and 1 otherwise.

  It follows from the QR factorisation of M · F, whose determinant is positive (Golub and Van Loan, 2013, §5.2): m is the sign of M's determinant, R(θ) the orthogonal factor, and U the triangular one with a positive diagonal.
- **What it reports:**
  - **the pixel size** p = √(s_x · s_y) = √|det M|;
  - **the rotation** θ, in degrees, from −180 to 180;
  - **mirrored**, m = −1;
  - **the aspect** s_x / s_y, and **the skew** k / p, which a sensor's square pixels on a square stage make 1 and 0;
  - **the residual,** the fit's root-mean-square error in pixels.

  A camera turned by θ and mirrored, with pixels of p at the sample, has M = p · R(θ) · F, as the simulator's mounting is built (P15).

### Measuring the matrix

1. **The frames.**
   - Each is the first frame whose exposure starts at least the settling time, 50 ms by default, after the stage reports it has arrived.
   - A colour mosaic becomes grey as the sums of its 2 × 2 cells, free of the mosaic, and the shifts found are doubled back into pixels. A grey frame is used as it is; red, green and blue are averaged.
   - The frame's middle is used, at most 2,048 × 2,048 pixels of it.
2. **The pattern.**
   - The stage visits a 3 × 3 grid around where it starts, a step δ apart along x and y.
   - δ is a quarter of the used part's smaller side, at the nominal pixel size, rounded to a whole µm: 123 µm at 10×.
3. **One way in.** Every position, the middle's too, is reached from the same side: from an approach point a distance a before it, 50 µm along −x and −y by default. A stage that reverses lags by its backlash, so positions reached from both sides would differ by it. Coming from one side leaves the lag the same for all (ISO 230-2's unidirectional approach).
4. **The shifts.**
   - Each frame is registered against the middle's by P31's registration, every shift considered: the turn isn't known yet.
   - A registration whose confidence is under 0.5 fails the calibration, saying where.
5. **The fit.** With the stage moves s_i and the shifts d_i, N (pixels per µm) is fitted by least squares to d_i = −N · s_i (Björck, 1996), and M = N⁻¹. The moves are commanded, so they are the exact side, and the shifts the measured one.
6. **The check.** The calibration is refused if:
   - the residual is above 0.5 px;
   - the pixel size is more than 25 % from the nominal, which a wrong objective gives;
   - the aspect is more than 2 % from 1, or the skew more than 0.02, which a fault in the stage or the frames gives.

   Otherwise it is saved as a new geometry set, and made active unless asked not to be.

### Backlash

The reversal value of ISO 230-2: a position reached from below and from above, measured in the image.

1. For each of x and y, from where the stage is, by the same 50 µm approach:
   - **from below:** to the target less a, then to the target; a frame;
   - **from above:** to the target plus a, then to the target; a frame.
2. The frame from above is registered against the one from below: the shift d. The stage's true move between them is −M · d, and its component along the axis is the backlash: a stage that lags reaches the target short of it from below, and past it from above.
3. Three cycles, and the median of each axis's.

It needs the active geometry set for the fitted objective and binning, for M.

### The stage set: soft limits and homing

- **Homing.** A stage that can home is homed first, unless asked not to be. The set records where homing left it: its position just after, in machine coordinates.
- **The soft limits** are recorded as the stage reports them once homed, each axis's least and greatest position. On grblHAL they follow from its travel settings and the homed origin.
- **The backlash,** measured as above.

All three are saved as a new stage set, and made active unless asked not to be.

### The objective check

After an objective is swapped, a quick check that the one fitted is the one in the light path:

1. **The moves.** Moves of δ along x, and along y, from where the stage is, each reached by the one-way approach, with a frame at each end. δ is as for the matrix.
2. **The measurement.** Each pair registered. A move of |s| µm shifts the image by |s| / p pixels, whatever the turn, so the measured pixel size is |s| / |d|; the mean of the two.
3. **The verdict.** It is compared with the fitted objective's pixel size: the active geometry set's if there is one, the nominal otherwise. It is **plausible** within 20 % of it: objectives differ by twice and more. Otherwise it is **flagged**, naming the catalogue's objective whose nominal pixel size is nearest the measured one.

The `calibration` Thing keeps the latest check's report.

### The calibration store

- **A set** has:
  - an ID;
  - its **kind**, `geometry` or `stage`;
  - its **key**: the objective and the binning, for geometry;
  - its **version** for that key, from 1;
  - when it was made, and by which account;
  - **the instrument's state:** the camera's model and sensor pixel, the binning, the stage's model, the objective with its magnification and NA, and the tube lens;
  - **its values.**
- **Versions:** a new set is the next version of its key; nothing is changed or removed.
- **Active:** at most one set of each key is active, and calibration's users take it: the fitted objective's geometry, and the stage's set.
- **Export and import.**
  - A set exports as its JSON.
  - An import is checked, then saved as the next version of its key, not active. It records the ID it came from, and keeps when it was made and by whom.
- **Files:** each set is a JSON file named by its ID in `calibration/` under the data root. A file records the active sets. Each file is written whole and renamed into place.

### The `calibration` Thing

- **Properties:**
  - `sets`, every set's summary, newest first;
  - `active`, the active sets for the fitted objective and the camera's binning, and the stage's;
  - `last_check`, the objective check's report.
- **Actions:** `activate`, `export` and `import`; and the jobs `geometry`, `stage` and `check_objective`.
- **The jobs** are hardware jobs: they hold the instrument while they move it. A job's result is the set it made, `calibration/<id>`.
- **Frames:** each waits at most 10 s for its frame, as captures do (P30). The camera streams.

### The simulator's truth

`[devices.simulator]` gains what calibration must recover:

- **`rotation`,** in degrees, and **`mirrored`:** the camera's mounting;
- **`scale`:** the true magnification over the catalogue's, as a tube lens a little off would make it;
- **`backlash`,** in µm.

The defaults, 0, false, 1 and 2 µm, are the simulator as it was.

### What the tests show

- **Recovered,** on the simulator with a mounting turned by 30° and mirrored, a scale of 1.03 and a backlash of 5 µm, against the truth:
  - the pixel size within 0.2 %;
  - the rotation within 0.1°;
  - mirrored, exactly;
  - the aspect within 0.2 % of 1, and the skew within 0.002;
  - the backlash within 0.2 µm;
  - the soft limits and homing as the stage has them.
- **The fit** recovers a matrix exactly from exact shifts. Its decomposition gives back θ, m and U across turns, mirrors, scales and skews.
- **A wrong objective is flagged:** fitted 20×, with the 10× in the light path, it names the 10×. The right one passes.
- **Store versioning:** versions per key; one active per key; export, then import as the next version, not active; refusals of malformed sets; the store read back from its files.
- **The API:** the Thing's jobs, properties and actions over HTTP.

### Sources

- G. H. Golub and C. F. Van Loan, *Matrix Computations*, 4th ed., Johns Hopkins University Press, 2013: the QR factorisation, §5.2.
- Å. Björck, *Numerical Methods for Least Squares Problems*, SIAM, 1996: linear least squares, and the normal equations.
- ISO 230-2:2014, *Test code for machine tools: Part 2, Determination of accuracy and repeatability of positioning of numerically controlled axes*: unidirectional and bidirectional approach, and the reversal value.
- P31's registration (Kuglin and Hines, 1975; Willert and Gharib, 1991), for the shifts.

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

{{CLEANROOM}}
