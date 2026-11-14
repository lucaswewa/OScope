# P32: Calibration I: pixel size, camera-to-stage affine, backlash, store

- Status: In review
- Branch / PR: `m1/p32-calibration` / [#46](https://github.com/lucaswewa/OScope/pull/46)
- Size: L · estimated ~3,000 before it started (the calibration sets and their store ~400; the fit and its decomposition ~150; the procedures: frames to grey, moves, registration, the pattern, backlash, the objective check, ~450; the `calibration` Thing and its jobs ~350; the simulator's truth ~60; tests ~920; `ex-calibrate` ~280; glue ~60; and about a tenth on top); one pull request, at the owner's standing choice for P24–P34 · measured **2,785** hand-written lines added (code 1,699: `oscope-workflows` 1,364, `oscope-api` 277, `oscope-sim` 50, `oscope-jobs` 8; tests 787; examples 251; glue 48), and 8 removed; the Thing Description's snapshot, 1,461 lines, is generated. **That is 1,885 over L's 900, and about 7 % under the estimate.**

## Summary

Calibration I: how the camera sits over the stage, and how the stage moves ([spec](../phases.md#p32); D18; plan §5.13; [ADR-0023](../../adr/0023-the-calibration-model.md)).

- **The calibration store:** sets versioned per key, one active per key, exported and imported, as JSON files under the data root.
- **The camera-to-stage matrix,** from a 3 × 3 grid of stage moves, each position reached from the same side:
  - each frame registered by P31's registration;
  - the matrix fitted by least squares, and decomposed into a turn, a mirror, two scales and a skew;
  - refused if the fit is poor, or the pixel size far from the objective's.
- **The stage:** backlash per axis from a target reached from both sides, the soft limits, and where homing left it.
- **The objective check:** two small moves measure the pixel size. Within 20 % of the objective's it is plausible; otherwise the catalogue's nearest objective is named.
- **The `calibration` Thing** and its three hardware jobs.
- **The simulator's truth:** a turn, a mirror, a scale and a backlash, which calibration recovers far within the spec's tolerances (30.0002°, the pixel size within 0.0002 %, the backlash within 0.002 µm).
- **P31 is marked Done.**

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

1. For each of x and y, where the stage is, by the same 50 µm approach:
   - **from below:** to the target less a, then to the target; a frame;
   - **from above:** to the target plus a, then to the target; a frame.
2. The frame from above is registered against the one from below: the shift d. The stage's true move between them is −M · d, and its component along the axis is the backlash: a stage that lags reaches the target short of it from below, and past it from above.
3. Three cycles, and the median of each axis's.

It needs the active geometry set for the fitted objective and binning, for M.

### The stage set: soft limits and homing

- **Homing.** A stage that can home is homed, unless asked not to be, once its backlash is measured, and taken back where it was (homing last: see the design notes). The set records where homing left it: its position just after, in machine coordinates.
- **The soft limits** are recorded as the stage reports them once homed, each axis's least and greatest position. On grblHAL they follow from its travel settings and the homed origin.
- **The backlash,** measured as above.

All three are saved as a new stage set, and made active unless asked not to be.

### The objective check

After an objective is swapped, a quick check that the one fitted is the one in the light path:

1. **The moves.** Moves of δ′ along x, and along y, from where the stage is, each reached by the one-way approach, with a frame where they start and one at each end. δ′ is a 32nd of the used part's smaller side at the fitted objective's pixel size, rounded to a tenth of a µm. So the image moves by 6 to 640 px even if the objective in the light path is ten times stronger or weaker than the one fitted, as the catalogue's 5× and 50× are.
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
- **Frames:** each waits at most 30 s for its frame (10 s at first, as captures wait: see the design notes). The camera streams.

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

| Where | What |
|---|---|
| `oscope-workflows`, `calibration` | `CalibrationSet` and its `Values` (`Geometry`, `StageValues`), and the `CalibrationStore`; `fit_matrix` and `decompose`; `measure_geometry`, `measure_stage` and `check_objective`; `calibration_jobs` |
| `oscope-api`, `calibration` | The `calibration` Thing: `sets`, `active` and `last_check`; `activate`, `export` and `import`; the jobs `geometry`, `stage` and `check_objective` |
| `oscope-sim` | `[devices.simulator]`'s `rotation`, `mirrored`, `scale` and `backlash`; `SimMount::scaled` |
| `oscope-jobs` | `JobCtx::account`: who submitted a job, for the sets it makes |
| `docs/adr/0023-the-calibration-model.md` | The calibration model |
| `examples/ex-calibrate` | Calibration through the API, against the simulator's truth |

## How to try it

[`ex-calibrate`](../../../examples/ex-calibrate/README.md):

```sh
cargo run --release -p ex-calibrate
```

It homes the simulated stage and takes it over the tissue, then checks the objective and calibrates the geometry and the stage, all through the API. It prints the truth injected into the simulator against what calibration recovered, and serves at port 8235 with links to the sets.

## Performance

No §5.20 budget is P32's. On the owner's PC:

- **`ex-calibrate` at real time,** on the 20 MP sensor in a release build: the check, the geometry and the stage in 100 s, most of it homing twice.
- **A geometry's registrations** run on the CPU: eight of 1,024 × 1,024 cells at most, under a second together. CUDA wasn't needed.
- **The procedures' tests,** on the quarter sensor at ten times real time, take 27 to 35 s in a debug build.

## Design notes and deviations from the plan

### The frame wait, 30 s, not the spec's 10

- **What happened.** At real time on the simulator's 20 MP sensor, the stage calibration failed, coming back from homing: "no frame came within 10 s".
- **Why.** Homing takes the camera across 47 mm of slide and 15 mm of focus while it streams. Each frame on the way renders afresh, in tiles of a new place and depth, and the tissue's tiles fall out of the simulator's cache. Two such frames took it more than 10 s.
- **The change.** A camera gives a frame within its interval, so the wait only bounds how long a fault takes to show. It is 30 s, and with it the run completed. The spec's line says so.

### Homing last, not first

- **What happened.** On two cores, in debug builds, the stage calibration failed coming back from homing, in the API's test 5 times out of 5 and in `ex-calibrate --smoke` once: "no frame came within 30 s".
- **Why.** As above, the simulator renders each frame of the way afresh, and on two cores a debug build fell further behind.
- **The change.** Backlash is relative, so it is measured where the stage is, before homing; then the stage is homed, its soft limits recorded, and it is taken back, with no frame needed after the long way. The spec's lines say so.

### Other choices

- **The procedures register on the CPU.** A calibration registers at most a few dozen pairs of 1,024 × 1,024 cells, which the CPU does in a second.
- **The registration's weight** follows the objective: P31's σ_w of 2 px, or the objective's in-focus blur, 0.21 λ / NA in green light, where that is larger (the 50×).
- **A colour mosaic becomes grey** by summing its 2 × 2 cells, as the simulator's end-to-end test of P31 did, and the shifts are doubled back.
- **Options are checked as a job is submitted.** A step that isn't positive, a negative approach or settling time, or no cycles: the action answers 400 at once. A bad step had waited for a frame first.
- **The list of sets** is newest first by when each was made. An import keeps when it was made where it came from, so it sits just after the set it copies.
- **The binning** of the sets in use is the camera's mode's. Switching the camera to bin2 leaves no geometry in use until bin2 is calibrated.
- **Who made a set:** the job service records who submitted a job, and `JobCtx::account`, new in `oscope-jobs`, gives it to the job.
- **The objective check expects** the active geometry's pixel size if there is one, and the nominal otherwise. The report says which.
- **A wrong objective can't be shown through the simulator's API,** as its mount puts in the light path whichever objective is fitted. The procedures' test fits the 20× and puts the 10×'s optics back behind it.
- **The soft limits** are the stage's capabilities, which M1's HAL fixes when the stage opens; grblHAL's driver (M3) reads them from its settings.
- **Where homing left the simulated stage** shows its backlash: (−2.5, −2.5, 20,002.5) µm with 5 µm of backlash, as its encoders report the true position.

### The size

L's budget is 900; P32 measured 2,785, about 7 % under its estimate of ~3,000. Under the owner's standing choice, it is one pull request with the overrun explained: the calibration model, its store, the three procedures and their Thing are one piece, and the tests of the procedures run the simulator end to end.

## Tests

| Crate | Test | What it checks |
|---|---|---|
| `oscope-workflows` | `calibration` (3, unit) | **The fit** recovers a matrix from exact shifts, and refuses moves that don't span the plane. **The decomposition** gives back the turn, the mirror, the scales and the skew that made a matrix, at turns from −179° to 180°. **The verdict** refuses each of the spec's bounds |
| `oscope-workflows` | `store` (3) | **Versions** per key; **one active per key**; an earlier version made active again; newest first. **The files read back**, an activation by hand among them, skipping what isn't a set. **Export, then import** as the next version, not active, keeping when and by whom. **Refused:** not a set, another format, a matrix that can't be inverted, soft limits upside down |
| `oscope-workflows` | `procedures` (4) | **On the simulator,** turned 30° and mirrored, a scale of 1.03, 5 µm of backlash: the geometry within the spec's tolerances (30.0002°, the pixel size within 0.0002 %, an aspect of 0.999996, a skew of 2.6 × 10⁻⁶, a residual of 0.002 px). **The stage:** its backlash (4.999 and 5.001 µm), its soft limits, homing, and back where it was. **At bin2,** its own geometry. **A wrong objective** (the 20× fitted, the 10× in the light path) flagged, at 1.94 and nearest the 10×, and its geometry refused; the right one plausible. **Frames that wait** for a stage settling over 50 ms |
| `oscope-api` | `calibration` (1) | **Over HTTP:** the three jobs; the sets in use, and the latest check; export, import as the next version, activation, and the list. **The sets in use follow the camera's binning.** **Refused:** a set that isn't there (404), one that isn't a set (400), parameters that can't be (400) |
| `oscope-api` | `devices`, `jobs` | The Thing's TD (a snapshot); the job kinds listed |
| `ex-calibrate` | `--smoke` | The tour on the quarter sensor at ten times real time, held to the spec's tolerances |

**Where they ran:**

- **On Windows,** `scripts/ci-local.sh`, run alone on `{{COMMIT}}`: {{WINDOWS}}.
- **On Linux,** `scripts/ci-clone.sh` under WSL, on a fresh clone of the same commit, run alone: {{WSL}}.
- **On two cores,** pinned, 5 times out of 5 each: `oscope-workflows`' `store`, unit tests and `procedures`; `oscope-api`'s `calibration`; and `ex-calibrate --smoke`. The first pinned runs failed: see "Homing last, not first" above. The procedures' tests now start the simulator over the tissue, stream a 768 × 576 region, and take turns.

**Mutation checks** (each broke the code on purpose, then the tests were run): twenty-two of twenty-two caught.

- **Reading the tests against the list first** found three gaps, now covered: the geometry's refusals, which the simulator never triggers; binning, which nothing ran; and a bad step, which waited for a frame before failing.
- **The first run missed two,** now caught: an activation not written to its file, and frames that don't wait for the stage to settle. The simulator's stage settles in 15 ms, sooner than its frames come; the test's settles over 50 ms.

| Broken on purpose | Caught by |
|---|---|
| Versions not counted per key | `versions_count_up_per_key_and_one_set_of_each_is_active`, and the store's two others |
| A measured set not made active | the same |
| An activation not written | `the_store_reads_back_its_files_and_skips_what_isnt_a_set` |
| An import keeps its ID | `a_set_exports_and_imports_as_the_next_version_not_active` |
| An import made active | the same |
| A file that isn't a set fails the store | `the_store_reads_back_its_files_and_skips_what_isnt_a_set` |
| The decomposition not made again on saving | `versions_count_up_per_key_and_one_set_of_each_is_active` |
| Soft limits upside down accepted | `a_set_exports_and_imports_as_the_next_version_not_active` |
| The fit's sign flipped | `the_fit_recovers_a_matrix_from_exact_shifts` |
| A mirror never found | `a_decomposition_gives_back_what_made_the_matrix` |
| The turn from the wrong column | the same |
| No one-way approach | the three procedures on the default truth |
| A mosaic's cells not doubled back | the same |
| No settling | `frames_wait_for_the_stage_to_settle` |
| Backlash's sign flipped | `the_geometry_and_the_stage_are_recovered_within_the_spec_s_tolerances` |
| Backlash from the other axis | the same |
| The check too lenient | `a_wrong_objective_is_flagged_and_its_geometry_refused` |
| The nearest objective the farthest | the same |
| The check ignores the calibration | `the_jobs_measure_and_the_sets_are_kept_listed_and_moved` |
| The geometry in use always at binning 1 | the same |
| The simulator's backlash ignored | the same |
| The simulator's scale ignored | the procedures, and the same |

## Follow-ups and known gaps

- **Calibration's users:** the sets are kept and chosen, but nothing reads them yet. Click-to-move in the live view, scans' tile positions and acquisitions' metadata will take the active sets.
- **z's backlash** needs focus: autofocus (P34) can measure it.
- **The objective check after a swap** is a job the client runs. The wizard (P33) and the M2 app can run it after each fit.
- **A swap the simulator hides,** an objective fitted while another stays in the light path, for trying the check by hand.
- **Thousands of versions** would want an index; a folder read at start serves the instrument's tens.
- **Sets made by an account** are tested in the store. The jobs pass the account, which no test signs in to give.

## Clean room

Nothing was copied from `microscope` or any other codebase. Each step is written from its sources, as the spec above sets them out:

- the least-squares fit, from the normal equations (Björck, 1996);
- the decomposition, from the QR factorisation by Gram and Schmidt's steps (Golub and Van Loan, 2013);
- one-way approach and the reversal value, from ISO 230-2;
- the shifts, from P31's registration.
