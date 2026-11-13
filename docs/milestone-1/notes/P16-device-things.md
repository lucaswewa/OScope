# P16: Device Things: camera, stage, illumination, objective, simulation

- Status: Done
- Branch / PR: `m1/p16-device-things` / [#29](https://github.com/lucaswewa/OScope/pull/29)
- Size: L · estimated ~2,500 before it started (the five Things ~1,000; the driver registry, the simulator's lights and acquisition ~400; tests ~700; `ex-http-walkthrough` ~400); one pull request, at the owner's standing choice for P12–P23, rather than the spec's possible split into P16 and P16b · measured **2,720** hand-written lines added (code 1,746; tests 697; examples 277), and 26 removed. Docs, `Cargo.lock` and the generated snapshots aren't counted. **That is 1,820 over L's 900, and about 220 over the estimate.**

## Summary

The instrument is on the control plane ([spec](../phases.md#p16), plan §5.4, §5.9).

- **Five Things,** with typed properties, units, semantic types, saved settings, actions and events:
  - `camera`: settings, modes, streaming;
  - `stage`: position, moves, jogs, homing, stopping;
  - `illumination`: the lights;
  - `objective`: the catalogue and the objective fitted;
  - `simulation`: samples listed, loaded, unloaded and imported, and the confocal switch.
- **Drivers from the configuration:** `[devices]` names each one, `sim`, `fake` or `manual`; M3 adds the real ones. Each device's commands take turns, as ADR-0012 asks of its device actors.
- **Driver errors answer with ADR-0012's HTTP statuses:** 400, 409, 503, 504 or 502. `teta-wot` answers every failed property with 500, so OScope works around it, in ADR-0034.
- **Streaming** sends frames to acquisition, in `oscope-acquire`. It completes each with:
  - the stage's position at the exposure's midpoint;
  - the objective fitted;
  - the lights.

  Then it publishes the frame on the frame bus.
- **On the simulator,** the lights and the objective mount set what the camera sees.
- **`ex-http-walkthrough`:** the Things with `curl`, and a control page with the stage's position and buttons.

## What was built

| Where | What |
|---|---|
| `oscope_api::devices` | **`Devices`** (the drivers, a gate for each, acquisition, the simulator) from **`DevicesConfig`**, the configuration's `[devices]`. **`CameraThing`, `StageThing`, `IlluminationThing`, `ObjectiveThing`, `SimulationThing`.** **`status`** (ADR-0012's table) and the **`statuses`** layer (ADR-0034) |
| `oscope_api` | **`ApiBuilder::devices`**, which serves them; **`ApiBuilder::settings`**, where Things save their settings |
| `oscope-server` | Builds the devices from `[devices]`, and saves settings in `{data root}/settings`. A bad `[devices]` exits with 2 |
| `oscope_acquire::Acquisition` | A camera's sink: each frame's position, objective and lights, then the bus |
| `oscope_sim::devices` | **`SimLights`** (transmitted, reflected, and four excitation lines; a confocal switch), **`SimMount`**, **`Simulator`** from **`SimulatorConfig`**, the configuration's `[devices.simulator]` |
| `oscope_sim::world` | Blurs beyond 64 pixels render at a coarser pixel: see below |
| `oscope_sim::catalogue` | `SlideLoader::add`, for imports while the server runs |
| `examples/ex-http-walkthrough` | The `curl` walkthrough, and the control page |
| `docs/adr/0034` | Driver errors' statuses, and two names, over `teta-wot` |
| Docs | P15 marked *Done*; P16's estimate and ADR-0034 in the change log |

## How to try it

```sh
cargo run -p ex-http-walkthrough
cargo test -p oscope-api -p oscope-acquire
```

The example's [README](../../../examples/ex-http-walkthrough/README.md) walks through every Thing with `curl`.

## Design notes and deviations from the plan

- **Device actors are a gate for each device,** not a thread. ADR-0012's traits are async, `Send` and `Sync`. A driver that needs a thread of its own keeps one behind its trait:
  - the simulated camera already does;
  - M3's Tucsen driver will, for its SDK.

  P16's actors are what ADR-0012 asks of them: each device's commands wait their turn, in order, on a fair async lock.
  - **`halt`** doesn't wait its turn, so that it can stop a move in progress.
  - **Cancelling a move's invocation** stops the stage too.
- **The Things reach the drivers through a service** of the server, `Devices`, linked when each Thing starts. `teta-wot` builds a Thing from a configuration that must be deserialisable, and drivers aren't.
- **The drivers:**
  - `sim`, `fake` or `manual` for each device. M3 adds `tucsen` and `grbl`.
  - The simulated camera needs the simulated stage: it renders where that stage truly is.
  - All four default to the simulator, so `oscope-server` with no configuration serves a simulated microscope.
- **Driver errors (ADR-0034):**
  - **A property's error** carries its status in a mark, and a layer answers with it, the kind as the problem's title.
  - **An action's request** is answered with 201 when it starts, so its error says its kind and status in the invocation: `busy (409): …`.
  - **`teta-wot` logs property failures at ERROR,** so a refused value is logged as one.
- **Two names differ from the plan's,** because `teta-wot` names an action after its method:
  - the stage's stop is `halt`, since every Thing has a `stop` of its own;
  - the camera's software trigger is `trigger_exposure`, beside its `trigger` property.
- **Saved settings:**
  - the camera's settings, as last applied;
  - the stage's default speed;
  - the lights;
  - the objective fitted.

  Each is saved after every change, in `{data root}/settings/{thing}/`, and set again when the server starts if the driver takes it. `teta-wot` creates each Thing's folder when the server is built, so the server sets the folder only when it serves: printing the documents creates nothing.
- **Acquisition:**
  - It waits up to 50 ms, on the camera's thread, for the stage to report after a frame's exposure midpoint. Then it interpolates, never guessing ahead.
  - A frame with no report in time has no position, and is counted.
  - The lights and the objective are those the Things last set.
- **The simulator's lights:** the camera sees the first channel that is on, in order: transmitted, reflected, then the excitation lines, each through the confocal if it is on. The simulated camera has one view at a time, and the instrument has one light path.
- **Thing Descriptions:** `teta-wot` titles each affordance with its doc comment's first line, so each first line is a short sentence. The Things are titled Camera, Stage, Illumination, Objective and Simulation.
- **What building it found:**
  - **The renderer could ask for gigabytes.** The simulated stage starts where it was switched on, mid-travel at z = 10 mm, with the slide in focus at 1 mm. Through the 20× that is a blur of 17,000 pixels, and the margin of 3σ asked for 8.3 GB: the streaming test crashed.
    - Now a layer blurred beyond 64 pixels renders at a coarser pixel, over the blur's own reach. It blurs there, and is interpolated back. The margin around a field stops at 3 × 64 pixels.
    - A field 9 mm out of focus now renders in a moment, even in each channel.
    - Blurs under 64 pixels are unchanged; every earlier test passes.
  - **Cut titles,** above.
  - **Two wrong expectations in my tests:**
    - a good property write is answered with 201, not 204;
    - a move ends within the stage's 2 µm of backlash, not exactly.
  - **The far-focus test,** at first, compared the three colour channels together.
  - **The Thing Descriptions' IDs differ between computers,** so they aren't in the snapshots.
- **Not done:**
  - **The simulator's imperfections** (backlash, lost steps) aren't settable through the `simulation` Thing yet; they are set where the stage is built.
  - **Uploads of images to import** come with the media plane's upload route; `import` takes a path on the server's disk.

## Tests

**All passing:** 23 tests in `oscope-api`, 8 in `oscope-acquire`, 96 in `oscope-sim` and 12 in `oscope-server`. The new ones:

| File | Tests |
|---|---|
| `oscope-api/tests/devices.rs` (8) | **Driver errors answer with their statuses:** the fake camera fails a write with each kind, answered 409, 400, 503, 504 and 502, with the kind as the problem's title. An action's error ends its invocation with `alarm (409)`. **The simulation lists, loads, unloads and imports samples,** a missing file refused. **The stage homes, moves and says so,** with its `moved` events. **Streaming publishes frames with their position, objective and lights.** **Settings are kept across a restart:** the exposure, the objective and a light. **The Things answer on a test port,** 8221, over plain HTTP/1.1. **The Things describe themselves:** five Thing Descriptions as `insta` snapshots, and the OpenAPI document's routes for them. **A move stops when halted or cancelled:** halted at once, not after its turn; cancelled, the stage stops short |
| `oscope-acquire/tests/acquisition.rs` (2) | A frame gets the position at its exposure's midpoint, waiting for a later report; and its objective and lights. Without a report in time it has no position, after the bounded wait, and none at all is waited for without a track |
| `oscope-sim/tests/devices.rs` (1) | The lights and the mount set what the camera sees: transmitted, then reflected, then every light off; the 10× and the 50× |
| `oscope-sim/tests/tiles.rs` (+1) | A field 9 mm out of focus renders at a bounded cost, even in each channel |

**`oscope-server`'s tests pass unchanged:** printing the documents still creates nothing, now with the devices in them.

**Breaking the code on purpose:** 13 changes, each made alone.

- 12 were caught the first time.
- **One was missed:** a cancelled move that didn't stop the stage. The move simply finished within the test's 10 s, so the stage did stop. Now the stage must stop within 2 s, far short of its target, and it is caught.

| Change | Caught by |
|---|---|
| Statuses left as 500 | `driver_errors_answer_with_their_statuses`, `the_things_answer_on_a_test_port` |
| Busy answered 400 | `driver_errors_answer_with_their_statuses` |
| An action's error without its status | the errors and the halting tests |
| No position | `a_frame_gets_the_position_at_its_exposure_s_midpoint` |
| No wait for a report | both acquisition tests |
| No objective | `a_frame_gets_the_position_at_its_exposure_s_midpoint` |
| Saved camera settings not applied | `settings_are_kept_across_a_restart` |
| The objective not saved | `settings_are_kept_across_a_restart` |
| The lights not saved | `settings_are_kept_across_a_restart` |
| The simulator's lights not shown | `the_lights_and_the_mount_set_what_the_camera_sees` |
| The mount not setting the optics | `the_lights_and_the_mount_set_what_the_camera_sees` |
| A cancelled move not stopping the stage | `a_move_stops_when_halted_or_cancelled` (strengthened) |
| `halt` waiting its turn | `a_move_stops_when_halted_or_cancelled` |

**The workspace,** with every step CI runs:

- `cargo fmt`;
- `clippy -D warnings`;
- `cargo test`;
- `cargo doc -D warnings`;
- `cargo deny`;
- the server's `--version`;
- every example's `--smoke`, `ex-http-walkthrough`'s among them.

Results:

- **With `scripts/ci-local.sh` on Windows:** all 23 steps passed; 104 test suites, 241 tests.
- **With `scripts/ci-clone.sh` under WSL (Ubuntu 24.04):** the same, on a fresh LF clone of the commit. The first replay failed there: the snapshots held the Things' IDs, which `teta-wot` makes differently on each computer. The test now checks an ID's form, and snapshots the rest.

P16 touches no CUDA path.

## Follow-ups and known gaps

- **Upstream `teta-wot`:**
  - statuses for property errors;
  - affordance names of their own, so `halt` can become `stop`.

  Recorded in ADR-0034; not part of M1, at the owner's choice.
- **P17:** the preview, from the frame bus.
- **P20:** long actions return a job, so their lives don't depend on `teta-wot`'s invocation history.
- **The simulation Thing's imperfections,** and image uploads for `import`.

## Clean room

Written for this phase from plan §5.4 and §5.9, ADR-0012, `teta-wot`'s guide, and the SAREF ontology's device classes for the semantic types. Nothing was copied from `microscope` or any other codebase.
