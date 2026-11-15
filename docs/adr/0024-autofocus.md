# ADR-0024: Autofocus

- Status: Accepted
- Date: 2026-11-15
- Phase: P34

## Context

- **Plan §5.14** asks for autofocus by Brenner's, Tenengrad's and the Laplacian's focus measures on a binned region; a coarse sweep, then a fine one; a fast mode that sweeps z continuously and matches frames to z by their timestamps; and different defaults for reflected and transmitted light. The depth of field sets the steps.
- **Scans** (P35–P37) focus at each tile or at a focus map's points, and z-stacks (P41) and depth from focus (P42) step through z. They need a focus they can trust, quickly, and a refusal they can act on where there is nothing to focus on.
- **The stage moves under the camera.** z has backlash like x and y (P32), and a camera's frame isn't taken at one instant: a rolling shutter exposes rows in turn, while the stage may move.

## Decision

**1. A focus measure** is computed on the frame's middle (a quarter of its area by default), made grey from a colour mosaic's 2 × 2 cells, binned until its longer side is at most 512 pixels, and divided by its mean's square, so that it doesn't change with the light's level. Three are offered: Brenner's gradient, Tenengrad and the variance of the Laplacian; transmitted light uses Brenner's by default, reflected light Tenengrad, and fluorescence the Laplacian's. They run on the CPU, or with CUDA when the server has a GPU, and agree within 10⁻⁴.

**2. The steps follow the depth of field,** d = λ n / NA² + n e / (M NA), from the fitted objective, the camera's pixel and binning, and the light's wavelength: a coarse sweep of ±6 d in steps of d, then a fine one of ±d in steps of d / 4, its peak refined by the three-point Gaussian estimate.

**3. Every position is approached moving up,** from 10 µm below the sweep's first, so that z's backlash shifts every position alike.

**4. The continuous sweep** moves the stage at a constant speed, d / 4 per frame interval, and puts each frame at the z the stage's position track gives for its exposure midpoint, its rolling shutter's middle row included. The peak is a parabola's vertex through the log of the measures around the best frame. A camera slower while the stage moves (the simulator renders each depth afresh) leaves its frames sparse: then the stepped sweep's fine pass about the best frame finds the peak. Stepped and continuous sweeps agree within d / 4.

**4a. A sweep's z is where the stage says it is.** The final move commands the peak plus the lag the sweep showed between the z commanded and the z reported: a stage that reports its true position lags, approached from below, by half its backlash.

**5. Refusals leave the stage where it started:** an empty field (the best measure under 1.2 times the median), or a peak at the coarse sweep's end (the focus beyond the range). Each says why.

**6. Auto exposure and auto white balance are jobs too.** Auto exposure scales the exposure by the target over the region's 99.5th percentile, the sensor being linear, within the camera's limits, until within 5 % of the target. Auto white balance takes the region's lightest pixels as the light's white, and reports their gains, which a client gives the preview's look: a job doesn't reach into another Thing.

**7. Two Things:** `autofocus` (the current measures, the defaults, the last report, and the job `run`) and `exposure` (the last report, and the jobs `auto` and `white_balance`).

## Consequences

- Scans call the same procedure, with a smaller range where a focus map predicts the focus.
- A slow camera makes the stepped sweep slow; the continuous sweep takes one pass, but needs the stage's track to cover the frames' exposures.
- The refusal at 1.2 times the median is set on the simulator; the instrument (M3) checks it on real slides.
- Auto exposure leaves the gain alone: raising it trades noise for speed, which is the user's choice.

## Alternatives considered

- **A golden-section or hill-climbing search,** which takes fewer frames on a clean curve but is misled by a second peak, as a section's two surfaces or dust on the coverslip give. A sweep sees the whole curve.
- **A single fixed measure,** simpler, but fluorescence's sparse, bright features and reflected light's specular texture favour other measures than brightfield.
- **A phase-detection or reflection-based focus,** which needs hardware the instrument doesn't have.
