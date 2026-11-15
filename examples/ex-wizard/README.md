# ex-wizard

P33's calibration wizard, walked from the command line on a simulated instrument whose truth is known:

- a camera turned by 30° on the stage, its image mirrored, through objectives that magnify 3 % more than the catalogue says;
- 5 µm of backlash in the stage;
- optics that vignette, the light at the field's corners falling to 64 % of the middle's, in the cos⁴ law's shape;
- a sensor whose colours cross: its red reads some green and its blue some green, as filters and a warm light do.

All through the API, as a client would, it homes the stage, takes it over the simulated H&E tissue, starts the camera, and starts the wizard (the `calibration` Thing's `wizard_start`). Then, for each of the wizard's six steps, it:

1. prints the step's title and its prompt, what a person is asked to do before confirming;
2. does that on the simulator, where the step needs it: the flat field's step moves the stage to clear glass beside the coverslip, and the colour step loads the colour target (`simulation/load`) and centres it;
3. confirms the step (`wizard_confirm`), which starts its job, and reads the wizard until the step has settled, done or failed;
4. prints what the step made: the objective check's verdict, or the calibration set saved and put in use.

At the end it prints the sets in use, as the wizard left them.

## How to try it

```sh
cargo run --release -p ex-wizard
```

It serves at http://127.0.0.1:8236/, whose page links to the wizard, the sets in use, every calibration set, and the API's documentation. The walk takes about 130 s at real time, homing twice most of it; `--rate 10` runs the simulator ten times as fast. On the owner's PC it printed:

```text
1. Check the objective
   Fit the objective that is in the light path, and focus on a textured part of the sample. Two small moves check that the image moves as that objective's should.
   ✓ plausible: the 10x

2. Calibrate the geometry
   Keep a textured part of the sample in focus, with room around it for moves of a quarter of the field each way.
   ✓ calibration/01a1dcbb-fe6a-72bc-813b-9ace0387e188

3. Calibrate the stage
   Keep the textured part in view: the stage measures its backlash there, then homes, and comes back.
   ✓ calibration/01a1dcbc-9ba7-7668-9d6a-be2086731336

4. Take a dark frame
   Lights that can be switched are turned off for the dark frame, and back on. Block any other light from reaching the camera.
   ✓ calibration/01a1dcbc-d27a-75e3-ba44-059f5964060a

5. Take a flat field
   Move to a blank part of the slide, clear glass with nothing on it, in focus, with the light on as it will be used.
   → the stage moves to clear glass, beside the coverslip
   ✓ calibration/01a1dcbc-ff4a-71f4-a706-60c2d8fd0b38

6. Calibrate colour
   Load the colour target, and centre it in the field of the 5× or the 10×, in focus, with the light on as it will be used.
   → the colour target is loaded, and the stage centres it
   ✓ calibration/01a1dcbd-24f7-710e-8864-e5f85976f110

what the wizard made:
  geometry   0.23301 µm a pixel, turned 30.00°, mirrored
  stage      backlash 5.00 µm in x, 5.00 µm in y
  dark       mean 0.03052, spread 0.00004
  flat       9.9 % uneven before; white balance 0.909, 1.000, 1.250
  colour     ΔE*ab 0.00 on average after the matrix (largest 0.01), from 15.28
```

- **The prompts are the wizard's own**, as a page would show them. The objective's step is done only if the check finds the objective plausible.
- **The dark frame** is taken with the lights off. The simulated lights can be switched, so the wizard switches them off and back on itself.
- **The flat field** is refused where the slide isn't blank. Over the tissue, the step would fail; `wizard_retry` would ask again, and the step could be redone over clear glass. `oscope-api`'s `calibration` test walks that.
- **The colour matrix** is fitted to the target's 24 patches, seen through the flat field and less the dark. ΔE\*ab is the CIE 1976 colour difference, in CIELAB, between each patch as corrected and as made: below about 2, the difference is hard to see. "From" is the white-balanced patches' before the matrix.
- **The simulator's crosstalk is a 3 × 3 matrix,** which a colour matrix undoes almost exactly: 0.00 here, 0.02 on the quarter sensor. A real sensor's colours aren't a linear mix of the eye's, so a real target leaves more.
- **The wizard is kept** in `.oscope/ex-wizard/data/calibration/wizard.json`, and the sets beside it. A second run starts a new wizard, and adds new versions of the sets.

## `--smoke`

```sh
cargo run -p ex-wizard -- --smoke
```

It runs the same on the quarter sensor at ten times real time, with a temporary data root. It holds the result to P33's tolerances:

- every step done and the wizard finished;
- the colour matrix's mean ΔE\*ab below 0.5, as the simulator's crosstalk is linear;
- the flat field finding the field uneven;
- the rotation within 0.1° of 30°, mirrored;
- the backlash within 0.2 µm of 5 µm.

It ends with `smoke: ok`, or says what failed and exits with code 1.
