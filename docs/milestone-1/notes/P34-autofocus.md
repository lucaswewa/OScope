# P34: Autofocus, auto exposure, auto white balance

- Status: In review
- Branch / PR: `m1/p34-autofocus` / [#48](https://github.com/lucaswewa/OScope/pull/48)
- Size: L · estimated ~4,200 before it started (the focus measures on the CPU ~150 and with CUDA ~200; the autofocus procedure, stepped and continuous sweeps, ~600; auto exposure ~200; auto white balance ~120; their jobs ~150; the `autofocus` and `exposure` Things ~350; the simulator's truth ~50; tests ~1,300; `ex-autofocus` ~400; `ex-checkpoint-c` ~250; glue ~60; and about a tenth on top); one pull request, at the owner's standing choice · measured **3,622** hand-written lines added (code 2,021: `oscope-workflows` 1,422, `oscope-api` 308, `oscope-compute` 237, `oscope-sim` 54; tests 912; examples 623; glue 66), and 2 removed; the three Thing Descriptions' snapshots, 1,000 lines, are generated. **That is 2,722 over L's 900, and 14 % under the estimate.**

## Summary

Autofocus, auto exposure and auto white balance ([spec](../phases.md#p34); plan §5.14; [ADR-0024](../../adr/0024-autofocus.md)), and Checkpoint C.

- **Focus measures:** Brenner's gradient, Tenengrad and the variance of the Laplacian, each over a binned region and divided by its mean's square. They run on the CPU and with CUDA, which agree to the bit.
- **Autofocus**, a hardware job:
  - **the stepped sweep:** a coarse sweep of ±6 depths of field, then a fine one, every position approached moving up;
  - **the continuous sweep:** frames put at z by their exposure midpoints on the stage's track;
  - **refusals:** an empty field, or a focus out of reach, the stage left where it started.
- **Auto exposure,** by the region's 99.5th percentile, within the camera's limits. **Auto white balance,** from the region's lightest part.
- **The `autofocus` and `exposure` Things.**
- **The simulator's truth:** each sample says where it is sharpest, which the simulation Thing serves.
- **On the simulator,** autofocus lands within a quarter of the depth of field of the truth on the H&E tissue, the reflective chip and a sparse field of fluorescent cells. Its errors run from 0.01 to 0.65 of that quarter.
- **`ex-autofocus`** shows it, and **`ex-checkpoint-c`** is Checkpoint C: calibrate, autofocus, expose, capture, and open in napari.
- **P33 is marked Done.**

## Behaviour spec

Written before the code, as a spec-first phase's must be ([spec](../phases.md#p34); plan §5.14; [ADR-0024](../../adr/0024-autofocus.md)).

### What it does

- **Autofocus** finds the stage's z at which the sample is sharpest, and leaves the stage there.
- **Auto exposure** sets the camera's exposure so that the frame's bright end sits at a target.
- **Auto white balance** finds the gains that make the light's white grey, and gives them to the preview.

All three are hardware jobs: they hold the instrument while they move it or change the camera. The camera streams.

### The frame they look at

- **The region:** the frame's middle half in each direction (a quarter of its area), unless a region is given.
- **Grey:** a colour mosaic becomes grey as the sums of its 2 × 2 cells, as P32's procedures do. A grey frame is used as it is.
- **Binned** in software, by whole factors, until its longer side is at most 512 pixels. Each binned pixel is its block's mean.
- **Linear,** from 0 to 1, the frame's black level taken away.
- **Each frame** is the first whose exposure starts at least the settling time after the change before it: 50 ms by default, on the instrument's clock.

### The focus measures

Each is computed over the binned region I(x, y), with N pixels and mean μ, and divided by N μ² so that it doesn't change with the light's level or the exposure (Groen, Young and Ligthart, 1985):

- **Brenner's gradient** (Brenner et al., 1976): Σ (I(x + 2, y) − I(x, y))², taken along y too and added, so that a sample's orientation doesn't matter.
- **Tenengrad** (Tenenbaum, 1970; Krotkov, 1987): Σ (Gx² + Gy²), with Gx and Gy from Sobel's 3 × 3 operators, over the pixels whose 3 × 3 neighbourhood is inside the region.
- **The variance of the Laplacian** (Pech-Pacheco et al., 2000): the variance of L = I(x − 1, y) + I(x + 1, y) + I(x, y − 1) + I(x, y + 1) − 4 I(x, y), over the same pixels, divided by μ² (not N μ²).

They are computed on the CPU, and with CUDA when the server has a GPU. The two agree within 10⁻⁴ of the measure.

**The defaults, by the light on:** transmitted light uses Brenner's; reflected light, Tenengrad; fluorescence, the variance of the Laplacian. Comparative studies rank these among the most reliable for their kinds of image (Groen et al., 1985; Sun, Duthaler and Nelson, 2004; Pertuz, Puig and Garcia, 2013). The simulator's samples check the choice.

### The depth of field

d = λ n / NA² + n e / (M NA), with λ 0.55 µm (green light; 0.52 µm, a green emission, in fluorescence, as the HAL's channels carry no wavelength), n the medium's index (1 for the dry objectives), e the sensor's pixel times the camera's binning, M and NA the fitted objective's (Inoué and Spring, 1997). With the MIChrome 20's 2.4 µm pixels: 31 µm at 5×, 7.9 µm at 10×, 1.9 µm at 50×, as plan §5.14 says.

### The stepped sweep

1. **The coarse sweep:** from z₀ − R to z₀ + R in steps of d, where z₀ is where the stage is and R is 6 d by default (13 positions).
2. **The fine sweep:** around the coarse sweep's best position, from it less d to it plus d, in steps of d / 4 (9 positions).
3. **The peak:** the fine sweep's best position k, refined by the three-point Gaussian estimate, z_k + s (ln F_{k−1} − ln F_{k+1}) / (2 (ln F_{k−1} − 2 ln F_k + ln F_{k+1})), as P31's registration refines its peak (Willert and Gharib, 1991). At an end of the fine sweep, its best position as it is.
4. **The end:** the stage moves to the peak.

**One way in.** Every position is reached moving up, from an approach point 10 µm below the sweep's first (ISO 230-2's unidirectional approach, as P32's calibrations). The stage's backlash in z then shifts every position the same, and the peak with them.

**Revised, from measurements: where the stage says it is.**
- **The curve's z** is where the stage reports each position once it has settled, when a frame after the settling time has come: where its frame is taken.
- **The final move** commands the peak plus the lag the sweep showed between the z commanded and the z reported.
- **Why.** The simulator's stage reports its true position, as a stage with encoders does. The first runs left it 1 µm, half its backlash, below the z they had found. Read as it arrives, the stage still overshoots by up to 0.3 µm.

**Refusals:**
- **An empty field:** the coarse sweep's best measure is less than 1.2 times its median. The stage goes back to z₀. The bound is checked on the simulator's blank glass and sparse fields.
- **The peak at the coarse sweep's end:** the focus is more than R from z₀. The stage goes back to z₀, and the report says which way it lies.

### The continuous sweep

A fast mode that tests the clock model early (plan §5.14):

1. The stage moves from z₀ − R to z₀ + R (approached from below) at a constant speed, chosen so that consecutive frames lie d / 4 apart in z: v = d / (4 T), with T the camera's frame interval measured from its frames. A stage that can't go so slowly or fast is clamped to its limits, and the report says so.
2. **Each frame's z** is where the stage's position track puts it at the frame's exposure midpoint: its exposure start, plus half its exposure, plus the line time times the region's middle row for a rolling shutter.
3. **The peak** is the vertex of a least-squares parabola through ln F of the best frame and its two neighbours on each side, in z.
4. The stage moves to the peak, from below.

Frames whose midpoint lies outside the track are dropped. The empty-field refusal is the same.

**Revised, from measurements: a camera slower while the stage moves.**
- **What happened.** The simulator renders each new depth afresh, so its frames come more slowly while the stage moves through z than at rest. Its first continuous runs caught 0 or 1 frames.
- **Sparse frames:** fewer than 9 within 2 d of the best frame, and the stepped sweep's fine pass about it finds the peak. Fewer than 3 in all, and the run sweeps stepped.
- **Each frame's z is looked up as it comes.** A slow sweep outlasted the track's 41 s of history, and its early frames were lost.

### Auto exposure

1. **The measure:** the region's 99.5th percentile, p, of the linear values (each site of a mosaic counts).
2. **The target:** t = 0.75 of full scale, by default.
3. **Each step:**
   - p above 0.98 (saturated): the exposure halves;
   - p below 0.01 (dark): it rises four times;
   - otherwise: it is multiplied by t / p, the sensor being linear (EMVA 1288).

   The exposure is clamped to the camera's limits.
4. **Converged** when |p / t − 1| ≤ 0.05.
5. **At most 10 steps.** Not converged then, or held at a limit short of the target, it ends at the best exposure found, and the report says why: at the longest exposure, open the light or raise the gain; at the shortest, dim the light.

The camera's gain is left as it is.

### Auto white balance

1. **The white:** the region's pixels whose grey level is between its 95th percentile and 0.98 of full scale (Land and McCann's white patch, 1971). In brightfield, the background is the lightest: the light itself.
2. **The gains:** each colour's mean over them, m_R, m_G and m_B, as gains (m_G / m_R, 1, m_G / m_B).
3. **For the preview:** a client gives them to the preview's look.

**Revised:** the job doesn't set the preview's look itself. A job holding the server, or the preview's Thing, might keep the server alive after it stops: in tests, its simulated camera would render on.

Refused if the white is under 0.05 of full scale (too dark), or the frame isn't colour.

### The `autofocus` and `exposure` Things

- **`autofocus`:**
  - `measures`: the current frame's three measures, without moving;
  - `defaults`: the measure for each light, and the sweep's defaults;
  - `last`: the latest run's report, with its curves;
  - the job `run`: the stepped or continuous sweep, with any of the options.
- **`exposure`:**
  - `last`: the latest auto exposure's report;
  - the jobs `auto` and `white_balance`.

Reports give the result, the measure used, the curve (z and measure for each frame), the time taken, and why a run was refused.

### The simulator's truth

Each sample says where it is in focus: the slide's top surface plus the height of its layers' middle, or, for the reflective chip, its surface's height at the field's centre.

**Revised, from measurements: the fluorescent cells.** Their truth is their dye's height, weighted by how much each slice holds about the point. They lie thicker near the glass: their slab's middle, 3.25 µm, was 1.5 µm above where they are sharpest.

### What the tests show

- **Autofocus within a quarter of the depth of field** of the truth, on the H&E tissue, the reflective chip and the fluorescent cells (a sparse field), started a few depths of field away.
- **Empty fields refused:** blank glass, and the stage back where it was.
- **The continuous and stepped sweeps agree** within a quarter of the depth of field.
- **The measures** peak at the truth on the simulator, and agree on the CPU and with CUDA.
- **Auto exposure converges** from too dark and from saturated, and holds at a limit when it can't.
- **Auto white balance** recovers a tinted light's gains.
- **The API:** the Things' jobs, properties and refusals over HTTP.

### Sources

- J. F. Brenner, B. S. Dew, J. B. Horton, T. King, P. W. Neurath and W. D. Selles, "An automated microscope for cytologic research: a preliminary evaluation", *Journal of Histochemistry & Cytochemistry* 24(1), 100–111, 1976.
- J. M. Tenenbaum, *Accommodation in computer vision*, PhD thesis, Stanford University, 1970; and E. Krotkov, "Focusing", *International Journal of Computer Vision* 1, 223–237, 1987.
- J. L. Pech-Pacheco, G. Cristóbal, J. Chamorro-Martínez and J. Fernández-Valdivia, "Diatom autofocusing in brightfield microscopy: a comparative study", *Proceedings of the 15th International Conference on Pattern Recognition*, vol. 3, 314–317, 2000.
- F. C. A. Groen, I. T. Young and G. Ligthart, "A comparison of different focus functions for use in autofocus algorithms", *Cytometry* 6(2), 81–91, 1985.
- Y. Sun, S. Duthaler and B. J. Nelson, "Autofocusing in computer microscopy: selecting the optimal focus algorithm", *Microscopy Research and Technique* 65(3), 139–149, 2004.
- S. Pertuz, D. Puig and M. A. Garcia, "Analysis of focus measure operators for shape-from-focus", *Pattern Recognition* 46(5), 1415–1432, 2013.
- S. Inoué and K. R. Spring, *Video Microscopy: The Fundamentals*, 2nd ed., Plenum Press, 1997: the depth of field.
- C. E. Willert and M. Gharib, "Digital particle image velocimetry", *Experiments in Fluids* 10(4), 181–193, 1991: the three-point Gaussian estimate.
- ISO 230-2:2014: unidirectional approach.
- EMVA Standard 1288, Release 4.0, 2021: the sensor's linear response.
- E. H. Land and J. J. McCann, "Lightness and retinex theory", *Journal of the Optical Society of America* 61(1), 1–11, 1971: the white patch.

## What was built

| Where | What |
|---|---|
| `oscope-compute`, `focus` | `FocusMeasure` and `measures`, on the CPU; `kernels/focus.cu` and `Cuda::focus_measures`, its twin; `Compute::focus_measures`, on the GPU when there is one |
| `oscope-workflows`, `autofocus` | `FocusRig`; `autofocus` (stepped and continuous sweeps), `auto_exposure` and `white_balance`; the defaults and the measures now; the jobs `autofocus.run`, `exposure.auto` and `exposure.white_balance` |
| `oscope-api`, `focus` | The `autofocus` Thing (`measures`, `defaults`, `last`, the job `run`) and the `exposure` Thing (`last`, `last_white_balance`, the jobs `auto` and `white_balance`) |
| `oscope-sim` | `Sample::in_focus`, each sample's truth, and `World::in_focus`; the simulation Thing's `in_focus` action |
| `docs/adr/0024-autofocus.md` | Autofocus |
| `examples/ex-autofocus`, `examples/ex-checkpoint-c` | Autofocus on the simulator's samples; Checkpoint C |

## How to try it

[`ex-autofocus`](../../../examples/ex-autofocus/README.md) focuses the simulator's H&E tissue, reflective chip and fluorescent cells, each from 15 µm off. It prints each against the truth, and plots the latest focus curve on a page at port 8237:

```sh
cargo run --release -p ex-autofocus
```

[`ex-checkpoint-c`](../../../examples/ex-checkpoint-c/README.md) is Checkpoint C. It calibrates the 10×, autofocuses, sets the exposure and white balance, and captures a frame into a project as OME-Zarr. Then it prints napari's command to open it:

```sh
cargo run --release -p ex-checkpoint-c
```

## Performance

No §5.20 budget is P34's. On the owner's PC, in release builds at real time, through a 1024 × 768 region of the simulated 20 MP sensor:

| Run | Error | ÷ a quarter of d | Frames | Time |
|---|---|---|---|---|
| H&E, stepped | +0.13 µm | +0.07 | 22 | 11.8 s |
| H&E, continuous | −0.16 µm | −0.08 | 33 | 16.0 s |
| Reflective chip, stepped | +0.90 µm | +0.46 | 22 | 15.6 s |
| Fluorescent cells, stepped | −1.22 µm | −0.65 | 22 | 15.5 s |
| Checkpoint C's autofocus | −0.03 µm | −0.01 | 22 | 9.5 s |

- **The time is the simulator's.** It renders each new depth of a frame afresh on the CPU.
  - Over the whole 20 MP, a stepped sweep took 212 s and a continuous one 304 s, with errors of 0.10 of a quarter.
  - The fluorescent cells' frames took longer than the 30 s each is waited for.
  - A camera gives its frames at its own rate wherever the stage is: the MIChrome 20's 7 fps would make a stepped sweep a few seconds.
- **The measures** look at a region binned to at most 512 pixels a side, so they cost little next to a frame.

## Design notes and deviations from the plan

### The spec's revisions, from measurements

As P31 did, the method was run on the simulator before the tests were written. Five revisions followed, each recorded in the spec above, and in ADR-0024 where it applies:

- **Where the stage says it is.** The simulator's stage reports its true position. The first runs left it 1 µm, half its backlash, below the z they had found. A sweep's curve now takes the reported z, and the final move commands the peak plus the lag the sweep showed.
- **A camera slower while the stage moves.** The simulator renders each new depth afresh, so its continuous sweeps caught 0 or 1 frames at first. Then three sparse frames gave a false "focus below the sweep", depending on the tile cache.
  - A continuous sweep sparser than the stepped coarse sweep, under 13 frames, now sweeps stepped.
  - One sparse about its best frame takes the stepped fine pass about it.
- **Frames put at z as they come.** A slow sweep outlasted the stage track's 41 s of history, and its early frames were lost.
- **White balance's gains go to the preview by the client.** A job that set the preview's look itself would hold the server or the preview's Thing. That might keep the server alive after it stops, and in tests its simulated camera would render on.
- **The fluorescent cells' truth** is their dye's height, weighted by how much each slice holds. Their slab's middle was 1.5 µm above where they are sharpest.

One more came from the tests' runs, and is recorded in the spec and ADR-0024 too:

- **Once the stage has settled.** The z was first read as the stage arrived, with up to 0.3 µm of its settling overshoot still in it.
  - A stepped sweep's focus lost nothing, as its curve's z carried the same overshoot as its lags.
  - A continuous sweep puts its frames at z while the stage moves, so its focus landed up to 0.3 µm low. A refused sweep's return to its start did too.
  - The empty-field test's second sweep starts where the first left it, so it came back up to 0.6 µm low. That test's check of 0.5 µm failed now and then: in four runs of its suite on all cores, one of them among six run to read its message, and in none of five on two cores.
  - Each z is now read once a frame after the settling time has come, where the frame is taken. The check is now 0.1 µm, which the old reading fails every time.

### A region of the sensor, on the simulator

The tests and `--smoke` runs look through a 256 × 192 region of the quarter sensor's middle; the examples' plain runs through 1024 × 768 of the 20 MP. The simulator renders each new depth of a frame afresh, and over whole frames a sweep takes it minutes. A region is an option autofocus offers anyway, and the measures look at the middle by default.

### The tests' turns, and two cores

- **One simulator at a time.** The workflows' tests take turns, as P33's do. The API's focus test is a single test, and stops its camera at its end, as P33 learned.
- **Two cores, five times each.** Pinned to two cores, every suite this phase adds or changes passed 5 times in 5, both before the settling fix and after it. They are slow there: a run of the workflows' `autofocus` suite took 9.3–9.6 minutes, and one of `ex-autofocus --smoke` 8.5–9.3.

### Other choices

- **The defaults by light:** Brenner's in transmitted light, Tenengrad in reflected, the Laplacian's in fluorescence. Each was checked on the simulator's sample in that light.
- **The empty-field refusal at 1.2 times the median.** Clear glass falls under it, and the sparse field of cells passes it, on the simulator.
- **One compute layer.** Only the `autofocus` Thing makes one, as only it measures focus. A CUDA server then opens one CUDA context for it, beside the preview's.
- **Refusals fail the job, and keep the report.** An empty field, a focus out of reach, and an exposure that didn't converge each fail their job with why. The report, with its curves, stays in the Thing's `last`.
- **Auto exposure leaves the gain alone.** Raising it trades noise for speed, which is the user's choice. Its test reaches the longest exposure with the light dimmed to 10⁻⁷.
- **White balance needs some background in view.** Over tissue alone, its gains are the tissue's colour. Checkpoint C balances over clear glass, then returns to the tissue from below.
- **The preview keeps its gains as `f32`s.** A client reading them back sees them rounded.

### The `zstd` crates, at two weeks

P28 took `zstd` 0.13.4 and `zstd-safe` 7.4.0 at 13 days old, for the fixes of three RustSec advisories, as the owner allowed. They turned two weeks old at 14:12 UTC on 2026-11-15, and were checked at 14:13:

- **crates.io** shows neither yanked, and both BSD-3-Clause, which `cargo deny` allows.
- **A fresh `cargo deny check` passes,** its advisory database updated that day.
- **Newer patch releases** came out on 2026-11-12: `zstd` 0.13.5 and `zstd-safe` 7.4.1. No advisory concerns the versions in use, so these wait until they are two weeks old.

### The size

L's budget is 900; P34 measured 3,622, about 14 % under its estimate of ~4,200.

- **One pull request,** under the owner's standing choice, with the overrun explained. The measures, the sweeps that use them, the Things that run them, and the simulator's truth they are held to are one piece, and Checkpoint C closes the stage.
- **Against the estimate,** the procedures took about 1,420 lines against ~1,070, after the measurements' revisions. Tests took 912 against ~1,300, and the examples 623 against ~650.

## Tests

| Crate | Test | What it checks |
|---|---|---|
| `oscope-compute` | `focus` (3, unit) | **Sharper images measure higher,** by each measure, ten times and more, and twice the light gives the same measures. **A flat image measures nothing,** and shapes are checked. **Each measure is its definition,** on a single bright pixel: Brenner's 4 terms, Tenengrad's 24, and the Laplacian's variance |
| `oscope-compute` | `cuda` (1 more) | The CUDA twin agrees with the CPU to the bit on 40 seeded images, and through the compute layer; a colour image refused alike (with `OSCOPE_CUDA=1`) |
| `oscope-sim` | `truth` (1 more) | **Each sample's truth:** the H&E section's middle, the chip's surface where it's looked at, the cells' dye below their slab's middle, the world's focus added |
| `oscope-workflows` | `autofocus` (4, unit) | **The peaks:** the three-point estimate exact on a Gaussian, a parabola's vertex near on uneven samples. **The refusals:** an empty field, a focus beyond the sweep. **A continuous sweep's frames** decide what follows: stepped, a refusal, the fine pass, or the vertex. **The depth of field** is the plan's at 5×, 10× and 50× |
| `oscope-workflows` | `autofocus` (6) | **Within a quarter of the depth of field** on the H&E tissue, the reflective chip and a sparse field of cells, each in its light's measure, and through 6 µm of backlash. **An empty field refused,** stepped and continuous, the stage back where it started to 0.1 µm. **Continuous and stepped agree.** **Auto exposure converges** from dark and from saturated, and holds at the longest exposure. **White balance** recovers a tinted light's gains, and finds the light at an island's edge, a quarter glass beside pink tissue; a light off is refused |
| `oscope-api` | `focus` (1) | **Over HTTP:** autofocus as a job, within a quarter of the depth of field of the truth the simulation Thing gives; its defaults, measures and report; auto exposure from dark; white balance, its gains given to the preview by the client; an empty field refused, the stage back; options that can't be, refused at submission |
| `oscope-api` | `devices`, `jobs` | The new Things' TDs and the simulation Thing's (snapshots); the job kinds listed |
| `ex-autofocus`, `ex-checkpoint-c` | `--smoke` | Each run within a quarter of the depth of field; the glass refused; the exposure and white balance jobs; Checkpoint C's calibration, focus, exposure and capture |

**Where they ran:**

- **On Windows,** `scripts/ci-local.sh --cuda`, run alone on `abecf3c`: 46 of 46 steps, 451 tests in 171 suites.
  - They included the Python tests, real, and the CUDA tests (`OSCOPE_CUDA=1` and the `cuda` feature), the focus measures' CUDA twin among them.
  - Every example's `--smoke` ran, `ex-autofocus`'s and `ex-checkpoint-c`'s new among them.
  - Its first run, on `c914ce1`, failed one step: `cargo doc` refused the focus module's doc, which linked to a private function.
- **On Linux,** `scripts/ci-clone.sh` under WSL, on a fresh clone of `abecf3c`, run alone: 45 of 45 steps, 451 tests in 171 suites, the Python tests real; 622 files, none with a CR.
- **On two cores,** pinned, 5 times out of 5 each:
  - `oscope-compute`'s unit tests;
  - `oscope-sim`'s `truth`;
  - `oscope-workflows`' unit tests and `autofocus`;
  - `oscope-api`'s `focus`;
  - `ex-autofocus --smoke` and `ex-checkpoint-c --smoke`.

  They passed 5 times in 5 before the settling fix too.

**Mutation checks** (each broke the code on purpose, then the tests were run): twenty-seven of twenty-seven caught, run again after the fault below was fixed.

- **Reading the tests against the list first** found five gaps, now covered:
  - Tenengrad's value;
  - one-way approach and the lag's correction, which a stage with 2 µm of backlash moves by less than a quarter of the depth of field: a case with 6 µm;
  - each light's measure;
  - white balance's lightest 5 %, which a blank field can't tell from every cell: an island's edge, a quarter glass beside pink tissue;
  - each sample's truth.
- **The first run missed two,** now caught:
  - Sobel's x gradient from the wrong corners. A single bright pixel is symmetric, and a stripe pattern only halves. A ramp's gradient is exact.
  - White balance over every cell, which only the empty-field test's intermittent failure seemed to catch. The colour target's mean is neutral, so every cell gave the light's gains too. An island's edge, its mean pink, now catches it.
- **The empty-field test's failures were a fault,** not chance: z read as the stage arrived (see *Once the stage has settled* above). Two mutants now read it so, and its check of 0.1 µm catches both.

| Broken on purpose | Caught by |
|---|---|
| Brenner's gradient only along x | `the_measures_are_their_definitions` |
| The measures not divided by the mean's square | the same, and `sharper_images_measure_higher_and_the_light_s_level_doesn_t_matter` |
| The Laplacian's middle weighed −3 | `the_measures_are_their_definitions` |
| Sobel's x gradient from the wrong corners | the same, on the ramp |
| The CUDA Laplacian's middle weighed −3 | `the_focus_measures_twin_matches_the_cpu`, with `OSCOPE_CUDA=1` |
| The chip's truth its slide's surface | `each_sample_says_where_it_is_sharpest` |
| The cells' truth unweighted | the same |
| No empty field | `an_empty_field_is_refused_and_the_stage_goes_back` |
| No out-of-reach refusal | `empty_fields_and_out_of_reach_peaks_are_refused` |
| No three-point estimate | `the_peaks_are_found_between_samples` |
| The lag not taken up | `autofocus_comes_within_a_quarter_of_the_depth_of_field_on_each_sample`, through 6 µm of backlash |
| Approached from above | the same |
| The curve at the z commanded | the same |
| The stepped sweep's lags taken as the stage arrives | `an_empty_field_is_refused_and_the_stage_goes_back`, to 0.1 µm |
| The continuous sweep's start taken as the stage arrives | the same |
| A sparse continuous sweep trusted | `a_continuous_sweep_s_frames_decide_what_follows` |
| No fine pass where the frames are sparse | the same |
| Reflected light measured by Brenner's | `autofocus_comes_within_a_quarter_of_the_depth_of_field_on_each_sample` |
| The depth of field's NA not squared | `the_depth_of_field_is_the_plan_s` |
| Saturated frames scaled, not halved | `auto_exposure_converges_and_holds_at_a_limit` |
| Converged within ten times the tolerance | the same |
| A limit not held | the same |
| The worst exposure kept | the same |
| White balance's gains inverted | `white_balance_recovers_a_tinted_light_s_gains` |
| White balance over every cell | `white_balance_finds_the_light_beside_a_colourful_sample` |
| The `autofocus` Thing's `run` submits auto exposure | `autofocus_exposure_and_white_balance_run_as_jobs` |
| The `exposure` Thing without white balance's job | the same |

## Follow-ups and known gaps

- **The M2 app** gives white balance's gains to the preview's look, and plots the focus curve.
- **Scans** (P35–P37) take autofocus at each tile, or at a focus map's points, with a smaller range where the map predicts the focus.
- **z's backlash,** measured by autofocus approached from both sides: P32's follow-up, not yet done.
- **On the instrument (M3):** the empty-field bound on real slides, and the continuous sweep at the MIChrome 20's 7 fps, where its frames should keep their spacing.
- **The emission's wavelength,** for fluorescence's depth of field, once the HAL's channels carry it.
- **The simulator's z sweeps** would be quicker if it kept tiles near a depth for the next.

## Clean room

Nothing was copied from `microscope` or any other codebase. Each method is written from its sources, as the spec above sets them out:

- the focus measures, from Brenner et al. (1976), Tenenbaum (1970) and Krotkov (1987), and Pech-Pacheco et al. (2000), normalised as Groen, Young and Ligthart (1985) discuss;
- the defaults' choice, informed by the comparative studies of Groen et al. (1985), Sun, Duthaler and Nelson (2004), and Pertuz, Puig and Garcia (2013);
- the depth of field, from Inoué and Spring (1997);
- the three-point Gaussian estimate, from Willert and Gharib (1991), as P31;
- one-way approach, from ISO 230-2;
- auto exposure, from the sensor's linear response (EMVA 1288);
- the white patch, from Land and McCann (1971).
