# P12: Simulated camera: sensor, modes, timing, triggers

- Status: In review
- Branch / PR: `m1/p12-sim-camera` / [#25](https://github.com/lucaswewa/OScope/pull/25)
- Size: L · estimated ~2,000 before it started; one pull request, at the owner's standing choice for P12–P23 · measured **1,913** hand-written lines added (code 940; tests 717; examples 256), and 11 removed. Docs and `Cargo.lock` aren't counted. **That is 1,013 over L's 900, and 87 under the estimate.**

## Summary

The simulator has its camera: a MIChrome 20 stand-in that sees the simulated world as the stage truly moves, on the simulation clock ([spec](../phases.md#p12), ADR-0013 §8).

- **Its sensor** turns light into counts. It adds shot, dark and read noise, and has a 12-bit ADC with a black level of 125. Binned pixels are summed. Frames go out in 8 bits, or as 16-bit samples with the 12 bits aligned high, so black reads 2,000, as on the MIChrome 20.
- **Its modes:**
  - `full`, `bin2` and `bin3`, at the MIChrome 20's measured rates for 8 and 16 bits;
  - `design`, full resolution in 16 bits at 14.5 fps, the 580 MB/s P29 records.

  ROIs crop without raising the rate.
- **Its timing:** frames free-run at the mode's interval, or start on a software trigger or on the stage's trigger pulses.
  - The rolling shutter exposes each row a line time after the one above, so a moving stage shears the frame, and blurs it over the exposure.
  - Frames that end while an earlier one is still rendering are dropped and counted; the stage moves on regardless.
- **Its mounting** on the stage (a rotation, a mirror and an offset) is the ground truth P32's calibration must recover.
- **The stage now remembers its recent motion,** so the camera can render past moments exactly.
- **`ex-sim-camera`** streams in every mode during a jog, and prints each mode's rate, drops, jitter, and the recorded position against the truth.

## What was built

| Where | What |
|---|---|
| `oscope_sim::sensor` | **`SensorModel`** (electrons a millisecond, full well, read noise, dark current, black level, noise on or off), with IMX183-class defaults: `counts_per_electron(gain)`, `count(signal, exposure, pixels, gain, random)`. **`Random`:** SplitMix64's sequence, Box–Muller normals, and Poisson draws. **`ADC_MAX`** |
| `oscope_sim::camera` | **`Mounting`** (rotation, mirror, offset; `to_stage`). **`SimCameraConfig`** (sensor size, pixel, `SensorModel`, mounting, readout times for each binning, seed): `default` (the MIChrome 20), `small`, `capabilities`. **`SimCamera`:** the `Camera` trait, and `set_optics`, `set_light`, `world`. **`BAYER8`, `BAYER16`** |
| `oscope_sim::clock` | **`SimClock::block_until(at, stop)`** and **`nudge`:** waiting on the simulated clock from a device's own thread |
| `oscope_sim::stage` | **A history of the stage's last 256 changes of motion,** so `true_position` is exact for recent past moments |
| `examples/ex-sim-camera` | Every mode and format during a jog: rate, drops, jitter, position error; `--full`, `--smoke` |
| Docs | P11 marked *Done*. The change log records the owner's standing decisions for P12–P23, and P12's estimate |

**The camera's capabilities:**

| | |
|---|---|
| Modes | `full` 14.49 / 7.24 fps (8 / 16 bits); `bin2` 53.14 / 26.57; `bin3` 67.26 / 33.87; `design` 14.5 in 16 bits |
| Line time | 60 ms of readout at full resolution and 15 ms binned 3 × 3, from P02b; 18 ms binned 2 × 2, an estimate; each over the mode's rows |
| Exposure, gain | 0.0184–19,340 ms; 0–240 |
| ROIs | steps of 12, at least 48 pixels; they don't raise the rate |
| Triggers | free-running, software, external (the stage's pulses); no output |
| Sensor timestamps, temperature | none, as the MIChrome 20 |

## How to try it

```sh
cargo run --release -p ex-sim-camera
cargo test -p oscope-sim
```

The example's [README](../../../examples/ex-sim-camera/README.md) shows what it prints.

## Performance

**Rendering on the CPU, in release:** a 1368 × 912 frame takes about 0.2 s, so in real time most modes drop frames. Every frame is counted, though: delivered and dropped come to each mode's rate, from `ex-sim-camera`:

| Mode | Rated | Delivered in 2 s | Dropped |
|---|---|---|---|
| full, 8 bits | 14.49 fps | 10 | 18 |
| full, 16 bits | 7.24 fps | 9 | 4 |
| bin3, 16 bits | 33.87 fps | 67 | 0 |

A full 5472 × 3648 frame takes a few seconds. **§5.20's budget for the simulated camera** (≥ 14.5 fps with CUDA, ≥ 5 fps on the CPU, at full resolution) is P15's, with its cached levels of detail.

## Hardware

None was used. The modes, rates, readout times, black level and 16-bit output follow the MIChrome 20 as P02b measured it (`docs/hardware.md`).

## Design notes and deviations from the plan

- **Frames leave their position empty; acquisition fills it.** The spec puts "the position at the exposure midpoint, from `PositionTrack`, in `FrameMeta`" in the camera, but ADR-0012 gives that job to acquisition (P16), which knows the stage, the objective and the lights. The camera follows the ADR. `ex-sim-camera` does acquisition's step itself, and shows the recorded position matching the truth.
- **ROIs move in steps of 12,** where the real camera's x steps by 2 and y by 1. Twelve keeps the mosaic RGGB at every binning (2 × 1, 2 × 2, 2 × 3), so a frame's format is always its settings' format.
- **Binning sums the electrons and digitises the sum on one scale.** So `bin2` is four times brighter than `full` at the same exposure, and saturates in light four times dimmer. The MIChrome 20 offers summed binning (`TUIDC_BINNING_SUM`, P02b), but how its binned modes scale is unmeasured; H0 can check.
- **Gain is a model:** 20 units to 10^0.1, so 240 is 24 dB. Tucsen doesn't say what its 0–240 means.
- **The sensor's numbers are the IMX183's class,** not measured: a 15,000-electron well, 2.5 electrons of read noise, 0.5 a second of dark current, and 800 electrons a millisecond under full light. A frame through the slide's glass at 15 ms then fills about two thirds of the well.
- **A frame renders once its last row's exposure has ended,** at the stage's true positions over its rows' exposures. Its `host_time` is when it is delivered, after rendering. The USB transfer, about 138 ms for a 40 MB frame on the real camera, isn't simulated.
- **The stage remembers its motion.** If a command came during a frame's exposure, P11's `true_position` gave the wrong place for rows exposed before it. The stage now keeps its last 256 phases. A test stops a move and then asks where the stage was before the stop.
- **One region of the world is rendered for each frame,** covering the sensor's footprint along the whole motion. It is then resampled through the mounting, row by row, averaged along each row's motion: up to 64 samples a pixel for blur. Focus is taken at the frame's middle; a focus move during one exposure isn't blurred in z.
- **The model and the tests found five bugs, all fixed:**
  - **A race at start.** A stepped test could advance the clock before the camera's thread read it, so the first exposure began late and its frame never came. The stream's start is now taken in `start`.
  - **Read noise clipped at zero electrons.** On a dark pixel this halved the noise and biased it up. A real sensor's black level leaves room below black, so only the ADC's floor clips now. A binned read-noise test found it.
  - **A wrong expectation in the jog test.** The image moved 5.7% further than v · Δt, and the camera was right. Content dy rows lower was exposed dy line times later, so the rolling shutter adds dy · L to Δt. The test now solves for it, and agrees to 0.03 pixels.
  - **Tests that depended on the host's speed.** One compared positions with a start read from the host at 300×; the conformance test ran in real time while other tests loaded the machine. They now measure the stage's speed, and run the suite at a quarter of real time.

    **GitHub's runners still failed both,** on Ubuntu and on Windows, though they passed here. The renderer shares rayon's pool, so the 768 × 576 frames of the slow-rendering test held up the conformance test's frames, which then dropped 5 and 14. And the slow-rendering test ran for 11 and 25 s, so its 300× clock took the jog to x's limit at 100 mm, and the measured speed fell to 20 and 9 µm/s. Three changes fix it:

    - The two tests on a scaled clock now have their own file, `camera_scaled.rs`, away from the stepped tests, and take turns.
    - The slow-rendering test renders 384 × 288, and checks the stage's first 10 s of motion, which the clock always passes before the limit.
    - The conformance suite runs at a tenth of real time. It took 7.5 s here, and passed three times out of three with every core kept busy and rayon held to one thread. At a quarter, under that load, it failed twice out of three.
  - **A redundant clamp** at the full well, which the ADC's own top always reached first, was removed: no test could have caught its loss.

## Tests

**58 tests in `oscope-sim`,** 17 of them new, all passing:

| File | Tests |
|---|---|
| `tests/sensor.rs` (6) | The full well spans the ADC above black; 24 dB of gain. Noise against the model: shot, dark and read, with quantisation's 1/12. Poisson draws, small and large. Saturation. Binned pixels sum their electrons, and their read noise |
| `tests/camera.rs` (8) | On a stepped clock, with a small sensor: <ul><li>free-running frames at the mode's interval, with their metadata;</li><li>a frame's noise against the model;</li><li>saturation;</li><li>binned frames four and nine times brighter;</li><li>software and stage-pulse triggers starting exposures exactly;</li><li>a jog moving the image through a turned, mirrored mounting by v · Δt, with the rolling shutter's share;</li><li>blur over 0.8 of speed × exposure, 10% to 90%;</li><li>the rolling shutter's shear, row by row.</li></ul> |
| `tests/camera_scaled.rs` (2) | On a scaled clock, one at a time: at 300×, slow rendering drops and counts frames, each on the schedule, while the stage keeps its speed. The HAL's conformance suite, at a tenth of real time |
| `tests/common` | The camera tests' rig, shared |
| `tests/stage.rs` (+1) | The stage remembers where it was before a stop |

The camera tests ran five times in a row without a failure; the scaled ones three more times under full load.

**Breaking the code on purpose.** 12 changes, each made alone, all caught the first time:

| Change | Caught by |
|---|---|
| No shot noise | `the_noise_matches_the_model` |
| Binned read noise not summed | `binned_pixels_add_their_read_noise` |
| No rolling shutter | the jog and shear tests |
| No motion blur | `the_blur_is_the_speed_times_the_exposure` |
| The mirror ignored; the turn backwards | `a_jog_moves_the_image_through_the_mounting` |
| Binned pixels not summed | `binned_values_are_sums` |
| Software triggers starting when the thread sees them; pulses starting late | `triggers_start_exposures` |
| The 16-bit black level unscaled | `free_running_frames_come_at_the_mode_s_interval` |
| Skipped frames not counted | `slow_rendering_drops_frames_but_not_the_motion` |
| The stage forgetting where it was | `the_stage_remembers_where_it_was` |

**The workspace,** with every step CI runs: `cargo fmt`, `clippy -D warnings`, `cargo test` (188 tests), `cargo doc -D warnings`, `cargo deny`, the server's `--version`, and every example's `--smoke`.

- **With `scripts/ci-local.sh` on Windows:** all 19 steps passed; 86 test suites, 188 tests.
- **With `scripts/ci-clone.sh` under WSL (Ubuntu 24.04):** the same, on a fresh LF clone of the commit, with no file holding a CR.

P12 touches no CUDA path.

## Follow-ups and known gaps

- **P13–P14:** more samples, which the camera renders through the same path. P14's slide loader uses `SimCamera::world`.
- **P15:** full resolution at the frame rate, with cached levels of detail and CUDA. Today a 20 MP frame takes seconds.
- **P16:**
  - filling each frame's position, objective and lights;
  - wiring the lights to `set_light`, and the objective mount to `set_optics`.
- **H0:** the MIChrome 20's binned modes' scaling, its gain's units, its noise, and its line times, to replace this model's guesses.

## Clean room

Written for this phase from:

- plan §5.10 and ADR-0013;
- `docs/hardware.md`'s measurements of the MIChrome 20 (P02b);
- the IMX183's published class figures, in the sense of typical full well and read noise, not a datasheet's numbers;
- the Box–Muller transform (Box and Muller, *A note on the generation of random normal deviates*, 1958);
- Knuth's Poisson method (*The Art of Computer Programming*, vol. 2);
- SplitMix64 (Steele, Lea and Flood, 2014).

Nothing was copied from `microscope` or any other codebase, nor from Tucsen's SDK.
