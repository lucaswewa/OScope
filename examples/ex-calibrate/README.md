# ex-calibrate

P32's calibration, on a simulated instrument whose truth is known:

- a camera turned by 30° on the stage, its image mirrored;
- objectives that magnify 3 % more than the catalogue says, as a tube lens a little off would;
- 5 µm of backlash in the stage.

All through the API, as a client would, it:

1. homes the stage, takes it over the simulated H&E tissue, and starts the camera;
2. checks the objective (the `calibration` Thing's `check_objective` job);
3. calibrates the 10×'s geometry: the pixel size and the camera-to-stage matrix, from a 3 × 3 grid of stage moves registered against the middle's frame (`geometry`);
4. calibrates the stage: homes it again, records its soft limits, and measures x's and y's backlash from both sides (`stage`).

It then prints the truth against what calibration recovered.

## How to try it

```sh
cargo run --release -p ex-calibrate
```

It serves at http://127.0.0.1:8235/, whose page links to the calibration sets, the ones in use, and the latest objective check. The calibration takes about 100 s at real time, the homing most of it; `--rate 10` runs the simulator ten times as fast. On the owner's PC it printed:

```text
the truth injected into the simulator, and what calibration recovered:
                      truth    recovered
pixel size       0.23301 µm   0.23301 µm
rotation            30.000°      30.000°
mirrored                yes          yes
aspect                    1      1.00000
skew                      0      0.00000
backlash x          5.00 µm     5.002 µm
backlash y          5.00 µm     4.998 µm

the objective check, before calibrating: the 10× plausible (measured over nominal 0.971)
```

- **The check passes the 10×** before the geometry is calibrated: its pixel size is 3 % from the nominal, as injected, within the check's 20 %.
- **A wrong objective** isn't shown here, as the simulator's mount always puts the objective fitted in the light path. `oscope-workflows`' `procedures` test fits the 20× with the 10× left in place, and the check names the 10×.
- **The sets are kept** in `.oscope/ex-calibrate/data/calibration/`, one JSON file each. A second run adds new versions beside them.

## `--smoke`

```sh
cargo run -p ex-calibrate -- --smoke
```

It runs the same on the quarter sensor at ten times real time, with a temporary data root, and holds each recovered value to P32's tolerances: the pixel size within 0.2 %, the rotation within 0.1°, the mirror exactly, the aspect within 0.2 % of 1, the skew within 0.002, and the backlash within 0.2 µm. It ends with `smoke: ok`, or says what failed and exits with code 1.
