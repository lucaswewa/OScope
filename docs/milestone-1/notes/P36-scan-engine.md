# P36: Scan engine, the `scan` Thing, workflow extensions

- Status: In review
- Branch / PR: `m1/p36-scan-engine` / [#50](https://github.com/lucaswewa/OScope/pull/50)
- Size: L · estimated ~3,600 before it started (the engine ~550; the scan job, its retries, cancel and end position ~200; continuous-scan fields reserved ~40; the `scan` Thing ~350; the `workflow` extension kind ~460; the server's wiring ~40; tests ~850; `ex-scan` ~500 and two sample workflows ~200; glue ~60; and about a tenth on top); one pull request, at the owner's standing choice, the spec's split candidate kept in it · measured **2,309** hand-written lines added (code 1,022: `oscope-workflows` 449, `oscope-api` 417, `oscope-ext` 70, `oscope-ext-sdk` 66, `oscope-store` 13, the Python SDK 7; tests 879; examples 381; glue 27), and 23 removed; the `scan` Thing's Thing Description snapshot, 2,219 lines, is generated. **That is 1,409 over L's 900, and 36 % under the estimate.**

## Summary

Any plan run as a hardware job, recording everything ([spec](../phases.md#p36); plan §5.14; D14; [ADR-0025](../../adr/0025-scan-planning-and-the-scan-engine.md)).

- **The engine** runs a plan stop and go. At each tile it moves there, focuses (from the plan's z, or by autofocus every n tiles), and takes each plane from the lowest, moving up. It records every frame with where the stage was once settled, into one recording, then ingests it as one acquisition with the plan.
- **Ending a scan:** a step that fails is tried twice more. Every scan ends where it started. A cancel, taken between frames, keeps the frames in scratch.
- **Progress:** the job's share of frames, with `tiles` and `frames` counters, which the `jobs` Thing sends as events; the `scan` Thing's `progress` lists the tiles done.
- **The `scan` Thing:** `patterns`, `plan` (a preview: the plan and its estimate, nothing moving) and `run` (a job).
- **Workflow extensions:** a `workflow` driver kind, in Rust (`ScanWorkflow`) and Python (`@driver(kind="workflow")`), that plans the tiles. The sample `example.spiral` plans a spiral, which the engine runs.
- **Continuous scanning's fields** are reserved: the plan's `motion`, and each frame record's `trigger`.
- **`ex-scan`** scans 1 × 1 mm of the tilted simulated H&E slide through the API, printing each tile as it arrives; its page draws them.
- **P35 is marked Done.**

## What was built

- **`oscope_workflows::scan`:**
  - **`scan_jobs`:** the `scan.run` job's factory, which refuses at submission what can't be: a continuous plan, no tiles, a tile without a z under a policy that isn't autofocus, settling or an approach out of range.
  - **`ScanRig`:** autofocus's rig, the projects and the scratch space. **`ScanOptions`:** settling, approach and autofocus's options.
  - **`ScanProgress` and `TileDone`,** kept in a `LastScan`.
  - **`Motion`** (`stop_and_go`; `continuous`, reserved) on `Plan`, and **`Pattern::Workflow`.**
  - `autofocus`'s frame helper is now crate-wide.
- **`oscope-store`:** `FrameRecord::trigger`, reserved and none; ingest moves a recording source's `plan` into the acquisition's `plan` (the field P28 reserved for P36).
- **`oscope-ext-sdk`:** `ScanWorkflow` (`schema`, `plan`), `ScanField`, `Extent`, `PlannedTile`, `workflow()`, and `Driver::Workflow`, called through `driver_call`. The ABI stays at version 3.
- **`oscope-ext`:** the `workflow` kind loaded from Rust and Python extensions, checked against `provides.workflows`, into `Workflows` (`names`, `schema`, `plan`), which `Extensions::workflows` gives.
- **The Python SDK:** the `workflow` kind.
- **`oscope-api`:**
  - **The `scan` Thing** (`scan.rs`).
  - **`Calibrations`:** the calibration store shared by the `calibration` and `scan` Things, opened once.
- **`examples/ex-scan`** (port 8240), and **`examples/ext-python-workflow`**, the sample spiral.

## How to try it

`cargo run --release -p ex-scan`, then open http://127.0.0.1:8240/. The [README](../../../examples/ex-scan/README.md) shows what it prints. On the owner's PC, at real time through 1024 × 768 of the 20 MP sensor, the 30 tiles took 43.6 s against the 36 s estimated, each frame within 1.41 µm of its tile.

The sample workflow: [`ext-python-workflow`](../../../examples/ext-python-workflow/README.md).

## Performance

No budget in §5.20 applies. A stop-and-go tile takes its move, 50 ms of settling and a frame. On the simulator at real time, `ex-scan`'s 30 tiles, three of them autofocusing, took 43.6 s.

## Design notes and deviations from the plan

### The engine

- **Moves.** A tile is reached in x and y at the z it has, then in z from 10 µm below, moving up (ADR-0024). A stack's later planes are above the last, so the stage moves straight up to each.
- **Focus.** A tile with a z takes it. Under `autofocus { every }`, every n-th tile autofocuses where it is, from the last z found. A refused autofocus keeps the last z, and is counted in `refused`.
- **A frame** is the first whose exposure starts after the move ended and the settling time passed. Its position is the stage's, read once settled, set as the frame's `position` before it's recorded.
- **One recording, one acquisition.** Frame k is tile ⌊k / P⌋'s plane k mod P. The acquisition keeps the plan, and its instrument the request that made it.

### Ending where it started, by the lag

- **The first run drifted.** The test that runs two scans in turn found the stage 2 µm below where it started.
- **Why.** The simulated stage reports its true position, which lags a z approached from below by half its backlash, as P34 found (ADR-0024, 4a). The engine read where the stage was, commanded that z to return, and stopped 1 µm short: twice, 2 µm.
- **The fix.** Each plane's lag, the z commanded less the z settled, is recorded, and the return commands the start plus their mean. Scans now end within 0.3 µm of their start, which the tests check.
- **A scan that takes no frame** has no lag to go by, and may end half the backlash short.

### Cancels, failures and retries

- **Retries.** A move, a frame that doesn't come in 30 s, or an autofocus that errors, is tried twice more, then fails the scan with the step, the count and the last reason. A cancel stops the retries.
- **A cancel** is taken before each frame and between retries. The stage returns to the start. The recording ends with `cancelled` and stays in scratch: the job's result names it, for the ingest job. A failed scan's does the same, with why.
- **Only a completed scan is ingested,** so a cancel doesn't wait on an ingest of what may be gigabytes.

### The `scan` Thing

- **`run`, not `start`.** `teta-wot` names an affordance after its method, and `start` is every Thing's own starting, as `stop` is (ADR-0034). The action takes autofocus's name for its job.
- **The field** is the camera's frame through the fitted objective: by its active geometry at the camera's binning (P32), or its nominal pixel times the binning. The `calibration` and `scan` Things share one calibration store, so a geometry just calibrated is the scan's.
- **A request** names its pattern, and what it needs: a region, a point, positions, a stack. Its z policy defaults to the stage's z now, and its overlap to a tenth.
- **The estimate** takes the stage's default speed, 50 ms of settling, the camera's frame interval or exposure, whichever is longer, and 10 s an autofocus.
- **Refusals** say `refused (400)`, as ADR-0034's names do; a device's error keeps its own status.
- **Regions in JSON** are an object of one key: `{"rect": {"origin", "size"}}`, `{"polygon": {"corners"}}`, `{"mask": {…}}`. P35 tagged them `{"kind": "rect", …}`, but `teta-wot`'s schemas lose a newtype variant's fields when the tag is inside, and the server then dropped the rectangle.
- **A scan's autofocus** measures on the CPU: only the `autofocus` Thing opens a GPU context (P34).

### Workflows

- **A workflow plans.** Given its parameters and the field, it gives the tiles in order, each with its z if it knows it, and its name. The engine runs them as positions, under the request's z policy.
- **Through the driver call** that drivers, importers and exporters use: the ABI stays at version 3, and the extension API's version at 1.0.0, as P27 and P30 left them.
- **Parameters** are the workflow's to check: OScope has no JSON Schema validator at run time. The sample refuses 60 rings, saying why.
- **The Rust kind** is tested through the ABI by an extension the test exports itself. The Python kind is tested end to end, by the sample installed into a server, under `OSCOPE_PYTHON=1`.

### Continuous scanning, reserved

The plan's `motion` is `stop_and_go` unless a plan says `continuous`, which the engine refuses, saying it comes in M3. Each frame record's `trigger`, where a stage's trigger output fired the frame, is always none until then. A frame's `position` is already the stage's at the exposure's midpoint, from its track, which a moving stage needs.

### The tests' turns, and two cores

{{TWOCORE}}

### The size

L's budget is 900; P36 measured 2,309, 36 % under its estimate of ~3,600.

- **One pull request,** under the owner's standing choice, the spec's split candidate kept in it: the engine, the Thing that runs it and the workflows it runs are one path, which `ex-scan` walks.
- **Against the estimate,** the engine and its job took 449 lines against ~790, the `scan` Thing and its wiring 417 against ~390, the workflow kind 143 against ~460, as the driver call carried it, and the examples 381 against ~700, with one sample workflow, in Python, where two were planned.

## Tests

| Crate | Test | What it checks |
|---|---|---|
| `oscope-workflows` | `scan_engine` (5) | **A scan records each frame where the stage was:** a tiled z-stack of 3 × 3 tiles by the tilted slide's plane, 27 frames, each within 2.5 µm and 1.5 µm of its tile's plane, every lag alike, the plan with them; the stage back within 0.3 µm; nothing left in scratch. **Autofocus at every second tile** finds the tilted slide within a quarter of the depth of field; the tiles between keep the last z; clear glass is refused and counted. **A cancel** keeps the frames in scratch, ended `cancelled`, and the stage returns. **A step that fails** twice is tried again and the scan completes; three times, the scan fails, saying why. **Plans that can't be run** are refused at submission |
| `oscope-api` | `scan` (2) | **Over HTTP:** the patterns; a preview of 3 × 3 tiles with its estimate, at the stage's z; a snake run into a project, each frame where it was taken, the plan with it, the progress completed; requests that can't be, refused (400). **The sample spiral** (with `OSCOPE_PYTHON=1`): listed with its schema, planned as 9 tiles from the middle round, run into a project; 60 rings refused |
| `oscope-api` | `devices` (1 changed) | The `scan` Thing's TD (a snapshot), and `run` in the OpenAPI document |
| `oscope-ext` | `workflows` (1) | **A Rust workflow through the ABI:** described as a `workflow`, its schema, its tiles from the field, a refusal saying why, and a method it hasn't |
| Python SDK | `test_drivers` (1 more) | A workflow plans its tiles from the field, through the worker |
| `ex-scan` | `--smoke` | A frame for each tile, each within 2.5 µm of it, the plan with them, the page served |

**Where they ran:**

{{RAN}}

**Mutation checks** (each broke the code on purpose, then the tests were run): nineteen of nineteen caught, the Python ones with `OSCOPE_PYTHON=1`.

- **They covered:** the retries; the cancel between frames; the return to the start, and its lag; a frame's position; the one-way approach of a tile's first plane; autofocus at every n-th tile, and a refusal counted; the refusals at submission; the plan kept with the acquisition; the `scan` Thing's z, overlap, workflow pattern and estimate; the Rust kind's parameters; the host's plan call and its registry; and the Python SDK's kind.
- **Reading the tests against the list first** found two gaps, now covered: a tile's first plane not approached from below, whose frames all stayed within the z tolerance, so the test checks that every lag is alike; and a refused autofocus, which no test met, so a tile on clear glass is scanned.
- **The first run missed one,** now covered: the estimate's frames by exposure alone, where the test checked only that the estimate was positive. It checks each frame takes at least the camera's interval.

| Mutant | Caught by |
|---|---|
| No step tried again; a refused autofocus not counted | the step that fails; autofocus at every second tile |
| No cancel between frames | the cancel |
| The return without the lag, or to the first tile | the scan, the cancel and the step that fails, each ending where it started |
| A frame without its position; the plan not kept | the scan's acquisition |
| A tile's first plane not approached from below | every lag alike, once that check was added |
| Autofocus at the wrong tiles | autofocus at every second tile |
| Continuous motion, or a tile without a z, taken | the refusals at submission |
| The scan's z not the stage's; the overlap a fifth | the `scan` Thing over HTTP |
| The estimate's frames by exposure alone | the `scan` Thing over HTTP, once its estimate was checked against the camera's interval |
| A workflow's plan called positions; the host's call without the field; workflows registered as devices | the sample spiral over HTTP |
| A Rust workflow given the wrong parameters | the Rust workflow through the ABI |
| The Python SDK without the kind | the Python SDK's tests |

## Follow-ups and known gaps

- **The field from an active geometry** isn't tested over HTTP: no test calibrates first. `Field::new` from a matrix is P35's, and tested there.
- **The live mosaic** of tiles as they arrive is P38's. `ex-scan`'s page draws the tiles' places and z.
- **Adaptive workflows,** deciding from the frames as they come, need frames to cross to the extension: a later kind.
- **Resuming a scan** from its recording is a later feature (plan §5.11).
- **The estimate's autofocus** takes 10 s. P37 or M3 can take it from the last runs.
- **A scan with no frame taken** may end half the backlash short of its start.

## Clean room

Nothing was copied from `microscope` or any other codebase. The engine follows ADR-0025 and ADR-0024's one-way approach (ISO 230-2), and its lag correction is P34's.
