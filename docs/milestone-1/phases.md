# Milestone 1 phase specifications

Companion to the [implementation plan](../implementation-plan.md). Each phase lists:

- its goal, dependencies, size and ADRs;
- its scope;
- its **example**, which every phase has (D4);
- when it's done;
- how it's tested.

Sizes and the Definition of Done are in [§6 of the plan](../implementation-plan.md#6-how-phases-work), and architecture references (§5.x) point into the plan. **Spec-first** phases open their notes with a behaviour spec and literature sources, written before the code. **Split candidate** marks a phase likely to exceed its budget; estimate it carefully before starting.

Examples live in `examples/<name>/` as small crates, run with `cargo run -p <name>`. Each has a README with "How to try it" and a `--smoke` flag for CI. A page an example serves is plain HTML and JavaScript, built on `examples/common`.

## Contents

- [Stage 0: teta-wot v0.2.0](#stage-0-teta-wot-v020-upstream): [U01](#u01) [U02](#u02) [U03](#u03) [U04](#u04) [U05](#u05) [U06](#u06)
- [Stage 1: Groundwork](#stage-1-groundwork): [P00](#p00) [P01](#p01) [P02](#p02) [P02b](#p02b) [P03](#p03) [P04](#p04) [P05](#p05)
- [Stage 2: Compute](#stage-2-compute): [P06](#p06) [P07](#p07) [P08](#p08)
- [Stage 3: Hardware abstraction and simulator](#stage-3-hardware-abstraction-and-simulator): [P09](#p09) [P10](#p10) [P11](#p11) [P12](#p12) [P13](#p13) [P14](#p14) [P15](#p15)
- [Stage 4: Live microscope](#stage-4-live-microscope): [P16](#p16) [P17](#p17) [P18](#p18) [P19](#p19)
- [Stage 5: Jobs and pipelines](#stage-5-jobs-and-pipelines): [P20](#p20) [P21](#p21) [P22](#p22) [P23](#p23)
- [Stage 6: Extensions](#stage-6-extensions): [P24](#p24) [P25](#p25) [P26](#p26) [P27](#p27)
- [Stage 7: Data, calibration and focus](#stage-7-data-calibration-and-focus): [P28](#p28) [P29](#p29) [P30](#p30) [P31](#p31) [P32](#p32) [P33](#p33) [P34](#p34)
- [Stage 8: Scanning and stitching](#stage-8-scanning-and-stitching): [P35](#p35) [P36](#p36) [P37](#p37) [P38](#p38) [P39](#p39) [P40](#p40)
- [Stage 9: 3D](#stage-9-3d): [P41](#p41) [P42](#p42) [P43](#p43)
- [Stage 10: Analysis and reports](#stage-10-analysis-and-reports): [P44](#p44) [P45](#p45) [P46](#p46) [P47](#p47) [P48](#p48)
- [Stage 11: System and delivery](#stage-11-system-and-delivery): [P49](#p49) [P50](#p50) [P51](#p51) [P52](#p52) [P53](#p53) [P54](#p54)

---

## Stage 0: teta-wot v0.2.0 (upstream)

These phases land in the `teta-wot` repository as branches `u/uNN-short-name`, under the same approval rules as OScope's ([plan §6](../implementation-plan.md#6-how-phases-work)). They are tracked in the OScope plan. Each one:

- adds a `teta-wot` example of its feature;
- adds a CHANGELOG entry under *Unreleased*, and updates the guide;
- writes its notes in OScope, as `docs/milestone-1/notes/UNN-short-name.md`.

<a id="u01"></a>
### U01: Linux support, Ubuntu CI, licence metadata

**Goal.** `teta-wot` builds, tests and runs on Ubuntu x86_64, and its licence is stated consistently.
**Depends on** — · **Size** M

**Scope**
- Add `ubuntu-24.04` to CI beside Windows.
- Platform code in small `cfg` blocks:
  - SIGINT and SIGTERM handling;
  - the `windows-service` feature only on Windows.
- `license = "MIT"` in the workspace package. Remove the guide's "no licence" text, and the old `wot` crate name where it remains.
- Fix or remove `docs/releasing.md`'s link to ADR-0061, which isn't in the repository.
- A short `CLAUDE.md` for `teta-wot`: its checks, branch and PR rules, CHANGELOG discipline, and where phase notes live (in OScope).
- How to check on Linux locally under WSL (Ubuntu 24.04), with a Linux-side `CARGO_TARGET_DIR` so Windows and Linux builds don't collide.

**Done when**
- CI is green on both systems, and the existing examples run on Ubuntu.

**Tests**
- The existing suite on Linux, plus a test that SIGTERM shuts the server down gracefully.

<a id="u02"></a>
### U02: Path parameters for endpoints

**Goal.** Endpoints can declare path parameters and a trailing wildcard, so routes like `dzi/{dataset}_files/{level}/{tile}` fit.
**Depends on** U01 · **Size** L (re-sized from M before it started)

**Scope**
- `#[endpoint(get, "tiles/{level}/{tile}")]` and `{*rest}`. Values reach the handler through an extractor.
- Precedence: a literal segment beats a parameter, and a parameter beats a wildcard. Ambiguous routes are rejected when the server is built.
- Parameters appear in the OpenAPI output.

**Tests**
- Routing-table tests, conflicts detected, and an OpenAPI snapshot.

<a id="u03"></a>
### U03: Router composition, custom listeners, HTTP/2

**Goal.** An application merges its own axum routes and layers into a `teta-wot` server, under the server's authentication, CORS and graceful shutdown. It serves on a listener it supplies, such as TLS, and over HTTP/2 when that listener is TLS.
**Depends on** U01 · **Size** L (re-sized from M before it started)

**Scope**
- Builder methods `.merge(Router)` and `.layer(…)`, with documented ordering relative to Thing routes and the fallback.
- A shutdown token that long-lived merged connections watch, so a WebSocket can't block shutdown.
- `serve_with_listener`, accepting a TCP or TLS acceptor.
- **HTTP/2** negotiated by ALPN on TLS listeners, beside HTTP/1.1; WebSockets stay on HTTP/1.1.
  - Browsers allow only six HTTP/1.1 connections per server, which throttles deep-zoom tiles; HTTP/2 multiplexes them (D33).
- An example, `custom-routes`.

**Tests**
- Authentication applies to merged routes.
- A merged WebSocket closes on shutdown.
- A TLS example with a self-signed certificate.
- A TLS client negotiates HTTP/2 and fetches many resources over one connection.

<a id="u04"></a>
### U04: Binary WebSocket streams

**Goal.** A first-class way to stream binary messages at high rates to many subscribers. When a subscriber falls behind it gets the newest message, and each subscriber has its own parameters.
**Depends on** U03 · **Size** L

**Scope**
- A binary stream affordance. The application receives a `Subscriber` for each connection:
  - its parameters as a `watch` receiver, set by validated JSON text messages;
  - a one-slot sink for `Bytes`;
  - counters for messages sent and dropped.
- The application runs its own producer for each subscriber, so each client can get frames of a different size.
- Described in the Thing Description: subprotocol and parameter schema.
- A benchmark: 1 MB messages at 60 Hz to 4 local clients.
- An example, `binary-stream`.

**Tests**
- A slow subscriber drops messages but never blocks the producer.
- Parameter validation.
- Shutdown closes streams.
- The benchmark is recorded.

<a id="u05"></a>
### U05: Pluggable authentication

**Goal.** An application supplies its own authentication and authorisation, and every affordance knows who is calling.
**Depends on** U03 · **Size** L

**Scope**
- An `Authenticator` trait: request parts in, a `Principal` or a rejection out.
- An `Authorizer` hook: (principal, Thing, affordance, operation, affordance metadata such as "changes state") → allow or deny.
- The principal is available in actions, property setters, endpoints and streams. A WebSocket can authenticate in its first message.
- The existing token and Basic schemes are reimplemented on the trait. Security schemes are described in the Thing Description.

**Tests**
- Rejection codes, the principal reaching every kind of affordance, WebSocket authentication, and the old schemes unchanged.

<a id="u06"></a>
### U06: Release v0.2.0

**Goal.** A tagged `teta-wot` v0.2.0 that OScope can pin.
**Depends on** U02–U05 · **Size** S

**Scope** — following `teta-wot`'s `docs/releasing.md`:
- the CHANGELOG section and the workspace version;
- migration notes from 0.1, and guide updates;
- the 24-hour and 1-hour soak runs on the release build: about a day of wall-clock time on the owner's machine;
- the licence review, then the tag.

---

## Stage 1: Groundwork

<a id="p00"></a>
### P00: Docs scaffolding, working rules, foundational ADRs

**Goal.** The documentation system and working rules that every later phase uses, and the decisions that frame the project.
**Depends on** — (can run beside Stage 0) · **Size** S · **ADRs** 0001, 0002, 0003, 0004

**Scope**
- Docs scaffolding:
  - `docs/README.md`, the index created with this plan;
  - `docs/adr/README.md` and `template.md`;
  - `docs/milestone-1/notes/README.md` and `template.md` (plan, Appendices A–B).
- `.github/pull_request_template.md`: the Definition of Done.
- `CLAUDE.md`, the working rules:
  - the process (plan §6);
  - the clean room (§4.1);
  - dependencies (§4.2);
  - unsafe code (§4.3);
  - ports: the owner's server on 8210; manual checks on 8211–8219; tests on 8220–8229; Vite (M2) on 8280–8289;
  - line endings (LF);
  - the checks to run before a PR.
- A rewrite of the root `README.md`.
- ADR-0001 to ADR-0004.

**Example** — none (docs only).

**Done when**
- The docs render on GitHub, the ADR index lists 0001–0004 as Accepted, and new PRs show the template.

<a id="p01"></a>
### P01: Rust workspace, layering test, CI

**Goal.** A buildable workspace with the crate boundaries of plan §5.2, enforced, and CI on Windows and Ubuntu.
**Depends on** P00, U06 · **Size** L (re-sized from M before it started) · **ADRs** 0005

**Scope**
- **Workspace `Cargo.toml`:**
  - resolver 3, edition 2024, `rust-version` matching `teta-wot` v0.2.0, licence MIT;
  - lints: `missing_docs = "warn"`, `unsafe_code = "forbid"`, with the §4.3 exceptions set per crate; clippy `all = "warn"`.
- **Toolchain and dependencies:** `rust-toolchain.toml`, and `teta-wot = { git = …, tag = "v0.2.0" }`.
- **Skeletons** for every M1 crate in §5.2, each with crate documentation.
  - `oscope-server --version` prints the version.
- **The layering test** (`xtask/` or `tests/layering.rs`):
  - it reads `cargo metadata` and checks every internal edge against an allowed-edges table;
  - its notes record a mutation check: a forbidden edge makes it fail.
- **Licence checks:** `deny.toml` (`cargo-deny`: licences, advisories, duplicates policy).
- **CI** on `windows-latest` and `ubuntu-24.04`: fmt, clippy `-D warnings`, test, doc `-D warnings`, deny.
- **Conventions:** `examples/README.md` sets out the example conventions.
- **`.gitignore`:** `.oscope/`, `node_modules/`, `web/dist/`.

**Example** — `cargo run -p oscope-server -- --version`.

**Done when**
- CI is green on both systems.

**Tests**
- The layering test, with its mutation check.

<a id="p02"></a>
### P02: Core types: units, geometry, time, IDs

**Goal.** The shared vocabulary every crate uses (§5.5).
**Depends on** P01 · **Size** M · **ADRs** 0007

**Scope** (`oscope-core`)
- **Units:** µm as `f64` newtypes at every API, with conversions (mm, nm); durations.
- **Clock:** a `Clock` trait with a system clock and a fake. A `Timestamp` is in nanoseconds since start-up, mapped once to UTC.
- **Geometry:** `Point2`, `Size2`, `Rect`, `Xyz`, and `Affine2` (compose, invert, apply, least-squares fit from point pairs).
- **IDs:** `FrameId`, `JobId`, `ProjectId`, `AcquisitionId`, `ResultId`.
- **Errors:** conventions (`thiserror`).
- **Versioning:** a helper for versioned schemas.
- **Derives:** `serde` and `schemars` on API types.

**Example** — `ex-core-tour`: unit conversions, an affine fitted to noisy point pairs, clock mapping.

**Tests**
- Property tests: an affine and its inverse round-trip; a fit recovers a known transform under noise.
- The clock is monotonic; serde round-trips.

<a id="p02b"></a>
### P02b: Hardware capability survey (docs only)

**Goal.** Know what the real camera, SDK, stage controller, lights and host offer before the frame model, the HAL and the simulator are fixed (D31).
**Depends on** P00 (can run beside Stage 0) · **Size** S (docs only; no driver code) · **ADRs** —

**Scope** — `docs/hardware.md`, from the vendors' documentation, the SDK's own samples, and measurements on the instrument:
- **Camera and SDK (MIChrome 20, TUCam):**
  - the SDK version, its platforms, and its licence, including whether it may be redistributed;
  - pixel formats: raw Bayer access, the pattern, and the bit depth and alignment in 16-bit words;
  - modes and their measured frame rates: full resolution, binning, ROIs;
  - exposure and gain ranges;
  - timestamps: their source and resolution;
  - trigger input and output modes, and their latency;
  - the buffer and callback model, and other controls (white balance, temperature).
- **Stage controller (grblHAL):**
  - the board, the firmware version and build options (`$I`);
  - each axis's travel, steps per mm, maximum rate and acceleration;
  - homing and limit switches;
  - how encoder positions are reported;
  - the status-report rate achievable by polling, measured;
  - the aux outputs (M62–M65), and how the camera trigger is wired.
- **Lights:** what is fitted, and how each is switched and dimmed today.
- **Host:** CPU, RAM, GPU, the USB controller, and the NVMe scratch disk's sustained write rate, measured.
- **Consequences:** where a finding contradicts this plan (no raw Bayer access, say, or lower frame rates), the same PR updates the affected specs (P03, P09, P11, P12, P17, §5.20) and adds a line to the change log.

**Example** — none. The notes' "How to try it" says how to repeat each measurement.

**Done when**
- `docs/hardware.md` is merged, every item above is answered or marked unknown with a reason, and the plan reflects the consequences.

<a id="p03"></a>
### P03: Frame model, buffer pool, frame bus

**Goal.** Raw frames that move through the backend without copies, with every consumer getting what it needs (§5.5, D5).
**Depends on** P02, P02b · **Size** L · **ADRs** 0008

**Scope** (`oscope-core`, `oscope-acquire`)
- **Frame types:**
  - `PixelFormat`: `Bayer16 { pattern, significant_bits }`, Bayer8, Mono8/16, Rgb8/16, Rgba8, planar F32;
  - `FrameMeta`: all the fields in §5.5;
  - `Frame`: a reference-counted pooled buffer plus its metadata;
  - strided views and ROI sub-views, without copying.
- **`FramePool`:**
  - fixed-size slabs;
  - a pluggable allocator, so P07 can supply pinned memory;
  - when the pool is exhausted, the newest frame is dropped and counted; it never panics.
- **`FrameBus`:**
  - lossless subscribers: bounded, in order, with a choice of blocking or counting drops;
  - latest-wins subscribers: one slot;
  - `next_frame_after(t)`;
  - counters for each subscriber.

**Example** — `ex-frame-bus`. A synthetic 20 MP Bayer16 source at 14.5 fps feeds three subscribers: a slow latest-wins one, a lossless counter, and a statistics printer. It prints throughput, drops and pool use.

**Done when**
- On the owner's laptop, the example sustains 14.5 fps × 40 MB with no drops at the lossless subscriber, recorded in the notes.

**Tests**
- Fan-out shares the same buffer (pointer equality).
- Latest-wins and lossless semantics.
- Pool exhaustion.
- `next_frame_after` with the fake clock.
- Property tests of ROI views.

<a id="p04"></a>
### P04: Server skeleton, `system` Thing, media plane, example helper

**Goal.** `oscope-server` serves Things and media routes from a configuration file (§5.4).
**Depends on** P03 · **Size** M · **ADRs** 0006

**Scope** (`oscope-api`, `oscope-server`, `examples/common`)
- **Configuration and CLI:**
  - a TOML file: listen address and port (default 8210), data root, scratch root, a placeholder devices section;
  - the CLI: `--config`, `--port`, `--data-root`, `--version`, `--print-openapi`, `--print-td`.
- **Serving:**
  - the serve loop on `teta-wot` v0.2.0;
  - the media router merged under `/api/v1/media/`, with a `ping` route;
  - `/health`.
- **The `system` Thing:** version, git hash, OS, CPU, memory, uptime, data and scratch roots with their free space.
- **`examples/common`:** serves an example's HTML page and assets next to the API (same origin), on a test port.

**Example** — `ex-server-hello`: a page showing the `system` Thing's properties and its Thing Description.

**Tests**
- Configuration and CLI parsing.
- `/health`.
- The `system` properties.
- The media route sits under the authentication layer (a test authenticator).
- Shutdown with a media WebSocket open.

<a id="p05"></a>
### P05: Logging and the `logs` Thing

**Goal.** Logs for the Logging tab and for diagnosis (§5.19).
**Depends on** P04 · **Size** M · **ADRs** 0009

**Scope**
- **Output:** a `tracing` subscriber writing to the console, daily files kept 30 days, and an in-memory ring.
- **Levels:** trace to error, plus critical (error marked `critical`).
- **Per-job capture:** a hook that P20 uses.
- **The `logs` Thing:** query by level, time range, target and text, with paging; download the files.
- **Live tail:** over a media-plane WebSocket.

**Example** — `ex-logs`: a page with a live tail and filters, and buttons that emit a record at each level.

**Tests**
- Retention, filters, the tail's order, and how critical maps.

---

## Stage 2: Compute

<a id="p06"></a>
### P06: Compute abstraction, CPU backend, benchmark harness

**Goal.** A compute layer that every algorithm targets, with the CPU as the reference (§5.6, D7).
**Depends on** P03 · **Size** M · **ADRs** 0010

**Scope** (`oscope-compute`)
- **Backends:** a backend abstraction, and `ComputeInfo` (CPU threads, SIMD level; no CUDA yet).
- **Thread pools:** separate rayon pools for live and batch work, so batch jobs can't starve the preview.
- **Benchmarks:** the criterion conventions (`benches/`).
- **Equivalence testing:** a helper that compares two outputs within a tolerance and reports the largest error and where it is. It is used by every CUDA twin later.
- **First CPU kernels:** u16 → f32, 2 × 2 binning, box downsampling.

**Example** — `ex-compute-info`: lists the backends and runs microbenchmarks.

**Tests**
- Kernel correctness, and that the two pools stay isolated.

<a id="p07"></a>
### P07: CUDA backend

**Goal.** CUDA acceleration that runs whenever an NVIDIA GPU is present, and stays invisible when it isn't (D7).
**Depends on** P06 · **Size** L · **ADRs** 0011

**Scope** (`oscope-compute`, feature `cuda`)
- **Loading:** `cudarc` with the driver, cuFFT and NVRTC loaded at run time; device selection and information.
- **Kernel build:** `build.rs` compiles `kernels/*.cu` to embedded PTX when `nvcc` is found, and fails with a clear message when the feature is on without it.
- **Execution:** streams, a device memory pool, and asynchronous copies to and from the device.
- **Pinned memory:** a pinned host allocator wired into P03's `FramePool`.
- **Fallback:** without a driver or GPU, it warns once and uses the CPU.
- **Kernels:** CUDA twins of P06's kernels.
- **Records** (ADR-0011): the CUDA runtime libraries that may be redistributed, and the toolkit version.
- **CI:** a weekly job that builds with the CUDA toolkit installed.

**Example** — `ex-cuda-check`: GPU information, twin comparisons, speed-ups or the reason for falling back.

**Done when**
- On the owner's laptop, with `OSCOPE_CUDA=1`, the twins match, and the upload time for a pinned 40 MB frame is measured.

**Tests**
- Equivalence (`OSCOPE_CUDA=1`), the forced fallback, and the weekly build.

<a id="p08"></a>
### P08: Raw Bayer and colour processing

**Goal.** Turn raw Bayer16 into colour images, fast, on the CPU and with CUDA (D5).
**Depends on** P07 · **Size** L · **ADRs** —

**Scope** (`oscope-imaging::color`)
- **Demosaicing:** 2 × 2 superpixel, bilinear, and Malvar–He–Cutler.
- **Colour:**
  - black-level subtraction;
  - white-balance gains;
  - a 3 × 3 colour matrix;
  - the display transform: black and white points, gamma or sRGB, 16 → 8 bits.
- **ROI crop and binning,** combined with demosaicing.
- **Writers** for 16-bit TIFF and 8-bit PNG.

**Example** — `ex-demosaic`: each method's output with timings, and a page comparing them side by side.

**Tests**
- Each method's PSNR on synthetic scenes (known colours under a mosaic) passes a threshold.
- CPU and CUDA give equivalent results.
- The §5.20 budgets are measured.

---

## Stage 3: Hardware abstraction and simulator

<a id="p09"></a>
### P09: HAL traits, capabilities, objective catalogue, fakes

**Goal.** The seams through which simulated, real and extension drivers plug in (§5.9).
**Depends on** P03 · **Size** M · **ADRs** 0012

**Scope** (`oscope-hal`)
- **Traits:** `Camera`, `Stage`, `Illumination`, `ObjectiveMount`, each with a capability report.
- **Supporting types:**
  - `CameraMode` tables;
  - `StageStatus`: idle, moving, homing, alarm, hold;
  - `PositionSample`;
  - `PositionTrack`, a ring buffer with interpolation at time t.
- **Objective catalogue** (TOML), with Mitutoyo defaults.
- **Manual implementations:** `ManualIllumination` and `ManualObjectiveMount`.
- **Fakes:** a scripted fake for each trait.
- **A HAL conformance suite** that any driver runs: the simulator now, the real drivers in M3.
- **The ADR** works through three targets and shows each one fits: the Tucsen camera, the grblHAL stage, and another vendor's camera arriving as an extension.

**Example** — `ex-hal-fakes`: drives the fakes and prints their capability reports.

**Tests**
- Property tests of `PositionTrack` interpolation, and the conformance suite run against the fakes.

<a id="p10"></a>
### P10: Simulated world, optics, H&E tissue

**Goal.** The core of the simulator: what the camera sees for any position, focus and objective (§5.10, D23).
**Depends on** P09 · **Size** L · **ADRs** 0013

**Scope** (`oscope-sim`)
- **World:** coordinates in µm, and a slide model (glass, coverslip, finite tissue islands, so scans have background).
- **Samples:** a `Sample` trait for 3D samples made of layers with z position, thickness and optical properties, rendered at a requested µm per pixel with levels of detail.
- **Optics:**
  - the objective's NA and magnification, the tube lens, the sample's pixel size;
  - a defocus PSF from |dz| and the NA;
  - the depth of field.
- **Image formation:** transmitted brightfield (Beer–Lambert through stained layers) and reflected light.
- **The procedural H&E sample:** hematoxylin nuclei, eosin stroma and cytoplasm, varying thickness, holes and edges. It exports nucleus positions as ground truth for P46.
- **ADR-0013** also sets the design of the simulation clock and the camera's mounting (D30, plan §5.10), which P11 and P12 build.

**Example** — `ex-sim-render`: fields at several z positions and objectives, written as PNGs, with a page showing them.

**Tests**
- The same seed gives the same image.
- Scale relationships between objectives hold.
- Blur grows with |dz|.
- Levels of detail agree once downsampled.
- Rendering time is recorded.

<a id="p11"></a>
### P11: Simulation clock and grblHAL-like stage

**Goal.** One clock for the whole simulator, and a stage that moves like the real one, in real time, with its imperfections and its trigger output (§5.10, D20, D30).
**Depends on** P09 · **Size** L · **ADRs** —

**Scope** (`oscope-sim`)
- **The simulation clock,** shared by every simulated device:
  - real time (1×, following the host's monotonic clock) by default, scaled (for demos and long tests), or stepped (tests advance it explicitly);
  - when the simulator is in use, it is P02's `Clock` for the rest of the server, so timestamps, position tracks and timeouts agree at any rate.
- **Motion timed by the clock:**
  - a move of path length d at speed v takes d ÷ v;
  - with acceleration a switched on (the default, with grblHAL-like values), a move long enough to reach v takes d ÷ v + v ÷ a, with trapezoidal profiles;
  - multi-axis moves follow the path at the set speed, as grblHAL's feed rate does;
  - jogs, focus moves and homing are timed the same way;
  - position is a function of the clock, never of how often it's read.
- **Commands:** jog and jog cancel, hold and stop, homing with switches.
- **Faults:** soft and hard limits that raise alarms.
- **Encoders:** they report the true position, which can differ from the commanded one through backlash and lost steps. Settling time is modelled too.
- **Status reports** at a set rate and latency, feeding `PositionTrack`.
- **Trigger output:** pulses at positions.
- Implements `Stage`.

**Example** — `ex-sim-stage`: moves, homes and jogs, printing each move's planned and measured duration, with a page plotting position against time.

**Tests**
- With the stepped clock: a move takes exactly d ÷ v with acceleration off, and d ÷ v + v ÷ a with it on; multi-axis moves keep the path speed.
- At 1×, durations match the host clock within a tolerance; at 10×, they take a tenth of the time.
- Limits and alarms.
- Property tests of backlash hysteresis.
- The conformance suite.

<a id="p12"></a>
### P12: Simulated camera: sensor, modes, timing, triggers

**Goal.** A MIChrome 20 stand-in that delivers realistic raw frames with exact metadata (D5).
**Depends on** P10, P11 · **Size** L · **ADRs** —

**Scope** (`oscope-sim`)
- **Sensor:**
  - 5472 × 3648 RGGB, 2.4 µm pixels;
  - a 12-bit ADC in 16-bit words;
  - gain, full well, black level;
  - shot, read and dark noise.
- **Modes:** full, 2 × 2 binning and ROIs, with frame times from a placeholder table. Exposure and gain.
- **Acquisition:** free-running, software trigger, and a hardware trigger fed by the simulated stage's trigger output. Frames are paced by the simulation clock at the mode's frame interval.
- **The camera mounting (ground truth):**
  - rotation, flip and offset of the sensor relative to the stage axes, set in the simulator's configuration; scale follows the fitted objective;
  - each frame shows the field at the stage's true positions through this mounting, so the image moves in live view as it would on the instrument;
  - P32's calibration has to recover the mounting.
- **Exposure:**
  - start and end timestamps;
  - motion blur, by integrating stage motion during the exposure;
  - the position at the exposure midpoint, from `PositionTrack`, in `FrameMeta`.
- **Keeping up:** if rendering is slower than the frame interval, the camera delivers fewer frames, each true to its moment, and counts the drops. Motion never slows to match rendering.
- **Output:** into `FramePool` and the frame bus. Implements `Camera`.

**Example** — `ex-sim-camera`: streams in each mode, and prints the frame rate, timestamp jitter, and the metadata position against the true position.

**Tests**
- Noise statistics match the model.
- Saturation.
- Binned values are sums.
- Trigger timing; at 1×, frames arrive at the mode's frame interval.
- During a constant-speed jog, the image shifts by v · Δt ÷ pixel size between frames, in the mounting's direction (checked against the renderer's ground truth).
- Blur length equals speed × exposure.
- Rendering slower than real time drops frames, and doesn't slow the motion.
- The conformance suite.

<a id="p13"></a>
### P13: Samples II: reflective surface, fluorescent cell volume

**Goal.** 3D samples with ground truth for depth from focus and for volumes (D8, D23).
**Depends on** P12 · **Size** L · **ADRs** —

**Scope** (`oscope-sim`)
- **A reflective surface sample:**
  - a height map of steps, slopes, etched features and texture, in reflected light;
  - local defocus follows the height, which becomes the ground truth for depth from focus.
- **Simulated fluorescence** (simulator only):
  - excitation channels, and emission colours on the colour sensor;
  - widefield versus confocal sectioning.
- **A 3D multi-channel cell volume:** nuclei, membranes and organelles, with its ground-truth volume exportable.

**Example** — `ex-sim-3d`: z-series of both samples and their ground truth, with a page to step through z.

**Tests**
- The sharpest z matches the height map.
- Confocal sectioning reduces out-of-focus energy compared with widefield.
- The same seed gives the same result.

<a id="p14"></a>
### P14: Samples III: sub-diffraction structures, imports, load and unload

**Goal.** The super-resolution sample, your own data as samples, and slide handling (D23, D24).
**Depends on** P13 · **Size** M · **ADRs** —

**Scope** (`oscope-sim`)
- **Sub-diffraction structures:** filaments (about 25 nm wide) and mitochondria-like tubules, with fine-scale ground truth.
- **Imported samples:**
  - an image (PNG, TIFF or JPEG), with its µm per pixel;
  - a stack (multi-page TIFF), with its z step.
- **A slide loader:** load, unload, empty stage.
- **A sample catalogue in the configuration,** recording for each sample whether it has ground truth.

**Example** — `ex-sim-samples`: lists, loads and unloads samples, and imports your own image.

**Tests**
- Imports land at the right scale.
- Unloading leaves only background and noise.
- Sub-diffraction detail is blurred as the optics predict (an MTF check).

<a id="p15"></a>
### P15: Simulator at full resolution

**Goal.** The simulator keeps up with a 20 MP camera (§5.20).
**Depends on** P14, P07 · **Size** M · **ADRs** —

**Scope** (`oscope-sim`)
- **A tile cache** of pre-blurred levels of detail, for each (objective, z bucket).
- **Rendering** tiled across rayon threads, with an optional CUDA path for defocus, noise and the mosaic.
- **A scaled-down sensor** (1368 × 912) for tests.

**Example** — `ex-sim-bench`.

**Done when**
- The budgets are measured and recorded: ≥ 14.5 fps with CUDA and ≥ 5 fps on the CPU at full resolution.

**Tests**
- Cached rendering matches uncached within a tolerance; the benchmarks are ignored tests.

---

## Stage 4: Live microscope

<a id="p16"></a>
### P16: Device Things: camera, stage, illumination, objective, simulation

**Goal.** The instrument on the control plane, driven by whichever drivers the configuration names (§5.4).
**Depends on** P05, P15 · **Size** L (split candidate: the `simulation` Thing could become P16b) · **ADRs** —

**Scope** (`oscope-api`)
- **The Things** `camera`, `stage`, `illumination`, `objective` and `simulation`:
  - properties with units and semantic types;
  - persisted settings;
  - actions: move, jog, stop and home; start and stop streaming; set mode; choose, load, unload and import a sample;
  - events: moved, alarm, mode changed.
- **Drivers:**
  - a registry, keyed by `devices.camera = "sim"` and the like in the configuration;
  - device-actor threads;
  - driver errors mapped to HTTP statuses.

**Example** — `ex-http-walkthrough`: a README of `curl` commands, and a plain control page with a position readout and buttons.

**Tests**
- HTTP integration tests on a test port.
- Thing Description snapshots (insta).
- The OpenAPI output generates.

<a id="p17"></a>
### P17: Preview service and binary WebSocket protocol

**Goal.** The light half of the dual stream: viewport-sized JPEG frames to every client (D6, §5.5).
**Depends on** P16, P08 · **Size** L · **ADRs** 0014

**Scope** (`oscope-acquire`, `oscope-api`)
- **Sessions:** one preview session per client, on U04's binary streams.
- **Client parameters:** viewport size, ROI in sensor coordinates, quality, maximum fps.
- **The chain:** crop the ROI or bin, demosaic, resize, display transform, then encode JPEG with SIMD. It uses CUDA where available.
- **Delivery:** latest-wins. Size and quality drop when a client keeps dropping frames.
- **The wire format** (ADR-0014):
  - a binary header: magic, version, kind, sequence, frame ID, timestamps, sensor ROI, output size, codec;
  - then the payload;
  - JSON text messages for control.
- **Statistics for each client:** fps, latency, bytes.
- **The `preview` Thing** for defaults.

**Example** — `ex-live-view`. A plain page that:
- reads the WebSocket and decodes with `createImageBitmap`;
- draws with WebGL;
- zooms by requesting an ROI;
- shows fps and latency;
- has jog buttons and a speed setting, so the image slides across the view at the speed and in the direction that the stage and the simulated camera mounting imply.

**Done when**
- On the owner's laptop, the §5.20 preview budget is met, and jogging at a set speed moves the live image at the expected rate and direction. The notes record both checks.

**Tests**
- Encoding and decoding the protocol.
- A slow client gets the newest frames.
- ROIs map correctly.
- The §5.20 budget is measured.

<a id="p18"></a>
### P18: Live statistics and overlay data

**Goal.** Accurate data for the live view's overlays, computed from raw frames (§5.5).
**Depends on** P17 · **Size** M · **ADRs** —

**Scope** (`oscope-acquire`)
- A statistics subscriber running at up to 10 Hz, which computes:
  - 12-bit histograms per channel, subsampled, with percentiles;
  - a saturation mask at preview resolution, packed into bits (binary message kind `mask`);
  - raw line profiles on request;
  - ROI statistics: mean, standard deviation, minimum and maximum per channel;
  - a focus value.
- All of them go out on the preview WebSocket.

**Example** — `ex-live-overlays`: the live page with a crosshair, an ROI to draw, a line-profile chart, a histogram, false colour (a WebGL LUT) and a saturation overlay.

**Tests**
- Histograms match a reference, mask packing, and profile sampling.

<a id="p19"></a>
### P19: Sidecar contract, service mode, shared-memory preview

**Goal.** Run as a service, be supervised by the desktop app, and feed it frames without HTTP (D21).
**Depends on** P17 · **Size** M · **ADRs** 0015

**Scope** (`oscope-server`, `oscope-shm`, `packaging/`)
- **The sidecar contract:**
  - `--managed`;
  - a JSON ready line with the port, PID and local token path;
  - closing stdin shuts the server down;
  - exit code 75 asks for a restart.
- **Service mode:** a Windows service (install, uninstall, run) and a systemd unit file.
- **The local token file,** with restricted permissions.
- **The shared-memory preview ring:**
  - a header plus N slots, made consistent with a seqlock;
  - Windows named file mappings, and `/dev/shm` on Linux.

**Example** — two programs.
- `ex-sidecar`: a small supervisor that starts the server in managed mode and restarts it after exit 75.
- `ex-shm-reader`: a second process that reads the ring and reports its rate and any torn reads.

**Tests**
- The contract tests (spawn, ready, stop, restart), with a mutation check.
- The ring stays consistent while it's read and written at once.

**Checkpoint A** — a demo script in P19's notes: start the server, open `ex-live-overlays`, jog the simulated stage, zoom to a full-resolution ROI, and watch the histogram and saturation change as exposure changes.

---

## Stage 5: Jobs and pipelines

<a id="p20"></a>
### P20: Job service and the `jobs` Thing

**Goal.** Every long-running piece of work, queued, observable and recorded (D28, §5.11).
**Depends on** P16 · **Size** L (split candidate: persistence could become P20b) · **ADRs** 0016

**Scope** (`oscope-jobs`, `oscope-api`)
- **The job model** in §5.11.
- **The hardware queue:**
  - FIFO, reorderable, one job at a time;
  - it holds the hardware lock, so the device Things refuse manual moves while it's held.
- **The compute pool:** N workers and a GPU-memory semaphore.
- **For code running inside a job:** progress reporting, a cancellation token, pause and resume where supported, and the per-job log capture from P05.
- **Persistence:** SQLite (`rusqlite`, bundled) with migrations. Jobs cut short are marked *interrupted* at start-up.
- **The `jobs` Thing:** list and filter, get, cancel, pause, resume and reorder; job-changed events.
- **The `JobRef` pattern** for actions.

**Example** — `ex-jobs`: submits simulated stage-move jobs and compute jobs, with a page showing the queue and progress live.

**Tests**
- Ordering and exclusivity.
- Cancellation at checkpoints.
- Persistence across a restart, and the interrupted marking.

<a id="p21"></a>
### P21: Pipeline engine

**Goal.** Processing as DAGs of typed nodes, live or in batch (D12, §5.7).
**Depends on** P20, P07 · **Size** L (split candidate: tiled execution could become P21b) · **ADRs** 0017

**Scope** (`oscope-pipeline`)
- **Nodes:** the node trait and descriptor, port types, and parameters validated by JSON Schema.
- **Recipes:** versioned JSON, validated for types, cycles and missing inputs.
- **Scheduling:** topological order with parallel branches; data placed on the CPU or GPU with as few transfers as possible; memoisation.
- **Live mode:** a latency budget, with late frames dropped.
- **Batch mode:** runs as a job, with progress.
- **Tiled execution:** tiles plus halos, reassembled afterwards.
- **A node registry,** and trivial nodes for the tests.

**Example** — `ex-pipeline`: runs a recipe file on an image from the command line, and prints the graph and the time for each node.

**Tests**
- Validation errors and scheduling order.
- Tiled and untiled runs are equivalent for nodes with halos.
- Cache hits.
- Cancellation.

<a id="p22"></a>
### P22: Stock nodes I: correction and display; live recipes

**Goal.** The nodes the live view and every acquisition need, and a live recipe on the preview.
**Depends on** P21, P08 · **Size** M · **ADRs** —

**Scope**
- **Nodes:** demosaic, dark subtraction, flat-field division, white balance, colour matrix, levels and gamma, resize and bin, crop, LUT and false colour, histogram and statistics, format conversion.
- **Live recipes:** `preview.live_recipe` runs a recipe inside the preview chain, checked against its latency budget.

**Example** — `ex-live-recipe`: the live page with a choice of live recipes, such as flat field plus false colour.

**Tests**
- The nodes agree with P08's functions, and the live latency budget holds.

<a id="p23"></a>
### P23: Stock nodes II: filters, enhancement, threshold, morphology

**Goal.** The general image-processing toolkit.
**Depends on** P22 · **Size** L · **ADRs** —

**Scope**
- **Nodes:**
  - Gaussian, median and bilateral filters;
  - unsharp mask, CLAHE;
  - manual and Otsu thresholds;
  - erode, dilate, open and close;
  - distance transform, connected-component labelling.
- **CUDA twins** for the costly ones: Gaussian, median, bilateral, CLAHE.

**Example** — `ex-filters`: a page comparing the outputs.

**Tests**
- Against small hand-computed cases, and CPU and CUDA equivalence.

---

## Stage 6: Extensions

<a id="p24"></a>
### P24: `.omext` packages, registry, `extensions` Thing

**Goal.** Install, inspect and enable extensions (D9–D11, §5.8).
**Depends on** P21 · **Size** M · **ADRs** 0018

**Scope** (`oscope-ext`, `oscope-api`)
- **The manifest schema** (TOML).
- **Validation:** the ID, semver, `oscope-api` compatibility, and whether the package has a build for this platform.
- **The zip reader,** with protection against path traversal.
- **Lifecycle:** install and uninstall into the data root; enable and disable; a load order at start-up; a registry for each kind.
- **The `extensions` Thing:** list, details, enable and disable, and install through the upload route.
- **`oscope-server ext pack <folder>`** builds an `.omext`.

**Example** — `ex-omext`: packs, installs and lists a dummy extension.

**Tests**
- Manifest errors, incompatible versions refused, idempotent installs, and path traversal blocked.

<a id="p25"></a>
### P25: Rust extension ABI and SDK

**Goal.** Third parties write processing nodes in Rust, and OScope loads them safely and fast (D9).
**Depends on** P24 · **Size** L · **ADRs** 0019

**Scope** (`oscope-ext-sdk`, `oscope-ext`)
- **The ABI:**
  - versioned C ABI tables and `#[repr(C)]` frame views;
  - strings and errors across the boundary;
  - ownership rules: the host allocates outputs through its own table;
  - panics caught at the boundary.
- **The SDK:** the `export!` macro and the safe `ProcessingNode` trait. Other kinds arrive with their features: drivers and panels in P27, importers and exporters in P30, workflows in P36, reports in P47.
- **The loader:** `libloading`, with an ABI version check.
- **A sample extension,** `examples/ext-rust-node` (an emboss node), packed as an `.omext`.

**Example** — build the sample, pack it, install it, and run it in a recipe.

**Tests**
- Layout assertions for the ABI.
- A panic becomes an error.
- A version mismatch is refused.
- The node runs in a pipeline.

<a id="p26"></a>
### P26: Python extension runtime and SDK

**Goal.** Third parties write nodes in Python with their own dependencies, isolated from the server (D10).
**Depends on** P24, P19 · **Size** L · **ADRs** 0020

**Scope** (`oscope-ext`, `python/oscope-ext/`)
- **The Python SDK:**
  - a node decorator;
  - a parameter schema, explicit or from type hints;
  - NumPy views of shared-memory frames;
  - the message loop.
- **uv:**
  - finding the uv binary;
  - creating each extension's virtual environment at install time from its manifest;
  - an option to install from an offline wheelhouse.
- **The worker supervisor:** spawn, health checks, restart and timeouts.
- **Transport:** frames in and out through shared memory.
- **A sample extension:** a scikit-image filter.

**Example** — `ex-python-node`: installs the sample and runs it in a recipe on a simulated frame, with timings.

**Tests**
- A worker killed mid-call gives a job error and is restarted.
- Timeouts.
- A shared-memory round trip.
- The SDK's pytest suite runs in CI, with uv installed.

<a id="p27"></a>
### P27: Driver and panel extensions

**Goal.** Extensions can add hardware drivers and describe UI panels (D11).
**Depends on** P25, P26 · **Size** L · **ADRs** —

**Scope**
- **Driver traits** in both SDKs:
  - Rust: camera, stage and illumination;
  - Python: stage and illumination, with camera frames delivered through shared memory.
- **Start-up registration:** extension drivers are registered before the server is built, and the configuration can then name them.
- **Server-described panels:** JSON Schema forms and result views, served from the `extensions` Thing for M2 to render.
- **Samples:** a Rust fake-camera driver extension, and a Python panel.

**Example** — run the server with the extension's camera instead of the simulator.

**Tests**
- The HAL conformance suite passes against the extension driver; the panel schema validates.

**Checkpoint B** — a demo script: one recipe with stock, Rust and Python nodes, run live on the preview and then as a batch job.

---

## Stage 7: Data, calibration and focus

<a id="p28"></a>
### P28: Projects, OME-Zarr store, SQLite index

**Goal.** Where every acquisition, result and report lives (D15, D16, §5.12).
**Depends on** P20 · **Size** L · **ADRs** 0021

**Scope** (`oscope-store`, `oscope-api`)
- **Projects:** the project, acquisition, result and report records and folders, and provenance.
- **OME-Zarr:**
  - read and write OME-Zarr v3 through `zarrs`;
  - NGFF 0.5 multiscales;
  - chunks aligned to deep-zoom tiles (512 px, holding 2 × 2 tiles; P40) and to stack planes;
  - zstd compression.
- **The SQLite index,** which can be rebuilt from the folders.
- **Thumbnails.**
- **The `projects` Thing:**
  - list, search and page;
  - create, rename, tag and delete;
  - metadata;
  - a thumbnail route.

**Example** — `ex-projects`: creates a project with a synthetic acquisition, shows Gallery-style cards on a page, and has a README on opening the store in napari or Python.

**Tests**
- Read and write round-trips.
- NGFF metadata validated against its schema.
- A rebuilt index equals the live one.
- Concurrent writers.

<a id="p29"></a>
### P29: Scratch recorder and ingest

**Goal.** The raw half of the dual stream: every frame to NVMe, then into the project (§5.5).
**Depends on** P28 · **Size** M · **ADRs** 0022

**Scope** (`oscope-store`, `oscope-acquire`)
- **The frame-log format:**
  - a header, frame records with their metadata, and an index footer;
  - recovery by scanning after a crash.
- **Writing:** preallocated, sequential, on a dedicated thread, as a lossless subscriber.
- **Ingest:** a job that writes a frame log into OME-Zarr with zstd, in parallel.
- **Scratch space:** health and a clean-up policy.

**Example** — `ex-record`: records N seconds of full-rate simulation to scratch, reports MB/s and drops, then ingests the log into a project.

**Done when**
- 580 MB/s for 10 minutes with no drops on the owner's NVMe (§5.20).

**Tests**
- Recovery from a truncated log.
- No drops at rate with the test sensor.
- Ingested data equals the recorded frames.

<a id="p30"></a>
### P30: Capture, downloads, importers and exporters

**Goal.** Single captures, getting data out, and bringing data in, including through extensions.
**Depends on** P29, P27 · **Size** M · **ADRs** —

**Scope**
- **The `capture` Thing:**
  - single raw captures and bursts, as acquisitions;
  - a display rendition, PNG or JPEG.
- **The data route,** with Range requests and content types.
- **Importers:** images (TIFF, PNG, JPEG) and multi-page TIFF stacks.
- **Exporters:** OME-TIFF, 16-bit TIFF and PNG.
- **Importer and exporter extension kinds,** in Rust and Python.

**Example** — `ex-capture`: captures from the simulator, downloads the raw frame as OME-TIFF, and imports an image.

**Tests**
- Metadata is complete, Range requests work, and export and import round-trip.

<a id="p31"></a>
### P31: Registration: phase correlation on CPU and CUDA

**Goal.** The registration primitive that calibration, autofocus checks and stitching build on. Spec-first.
**Depends on** P23 · **Size** M · **ADRs** —

**Scope** (`oscope-imaging::registration`)
- **Phase correlation:** a Hann window, a sub-pixel peak, and a search restricted to a window around the expected offset.
- **A confidence score** from the peak ratio.
- **FFTs:** rustfft on the CPU and cuFFT on CUDA.
- **A batch API** for many pairs at once.
- **Sources,** cited in the notes: Kuglin and Hines (1975); Foroosh et al. (2002).

**Example** — `ex-register`: registers simulated tiles with known offsets and prints the errors.

**Tests**
- Known shifts are recovered within 0.1 px with noise present.
- Featureless or barely overlapping pairs score low confidence.
- CPU and CUDA are equivalent.
- The §5.20 budget is measured.
- End to end with the simulator: frames from a constant-speed jog register to shifts of v · Δt ÷ pixel size, in the direction of the simulated camera mounting.

<a id="p32"></a>
### P32: Calibration I: pixel size, camera-to-stage affine, backlash, store

**Goal.** Per-objective geometry calibration, versioned and managed (D18, §5.13). Spec-first.
**Depends on** P30, P31 · **Size** L · **ADRs** 0023

**Scope** (`oscope-workflows`, `oscope-api`)
- **The calibration store:** versioned sets for each objective and binning; activate, list, export and import. The `calibration` Thing.
- **Objective choice:** choosing the fitted objective, and the plausibility check.
- **Pixel size:** the nominal value from the catalogue and tube lens.
- **The camera-to-stage affine:** stage moves in a pattern, the frames registered, and a least-squares fit for rotation, scale, flip and skew.
- **Backlash:** measured per axis, by approaching from each side.
- **Soft limits** taken from homing.

**Example** — `ex-calibrate`: calibrates the simulator with a rotation, scale and backlash injected, and prints the recovered values against the truth.

**Tests**
- The injected parameters are recovered within tolerance.
- A wrong objective is flagged.
- Store versioning.

<a id="p33"></a>
### P33: Calibration II: dark, flat, colour; the wizard

**Goal.** Photometric calibration, and the calibration wizard on the server (§5.13).
**Depends on** P32 · **Size** L · **ADRs** —

**Scope**
- **Dark frames,** for each range of exposure and gain.
- **Flat fields,** for each objective and light, from a blank area, averaged and normalised.
- **White balance.**
- **The colour-correction matrix,** from a simulated colour target added to the slide model here.
- **The wizard:** a state machine of steps, prompts, required actions, validation and resume.

**Example** — `ex-wizard`: walks the wizard from the command line against the simulator.

**Tests**
- Flat correction improves uniformity under simulated vignetting.
- Dark subtraction removes the offset.
- The colour matrix recovers the simulated colours within a ΔE threshold.
- Wizard transitions.

<a id="p34"></a>
### P34: Autofocus, auto exposure, auto white balance

**Goal.** Fast, reliable focus and exposure on every simulated sample (§5.14). Spec-first.
**Depends on** P33 · **Size** L · **ADRs** 0024

**Scope** (`oscope-workflows`, `oscope-api`)
- **Focus measures:** Brenner, Tenengrad and variance of Laplacian, on the CPU and with CUDA.
- **The autofocus job:**
  - a coarse sweep, then a fine one;
  - a continuous-sweep mode that matches frames to z by timestamp;
  - approach from one direction, to avoid backlash;
  - an ROI;
  - defaults for each light.
- **The auto exposure job:** a target percentile, within limits, until it converges.
- **Auto white balance.**
- **The `autofocus` and `exposure` Things.**

**Example** — `ex-autofocus`: focuses the H&E, reflective and sparse samples, prints the error against the true focus and the time taken, and plots the focus curve on a page.

**Tests**
- Error within a quarter of the depth of field on the default samples, sparse fields included.
- Empty fields are detected.
- Continuous and stepped sweeps agree.
- Auto exposure converges.

**Checkpoint C** — a demo script: calibrate a simulated objective, autofocus, auto-expose, and capture into a project; open the result in napari.

---

## Stage 8: Scanning and stitching

<a id="p35"></a>
### P35: Scan planners and focus maps

**Goal.** Every scan pattern, planned and previewable before the stage moves (§5.14).
**Depends on** P34 · **Size** M · **ADRs** —

**Scope** (`oscope-workflows::scan`)
- **The plan model:** tiles with positions, a z policy, overlaps and an order.
- **Planners:** raster, snake, z-scan, tiled z-stack and multi-position.
- **Regions:** rectangles, polygons, and masks.
- **The focus map:** sample points, then a plane or thin-plate-spline fit with outliers rejected.
- **Overlap** from the calibrated field of view.
- **Estimates** of time and data size, and a preview as JSON.

**Example** — `ex-plan`: prints plans, with a page that draws their tile layouts.

**Tests**
- Regions are covered, overlaps are correct, snake order holds, and the surface fit recovers a tilted simulated slide.

<a id="p36"></a>
### P36: Scan engine, the `scan` Thing, workflow extensions

**Goal.** Run any plan as a hardware job, recording everything (D14).
**Depends on** P35 · **Size** L (split candidate: workflow extensions could become P36b) · **ADRs** 0025

**Scope**
- **The engine,** as a hardware job:
  - for each tile: move, settle, focus (from the map or by autofocus), capture a frame or a stack, record to scratch;
  - then ingest.
- **Acquisition metadata** includes every frame's position.
- **Ending a scan:** a clean cancel and a defined end position; retries on errors.
- **Progress events.**
- **The `scan` Thing:** planners, preview, and start, which returns a `JobRef`.
- **The `ScanWorkflow` extension kind,** in Rust and Python.
- **Continuous-scan fields** reserved in the plan and metadata.

**Example** — `ex-scan`: a snake scan of the simulated H&E slide, with a page showing the tiles as they arrive.

**Tests**
- An end-to-end scan on the test sensor.
- A cancel mid-scan.
- Complete metadata.
- A sample Python workflow runs.

<a id="p37"></a>
### P37: Histo scan: overview and tissue detection

**Goal.** Scan only where there's tissue. Spec-first.
**Depends on** P36 · **Size** M · **ADRs** —

**Scope**
- **The overview pass:** binned, fast and widely spaced.
- **Tissue detection:** a colour and saturation threshold, morphology and a minimum area.
- **Tiling:** the detected regions become tiles, with a focus map over the tissue, run through the engine.

**Example** — `ex-histo-scan`.

**Tests**
- At least 99 % of the simulated tissue is covered with fewer tiles than a full raster.
- An empty slide gives no tiles.

<a id="p38"></a>
### P38: Live stitching: in scans and free-roam

**Goal.** A mosaic that builds as frames arrive, from scans or from jogging by hand (D13, §5.15).
**Depends on** P36 · **Size** L · **ADRs** 0026

**Scope** (`oscope-acquire`, `oscope-api`)
- **The live mosaic canvas:** multiscale 256-px tiles, bounded memory, eviction to scratch.
- **Placement** through the calibrated affine at each frame's exposure-midpoint position.
- **Blending:** the newest frame wins, or edges are feathered.
- **Tiles:** dirty-tile events on the preview WebSocket, and a tile route.
- **The `live_mosaic` Thing:** start, stop and clear.
- **Free-roam gating** by displacement, sharpness and estimated motion blur.
- **A hook in the scan engine.**

**Example** — `ex-live-mosaic`: live view beside an OpenSeadragon mosaic that grows as you jog or scan.

**Tests**
- Placement error against the simulator's truth, with latency switched on, stays below a bound.
- The gating rules.
- The memory bound.

<a id="p39"></a>
### P39: Post-acquisition stitching

**Goal.** Seamless mosaics refined by image correlation (§5.15). Spec-first.
**Depends on** P38 · **Size** L · **ADRs** 0027

**Scope** (`oscope-stitch`)
1. Build the overlap graph from tile positions.
2. Register the pairs in batch (P31) near their expected offsets, and filter by confidence.
3. Solve a global weighted least-squares problem (sparse), rejecting outliers iteratively.
4. Optionally estimate a flat field from the tiles.
5. Blend with feathering.
6. Write a multiscale OME-Zarr out of core, tile by tile.

- Z-stacks reuse the per-tile transforms.
- It all runs as the stitch job.

**Example** — `ex-stitch`: stitches P36's scan with stage errors injected, and reports residuals against the truth.

**Tests**
- Residuals against the truth.
- Robustness to featureless tiles.
- The memory bound.
- The §5.20 throughput.

<a id="p40"></a>
### P40: Deep-zoom serving, tile cache, DZI export

**Goal.** Any mosaic browsable at any zoom (§5.15).
**Depends on** P39 · **Size** M · **ADRs** 0028

**Scope** (`oscope-store`, `oscope-api`)
- **Serving:** DZI descriptors and tiles from the multiscales, with display-transform parameters in the query, encoded as JPEG.
- **The disk tile cache** on scratch:
  - LRU, with a size limit;
  - its layout is documented for the desktop fast path.
- **Speed** (plan §5.15, D33):
  - a memory cache of recently decompressed chunks, so the other tiles in a chunk cost only the JPEG encode;
  - the coarse levels rendered into the cache when stitching finishes;
  - tile work on the batch pool at low priority, with a thread cap while a hardware job runs;
  - a `Server-Timing` header on every tile, so the browser's developer tools show cache hits and server time.
- **Exporting:** a static DZI folder, as an exporter and as a node.

**Example** — `ex-deep-zoom`: an OpenSeadragon page with levels and gamma controls on the stitched slide, showing cold and cached tile times.

**Done when**
- The §5.20 deep-zoom budgets are met on the owner's laptop, for cold and cached tiles, with and without a scan running.

**Tests**
- Tile geometry at edges and at every level.
- Cache hits and misses.
- An export equals what is served.
- A scripted client times a full-screen view of a region never seen before, against the budget.

**Checkpoint D** — a demo script: histo-scan the simulated H&E slide while the mosaic builds live, stitch it, then browse it in deep zoom.

---

## Stage 9: 3D

<a id="p41"></a>
### P41: Z-stacks, volume store, slice and brick routes

**Goal.** Volumes acquired, stored and served for VTK.js (D8, §5.16).
**Depends on** P36 · **Size** L · **ADRs** 0029

**Scope**
- **Acquisition:** z-stacks into OME-Zarr (z, c, y, x) with their voxel size.
- **Multiscales:** volumes downsampled in xy and z.
- **Media routes:** metadata, orthogonal slices, and bricks (a binary header plus uint8 or uint16) at a chosen level and ROI.

**Example** — `ex-volume`: a stack of the fluorescent cell sample, rendered by VTK.js (MIP and composite) on a page.

**Tests**
- Slices and bricks match the source, the header format holds, and levels are chosen correctly.

<a id="p42"></a>
### P42: Depth from focus and extended depth of field

**Goal.** Height maps and all-in-focus images from z-stacks (§5.16). Spec-first.
**Depends on** P41 · **Size** L · **ADRs** —

**Scope** (`oscope-volume`)
- **The focus-measure volume,** on the CPU and with CUDA.
- **Height:**
  - a peak with a parabolic fit at sub-step precision;
  - a confidence map;
  - a median filter on the height map.
- **The all-in-focus (extended depth of field) image.**
- **Meshes:** export as PLY, STL or OBJ.
- **Pipeline nodes** for each step.
- **The surface,** served as a heightfield brick.

**Example** — `ex-depth-from-focus`: a z-stack of the reflective sample becomes a VTK.js surface textured with the all-in-focus image; the error against the truth is printed.

**Tests**
- Height RMS error below half a z step on textured regions.
- Low confidence on featureless regions.
- CPU and CUDA equivalence.

<a id="p43"></a>
### P43: Deconvolution and volume rendering

**Goal.** Sharper volumes, checked against ground truth (D24). Spec-first.
**Depends on** P42 · **Size** L · **ADRs** —

**Scope** (`oscope-volume`)
- **A theoretical widefield PSF:** Born–Wolf, or Gibson–Lanni if time allows.
- **Richardson–Lucy:**
  - rustfft on the CPU, cuFFT on CUDA;
  - optional TV regularisation;
  - controls for the number of iterations.
- **Pipeline nodes.**
- **Applied to** the fluorescent volume and the sub-diffraction sample.

**Example** — `ex-deconvolve`: a VTK.js page with MIP, composite (with a transfer function) and isosurface modes, and a before-and-after toggle.

**Tests**
- NRMSE against the truth falls over the iterations.
- Energy is conserved.
- CPU and CUDA equivalence.

**Checkpoint E** — a demo script: a height-map surface of the reflective sample, and a deconvolved fluorescent volume, both in VTK.js.

---

## Stage 10: Analysis and reports

<a id="p44"></a>
### P44: Annotations and measurements

**Goal.** Mark up images and measure them in real units (§5.17).
**Depends on** P40 · **Size** M · **ADRs** —

**Scope**
- **Annotations:**
  - GeoJSON in µm, in layers, attached to acquisitions and results;
  - stored in the project, with CRUD through the `projects` Thing.
- **Measurements:**
  - length, area, perimeter;
  - intensity statistics within an ROI, computed tile by tile on large images;
  - CSV export.

**Example** — `ex-annotate`: an OpenSeadragon page where you draw polygons and get measurements back.

**Tests**
- Measurements of synthetic shapes.
- Unit conversions.
- ROI statistics are the same tiled or untiled.

<a id="p45"></a>
### P45: Stain analysis

**Goal.** Separate the stains in pathology images and quantify DAB (D25). Spec-first.
**Depends on** P44 · **Size** M · **ADRs** —

**Scope**
- **Colour deconvolution** (Ruifrok and Johnston, 2001):
  - stain vectors for H&E, H-DAB, or your own;
  - optional estimation from the data (Macenko).
- **DAB quantification:**
  - positive-pixel thresholds;
  - percentage of positive area;
  - intensity bins.
- **Nodes,** run tile by tile across whole slides.

**Example** — `ex-stain`: separates the stains of the stitched H&E slide, and of a simulated DAB variant.

**Tests**
- Stain separation recovers the simulator's known concentrations within a tolerance.
- DAB percentages on synthetic patches.

<a id="p46"></a>
### P46: Cell detection and counting

**Goal.** Find, count and measure cells (D25). Spec-first.
**Depends on** P45 · **Size** L · **ADRs** —

**Scope**
- **Preprocessing:** the hematoxylin channel or grey.
- **Segmentation:**
  - global, Otsu or adaptive thresholds;
  - distance transform;
  - seeded watershed;
  - filters on size and shape.
- **Measurements for each object:** area, perimeter, circularity, intensity per channel, centroid in µm.
- **Outputs:** a results table (CSV), and the objects as a GeoJSON overlay.
- **Nodes,** working on brightfield and fluorescent samples.
- **A stub Python extension** showing where cellpose or StarDist would plug in.

**Example** — `ex-count-cells`: counts nuclei on the stitched slide and overlays them in OpenSeadragon.

**Tests**
- Count and position accuracy against the H&E ground truth from P10.
- Touching nuclei are split.

<a id="p47"></a>
### P47: Reports, and report extensions

**Goal.** Shareable PDF and HTML reports with provenance (§5.17).
**Depends on** P46 · **Size** L · **ADRs** 0030

**Scope**
- **The report model:** metadata, images (rendered regions and thumbnails), tables, simple charts as SVG, provenance.
- **Rendering:** Typst templates compiled in-process to PDF; minijinja for HTML.
- **Template management.**
- **The report-generator extension kind.**
- **The report job.**

**Example** — `ex-report`: a report on P46's results.

**Tests**
- The PDF is generated with the expected page count and text.
- An HTML snapshot.
- Template errors are reported.

<a id="p48"></a>
### P48: The `workspace` Thing: analyses on datasets, results, provenance

**Goal.** The backend of the Workspace tab: run, keep and re-run analyses (§5.4).
**Depends on** P47 · **Size** M · **ADRs** —

**Scope**
- **Running:** a recipe on a dataset or ROI, as a job.
- **Results:** results for each project, their metadata and provenance; delete; re-run with changed parameters, showing the provenance difference.
- **Annotation routes:** consolidated from P44.

**Example** — `ex-workspace`: runs the cell-counting recipe on a stitched slide over HTTP, with a page listing the results.

**Tests**
- Provenance is complete: software, node and extension versions, and hashes of the inputs.
- A re-run with the same inputs reproduces its results.

**Checkpoint F** — a demo script: count cells and quantify DAB on a stitched slide, then produce a PDF report.

---

## Stage 11: System and delivery

<a id="p49"></a>
### P49: Accounts, sessions, control lease, TLS

**Goal.** Safe remote use: login, one controller at a time, encrypted links (D22, §5.18).
**Depends on** P48 · **Size** L (split candidate: TLS could become P49b) · **ADRs** 0031

**Scope** (`oscope-api`, `oscope-server`)
- **Accounts:**
  - argon2-hashed passwords;
  - `oscope-server account add|remove|passwd`.
- **Sessions:** login and logout, with expiry.
- **Authentication:** the U05 authenticator, applied to WebSockets too.
- **The control lease:** acquire, release, request, hand over, expire when idle, and the local desktop's override.
- **Authorisation:** an authoriser that requires the lease for every affordance that changes state.
- **The local desktop token.**
- **Network listening** requires at least one account, and TLS (rustls) with a generated or supplied certificate, through U03's listener.

**Example** — `ex-two-clients`: two pages. One holds control; the other watches and asks for control.

**Tests**
- A test generated from the Thing Descriptions: every affordance that changes state refuses a caller without the lease.
- Session expiry, lease handover, token file permissions, and the TLS handshake.

<a id="p50"></a>
### P50: Instrument status and health

**Goal.** The data behind the instrument status indicator (§5.19).
**Depends on** P49 · **Size** M · **ADRs** —

**Scope**
- **Health reporters** for each subsystem.
- **GPU statistics** through NVML, loaded at run time.
- **Disk monitors:** free space and recent write rate.
- **Roll-up rules.**
- **The `status` Thing,** and its events.
- **Fault injection** in the simulator: unplug the camera, raise a stage alarm, fill the scratch disk.

**Example** — `ex-status`: a page with the status indicator, and buttons that inject faults.

**Tests**
- The roll-up rules, and each injected fault reported correctly.

<a id="p51"></a>
### P51: Power actions

**Goal.** Restart the service, reboot or shut down the host, safely (§5.19).
**Depends on** P50 · **Size** S · **ADRs** 0032

**Scope** (the `power` Thing)
- **Restart** exits with code 75.
- **Reboot and shutdown:** `shutdown.exe` on Windows; on Linux, `systemctl` with the polkit rule installed by the packaging.
- **Safeguards:** the lease, a confirmation, and refusal during a hardware job unless forced.
- **A dry-run mode.**

**Example** — `ex-power`, in dry-run mode.

**Tests**
- The safeguards, and the commands built on each platform (dry run).

<a id="p52"></a>
### P52: Settings and configuration migration

**Goal.** One place for system settings, with old configurations upgraded safely.
**Depends on** P51 · **Size** M · **ADRs** —

**Scope**
- **The `settings` Thing:**
  - system: data and scratch roots, network, ports, defaults;
  - display preferences for each account;
  - simulator settings.
- **Configuration versioning,** with migration and a backup.
- **Export and import** of settings.

**Example** — `ex-settings`.

**Tests**
- Migration from v1 fixtures, and validation.

<a id="p53"></a>
### P53: Performance budgets and nightly soak

**Goal.** Measure the §5.20 budgets, and catch leaks and regressions before they reach users.
**Depends on** P52 · **Size** M · **ADRs** —

**Scope**
- **`cargo xtask perf`:** runs the benchmarks on the owner's machine and writes an HTML report against the budgets.
- **A nightly CI soak** on the CPU with the scaled sensor:
  - server, simulator, a preview client, scans and stitching, for hours;
  - it checks memory, handles and frame drops.

**Example** — the performance report.

**Tests**
- The soak's leak detector, with a mutation check (an injected leak fails it).

<a id="p54"></a>
### P54: Packaging, release workflow, M1 retrospective

**Goal.** Installable M1 builds and a look back.
**Depends on** P53 · **Size** M · **ADRs** 0033

**Scope**
- **Windows:** a zip with service install and uninstall scripts. The Tauri installer replaces it in M2.
- **Ubuntu:** a .deb with a systemd unit, the polkit rule, a user and group, and its directories.
- **Third-party notices** (cargo-about), a release workflow on tags, and the version embedded in the build.
- **`docs/milestone-1/retrospective.md`.**

**Example** — install the .deb in a clean Ubuntu container and run the smoke test.

**Done when**
- A tagged release publishes both packages, and the retrospective is merged.
