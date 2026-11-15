# P34: Autofocus, auto exposure, auto white balance

- Status: In review
- Branch / PR: `m1/p34-autofocus` / [#48](https://github.com/lucaswewa/OScope/pull/48)
- Size: L · estimated ~4,200 before it started (the focus measures on the CPU ~150 and with CUDA ~200; the autofocus procedure, stepped and continuous sweeps, ~600; auto exposure ~200; auto white balance ~120; their jobs ~150; the `autofocus` and `exposure` Things ~350; the simulator's truth ~50; tests ~1,300; `ex-autofocus` ~400; `ex-checkpoint-c` ~250; glue ~60; and about a tenth on top); one pull request, at the owner's standing choice · measured {{SIZE}}

## Summary

{{SUMMARY}}

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

d = λ n / NA² + n e / (M NA), with λ 0.55 µm (green light; a fluorescent light's emission instead), n the medium's index (1 for the dry objectives), e the sensor's pixel times the camera's binning, M and NA the fitted objective's (Inoué and Spring, 1997). With the MIChrome 20's 2.4 µm pixels: 31 µm at 5×, 7.9 µm at 10×, 1.9 µm at 50×, as plan §5.14 says.

### The stepped sweep

1. **The coarse sweep:** from z₀ − R to z₀ + R in steps of d, where z₀ is where the stage is and R is 6 d by default (13 positions).
2. **The fine sweep:** around the coarse sweep's best position, from it less d to it plus d, in steps of d / 4 (9 positions).
3. **The peak:** the fine sweep's best position k, refined by the three-point Gaussian estimate, z_k + s (ln F_{k−1} − ln F_{k+1}) / (2 (ln F_{k−1} − 2 ln F_k + ln F_{k+1})), as P31's registration refines its peak (Willert and Gharib, 1991). At an end of the fine sweep, its best position as it is.
4. **The end:** the stage moves to the peak.

**One way in.** Every position is reached moving up, from an approach point 10 µm below the sweep's first (ISO 230-2's unidirectional approach, as P32's calibrations). The stage's backlash in z then shifts every position the same, and the peak with them.

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
3. **Given to the preview:** its look's gains are set to them, if the server's preview runs.

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
