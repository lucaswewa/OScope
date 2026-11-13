# P09: HAL traits, capabilities, objective catalogue, manual implementations

- Status: In review
- Branch / PR: `m1/p09-hal-traits` / [#20](https://github.com/lucaswewa/OScope/pull/20)
- Size: L · estimated ~1,900 for the whole of P09 before it started, so the owner split it, and ~1,150 for this half · measured **2,157** hand-written lines added (code 1,152; tests 767; examples 185; glue 53), and 3 removed. Docs and `Cargo.lock` aren't counted. **That is 1,257 over L's 900, and about 1,000 over the estimate** (*Design notes*).

## Summary

`oscope-hal` now holds the seams through which the simulator, the real drivers and extension drivers plug in ([spec](../phases.md#p09), [ADR-0012](../../adr/0012-the-hardware-abstraction-layer.md)).

- **Four traits:** `Camera`, `Stage`, `Illumination` and `ObjectiveMount`.
  - They are async, through a boxed future, `HalFuture`, so that the configuration can choose a driver at run time and hold it as `Arc<dyn Camera>`.
  - Each device reports its capabilities. The capability types carry the checks, so that every driver refuses the same things with the same errors.
- **The camera** takes all its settings at once, and returns them as applied: the MIChrome 20 rounds an ROI's x and width to even numbers (P02b). It streams into a sink, and acquisition completes each frame's metadata.
- **The stage's `PositionTrack`** says where the stage was at any moment between its reports, and never guesses beyond them.
- **The objective catalogue** is a TOML file, [`configs/objectives.toml`](../../../configs/objectives.toml), with Mitutoyo's M Plan Apo 5×–50× built in as the default.
- **`ManualIllumination` and `ManualObjectiveMount`** record what the user says they set or fitted.
- **`ex-objectives`** prints the catalogue on the MIChrome 20: each objective's pixel at the sample, field of view and resolution. Then it drives the manual implementations.

**P09b** adds the scripted fakes, the conformance suite and `ex-hal-fakes`.

## What was built

| Where | What |
|---|---|
| `oscope-hal` (`lib`) | **`HalFuture<'a, T>`:** `Pin<Box<dyn Future<Output = Result<T>> + Send + 'a>>`. The crate's map |
| `oscope-hal::error` | **`Error`:** `Unsupported`, `OutOfRange` (value and limits), `Unknown` (a mode, light or objective), `Busy`, `Stopped`, `Alarm`, `Disconnected`, `Timeout`, `Device`, `InvalidCatalogue`, `CatalogueFile` |
| `oscope-hal::camera` | **`CameraCapabilities`:** model, sensor and pixel size, `Shutter`, `CameraMode`s, exposure and gain limits, `RoiRules` (steps, least size, `raises_rate`), `TriggerCapabilities`, sensor timestamps, temperature. **`check`** and **`round_roi`**. **`CameraMode`:** size, binning, a `ModeRate` (fastest fps) for each pixel format, line time. **`CameraSettings`:** mode, format, exposure, gain, ROI, `Trigger` (free-running, software, external on an `Edge`). **`FrameSink`.** **`Camera`:** `capabilities`, `settings`, `configure`, `start(pool, sink)`, `stop`, `streaming`; `trigger` and `temperature` unsupported unless a driver offers them |
| `oscope-hal::stage` | **`UmPerS`.** **`StageStatus`:** idle, moving, homing, hold, alarm with a reason. **`PositionSample`**, **`Axis`**, **`Travel`**. **`StageCapabilities`:** model, soft limits, fastest and default speeds, homing, encoders, trigger output, report interval; **`check_target`** and **`check_speed`**. **`Stage`:** `status`, `position`, `track`, `move_to`, `move_by` (by default from `position` and `move_to`), `jog`, `stop`, `clear_alarm`; `home` unsupported unless offered |
| `oscope-hal::track` | **`PositionTrack`:** a ring of samples in time order, shared between threads. `push` ignores a late sample and replaces one at the same time. `at(t)` interpolates linearly, and is `None` outside the samples |
| `oscope-hal::illumination` | **`LightKind`**, **`LightChannel`** (name, kind, dimmable), **`IlluminationCapabilities`** with **`check`**, the **`Illumination`** trait, and **`ManualIllumination`**, with `brightfield()` |
| `oscope-hal::objective` | **`Objective`** (name, model, magnification, NA, working distance, design tube lens) and **`ObjectiveCatalogue`** (tube lens, objectives): `mitutoyo()`, `from_toml`, `validate`, `get`, `magnification`, `pixel_size`. **`ObjectiveMountCapabilities`**, the **`ObjectiveMount`** trait, and **`ManualObjectiveMount`** |
| `configs/objectives.toml` | The default catalogue, in micrometres |
| `examples/ex-objectives` | The catalogue on the MIChrome 20, and the manual implementations with their capability reports; `--catalogue FILE` and `--smoke` |
| Docs | [ADR-0012](../../adr/0012-the-hardware-abstraction-layer.md). The split: `phases.md` (P09, a new P09b, P11 now after P09b), plan §7 and its change log, and the phase count of 59. P08c marked *Done* |

## How to try it

```sh
cargo run -p ex-objectives
cargo run -p ex-objectives -- --catalogue my-objectives.toml
cargo test -p oscope-hal
```

The example's [README](../../../examples/ex-objectives/README.md) shows what it prints. At 50×, the pixel at the sample (0.048 µm) is far finer than the resolution (0.61 µm), as plan §5.5 says.

## Performance

Nothing in P09 is on a hot path. `PositionTrack::at` takes a mutex and searches the ring by bisection: a few hundred nanoseconds, once for each frame. The traits box one future for each device command, at the rate of commands, not of frames.

## Hardware

None was used. The MIChrome 20's capabilities in the tests are P02b's measurements (`docs/hardware.md`): its three modes with their 8- and 16-bit rates, and its ROI rounding.

## Design notes and deviations from the plan

- **The split.** Before it started, P09 was estimated at ~1,900 lines, about three times M. The owner chose to split it, so that the trait design and ADR-0012 are reviewed before the fakes and the conformance suite build on them. This half was estimated at ~1,150, so sized L.
- **The size,** 2,157, came in at almost twice that estimate:
  - **Tests,** at 767 against ~240, did most of it:
    - the property tests of the track;
    - a check for each catalogue rule;
    - a test of each camera and stage check, with a MIChrome 20 fixture;
    - two small trait implementations in `tests/capabilities.rs`, for the traits' default operations.
  - **The code,** at 1,152 against ~810, is about half documentation, as P02's was: every field of the capability types says what it is and where the value comes from.

  P05's notes had already found tests underestimated. **P09b's ~850 is likely low by the same factor,** so it will be estimated again, from this phase's measured sizes, before it starts.
- **Boxed futures, not `async fn`.** On Rust 1.98.1 a trait with `async fn` can't be a trait object: it fails with error E0038, checked before writing the traits. ADR-0012 gives the alternatives.
- **One settings value.** `Camera::configure` takes every setting at once and returns them as applied, rather than a setter for each. The MIChrome 20 takes its settings before streaming starts, and rounds what it is given.
- **The sink takes the slab and the camera's metadata, unsealed.** Acquisition fills in the position, the objective and the lights, then seals the frame. A sealed `Frame` can't be changed (ADR-0008), and the camera can't know them. This adds a step to P16: wait, a bounded time, for the stage report after each frame's exposure midpoint.
- **`UmPerS`** is a new unit type, for speeds: ADR-0007 has lengths (`Um`) and durations (`Ms`), and a speed is their ratio.
- **`Stage::jog`** takes a direction (`towards_max`) and an optional speed, rather than a signed speed, so that its speed checks are the same as a move's.
- **Programming the stage's trigger output** (pulses at positions) isn't in the trait yet. P11 adds it, as ADR-0012 says, as an optional operation behind `StageCapabilities::trigger_output`.
- **The catalogue's lengths are micrometres** (ADR-0007), so its file says 200000 for 200 mm, as its header explains.
- **The numerical aperture is capped at 1.6,** an oil immersion objective's, though the instrument's are dry, so that the catalogue can describe any objective.

## Tests

20 tests, all passing:

| File | Tests |
|---|---|
| `tests/track.rs` | **Property tests** over seeded random tracks: <ul><li>a sample's own time gives its own position, exactly;</li><li>between two samples, the position is their mean weighted by time, against an oracle that scans for the neighbours;</li><li>a steady move in three axes is followed to within 10⁻⁶ µm.</li></ul>Also: nothing outside the samples; the ring keeps the latest; a late sample ignored and a repeated time replaced; three readers and a writer on separate threads, each read on the stage's line |
| `tests/catalogue.rs` | Mitutoyo's values, and the plan's pixel sizes of 0.48, 0.24, 0.12 and 0.048 µm. The shipped file equals the built-in catalogue. The magnification follows the tube lens. Each rule refuses the catalogue that breaks it, with its reason. Files that aren't catalogues are refused. An unknown objective is named in its error |
| `tests/manual.rs` | The manual lights and mount, through their traits: their JSON capability reports; set and fitted values kept; what they refuse leaves them as they were; a light that can't dim is at full intensity |
| `tests/capabilities.rs` | The MIChrome 20's settings checked as the camera would: the ROI (333, 333, 1001 × 1001) becomes (332, 333, 1000 × 1001), as measured. Each refusal, with its message. The stage's soft limits and speeds. The JSON of statuses, triggers and capabilities. The default operations: unsupported trigger, temperature and homing, and `move_by` from the current position |

**Breaking the code on purpose.** 17 changes, each made alone, and each caught by a test:

| Change | Caught by |
|---|---|
| Interpolation weighted the wrong way | `between_two_samples_it_is_their_weighted_mean`, `a_steady_move_is_followed_exactly` |
| A late sample accepted | `a_late_sample_is_ignored_and_a_repeated_time_replaced` |
| The ring keeps one sample too many | `the_ring_keeps_the_latest_samples` |
| A time before the oldest sample answered | `it_never_guesses_outside_its_samples`, and two more |
| A pixel format the mode doesn't offer accepted; the exposure not checked; an ROI outside the sensor accepted; an external trigger accepted without an input; the ROI's y rounded with the x step | `the_michrome_20_s_settings_are_checked_as_the_camera_would` |
| z's soft limits not checked; a speed of 0 accepted | `a_stage_checks_targets_against_its_soft_limits_and_speeds` |
| A relative move from the origin | `the_traits_default_operations` |
| A light that can't dim keeps its intensity | `a_light_that_can_t_dim_is_at_full_intensity` |
| The intensity not checked | `manual_lights_record_what_the_user_set` |
| Two objectives with one name accepted; any numerical aperture over 0 accepted | `each_rule_rejects_a_catalogue_that_breaks_it` |
| The tube lens ignored | `magnification_follows_the_tube_lens` |

**The workspace,** with every step CI runs: `cargo fmt`, `clippy -D warnings`, `cargo test`, `cargo doc -D warnings`, `cargo deny`, the server's `--version`, and every example's `--smoke`.

- **On Windows:** all 8 steps passed; 70 test suites, 120 tests.
- **Under WSL (Ubuntu 24.04.5):** the same, on a fresh LF clone of the commit, with no file holding a CR.

P09 touches no CUDA path, so the CUDA tests weren't needed.

## Follow-ups and known gaps

- **P09b:**
  - the scripted fakes, the conformance suite run against them and the manual implementations, and `ex-hal-fakes`;
  - re-estimated before it starts, from this phase's sizes.
- **P11:** programming the stage's trigger output, as an optional `Stage` operation.
- **P16:**
  - the device actors;
  - completing each frame's metadata from the track, the mount and the lights, with a bounded wait for the stage's next report;
  - ADR-0012's errors as HTTP statuses.
- **M3 (H0):**
  - Mitutoyo's values, confirmed on the instrument;
  - whether the MIChrome 20's SDK software trigger works;
  - its line times, measured.

## Clean room

Written for this phase from:

- plan §5.9, ADR-0007 and ADR-0008, and `docs/hardware.md`'s measurements (P02b);
- the objectives' values: Mitutoyo's published specifications for the M Plan Apo series, as plan §5.9 lists them;
- the resolution in `ex-objectives`: Rayleigh's criterion for a circular aperture, 0.61 λ / NA (M. Born and E. Wolf, *Principles of Optics*, Cambridge University Press);
- grblHAL's machine states, in ADR-0012: the grblHAL core wiki (`docs/hardware.md`, sources 5–7);
- linear interpolation, which needs no source.

Nothing was copied from `microscope` or any other codebase, and the owner's MIChrome 20 test tool wasn't used for code.
