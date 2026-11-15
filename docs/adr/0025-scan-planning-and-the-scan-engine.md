# ADR-0025: Scan planning and the scan engine

- Status: Accepted
- Date: 2026-11-15
- Phase: P36

## Context

- **Plan §5.14 and D14** ask for raster, snake, z-scan, tiled z-stack and multi-position scans, and focus maps, run stop and go in M1. Continuous scanning, frames triggered on the fly, comes in M3, and the plan and frame model must already carry what it needs.
- **P35 built the plans:** regions, the field from the calibration's geometry, the planners, focus maps and estimates, all serialised as JSON. Nothing ran them.
- **D11 lets extensions add scan workflows,** loaded at start-up like drivers. The extension ABI already calls a driver's methods by name with JSON (ADR-0036), as importers and exporters use it (ADR-0037).
- **Scans are long.** A coverslip at the 10× is over a thousand tiles and half an hour. A scan must be cancellable, must say how far it is, must survive a step that fails once, and must leave the stage somewhere known.
- **Scans make big data.** Frames go through scratch space and are ingested into a project (ADR-0022).

## Decision

**1. A plan** is P35's: the tiles in order, each with its centre and its z if known; the z policy (fixed, a focus map, or autofocus every n tiles); a stack's planes, from the lowest; and the field that spaced the tiles. Its preview is the plan and its estimate of time and data, in JSON. Its `motion` is `stop_and_go`, the only kind the engine runs in M1. `continuous`, a row swept at a constant speed with frames triggered at the tiles' positions, is reserved for M3, and refused until then.

**2. The engine runs a plan as a hardware job,** stop and go. For each tile, in order:

- **It moves there,** in x and y at the z it has, then in z from 10 µm below the tile's z, moving up, as autofocus approaches (ADR-0024).
- **It focuses.** A tile with a z, fixed or the map's, takes it. Under `autofocus`, every n-th tile from the first autofocuses there, starting from the last z found, and the tiles between take the last z found. A refused autofocus, an empty field, keeps the last z, and is counted.
- **It takes each plane,** from the lowest, moving up to it. It takes the first frame whose exposure starts after the move has ended and the stage has settled. It reads the stage's position once settled, and records the frame with it as the frame's position.

All the frames go into one recording in scratch space, with the plan as its source. Ingest then makes one acquisition: a plane for each frame, in order, frame k being tile ⌊k / P⌋'s plane k mod P, for P planes. Each frame keeps its position and time, and the acquisition keeps the plan.

**3. A step that fails is tried again,** twice: a move, a frame that doesn't come, an autofocus that errors. A third failure fails the scan.

**4. Every scan ends where it started.** A scan that completes, fails or is cancelled returns the stage to where it was when it started, approached from below. A cancel is taken between frames. A completed scan is ingested into its project at once, and its recording deleted. A cancelled or failed one keeps its frames as a recording in scratch, ended with why, which the ingest job can take into a project.

**5. Progress** is the share of the frames taken, with the counters `tiles` and `frames`; the `jobs` Thing sends it as events (ADR-0016). The `scan` Thing's `progress` property gives the running scan's plan and the tiles done, each with its position and z, for a page to draw.

**6. The `scan` Thing** offers:

- **`patterns`:** the planners, and the extensions' workflows with their parameters' schemas;
- **`plan`:** a preview, the plan and its estimate, with nothing moving;
- **`run`:** the scan, as a job, answered with its `JobRef`. The spec's `start` is every Thing's own starting in `teta-wot`, as `stop` is (ADR-0034), so the action takes autofocus's name for its job.

The field is the camera's frame through the fitted objective, by its active geometry (ADR-0023), or its nominal pixel without one. The estimate takes the stage's default speed, the camera's frame interval or exposure, whichever is longer, 50 ms of settling, and 10 s an autofocus.

**7. A workflow extension plans a scan** (D11). Given its parameters, JSON its schema describes, and the field, it gives the tiles' positions, each with its z if it has one, and its name. It refuses parameters it can't take, saying why. The engine runs the tiles as a list of positions.

- **In Rust,** a workflow is a driver of the kind `workflow`: `schema` and `plan`, called through `driver_call`. The ABI stays at version 3.
- **In Python,** a class declared with `@driver(name, kind="workflow")`, with the same two methods, called through the worker.
- **Manifests** list workflows under `provides.workflows`. The extension API's version stays 1.0.0, as it did when P27 and P30 added kinds, before the first release.

## Consequences

- **Scans run any plan,** and a workflow from an extension is planned and run like the built-in ones.
- **One acquisition for a scan** keeps its frames, positions and plan together, which stitching (P38, P39) reads. Its planes are frames, not a z-axis: a tiled z-stack's planes are told apart by their index.
- **Cancelling keeps what was taken,** in scratch, so a long scan isn't lost to a slip of the hand, and a cancel doesn't wait for an ingest.
- **A failed tile fails the scan** after its retries, rather than leaving a hole a mosaic would have to explain.
- **Workflows only plan.** One that decides from the frames as they come, adapting the scan, needs frames to cross to the extension, and is a later kind.

## Alternatives considered

- **An acquisition for each tile.** Simple to write, but a thousand acquisitions for one scan, and stitching would have to find them again.
- **Ingesting a cancelled scan's frames at once.** It makes the cancel wait for an ingest, of possibly tens of gigabytes.
- **Skipping a tile that fails.** It leaves holes in the mosaic that nothing explains. A scan that fails says so, and keeps its frames.
- **A new ABI version for workflows,** with their own table function. The JSON call that drivers, importers and exporters use already carries a plan.
- **Returning the stage to the first tile.** The place it started is where the user was looking, and is known whatever the plan.
