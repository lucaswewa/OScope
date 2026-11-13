# P09b: HAL fakes and conformance suite

- Status: In review
- Branch / PR: `m1/p09b-hal-fakes` / [#21](https://github.com/lucaswewa/OScope/pull/21)
- Size: L · estimated ~850 when P09 was split, then ~2,000 just before starting, from P09's measured sizes; the owner chose one pull request · measured **1,956** hand-written lines added (code 1,450; tests 290; examples 215; glue 1), and none removed. Docs and `Cargo.lock` aren't counted. **That is 1,056 over L's 900, and 44 under the estimate.**

## Summary

Every HAL trait now has a scripted fake, and every driver has a conformance suite to pass ([spec](../phases.md#p09b), [ADR-0012](../../adr/0012-the-hardware-abstraction-layer.md)).

- **The fakes,** in `oscope_hal::fake`:
  - **`FakeCamera`** streams frames of moving stripes from the pool it is given, on a thread of its own. It delivers at its mode's rate, or one frame for each software trigger, and counts the frames it loses to a full pool.
  - **`FakeStage`** reaches each target at once, and reports it on its track. It can be put in alarm.
  - **`FakeIllumination`** and **`FakeObjectiveMount`** behave as the manual implementations do.

  Each has a **`Script`**, which records the operations called, and makes chosen ones fail.
- **The conformance suite,** in `oscope_hal::conformance`, has one async function for each trait. Each exercises a device, puts it back as it found it, and returns the first check it failed. The simulator runs it from P11, the real drivers in M3, and extension drivers in P27.
- **`ex-hal-fakes`** prints the fakes' capability reports and drives them. Then it runs the suite on each fake, and on a camera scripted to fail.

## What was built

| Where | What |
|---|---|
| `oscope_hal::fake` | **`Script`:** `fail_next(operation, error)` and `calls()`. **`FakeCamera`:** `new`, `default_capabilities` (640 × 480 RGGB, modes `full` and `bin2`, 8- and 16-bit, ROIs in steps of 2, a software trigger, a temperature), `with_capabilities`, `script`. **`FakeStage`:** `new`, `default_capabilities` (100 × 75 × 20 mm, homing, a trigger output), `with_capabilities`, `script`, `raise_alarm`. **`FakeIllumination`:** `new`, `two_lights` (a dimmable transmitted light, a reflected ring that can't dim), `script`. **`FakeObjectiveMount`:** `new`, `script`; motorised |
| `oscope_hal::conformance` | **`camera(&dyn Camera)`**, **`stage(&dyn Stage, tolerance)`**, **`illumination(&dyn Illumination)`** and **`objective_mount(&dyn ObjectiveMount)`**, each returning `Result<(), Failure>`. **`Failure`:** the check, and what went wrong |
| `oscope_hal::camera` | **`CameraCapabilities::frame_shape(settings)`:** the sensor's pixels that frames cover, and their size after binning. The suite and the fake use it, and acquisition will size its pool from it |
| `examples/ex-hal-fakes` | The fakes' capability reports, frames, moves and track, lights and objective; the suite on each, and on a camera scripted to fail; `--smoke` |
| Docs | P09 marked *Done*. The change log records P09b's estimate and the owner's choice |

**What the suites check:**

- **The camera:**
  - its capabilities make sense: unique modes that fit the sensor, positive rates, a line time exactly when the shutter rolls, coherent ranges;
  - every mode and format configures, and reads back as its capabilities' `check` gives;
  - it refuses, with the right error kind, an unknown mode, an exposure or gain over the most, triggers it lacks, and ROIs it can't take. A refusal changes nothing;
  - streamed frames match their settings, with rising numbers and times, and no drops;
  - a second start, or a new frame shape while streaming, is refused as busy;
  - an ROI is applied as the camera's rules round it, and its frames cover it;
  - frames lost while the pool is full are counted in the next frame;
  - a software trigger gives one frame each, and none comes without one;
  - a temperature, if it has one;
  - no sink call after `stop` has returned.
- **The stage:**
  - its capabilities make sense; it clears an alarm, homes if it can, or refuses to;
  - it reaches targets within the tolerance at the default and the fastest speed, then is idle;
  - its position and its track report where it is, and a relative move goes from where it is;
  - it refuses, without moving, a target beyond the soft limits, a speed over the fastest, and a speed of 0;
  - it jogs and stops, idle and within its limits;
  - it goes back to where it started.
- **The lights:** unique channels; a state for each, in order; each set as its capabilities' `check` gives, and kept; an intensity over 1 and an unknown light refused, changing nothing. They are put back.
- **The objective mount:** a valid catalogue; each objective fitted and read back; an unknown one refused, changing nothing. The first objective is put back.

## How to try it

```sh
cargo run -p ex-hal-fakes
cargo test -p oscope-hal
```

The example's [README](../../../examples/ex-hal-fakes/README.md) shows what it prints.

## Performance

The suites take about 0.3 s against the fakes in a debug build. Against a real camera they take longer, by a few frame intervals for each stream: about a dozen streams of a few frames each.

## Hardware

None was used.

## Design notes and deviations from the plan

- **One pull request, though it is over twice L.** P09b was first estimated at ~850, when P09 was split. P09 then came in at almost twice its own estimate, so P09b was estimated again from P09's sizes before starting, at ~2,000. The owner chose one pull request. It measured 1,956.
- **The suite is library code,** in `oscope_hal::conformance`, not a test helper, so that `oscope-sim`, M3's drivers and extension adapters can each run it in their own tests.
- **It needs Tokio's timer,** to wait for frames with a deadline, so `oscope-hal` now depends on `tokio` (`sync`, `time`). Tokio is already a workspace dependency, and the layering rules allow it at layer 2.
- **It waits for conditions, with a deadline:**
  - for each frame, its interval and exposure plus 10 s, so a driver that never delivers fails instead of hanging;
  - for the pool to run out, by watching the pool's own count.

  **The one fixed wait** is three frame periods after the third software trigger: showing that no frame comes needs one. Breaking the code on purpose showed it is needed (below).
- **Refusals are checked by error kind.** ADR-0012 has drivers check settings with their capabilities' checks, so the suite holds every driver to the same kinds. A driver that refuses for its own reasons as well isn't held back: only what the capabilities refuse is checked.
- **The suite reads frames promptly,** and expects no drops in the first ones from a pool of 8. A driver that drops frames even then fails.
- **`frame_shape`** joins P09's `CameraCapabilities`. The fake camera needed it, so does the suite, and acquisition will (P16).
- **The fake camera's `stop` joins its thread,** which blocks the caller for at most one frame interval. That is what makes "no sink call after `stop`" true.
- **The fake stage's moves are instant.** So the suite can't check `Error::Stopped`, which needs a move that takes time. P11's simulated stage, whose moves follow the clock, is the first that can; its tests check it.

## Tests

**30 tests in `oscope-hal`** (20 from P09), all passing:

| File | Tests |
|---|---|
| `tests/conformance.rs` | The suites pass on the fakes, on plainer fakes (a global shutter, no ROI, trigger or temperature; a stage without homing), and on the manual lights and mount, which are put back. **A scripted failure fails the suite at the check named,** for 15 operations across the four fakes. A tolerance that can't be met, and capabilities that make no sense, fail too |
| `tests/fakes.rs` | The camera's moving stripes, frame by frame. Its script's calls. No frame after `stop`: the channel closes. A busy start, a busy change of mode, and an exposure that may change while streaming. One frame for each software trigger. Lost frames counted, and a slab too small refused. The stage's alarm until it is cleared or homed |

The streaming tests were run 13 times in a row without a failure.

**Breaking the code on purpose.** Each fake was made to misbehave as a faulty driver might, one fault at a time. The suite then failed at the check named:

| Fault | Caught at |
|---|---|
| Frame numbers that don't rise | frames that match their settings |
| A frame two pixels wider than its settings | frames that match their settings |
| Lost frames not counted | count the frames lost to a full pool |
| A stop that leaves the thread running | no frames after stopping |
| Frames without a software trigger | one frame for each software trigger |
| Settings applied without being checked | refuse an unknown mode |
| An ROI applied, but reported unrounded | apply an ROI as rounded |
| The frame changed while streaming | refuse a new frame while streaming |
| A second start accepted | refuse to start twice |
| A move not reported on the track | report where it is |
| The soft limits not checked | refuse a target beyond the soft limits |
| Still moving after a stop | be idle after a stop |
| The lights' states out of order | a state for each light, in order |
| An objective turned to, but not recorded | fit each objective |

**The first run missed one:** a camera that free-ran under a software trigger. It still gave a frame for each trigger, and the stop came before an extra one. The suite now waits three frame periods after the last trigger, and that fault is caught.

**The workspace,** with every step CI runs: `cargo fmt`, `clippy -D warnings`, `cargo test`, `cargo doc -D warnings`, `cargo deny`, the server's `--version`, and every example's `--smoke`.

- **On Windows:** see the pull request.
- **Under WSL (Ubuntu 24.04):** see the pull request.

P09b touches no CUDA path.

## Follow-ups and known gaps

- **P11:** the simulated stage runs the stage suite. Its timed moves let its own tests check `Error::Stopped`.
- **P12:** the simulated camera runs the camera suite.
- **P16:** acquisition sizes its pool with `frame_shape`, and uses the fakes in its tests.
- **P27:** extension adapters run the suites.
- **M3:** each real driver runs its suite, on the instrument, behind `OSCOPE_TUCSEN` and `OSCOPE_GRBL_PORT`.

## Clean room

Written for this phase from plan §5.9 and ADR-0012. The fakes and the suite need no outside method. Nothing was copied from `microscope` or any other codebase.
