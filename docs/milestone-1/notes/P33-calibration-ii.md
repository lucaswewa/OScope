# P33: Calibration II: dark, flat, colour; the wizard

- Status: Done
- Branch / PR: `m1/p33-calibration-ii` / [#47](https://github.com/lucaswewa/OScope/pull/47)
- Size: L · estimated ~4,150 before it started (the simulator's vignetting, colour response and colour target ~300; the colour target's layout and the colour science ~250; dark, flat and colour sets with their images ~300; the procedures ~450; their jobs ~120; the wizard ~400; the `calibration` Thing's additions ~300; tests ~1,300; `ex-wizard` ~300; glue ~60; and about a tenth on top); one pull request, at the owner's standing choice for P24–P34 · measured **3,156** hand-written lines added (code 1,864: `oscope-workflows` 1,224, `oscope-api` 270, `oscope-imaging` 181, `oscope-sim` 175, `oscope-compute` 14; tests 1,018; examples 253; glue 21), and 54 removed; the Thing Description's snapshot, 3,157 lines added and 618 removed, is generated. **That is 2,256 over L's 900, and 24 % under the estimate.**

## Summary

Calibration II: the photometric calibrations, and the calibration wizard on the server ([spec](../phases.md#p33); plan §5.13; [ADR-0023](../../adr/0023-the-calibration-model.md)).

- **Dark frames,** for each camera mode, gain and octave of exposure: the lights switched off, frames averaged, the lights put back.
- **Flat fields,** for each objective, light and binning, from a blank area:
  - frames averaged, and the dark taken away;
  - refused if the field isn't blank;
  - each colour normalised to a mean of 1.

  The blank field's colour gives **the white balance**.
- **The colour-correction matrix,** for each light, from a colour target added to the simulator's slides:
  - 24 patches found through the active geometry, the dark taken away and the flat field divided out;
  - white-balanced on the lightest grey;
  - a 3 × 3 matrix fitted by least squares.

  ΔE\*ab is reported before and after, and a fit above a mean of 5 is refused.
- **Dark frames and flat fields** keep their image beside their set, and stay on the instrument: they are neither exported nor imported.
- **The wizard:**
  - a state machine of six steps: the objective check, the geometry, the dark frame, the flat field, colour, and the stage last;
  - each step has a prompt, a job, and a check of what the job gave;
  - a failed step is retried or skipped;
  - the wizard is saved after every change, so it resumes after a restart.
- **The simulator's truth:**
  - vignetting in the cos⁴ law's shape, on the CPU and the GPU;
  - a sensor colour response;
  - a colour target among its samples.

  Calibration recovers it: through a crossed colour response, ΔE\*ab falls from 15 to 0.02.
- **P32 is marked Done.**

## The methods

### Frames

- **Units.** Frames are averaged as the camera gives them, in linear units from 0 to 1: 16 bits ÷ 65,535, 8 bits ÷ 255. A colour mosaic is kept as it is: each correction works on its sites, colour by colour.
- **Timing.** The first frame averaged is the first whose exposure starts at least the settling time after the request, 50 ms by default, on the instrument's clock; then each next frame, 16 by default.

### Dark frames

- **The lights.** The lights that are on are switched off through the Illumination HAL, the frames averaged, and the lights put back as they were, even if the frames failed. Lights that can't be switched are the prompt's to block.
- **What it records:** the mean, the spread over the frame (the fixed pattern's), the region, the exposure it was taken at, and the octave of exposure it serves.
- **The octave:** from a power of 2 ms up to the next. 30 ms's dark serves 16 to 32 ms.
  - A dark signal is an offset, which doesn't change with the exposure, and a part that grows in proportion to it (EMVA 1288).
  - Within an octave, that part differs by at most twice.
- **Its key** is `dark/{mode}/{gain}/{the octave's start}`.
- **Its users** take the active dark frame for a frame's mode, gain and exposure, if its size matches. With none, they take the frame's black level instead.

### Flat fields

1. **The frames:** at a blank area of the slide, through the fitted objective, in the lights that are on. They are averaged, and the dark taken away.
2. **Blank.**
   - Green's sites, as an image of their own, are compared with a box blur of themselves. The blur's radius is a 32nd of the larger side, and at least 4.
   - The root-mean-square residual must be under 3 % of the level. Vignetting is smooth, so it passes; tissue, dust or an edge doesn't.
3. **Normalised per colour.** Each colour's sites are divided by their mean, so the flat field corrects shading only, not colour: corrected = (raw − dark) ÷ flat. This is flat fielding, as Janesick (2007) describes it for removing a sensor's and its optics' fixed pattern.
4. **The white balance** is the blank field's colour: the gains (G/R, 1, G/B) from the colours' means, which make the light's blank field grey.
5. **The unevenness** is the blurred green's root-mean-square spread about its mean, over its mean: how much the shading varies, before correction.

Its key is `flat/{objective}/{light}/{binning}`, the light being the channels on, joined with `+`.

### Colour

1. **The target** is OScope's own, in the simulator's slide model.
   - 6 × 4 patches, 100 µm square on a 140 µm pitch, between borders of optical density 2.
   - Six greys, transmitting 0.9 down to 0.05.
   - 18 colours around the hue circle at two saturations, each the linear value, by the sRGB transfer function (IEC 61966-2-1), of an HSV colour.
   - It is 840 × 560 µm: inside the field of the 5× and the 10×.
2. **Found** through the active geometry. A patch's centre, s µm from the target's middle, lies at M⁻¹ · s pixels from the image's middle, so the camera's turn and mirror are allowed for. The target must be centred.
3. **Measured:** each patch's middle half, colour by colour. The frames are averaged, the dark taken away, and the active flat field divided out.
4. **White-balanced** on the lightest grey: gains that make it its known value.
5. **The matrix** is the M that minimises Σ |t − M · m|² over the patches, from the normal equations: M = (Σ t mᵀ)(Σ m mᵀ)⁻¹. This is the linear, 3 × 3 case of Hong, Luo and Rhodes' (2001) models, by Björck's (1996) least squares.
6. **ΔE\*ab** (CIE 1976) is measured between each patch, corrected, and its known colour.
   - It is taken in CIELAB (CIE 15:2004), from XYZ under D65 by IEC 61966-2-1's matrix, white being full transmission.
   - Reported: the mean before the matrix (white-balanced), and the mean and the largest after it.
   - Refused above a mean of 5 after.

Its key is `colour/{light}`.

### The wizard

- **Its steps,** in order: the objective check, the geometry, the dark frame, the flat field, colour, and the stage. `wizard_start` may name some of them, in their order.
- **The stage's step comes last.** It homes, which takes the sample out of view and back, so no step after it waits for a frame. See the design notes.
- **A step's states:**
  - **waiting**, then **prompting**;
  - confirmed, it is **running**;
  - its job's end makes it **done** or **failed**;
  - a failed step is retried, and prompts again; a prompting or failed one may be **skipped**.

  Once a step is done or skipped, the next prompts. Once the last is settled, the wizard is finished. Cancelling ends it anywhere: a running job carries on, but the wizard no longer waits for it.
- **Validation.**
  - A step is done when its job completes. The objective's is done only if the check finds the objective plausible.
  - A job's refusal fails its step, saying why: the field isn't blank, the target isn't in the frame, or the colours fit poorly.
- **Required actions are prompts.** The server can't see whether the slide is blank or the target loaded. The jobs' checks refuse what isn't.
- **Resume.**
  - The wizard is saved as `calibration/wizard.json` after each change, written whole and renamed into place, and read when the server starts.
  - A step whose job ended while no one looked is settled from the job service's record, on the next read.
  - A step whose job isn't known fails with "retry it".
  - A saved wizard that can't be read is set aside with a warning.

### The simulator's truth

- **Vignetting.**
  - The light falls off as 1 / (1 + s r²)², where r² is the squared distance from the sensor's middle over the half-diagonal's. This is the cos⁴ law's form: cos⁴ θ = 1 / (1 + tan² θ)², with tan θ in proportion to r (Hecht, *Optics*).
  - At the corners the light is 1 / (1 + s)² of the middle's.
  - It applies by the sensor's pixel, through any region and binning, in the CPU's exposure and the CUDA kernel's.
- **The colour response** is a 3 × 3 matrix applied to the light's colours in the tile cache, which both paths share.
- **The colour target** is the sample `colour target`, at the coverslip's middle:
  - an absorbing layer, each patch's optical density from its transmittance, and the borders' 2;
  - the glass's four surfaces pass 0.96⁴ of the light (Fresnel's 4 % lost at each; Hecht).

## What was built

| Where | What |
|---|---|
| `oscope-workflows`, `calibration` | `Dark`, `Flat` and `Colour` sets and their keys; their images kept beside them (`save_with_image`, `image`); `measure_dark`, `measure_flat` and `measure_colour`, and their jobs; the `Wizard` |
| `oscope-api`, `calibration` | The `calibration` Thing's jobs `dark`, `flat` and `colour`; `active`, with the dark frame, flat field and colour in use; the `wizard` property, and `wizard_start`, `wizard_confirm`, `wizard_skip`, `wizard_retry` and `wizard_cancel` |
| `oscope-imaging`, `color` | `ColourTarget`; CIELAB, ΔE\*ab, and the colour matrix's fit |
| `oscope-sim` | `[devices.simulator]`'s `vignetting` and `response`; vignetting in the CPU's exposure; the colour response in the tile cache; the colour target sample |
| `oscope-compute` | Vignetting in the CUDA exposure kernel |
| `examples/ex-wizard` | The wizard walked from the command line |

## How to try it

[`ex-wizard`](../../../examples/ex-wizard/README.md):

```sh
cargo run --release -p ex-wizard
```

It sets up the simulator with a known truth:

- a camera turned 30° and mirrored;
- objectives 3 % off in magnification;
- 5 µm of backlash;
- optics that vignette;
- a sensor whose colours cross.

It then walks the wizard through the API. For each step it prints the prompt, does what it asks on the simulator, confirms, and prints what the step made. At the end it prints the sets the wizard made, and it serves at port 8236 with links to the wizard and the sets.

## Performance

No §5.20 budget is P33's. On the owner's PC:

- **`ex-wizard` at real time,** on the 20 MP sensor in a release build, took 119 s.
  - The steps took: the objective check 11 s, the geometry 20 s, the dark frame 3 s, the flat field 7 s, colour 5 s, and the stage 45 s, homing most of it. Homing and the move to the tissue, before them, took 20 s.
  - With the stage's step third, as at first, the dark frame's after it took 14 s: its first frame waited for the simulator to render the frames after homing.
  - A photometric step's time is mostly the simulator's, rendering 16 frames of 20 MP on the CPU.
- **The procedures' own work** over a 20 MP frame is sums and one blur. The blank check's blur, over the green sites' 2,736 × 1,824, takes 86 ms in a release build by running sums. Summing each window afresh, as it did at first, took 435 ms.
- **The tests,** on the quarter sensor at ten times real time in a debug build: `photometry` takes 7 s, and the API's `calibration` test 48 s, its three tests one at a time.

## Design notes and deviations from the plan

### The stage's step comes last

- **What happened.** The wizard first ran the stage's step third, after the geometry, as plan §5.13 lists the calibrations. On two cores, in debug builds, the dark frame's step after it failed 5 times out of 5 in `ex-wizard --smoke`: "no frame came within 30 s".
- **Why.** P32 met it first: the stage homes while the camera streams, and the simulator renders each frame of the way afresh, at new places and depths. Back over the tissue, it renders the tissue's tiles again. A probe on two cores measured it: the first frame after homing came 26 s after the stage was back, where a cold start's came in 8 s, and the frames after each came 0.2 s apart. With a server sharing the two cores, it passed 30 s.
- **The change.** The stage's step comes last, as P32's stage calibration homes last: no step after it waits for a frame. None of the photometric steps needs the stage's calibration. Its prompt asks for a textured part of the sample back in view.
- **Before it,** `ex-wizard` and the API's test take the stage back to the tissue's place, and only then load the H&E slide. Loaded first, the move's frames rendered tissue at each new place on the way. That way round, the stage's step waited more than 30 s for its first frame 2 times in 5 on two cores.
- **And the API's tests take turns.** Its three tests each start a server, whose simulator streams at ten times real time. On two cores, run together, they starved each other's frames, P32's test among them.
  - They now run one at a time, as `photometry`'s and the procedures' tests do.
  - Each stops its camera before its turn ends. Otherwise the camera's last frame, after homing, rendered on into the next test's first: P32's test failed so 2 times in 5.

### Flat fields for each binning too

Plan §5.13 keeps a flat field for each objective and light. P33's are for each binning too, as the flat's image is the frame's, and binning changes it.

### Dark frames for each octave of exposure

The plan's "per exposure and gain range" is, for exposure, an octave: from a power of 2 ms up to the next. A dark frame's offset doesn't change with exposure, and its dark current grows in proportion to it. Within an octave, that part differs by at most twice. The set records the exposure it was taken at too.

### Two white balances

- **The flat field's** comes from the blank field, the light's colour through the optics. It suits any sample in that light.
- **The colour calibration's** comes from the target's lightest grey. It goes with its matrix, which was fitted after it.

Both are kept. Auto white balance (P34) and the M2 app's display choose between them.

### Dark frames and flat fields stay on the instrument

- **They aren't exported or imported.** Their image is this instrument's own sensor and optics, and another instrument's would mislead.
- **The image beside the set.** It is `<id>.f32`: its width, height and channels as three 32-bit words, then little-endian `f32`s. It is written whole before the set's JSON, so a set read back always has its image.
- **The JSON** keeps what describes it: the mean, the spread, the white balance, the unevenness.

### The colour target is OScope's own

- **Its layout and colours** are chosen here: greys spaced in transmittance, and hues every 20° at two saturations. They follow no commercial chart's.
- **Its size:** the patches are 100 µm on a 140 µm pitch. At first they were 120 µm on a 160 µm pitch, and the target didn't fit the 10×'s field on the quarter sensor once the camera was turned by 20°. The colour step failed then with "the colour target isn't all in the frame".

### The simulator's crosstalk is linear

The simulated sensor's colours are a 3 × 3 mix of the light's, which a colour matrix undoes almost exactly: ΔE\*ab 0.02 on the quarter sensor and 0.00 on the 20 MP. A real sensor's spectral responses aren't a linear mix of the eye's, so a real target leaves a few units (Hong, Luo and Rhodes, 2001). The refusal at a mean of 5 is set for that.

### A saved wizard that can't be read is set aside

- **What was wrong.** The `calibration` Thing read the saved wizard as it started, and an error there failed the Thing. A `wizard.json` that didn't parse, after a later phase renamed a step say, would have stopped the server.
- **The change.** It is set aside with a warning, and the next wizard replaces it.
- **The store** skips the wizard's file beside its sets, as it skips `active.json`.

### Other choices

- **Frames are taken whole,** the mosaic kept: a flat field corrects each site, so each colour's shading is its own.
- **A patch's middle half** is measured, leaving a quarter of its side on each edge for blur and for the target's placement.
- **The blank check's 3 %** passes vignetting, which a blur follows, and the sensor's noise averaged over 16 frames, and refuses tissue. Over the simulator's H&E it measured 21 %.
- **The objective's step** is done only if the check finds the objective plausible. The check's own job completes either way, with its report.
- **The sets in use:**
  - the dark frame for the camera's mode, gain and exposure now;
  - the flat field for the fitted objective, the lights on and the binning;
  - colour for the lights on.

### The size

L's budget is 900; P33 measured 3,156, about 24 % under its estimate of ~4,150.

- **One pull request,** under the owner's standing choice, with the overrun explained. The photometric sets, their procedures and jobs, the wizard that runs them, and the simulator's truth they are tested against are one piece.
- **Against the estimate,** every part but the procedures came in under it. The procedures took about 540 lines against ~450. Tests took 1,018 against ~1,300: a third of the lines, as estimated. The tenth on top wasn't needed.

## Tests

| Crate | Test | What it checks |
|---|---|---|
| `oscope-sim` | `truth` (3) | **Vignetting:** a blank frame's corner against its middle, as 1 / (1 + s r²)² says; and through a bin2 region of the sensor's lower right, where the falloff follows each pixel's place on the sensor, not in the frame. **The colour response:** white light read through a tint, colour by colour. **The colour target:** each patch's colour at its place, through the glass's 0.96⁴; the borders passing 1 %; clear glass beyond |
| `oscope-sim` | `gpu` | The GPU's exposure, vignetted and tinted through a bin2 region, against the CPU's (with `OSCOPE_CUDA=1`) |
| `oscope-imaging` | `color::target` (3, unit) | **CIELAB:** white at lightness 100, a grey neutral, sRGB's red at its published values, and the linear branch below (6/29)³. **The matrix** recovered exactly from exact colours, and none from colours that don't span. **The target's patches** on its grid, within it |
| `oscope-workflows` | `photometry` (6) | **Dark subtraction** removes the offset, and the lights come back on. **A slow lamp:** a dark frame waits out the settling time, for lights that go dark 300 ms after they are switched off. **The flat field** evens out vignetting of 0.3, from a spread of more than 5 % to less than a fifth of it and under 2 %. Each colour of it averages 1, and it gives the white balance of a tinted light. **Refused** over the tissue. **The colour matrix** recovers the target's colours through a crossed response and vignetting: from a ΔE\*ab of more than 5 to a mean under 0.5 and a largest under 1, with the white balance of the light's white through the response. **A poor fit is refused:** over the tissue, a mean of 42 |
| `oscope-workflows` | `store` (1 more) | **A dark frame** saved with its image, and only so; one for an octave of exposure; read back with its image; neither exported nor imported |
| `oscope-workflows` | `wizard` (3, unit) | **The transitions:** prompting, running, done; failed, retried, failed, skipped; another job's end ignored; finished. **Refused:** what a step can't do (a retry of what hasn't failed, a second confirmation, a skip while running, anything once finished), and a wizard of no steps. **Resume:** a saved wizard read back, its running step then settled |
| `oscope-api` | `calibration` (2 more) | **The wizard's walk** over HTTP: every step done, the stage last, back over the tissue; the flat field over the tissue failed, and retried over clear glass; the dark frame, flat field and colour in use; a saved wizard that can't be read set aside. **A wrong magnification** fails the objective's step, naming the 20×; then skipped, and the wizard cancelled. **Resumed:** the server started again finds the wizard as it was |
| `oscope-api` | `devices`, `jobs` | The Thing's TD (a snapshot); the job kinds listed; the samples counted |
| `ex-wizard` | `--smoke` | The walk on the quarter sensor at ten times real time, held to P33's tolerances |
| `ex-sim-samples`, `ex-http-walkthrough` | `--smoke` | The simulator's samples, the colour target among them |

**Where they ran:**

- **On Windows,** `scripts/ci-local.sh --cuda`, run alone on `766b596`: 44 of 44 steps, 436 tests in 167 suites.
  - They included the Python tests, real, and the CUDA tests (`OSCOPE_CUDA=1` and the `cuda` feature), the simulator's vignetted GPU exposure among them.
  - Every example's `--smoke` ran, `ex-wizard`'s new among them.
  - Its first run, on `d2101d9`, failed one step: `ex-http-walkthrough --smoke` still counted the simulator's five samples, where there are now six.
- **On Linux,** `scripts/ci-clone.sh` under WSL, on a fresh clone of `766b596`, run alone: 43 of 43 steps, 436 tests in 167 suites, the Python tests real; 598 files, none with a CR.
- **On two cores,** pinned, 5 times out of 5 each:
  - `oscope-sim`'s `truth`;
  - `oscope-workflows`' `store`, unit tests, `photometry` and `procedures`;
  - `oscope-api`'s `calibration`;
  - `ex-wizard --smoke` and `ex-calibrate --smoke`.

  The first pinned runs failed: `oscope-api`'s `calibration` 5 times in 5, and `ex-wizard --smoke` 5 times in 5. See "The stage's step comes last" above.
  - The wizard now homes last.
  - The API's tests take turns, and stop their cameras.
  - Its helper waits up to 300 s for an action: stopping the camera after homing once took more than 60 s.

**Mutation checks** (each broke the code on purpose, then the tests were run): thirty-six of thirty-six caught.

- **Reading the tests against the list first** found six gaps, now covered:
  - the store's refusals of image sets;
  - the colour fit's refusal;
  - the objective's verdict in the wizard;
  - the dark frame in use, and the flat field's use of it;
  - the simulator's vignetting and response through `[devices.simulator]`;
  - CIELAB beyond white and grey.
- **The first run missed five,** now caught:
  - **No settling:** the simulator's lights switch off at once. A test's lamp now goes dark 300 ms after.
  - **A flat field's colours all scaled by green's:** the test looked only at green, and the colour step's white balance took the difference up. Each colour's mean is now checked.
  - **The flat field not divided out of the target:** the colour test's bounds, a mean of 2 and a largest of 4, were loose. The method gives 0.04, and without the flat field it is 1. The bounds are now 0.5 and 1.
  - **No white balance before the matrix:** a 3 × 3 matrix takes it up. The white balance itself is now checked.
  - **The wizard not saved after a change:** nothing restarted the server. The API's test now does.

| Broken on purpose | Caught by |
|---|---|
| The lights not turned off for a dark frame | `dark_subtraction_removes_the_offset_and_the_lights_come_back` |
| The lights not restored after it | the same |
| Frames summed, not averaged | the photometry tests: dark, flat and colour |
| No settling after a change | `a_dark_frame_waits_for_a_lamp_that_dims_slowly` |
| The blank check off | `a_flat_field_is_refused_where_the_slide_isnt_blank` |
| A flat field's colours all scaled by green's | `the_flat_field_evens_out_vignetting_and_gives_the_white_balance`, `the_colour_matrix_recovers_the_target_s_colours` |
| The white balance inverted | `the_flat_field_evens_out_vignetting_and_gives_the_white_balance` |
| A matching dark frame found but not taken away | `the_wizard_walks_every_step_and_a_failed_one_is_retried` |
| The black level not taken away | the flat field's and the colour matrix's tests |
| A 16-bit black level scaled as 8-bit | four of the photometry tests |
| Patches placed without the geometry's turn | `the_wizard_walks_every_step_and_a_failed_one_is_retried` |
| The flat field not divided out of the target | `the_colour_matrix_recovers_the_target_s_colours` |
| No white balance before the matrix | the same |
| ΔE\*ab before taken after the matrix | the same |
| A poor fit accepted | `a_colour_matrix_that_fits_poorly_is_refused` |
| The matrix fitted the other way round | `a_matrix_is_recovered_from_exact_colours` |
| CIELAB's a\* from the wrong pair | `white_is_lightness_100_and_grey_neutral` |
| CIELAB's dark branch a cube root | the same |
| A dark frame for each exposure, not each octave | `a_dark_frame_keeps_its_image_and_stays_on_its_instrument` |
| An image read back turned | the same |
| An image set exported | the same |
| An image set imported | the same |
| An image set saved without its image | the same |
| A failed step moves on | `steps_prompt_run_and_settle_in_turn` |
| Another job's end settles the step | the same |
| A step retried that hasn't failed | `what_a_step_can_t_do_is_refused` |
| A running step skipped | the same |
| A dark frame never made active | `the_wizard_walks_every_step_and_a_failed_one_is_retried` |
| The wizard not saved after a change | `the_wizard_s_objective_step_fails_through_a_wrong_magnification` |
| The objective's verdict ignored | the same |
| A saved wizard that can't be read stops the server | `the_wizard_walks_every_step_and_a_failed_one_is_retried` |
| Vignetting left out on the CPU | `the_light_falls_off_towards_the_corners_as_cos4_says` |
| The colour response left out | `the_sensor_reads_white_light_through_its_colour_response` |
| The simulator's vignetting not given to the camera | `the_wizard_walks_every_step_and_a_failed_one_is_retried` |
| The simulator's response not given to the camera | the same |
| The GPU's exposure not vignetted | `the_gpu_exposes_frames_as_the_cpu_does`, with `OSCOPE_CUDA=1` |

## Follow-ups and known gaps

- **Calibration's users:** the dark frame, the flat field and the colour matrix are kept and chosen, but nothing applies them yet. The M2 app's display and P34's auto white balance will take them.
- **A dark library:** one dark frame for each of several octaves of exposure, taken in one job.
- **Defective pixels:** a dark frame's spread is recorded; a map of hot pixels from it is a follow-up.
- **A real colour target,** such as a calibrated slide, has its own layout. The target's layout would then be configurable.
- **The blank check's 3 %** is set on the simulator. On the instrument (M3), a blank field's roughness will be measured, the sensor's fixed pattern included, and the bound checked against it.
- **The wizard's lost job:** a step whose job the job service doesn't know fails with "retry it". No test gets the service to forget a job.
- **The objective's step after a swap:** the wizard runs the check as its first step. Running it again after each swap belongs to the M2 app.

## Clean room

Nothing was copied from `microscope` or any other codebase. Each method is written from its sources:

- dark frames' offset and dark current, from EMVA 1288;
- flat fielding, from Janesick (2007);
- the colour matrix, by least squares, from Hong, Luo and Rhodes (2001) and Björck (1996);
- CIELAB and ΔE\*ab, from CIE 15:2004, with sRGB's matrix and transfer function from IEC 61966-2-1;
- vignetting's cos⁴ law and Fresnel's reflection, from Hecht.

The colour target's layout and colours are OScope's own.

### Sources

- EMVA Standard 1288, *Standard for Characterization of Image Sensors and Cameras*, Release 4.0, European Machine Vision Association, 2021: the dark signal, an offset and a part growing with exposure.
- J. R. Janesick, *Photon Transfer: DN → λ*, SPIE Press, 2007: fixed-pattern noise, and flat fielding.
- G. Hong, M. R. Luo and P. A. Rhodes, "A study of digital camera colorimetric characterization based on polynomial modeling", *Color Research & Application* 26(1), 76–84, 2001.
- Å. Björck, *Numerical Methods for Least Squares Problems*, SIAM, 1996.
- CIE 15:2004, *Colorimetry*, 3rd ed.: CIELAB, and the colour difference ΔE\*ab.
- IEC 61966-2-1:1999, *Multimedia systems and equipment: Colour measurement and management: Part 2-1, Default RGB colour space: sRGB*.
- E. Hecht, *Optics*, 5th ed., Pearson, 2017: the cos⁴ law, and Fresnel's equations.
