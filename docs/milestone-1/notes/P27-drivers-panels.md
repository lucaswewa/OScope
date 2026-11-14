# P27: Driver and panel extensions

- Status: In review
- Branch / PR: `m1/p27-drivers-panels` / [#41](https://github.com/lucaswewa/OScope/pull/41)
- Size: L · estimated ~3,700 before it started (driver traits in both SDKs, with the Rust ABI's second version, ~650; the host's adapters that make extension drivers the HAL's, and the drivers' registry, ~650; panels ~200; the batch job for Checkpoint B ~200; the samples and examples ~450; tests ~600; and a fifth on top); one pull request, at the owner's standing choice for P24–P34 · measured **3,188** hand-written lines added (Rust code 1,624; the Python SDK 117; tests 771, of which Python 74; examples 676), and 163 removed. **That is 2,288 over L's 900, and about a seventh under the estimate.**

## Summary

Extensions add hardware drivers and describe UI panels ([spec](../phases.md#p27); D11; [ADR-0036](../../adr/0036-extension-drivers-and-panels.md)).

- **Drivers in both SDKs:**
  - Rust: the traits `CameraDriver`, `StageDriver` and `IlluminationDriver`, in the HAL's types, exported with `export!(nodes: [...], drivers: [...])`. The ABI's version 2 carries them;
  - Python: classes declared with `@driver(name, kind=...)`, for stages, lights and cameras, whose frames go through shared memory.
- **OScope keeps the HAL's promises.** Its adapters check settings, targets and lights against the capabilities; time, number and pool frames; wait for moves to end; and keep the position track. So an extension's driver passes the HAL's conformance suite as it is.
- **Registration at start-up:** the server loads extensions before devices, and `[devices]` may name an extension's driver.
- **Panels:** `panels/<name>.json`, each a form in JSON Schema, a submission (one of the extension's nodes, or a Thing's action) and a view. Checked at install; served by the `extensions` Thing for the M2 app.
- **`pipeline.batch`,** a job kind: a recipe over the camera's next frames, its images saved as PNG files.
- **Samples:** `ext-rust-camera`, a fake camera in Rust, and a panel for the Python sample. **Examples:** `ex-ext-camera`, the server with the extension's camera in place of the simulator, and `ex-checkpoint-b`, Checkpoint B.

## What was built

| Where | What |
|---|---|
| `oscope-ext-sdk` `abi.rs` | ABI 2: the host's `reply`; the extension's `driver_call` and `capture`; the description an object, `{"nodes", "drivers"}` |
| `oscope-ext-sdk` `drivers.rs` | `CameraDriver`, `StageDriver`, `IlluminationDriver`; `Driver`, `NamedDriver` and `camera()`, `stage()`, `illumination()`; each driver's methods over JSON |
| `oscope-ext-sdk` `lib.rs` | `export!(nodes: [...], drivers: [...])`, and `export!(node, ...)` as before; the glue for driver calls and captures, panics caught; `oscope_core`, `oscope_hal` and `serde_json` re-exported |
| `oscope-ext` `native.rs` | Libraries of ABI 2, their drivers described, called and captured from |
| `oscope-ext` `drivers.rs` | `Drivers`, the registry by name and kind; the adapters `ExtCamera`, `ExtStage` and `ExtIllumination`; the links to a library or a worker |
| `oscope-ext` `worker.rs` | Driver calls and captures through the worker, a capture through a segment; a description with drivers, and P26's list of nodes still read |
| `oscope-ext` `panels.rs` | `Panel`, `Submit`, `View`; panels read and checked |
| `oscope-ext` `installed.rs`, `manifest.rs` | Drivers and panels loaded with the code, and checked against the manifest; `Extensions::drivers()` and `panels()`; a panel's document must be among the assets |
| `oscope-shm` `segment.rs` | `read_bytes` |
| `python/oscope-ext` | `_drivers.py`: `driver`, `drivers`, calls and captures; the worker's `driver` and `capture` messages; `Segment.bytes`; 3 tests |
| `oscope-api` | `Devices::from_config_with`; the `extensions` Thing's `panels` property and `panel/{name}` endpoint; `batch.rs`, the `pipeline.batch` job kind, which the `preview` Thing registers |
| `oscope-server` | Extensions opened before devices; the printed documents take an extension's driver for a fake |
| `examples/ext-rust-camera` | The sample camera: 640 × 480, two modes, 8 and 16 bits, ROIs, a software trigger |
| `examples/ex-ext-camera` | The server with the extension's camera; its conformance test |
| `examples/ex-checkpoint-b` | Checkpoint B |
| `examples/ext-python-node` | Its panel, `example.denoise_tv`; its node may run live |
| `examples/ex-omext`, `examples/ex-rust-ext` | The dummy's panel a real one; packing any sample |

## How to try it

```sh
cargo run -p ex-ext-camera       # the live view of the extension's camera, on http://127.0.0.1:8231/
cargo run -p ex-checkpoint-b     # Checkpoint B, on http://127.0.0.1:8232/; needs uv
```

The READMEs of [`ex-ext-camera`](../../../examples/ex-ext-camera/README.md), [`ex-checkpoint-b`](../../../examples/ex-checkpoint-b/README.md) and [`ext-rust-camera`](../../../examples/ext-rust-camera/README.md) say what they show, and how to use the sample camera in the server.

## Performance

No budget of plan §5.20 is P27's. On the owner's PC, in release builds:

| | Time |
|---|---|
| A Rust driver's call: `configure`, its settings' JSON both ways, on Tokio's blocking threads | 10–11 µs |
| A Python driver's call: `status` or `configure`, through its worker | 34–43 µs |
| The sample camera's frames through OScope's adapter, in its full mode, 8 bits | 60.3 a second for its mode's 60, none lost, over 3 s |
| Checkpoint B's live recipe, as the preview checks it on a 320 × 240 image: the middle of three runs after one to warm up | 27–92 ms in three runs; 98–184 ms in debug builds |

The calls' times are means of 2,000 or 5,000 calls, three times over, from a test written for the measurement and then removed. A stage costs a status and a position call each report interval while it moves.

## Design notes and deviations from the plan

**Worth your review:**

- **One pull request,** as you decided for P24–P34. It came in about a seventh under the estimate, and 2,288 lines over L's 900. The adapters that keep the HAL's promises for every extension driver are the largest part, about 600 lines, and the tests are a quarter.
- **ABI 2 replaces ABI 1.** ADR-0019 makes any change to the ABI a new version, and a library of ABI 1 is now refused. None exists outside this repository, whose samples are built with the SDK. ADR-0019 and ADR-0020 are marked *extended by ADR-0036*.
- **Python cameras too.** The spec gives Python stages and lights, "with camera frames delivered through shared memory". P27 reads that as Python cameras whose frames travel through shared memory, and a test passes one through the conformance suite. Each frame costs a message and a copy, so they suit slow or unusual cameras, not the MIChrome 20 at full rate.
- **Checkpoint B is an example program,** `ex-checkpoint-b`, not a shell script. It does the same through the API, runs headless as a smoke check in the local replays on both platforms, and needs no `curl` or WebSocket tool.
- **The batch job takes the camera's whole frames,** so Checkpoint B reads out the sensor's middle 960 × 720: three of the simulated MIChrome 20's 20-megapixel frames overran its wait on two cores in a debug build.
- **The batch job saves PNG files,** 8 bits a channel, in `{data root}/batch/`, until projects and their stores (P28) give results a home.
- **The Python sample's node now runs live** (`live=True`), so that Checkpoint B's recipe can be the live view's. A live recipe with it needs a budget of seconds: Checkpoint B sets 2 s.
- **The printed documents load no extension,** so `--print-td` and `--print-openapi` take a driver that isn't OScope's own for a fake. Its Thing's description is the same.
- **`ex-omext`'s dummy panel,** `greet.json`, is now `hello.greeting.json`, a real panel, since panels are checked at install.

**Other choices:**

- **Frames are pulled.** OScope's thread calls the driver's `capture`, which blocks until the frame is in, into OScope's buffer. A driver whose SDK pushes frames from its own thread keeps the newest for `capture`.
- **A frame lost to a full pool** is still captured, into a spare buffer, so that the driver's pacing holds, and counted in the next frame's `dropped_before`.
- **A move ends when the driver says so:** OScope reads the status and the position each report interval until the stage is idle, records each position in the track, and fails with the alarm if the move ends in one.
- **OScope's own drivers' names,** `sim`, `fake` and `manual`, are refused in a manifest's `provides.drivers`, as stock nodes' IDs are reserved: the configuration would always mean OScope's own.
- **A driver named for the wrong kind** says so: "`example.fakecam` is a camera driver, not a stage driver".
- **A panel's action** may be any Thing's: Things are fixed only when the server is built, so the install can't check it. A panel's node must be the extension's own.

## Tests

| Crate | Test | What it checks |
|---|---|---|
| `ex-ext-camera` | `conformance` (2) | **The Rust sample's camera,** packed, installed and loaded as a start loads it, **passes the HAL's conformance suite;** its pixels come through, the sample's stripes a pixel further on in each frame. **The registry** knows it for a camera, and refuses it as a stage. **The devices' configuration** names it, and a name no one knows is refused with the drivers there are, the extension's among them. **A manifest providing another driver** than its library has: the extension isn't loaded, and says why |
| `oscope-ext` | `python` (a test more, with `OSCOPE_PYTHON=1`) | **Python's stage, lights and camera pass the conformance suite;** the camera's pixels come through shared memory, each frame its count. **A move that ends in alarm** fails with it, and the next is refused without moving until the alarm is cleared. The test stage arrives on the second look at its status, so a move that isn't waited for fails |
| `oscope-ext` | `panels` (3) | **The Python sample's panel** validates and loads. **A panel that doesn't hold** is refused at install, leaving nothing: a field missing, a form the pipeline's checks refuse, a form not of an object, a node the extension doesn't provide, a submission of no known kind. A node's and an action's panel install. **A panel's document** must be among the assets |
| `oscope-ext` | `packages` (a case more) | A manifest providing a driver named as one of OScope's own, `fake`, refused |
| `oscope-ext` | `native` | ABI 2: libraries of ABI 3 refused; a table without `driver_call` or `capture` refused; the description an object |
| `oscope-ext-sdk` | `sdk` | The tables' layouts in ABI 2; the host's `reply` |
| `oscope-api` | `batch` | **A recipe over the camera's next two frames** as a job: two PNG files, named by frame and output. Refused: no frames, and a node there is none of |
| `oscope-api` | `extensions` (a test more) | **Panels are served** from the next start: the list, a panel whole, and 404 for a name there is none of |
| `oscope-api` | `jobs` | The kinds now include `pipeline.batch` |
| `oscope-server` | `cli` (a test more) | **Printing the TD** with a configuration that names extensions' drivers takes them for fakes, and creates nothing |
| `python/oscope-ext` | pytest (3 more, 11 in all) | Drivers described and called with JSON, a missing method said; a camera's frame into the host's segment, and one of the wrong size refused; a driver of no known kind refused |
| `ex-ext-camera`, `ex-checkpoint-b`, `ex-omext` | `--smoke` | The camera Thing is the extension's, and three previews come; Checkpoint B's live recipe, five previews through it, and a batch job of three frames saved as PNG files; the dummy's panel installs |

**Where they ran:** on Windows, and on Linux under WSL, both with uv, in the local replays (`scripts/ci-local.sh` on Windows, and `scripts/ci-clone.sh` on a fresh clone under WSL: 37 of 37 steps each, 355 tests in 144 suites, the Python tests real). On Windows, the `native`, `python`, `panels`, `sdk`, `batch`, `extensions` and `conformance` suites, `ex-ext-camera --smoke` and `ex-checkpoint-b --smoke` passed 5 times out of 5 each with their binaries pinned to two cores. The first pinned runs of `ex-checkpoint-b --smoke` failed: its batch job took the simulated camera's whole 20-megapixel frames, and on two cores in a debug build three of them overran its five-minute wait. It now reads out the sensor's middle 960 × 720; its pinned runs took 38–40 s each, the live recipe checked at 495 ms against its budget of 2 s.

**Mutation checks** (each broke the code on purpose, then the tests were run): twenty of twenty caught. Before they ran, reading the tests against the list found five gaps, and the tests now cover them: no test put a stage in alarm, checked that a camera's pixels cross the boundary, named an extension's driver in the devices' configuration, gave a library drivers its manifest doesn't provide, or printed the documents with an extension's driver named. The Python test stage also arrived at once, so a move that wasn't waited for would have passed; it now arrives on the second look at its status.

| Broken on purpose | Caught by |
|---|---|
| A camera's mode, format, ROI or trigger changed while it streams | `the_extension_s_camera_passes_the_hal_conformance_suite` |
| A second start allowed | the same |
| Frames lost to a full pool not counted | the same |
| Software triggers not waited for | the same |
| The Rust SDK not calling the camera's `capture` | the same, by its pixels |
| A stage's target outside its travel accepted | `python_drivers_pass_the_hal_conformance_suite` |
| A move not waited for | the same |
| A move allowed in alarm | the same |
| A move ending in alarm taken for arrived | the same |
| Lights not checked against their capabilities | the same |
| A Python camera's frame not read from its segment | the same, by its pixels |
| A library's drivers not checked against its manifest | `the_devices_name_the_extension_s_camera_and_a_manifest_must_match_its_drivers` |
| The extensions' drivers left out of an unknown name's refusal | the same |
| A panel's form not of an object accepted | `a_panel_that_doesn_t_hold_is_refused_at_install` |
| A panel submitting to a node the extension doesn't provide accepted | the same |
| A panel's document outside the assets allowed | `a_panel_must_have_its_document_among_the_assets` |
| A batch of no frames accepted | `a_recipe_runs_over_the_camera_s_next_frames_as_a_job` |
| An extension's driver, printing, not taken for a fake | `printing_takes_an_extension_s_driver_for_a_fake_and_loads_no_extension` |
| A Python camera's frame of the wrong size written (SDK) | `test_a_camera_s_frame_goes_into_the_host_s_segment` |
| A driver named as one of OScope's own accepted | `a_manifest_says_what_is_wrong_with_it` |

## Follow-ups and known gaps

- **The M2 app** draws panels and submits them; P27 only serves them.
- **Batch results into projects** (P28 onwards).
- **A Rust driver runs in the server's process,** as a Rust node does (ADR-0019): a library that crashes takes the server with it. Python drivers run in their worker.
- **Python cameras at high rates:** a pool of segments reused across frames would save making one each frame.

## Clean room

Nothing was copied from `microscope` or any other codebase. No published algorithm is new in P27: the sample camera's test pattern is a diagonal ramp of OScope's own, and the batch job's PNG files are written with the encoder P08b added.
