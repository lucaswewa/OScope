# OScope implementation plan

| | |
|---|---|
| Status | Approved 2026-11-10; a living document, updated as phases land (Appendix C) |
| Covers | The whole product: architecture for milestones 1–3, Milestones 1 and 2 phase by phase, and an outline of Milestone 3 |
| Inputs | [`goal.md`](goal.md), the planning Q&A (recorded in [§3](#3-decisions-made-while-planning)), a study of `teta-wot` v0.1.0, and of `microscope` (ideas and look only, [D1](#3-decisions-made-while-planning)) |
| Companions | [milestone-1/phases.md](milestone-1/phases.md) and [milestone-2/phases.md](milestone-2/phases.md): the specification of every phase of Milestones 1 and 2 |

## Contents

1. [OScope in one page](#1-oscope-in-one-page)
2. [In and out of scope](#2-in-and-out-of-scope)
3. [Decisions made while planning](#3-decisions-made-while-planning)
4. [Ground rules](#4-ground-rules)
5. [Architecture](#5-architecture)
6. [How phases work](#6-how-phases-work)
7. [Milestone 1 phase overview](#7-milestone-1-phase-overview)
8. [Milestone 2 phase overview](#8-milestone-2-phase-overview)
9. [Milestone 3 outline: real hardware and performance](#9-milestone-3-outline-real-hardware-and-performance)
10. [Planned ADRs](#10-planned-adrs)
11. [Documentation conventions](#11-documentation-conventions)
12. [Risks](#12-risks)
13. [Points for your review](#13-points-for-your-review)
14. [Appendices](#14-appendices)

---

## 1. OScope in one page

OScope (OpenScope) is the control and analysis software for a motorised brightfield microscope:

- **Camera:** Tucsen MIChrome 20, a 20 MP colour camera (5472 × 3648, 2.4 µm pixels, rolling shutter, 14.5 fps at full resolution in 8 bits and 7.2 fps in 16), on USB with no trigger input. A camera with an external trigger is to replace it.
- **Stage:** an XYZ stage on a grblHAL controller, with homing switches, linear encoders, and an aux output for a camera trigger.
- **Objectives:** Mitutoyo long-working-distance objectives (5×, 10×, 20×, 50×) in a single mount, swapped by hand.
- **Light:** transmitted and reflected sources, not chosen yet; simulated until they are.

The backend is a layered Rust service. It serves the instrument over HTTP through [`teta-wot`](https://github.com/lucaswewa/teta-wot) and keeps every pristine 16-bit raw frame in Rust memory or on an NVMe scratch disk. It sends the UI only light, viewport-sized JPEG previews. Image work runs as a graph of processing nodes, on a CUDA GPU when one is present and on the CPU otherwise. Third parties extend it with `.omext` packages, written in Rust or Python.

**The three milestones**

| Milestone | Delivers | Hardware |
|---|---|---|
| **M1: backend** | The whole Rust backend, with a runnable example in every phase: live preview, jobs, pipelines and extensions, projects in OME-Zarr, calibration, autofocus, scans with live and post-acquisition stitching, deep zoom, depth from focus, volumes, deconvolution, analysis and reports, authentication, status and power | Simulated camera, stage and lights, with 3D procedural samples |
| **M2: frontend** | The Vue 3 app, shared by the web build and a Tauri 2 desktop build, with every tab in goal.md, a node-graph editor, OpenSeadragon deep zoom, VTK.js 3D, and the desktop app's local fast path | Simulated |
| **M3: hardware** | The grblHAL and Tucsen drivers, a light driver, continuous on-the-fly scanning with hardware triggers, calibration and tuning on the real instrument, performance work, release 1.0 | Real |

At the end of Milestone 1:

- **`oscope-server`** runs as a console program, a Windows service or a systemd unit, on Windows 11 and Ubuntu LTS. It serves:
  - **a control plane:** teta-wot Things for the camera, stage, lights, objective, jobs, projects, pipelines, scans and so on;
  - **a media plane:** binary preview WebSockets, deep-zoom tiles, volume bricks and downloads.
- **The simulator** stands in for every device. Its samples are true 3D: H&E tissue, a reflective surface, fluorescent cell volumes and sub-diffraction structures. Depth from focus, volumes and deconvolution therefore have ground truth to check against. The simulator has its own clock: a move takes the time its distance and speed imply, and the live image slides across the view as it would on the real instrument.
- **Every phase** ships `cargo run` examples. Where a picture helps, an example serves one plain HTML page: live view, overlays, a live mosaic, deep zoom, or a VTK.js surface or volume.

## 2. In and out of scope

**In scope (all milestones)**

- Everything in [goal.md](goal.md), as interpreted by the decisions in [§3](#3-decisions-made-while-planning).
- Upstream changes to `teta-wot` that OScope needs, released as `teta-wot` v0.2.0 before OScope depends on them (D27).

**Out of scope, or deferred**

| Item | Where |
|---|---|
| Fluorescence on the real instrument | Not planned: the hardware has no fluorescence. Fluorescent modalities exist in the simulator only, and imported data can be processed |
| True light-field (microlens) microscopy | Not planned (D8) |
| SRRF, SMLM and other super-resolution methods beyond deconvolution | Extensions (D24) |
| Motorised or coded objective turrets | Later. The `ObjectiveMount` interface leaves room for one (D18) |
| User roles, an audit trail, regulated-lab compliance (21 CFR Part 11 and similar) | Later. M1 has named accounts and a control lease (D22) |
| Frontend code loaded from extensions at run time | Not planned. Extensions describe panels as data, and the app renders them (D11) |
| Linux ARM64 (Raspberry Pi, Jetson), macOS | Not planned (D26) |
| H.264 or WebP previews | Later, only if remote bandwidth demands it (D6) |
| Continuous on-the-fly scanning | Designed in M1, built in M3 (D14) |
| Non-English UI | Later. The UI is i18n-ready from the start (D30) |

## 3. Decisions made while planning

These came from the planning Q&A on 2026-11-10 (D1–D29) and from the owner's review of the first draft (D30–D35). Each is recorded as an ADR, or in the phase named ([§10](#10-planned-adrs)).

| # | Topic | Decision | Recorded in |
|---|---|---|---|
| D1 | Relation to `microscope` | **Clean room:** a new repository, with all code written fresh. Its ideas may be reused where they fit: hash routing, a host adapter, a sidecar contract, generated API types, its retrospective's lessons. So may its look: palette, magenta accent, compact density, the rail → control pane → view layout. Nothing is copied file by file | ADR-0002 (P00) |
| D2 | Plan depth | Architecture for all three milestones; phase specs for M1 only. M2 and M3 are specified in detail as M1 nears its end | This plan |
| D3 | Delivery process | As in `microscope`: one branch and PR per phase, merged only after the owner approves, ADRs and notes in every PR, S/M/L line budgets ([§6](#6-how-phases-work)) | ADR-0001 (P00) |
| D4 | M1 examples | Runnable `cargo run -p ex-…` examples in every phase. Where a picture helps, the example serves one plain HTML/JS page; libraries (OpenSeadragon, VTK.js) are vendored only where needed. No Vue before M2 | P01, P04 |
| D5 | Raw frames | The sensor's Bayer mosaic in 16-bit words, with its significant bit count (12) in the metadata. Demosaicing and colour are pipeline nodes, so raw stays reversible | ADR-0008 (P03) |
| D6 | Live preview | JPEG over a binary WebSocket, sized to each client's viewport, or a full-resolution ROI crop when zoomed in. When a client falls behind, it gets the newest frame and older ones are dropped. The preview never runs faster than the camera | ADR-0014 (P17) |
| D7 | GPU | CUDA through `cudarc`. A CPU path (rayon + SIMD) is the reference that every CUDA kernel is tested against, and the fallback when no NVIDIA GPU is present | ADR-0010 (P06), ADR-0011 (P07) |
| D8 | "Light field and confocal" | Brightfield/widefield z-stacks rendered as volumes, and confocal-style (optically sectioned) fluorescence stacks from the simulator or imported data. VTK.js renders both | ADR-0029 (P41) |
| D9 | Rust extensions | In-process native libraries (`cdylib`) behind a versioned C ABI, which an SDK crate hides behind safe traits. Extensions are trusted code | ADR-0019 (P25) |
| D10 | Python extensions | One worker process per extension, each with its own uv-managed virtual environment. Frames are shared as zero-copy NumPy arrays through shared memory | ADR-0020 (P26) |
| D11 | What extensions may add | Processing nodes, hardware drivers, scan workflows, server-described UI panels, importers, exporters and report generators. Extensions that add Things (drivers, workflows) load at start-up. Each kind arrives with the feature it extends | ADR-0018 (P24); P27, P30, P36, P47 |
| D12 | Pipelines | A DAG engine with typed ports and JSON recipes, run per frame (live) or in batch (datasets, tiled). M2's Workspace edits recipes in a node-graph editor | ADR-0017 (P21) |
| D13 | Live stitching | Both free-roam (a mosaic builds up as you jog by hand) and in scans (tiles placed as they arrive) | ADR-0026 (P38) |
| D14 | Scan motion | Stop-and-go in M1. The frame and position model is designed for continuous scanning, which is built in M3 with hardware triggers | ADR-0025 (P36) |
| D15 | Raw dataset format | OME-Zarr v3 (OME-NGFF), through the `zarrs` crate | ADR-0021 (P28) |
| D16 | Projects | A project (one slide or experiment) holds acquisitions (raw), results (derived, with provenance) and reports. The Gallery shows projects as cards | ADR-0021 (P28) |
| D17 | Hardware in hand | The camera, the Tucsen SDK, the grblHAL XYZ stage and a separate NVMe scratch disk are all available now | M3 hardware.md |
| D18 | Objectives | A single mount, swapped by hand. The user says which objective is fitted, and each objective has its own calibration | ADR-0012 (P09), ADR-0023 (P32) |
| D19 | Lights | Transmitted brightfield and reflected/coaxial light. The controller isn't decided yet: M1 has an `Illumination` interface with a manual implementation, and the real driver comes in M3 | ADR-0012 (P09) |
| D20 | grblHAL features | Homing switches, linear encoders, and an aux output for the camera's trigger input. The MIChrome 20 has none, so the trigger waits for the camera that replaces it (P02b) | ADR-0012 (P09), M3 |
| D21 | Desktop app | The backend stays a separate service (sidecar). The Tauri app gets a local fast path: preview frames through shared memory, and tiles read from disk through a custom protocol, with no HTTP and no re-encoding | ADR-0015 (P19) |
| D22 | Remote access | Login with a password; one client holds the control lease, and the others view read-only and may ask for control. The local desktop app is trusted | ADR-0031 (P49) |
| D23 | Simulated samples | Procedural 3D samples in the style of goal.md's three reference images (pathology, confocal, super-resolution), plus a reflective surface. Deterministic from a seed. Your own images or stacks can be loaded at run time. Tucsen's pictures are never committed | ADR-0013 (P10) |
| D24 | Super-resolution | A sample with sub-diffraction ground truth, plus stock Richardson–Lucy deconvolution (CPU and CUDA). Other super-resolution methods are left to extensions | P14, P43 |
| D25 | Workspace analysis | All stock: cell detection and counting, measurements and annotations, stain analysis (H&E/IHC), and reports | P44–P48 |
| D26 | Platforms | Windows 11 and Ubuntu 22.04/24.04 LTS, both x86_64. A Windows service and installer; a .deb with a systemd unit | ADR-0003 (P00) |
| D27 | `teta-wot` | Extended upstream first: Linux CI, path parameters, router composition, binary WebSocket streams and pluggable authentication. Released as v0.2.0 in an up-front Stage 0; OScope pins that tag. Jobs stay in OScope | ADR-0004 (P00) |
| D28 | Jobs | Hardware jobs run one at a time from a queue; compute jobs run in parallel on a worker pool that shares the GPU. Every job has progress, logs and cancel, and its history is kept in SQLite | ADR-0016 (P20) |
| D29 | UI language | English, with every string going through vue-i18n from the start | M2 |
| D30 | Simulated time and motion | The simulator has its own clock, shared by every simulated device. A move takes its path length ÷ the set speed, plus acceleration ramps when they're switched on. The live image moves at the matching speed and direction, through the simulator's ground-truth camera mounting, which the camera-to-stage calibration then recovers. At 1× the clock follows the host's; tests can step it or run it faster | ADR-0013 (P10); P11, P12 |
| D31 | Hardware survey | A docs-only phase, P02b, records what the real camera, SDK, stage controller, lights and host offer before the frame model is fixed | P02b |
| D32 | Access without the lease | A logged-in client without the lease can't change any state: it can only watch and read | ADR-0031 (P49) |
| D33 | Deep-zoom speed | Tiles on demand, with chunks aligned to tiles, a decompressed-chunk cache, coarse levels rendered eagerly, low priority during scans, and HTTP/2 over TLS for remote browsers ([§5.15](#515-stitching)) | ADR-0028 (P40), U03 |
| D34 | Default port | 8210 | P04 |
| D35 | CI | Windows and Ubuntu on every PR | ADR-0003 (P00), P01 |

## 4. Ground rules

### 4.1 Clean room

- **No code, styles, tests or text are copied** from `microscope`, nor ported line by line. The look (colours, density, layout) and the ideas listed in D1 may be reused, described in our own words. Every PR has a "clean room" checkbox (P00).
- **Algorithms are written from published methods.** Phase correlation, Malvar–He–Cutler demosaicing, Richardson–Lucy, colour deconvolution (Ruifrok–Johnston), focus measures and watershed come from the literature, and each spec-first phase cites its sources in its notes.
- **Tucsen's sample images** are never committed (D23). The Tucsen SDK is never redistributed unless its licence allows it; until confirmed, users install it.

### 4.2 Dependencies

- **Licences** must allow MIT distribution: MIT, Apache-2.0, BSD, ISC, Zlib, Unicode, or public domain. MPL-2.0 is allowed only as an unmodified development dependency.
  - Candidates checked by name: `cudarc`, `zarrs`, `rusqlite` with bundled SQLite, `rayon`, `rustfft`, `jpeg-encoder`, `typst`, `minijinja`, `argon2`, `nvml-wrapper`, `libloading`, `zip`, OpenSeadragon, VTK.js, Vue Flow, vue-i18n and uv.
  - Each phase confirms the licences it adds; `cargo-deny` enforces them from P01.
- **Only versions at least two weeks old** (`npm install --before=…`; for crates, a release date checked in the phase).
- **CUDA libraries** load at run time. The binary runs, and falls back to the CPU, without them. P07 records which CUDA runtime libraries the installers may ship under NVIDIA's EULA.

### 4.3 Unsafe code

`unsafe_code = "forbid"` is the workspace default. These crates may use it, each with a `SAFETY:` comment on every block:

- `oscope-compute`: CUDA kernel launches;
- `oscope-shm`: shared memory;
- `oscope-ext`: loading native libraries;
- `oscope-ext-sdk`: the generated C ABI glue;
- `oscope-tucsen` (M3): the SDK's C API.

### 4.4 Small, self-contained phases with examples

Each phase does one job end to end: code, tests, an example and docs. It must be reviewable in one sitting ([§6](#6-how-phases-work)).

## 5. Architecture

### 5.1 Context

```text
                ┌──────────────── OScope host: Windows 11 or Ubuntu LTS, x86_64 ────────────────┐
 Remote browser │ oscope-server (service)                                                         │
 (Vue app, M2) ─┼─▶ HTTP/WS ─┬─ control plane: teta-wot Things (TDs, properties, actions, events)   │
                │            └─ media plane: preview WS, DZI tiles, volume bricks, data downloads   │
 Desktop app    │                                                                                 │
 (Tauri 2, M2) ─┼─▶ the same HTTP/WS, plus the local fast path: shared-memory preview ring and     │
                │   tile cache read straight from disk                                            │
                │ devices: camera (sim | Tucsen) · stage (sim | grblHAL) · lights · objective     │
                │ disks:   NVMe scratch (frame logs, caches) · data disk (projects in OME-Zarr)    │
                │ GPU:     CUDA if present, else CPU  ·  Python extension workers (shared memory)  │
                └─────────────────────────────────────────────────────────────────────────────────┘
```

### 5.2 Layers and crates

Each crate depends only on crates in lower layers. A test in P01 reads `cargo metadata` and fails on any upward or sideways edge the table doesn't allow.

| Layer | Crate | What it holds |
|---|---|---|
| L0 Foundation | `oscope-core` | Units (µm, ms), geometry (rects, 2D affines), the monotonic clock and timestamps, IDs, the frame and metadata types, errors. No I/O, no async |
| L1 Compute | `oscope-compute` | CPU and CUDA backends, device buffers, pinned host memory, streams, the kernel build (unsafe allowed) |
| | `oscope-shm` | Named shared memory and the frame-ring layout, on Windows and Linux (unsafe allowed) |
| L1 Algorithms | `oscope-imaging` | Pure image algorithms with CPU and CUDA paths: Bayer and colour, filters, FFT and registration, focus measures, morphology, segmentation, stain separation |
| | `oscope-stitch` | Placement, the registration graph and its global solve, blending, multiscale pyramids |
| | `oscope-volume` | Stacks, depth from focus, extended depth of field, deconvolution, bricking for the client |
| L2 Devices | `oscope-hal` | Device traits (camera, stage, lights, objective mount), capabilities, and test fakes |
| | `oscope-sim` | The simulated world: samples, optics, camera, stage and lights |
| | `oscope-grbl` (M3) | The grblHAL driver and its protocol fake |
| | `oscope-tucsen` (M3) | The Tucsen SDK binding and camera driver (unsafe allowed) |
| L2 Data | `oscope-store` | Projects, OME-Zarr stores, the scratch frame log, the SQLite index, the tile cache |
| L3 Engines | `oscope-jobs` | The job service |
| | `oscope-pipeline` | The DAG engine, node registry and stock nodes |
| | `oscope-ext` | `.omext` packages, the native loader, the Python worker supervisor (unsafe allowed) |
| | `oscope-ext-sdk` | The SDK that Rust extension authors build against (unsafe allowed in its generated glue) |
| | `oscope-report` | Reports: their model, Typst compiled in process to PDF, minijinja to HTML, SVG charts and the templates (P47, ADR-0030) |
| L4 Services | `oscope-acquire` | The frame bus, preview, live statistics, recorder, live mosaic and shared-memory publisher |
| | `oscope-workflows` | Autofocus, exposure, calibration routines, scan planners and engine, analysis runners |
| L5 Interface | `oscope-api` | teta-wot Things, media-plane routes, authentication and the control lease. **The only crate that depends on `teta-wot`** |
| L6 Applications | `oscope-server` | The binary: configuration, service mode, the sidecar contract |
| | `desktop/` (M2) | The Tauri 2 app |

Python authors use `python/oscope-ext/`, which provides the `oscope_ext` package.

### 5.3 Repository layout

```text
OScope/
├─ Cargo.toml                 workspace: resolver 3, edition 2024, lints
├─ rust-toolchain.toml        pinned to teta-wot v0.2.0's toolchain
├─ crates/oscope-*/           the crates in §5.2
├─ examples/                  one small crate per example (ex-live-view, ex-stitch, …); each has a README
│  ├─ common/                 a helper that serves an example's plain HTML page next to its API
│  └─ vendor/                 vendored JS (OpenSeadragon, VTK.js) with their licences
├─ python/oscope-ext/         the Python extension SDK
├─ configs/                   sample configurations (simulation.toml, …)
├─ web/                       M2: the Vue 3 app
├─ desktop/                   M2: the Tauri 2 app
└─ docs/                      §11
```

### 5.4 Control plane and media plane

**The control plane** is teta-wot Things: Thing Descriptions, properties, actions and events. It's typed, self-describing and turned into OpenAPI, from which M2 generates TypeScript types. An action that starts long work returns a `JobRef` at once; the work then runs in the job service (§5.11), so its lifetime doesn't depend on teta-wot's in-memory invocation history.

| Thing | Holds |
|---|---|
| `system` | Version, platform, GPU, disks, licences |
| `logs` | Log records, filters, live tail |
| `status` | Health of each subsystem, rolled up into one instrument status |
| `power` | Restart the service; reboot or shut down the host |
| `settings` | System, display defaults, data and scratch locations |
| `auth` | Accounts, sessions, the control lease |
| `camera` | Exposure, gain, pixel format, binning, ROI, mode, trigger, streaming |
| `stage` | Position, moving, move, jog, stop, home, limits, capabilities |
| `illumination` | Channels (transmitted, reflected), intensity, on/off; manual or driven |
| `objective` | The catalogue, which objective is fitted, and a plausibility check |
| `simulation` | Samples (choose, load, unload, import) and simulated imperfections |
| `preview` | Defaults for the preview and the live recipe, the sessions open, and the stream itself, a binary stream (ADR-0014) |
| `live_mosaic` | Free-roam start, stop and clear, plus the mosaic's extent |
| `calibration` | Per-objective calibration sets, routines and the wizard's steps |
| `autofocus`, `exposure` | Autofocus, auto exposure and auto white balance (each a job) |
| `capture` | Single captures and bursts (jobs) |
| `scan` | Planners, plan preview, start (a job), live progress |
| `jobs` | Queue, history, progress, cancel, pause, reorder |
| `projects` | The Gallery: projects, acquisitions, results, thumbnails, metadata |
| `pipeline` | Node catalogue, recipes (CRUD), run (a job) |
| `workspace` | Analyses on datasets as jobs; results with provenance, made again and compared; annotations and measurements (P48). Reports are the `projects` Thing's (P47) |
| `extensions` | Installed packages; install, enable, disable |

**The media plane** is OScope's own axum routes, mounted beside the Things through teta-wot v0.2.0's router composition (U03), so they get the same authentication, CORS and shutdown handling.

| Route | Carries |
|---|---|
| `GET /api/v1/preview/stream` (WebSocket; the `preview` Thing's binary stream, ADR-0014) | Binary JPEG frames, and binary masks from P18; JSON messages from the client for the viewport, ROI, quality, rate and acknowledgements |
| `GET /api/v1/media/dzi/{dataset}.dzi`, `…/{dataset}_files/{level}/{col}_{row}.jpg` | Deep-zoom tiles, made on demand from OME-Zarr multiscales and cached on disk |
| `GET /api/v1/media/mosaic/live/…` | The live mosaic's tiles |
| `GET /api/v1/media/volume/{dataset}/meta`, `/slice`, `/brick` | Volume metadata, orthogonal slices and bricks (binary, with a small header) for VTK.js |
| `GET /api/v1/data/{project}/{path}` | Downloads, with Range support |
| `POST /api/v1/upload/{name}` | A client's file, staged in the scratch space for a job: an image or a stack to import, a sample (W6.3, ADR-0051). `.omext` packages go to `extensions/install` (P24) |

### 5.5 Frames, the clock, and the two streams

**The frame model** (ADR-0008):

- `PixelFormat` covers `Bayer16 { pattern, significant_bits }`, Bayer8, Mono8/16, Rgb8/16, Rgba8 and planar F32.
- A `Frame` is a pooled buffer plus `FrameMeta`:
  - sequence number;
  - the host timestamp, and the sensor's when the camera gives one (the MIChrome 20 doesn't); the exposure's start, end and midpoint;
  - for a rolling shutter, the line time, which places each row's exposure in time;
  - exposure, gain, mode, binning, ROI and black level;
  - the objective, and a snapshot of the lights;
  - the stage position interpolated at the exposure's midpoint;
  - the count of dropped frames.
- **Buffers come from a pool**, in pinned memory when CUDA is present, and frames are shared by reference count. One 20 MP Bayer16 frame is 40 MB. The default pool of 24 frames (about 1 GB) can be configured.

**One clock.**

- Every timestamp is OScope's monotonic clock, in nanoseconds since start-up, mapped once to UTC.
- The stage's position reports are timestamped when they arrive, after subtracting an estimated latency, and kept in a position track.
- A frame's position is interpolated from the track at its exposure midpoint. In trigger mode (M3), the stage fires the camera at known positions instead.

**The frame bus** fans each frame out to subscribers of two kinds:

- **lossless** subscribers, which get every frame and apply backpressure, such as the recorder;
- **latest-wins** subscribers, which drop frames when they fall behind, such as the preview, statistics and the live mosaic.

```text
camera ─▶ frame pool (Bayer16) ─▶ frame bus ─┬─▶ recorder (lossless) ─▶ scratch frame log ─▶ ingest job ─▶ OME-Zarr in the project
                                             ├─▶ preview (latest-wins) ─▶ ROI crop / bin + demosaic ─▶ display transform ─▶ JPEG ─▶ WS per client
                                             │                                                                              └─▶ shm ring (desktop)
                                             ├─▶ live statistics (≤ 10 Hz) ─▶ 12-bit histogram, saturation mask, profile, ROI stats, focus value
                                             ├─▶ live mosaic (latest-wins) ─▶ placed by position ─▶ multiscale canvas ─▶ dirty-tile events
                                             └─▶ workflows: "the first frame exposed after t" (autofocus, scans, calibration)
```

**Raw stream.** Raw frames never leave Rust except through an explicit download or export.

- During acquisition, the recorder appends frames to a frame log on the scratch NVMe: preallocated, sequential and uncompressed, with an index.
- The design rate is 580 MB/s: 16-bit full frames at 14.5 fps (§5.20). The MIChrome 20 delivers 289 MB/s (`hardware.md`), so the budget is headroom for the camera that replaces it. Logging never compresses on the hot path.
- An ingest job then writes the frames into the project's OME-Zarr store, compressed with zstd.

**Preview stream** (ADR-0014):

- **What each client gets.** Each client asks for a viewport size, and when zoomed in, for an ROI.
  - The server crops the raw ROI or bins it (a 2 × 2 Bayer superpixel at small scales).
  - It then demosaics, resizes and applies the display transform: black and white levels, white balance, gamma, and an optional live recipe.
  - It encodes JPEG with SIMD and sends the result.
- **Falling behind.** Each client has a one-slot mailbox, so a slow client always gets the newest frame. A client that acknowledges what it has shown has at most two frames it hasn't shown, so no socket buffer holds stale frames for it (ADR-0014).
- **Adapting to the link.** If the link is slow, the server lowers the size or quality.
- **Rate.** The preview rate is the camera's rate, never more: on the MIChrome 20, 14.5 fps at full resolution in 8 bits and 7.2 fps in 16, and more in its binned modes. Its ROIs don't raise the rate (`hardware.md`).
  - At 20× and 50× the optics, not the sensor, limit resolution, so 2 × 2 binning loses little.
  - At 50×, a pixel is 0.048 µm and the diffraction limit about 0.6 µm.

**Live statistics** are computed from raw data, not from the 8-bit preview:

- a 12-bit histogram per channel;
- a saturation mask at preview resolution;
- line profiles;
- ROI statistics;
- a focus value.

The client draws the crosshair, ROI, profile, false colour, saturation and histogram from these.

### 5.6 Compute

**Backends** (ADR-0010, ADR-0011):

- `oscope-compute` has two backends: `Cpu` (rayon, with SIMD where it pays) and `Cuda` (`cudarc`, with the driver, cuFFT and NVRTC loaded at run time).
- Kernels are CUDA C, compiled to PTX by `build.rs` when the `cuda` feature is on and `nvcc` is found.
- With no GPU, no driver, or a build without CUDA, everything runs on the CPU.

**Every CUDA kernel has a CPU twin.** Equivalence tests compare them within a stated tolerance. CI has no GPU, so:

- CPU tests run everywhere;
- CUDA tests run when `OSCOPE_CUDA=1`, on the owner's machine;
- a weekly CI job builds the CUDA feature without running it.

**Keeping data on the GPU.** The pipeline engine keeps data on the GPU between consecutive CUDA nodes, and copies it back only at the boundaries.

### 5.7 Processing pipeline

**Nodes and recipes** (ADR-0017):

- A node declares an ID, a version, a category, typed input and output ports, its parameters as a JSON Schema, and its capabilities: CPU and/or CUDA, live-capable, tileable, and its halo in pixels.
- Port types are Image, Stack/Volume, Mask, Table, Geometry (GeoJSON), Scalar/JSON and Report fragment.
- A recipe is JSON: nodes, edges and parameters. It's validated for types and cycles before it runs.

**Modes of execution**

- **Live:** per frame, within a latency budget, using live-capable nodes only. Late frames are dropped.
- **Batch:** a compute job over a dataset, with progress. Large mosaics run tile by tile, each tile read with its halo, so a 100 k × 100 k slide never sits in memory.

**Scheduling and caching.** The scheduler orders nodes topologically, runs independent branches in parallel, and places data on the CPU or GPU. Intermediate results are memoised by a hash of their inputs and parameters, so editing one node in M2's graph editor re-runs only what changed.

**Stock nodes** arrive across M1 (P22–P23, P30–P48): correction and display, filters, morphology, registration, depth from focus, deconvolution, segmentation and measurement, stain separation, exporters. Extensions add more.

### 5.8 Extensions (`.omext`)

**The package** (ADR-0018). An `.omext` is a zip file with a `manifest.toml` that gives:

- an ID (reverse DNS), name, version and licence;
- `oscope-api` (a semver range);
- `kind = "rust" | "python"`;
- `provides`: nodes, drivers, workflows, panels, importers, exporters and reports;
- for Rust, a native library per platform (`windows-x86_64`, `linux-x86_64`);
- for Python, `requires-python`, the dependencies and an entry point;
- assets: panel schemas and report templates.

**Lifecycle.**

- Installing unpacks the package to `<data>/extensions/<id>/<version>/` and checks compatibility; enabling and disabling take effect at the next service start.
- All extensions load at start-up, because teta-wot fixes its Things when the server is built.
- Extensions are trusted code. Installing one needs the control lease, and the UI shows its publisher and hash.

**Rust extensions** (D9, ADR-0019):

- Authors implement safe traits from `oscope-ext-sdk` (`ProcessingNode`, `CameraDriver`, `StageDriver`, `ScanWorkflow`, `Importer`, `Exporter`, `ReportGenerator`). An `export!` macro generates a versioned C ABI from them: tables of `extern "C"` functions with `#[repr(C)]` frame views.
- Panics are caught at the boundary and become errors. A segfault still ends the process, which the service manager restarts.

**Python extensions** (D10, ADR-0020):

- **Environment.** Each extension gets its own virtual environment, created by uv at install time with a uv-managed Python 3.12 or later. Wheels can be installed offline.
- **Worker.** A supervisor starts the worker on first use, keeps it warm, restarts it if it crashes, and stops it on a timeout.
- **Messages and data.** Control messages are length-prefixed JSON over stdin/stdout. Frames are passed as names of shared-memory segments, which the `oscope_ext` package wraps as zero-copy NumPy arrays.
- **Cost.** About 1 ms per call, so Python nodes suit batch work. They can run live at low rates.

**Panels** are server-described: JSON Schema forms and result views that the M2 app renders. Extensions never ship JavaScript.

### 5.9 Hardware abstraction

`oscope-hal` (ADR-0012) defines async device traits, each of which reports its **capabilities**:

| Device | Operations | Capabilities |
|---|---|---|
| **Camera** | Modes (resolution, binning, ROI, frame-rate table), exposure, gain, pixel format, start and stop streaming, software and hardware trigger | Sensor timestamps, external trigger (input modes, and an output), rolling or global shutter and its line time, temperature, the pixel formats it offers |
| **Stage** | Position in µm (f64), absolute and relative moves, continuous jog, stop, home, soft limits, a moving flag | Homing, encoders, a trigger output, position-report rate |
| **Illumination** | Channels (transmitted, reflected, and any others a controller offers), intensity, on/off | The lights aren't chosen yet (P02b): the simulator's lights, and a manual implementation that records what the user set |
| **ObjectiveMount** | Which objective is fitted, from a catalogue | Manual in M1 (D18) |

**Objective catalogue.** Its defaults are Mitutoyo's published values, confirmed in M3:

| Objective | NA | Working distance |
|---|---|---|
| 5× | 0.14 | 34 mm |
| 10× | 0.28 | 33.5 mm |
| 20× | 0.42 | 20 mm |
| 50× | 0.55 | 13 mm |

With a 200 mm tube lens, a pixel at the sample is 0.48, 0.24, 0.12 and 0.048 µm.

**Drivers** run on device-actor threads. Each real driver has a fake at its protocol or SDK boundary for CI. Configuration picks the drivers, and extension drivers register at start-up.

### 5.10 Simulator

`oscope-sim` (ADR-0013) implements every HAL trait. It's deterministic from a seed, and takes time only from its own simulation clock.

**Samples are 3D.** Each is a set of layers or a volume, defined in µm.

| Sample | Light | Used for |
|---|---|---|
| H&E tissue | Transmitted brightfield, absorbing stain | Pathology, scans, stitching, stain analysis |
| Reflective surface | Reflected light, a height map with texture | Depth from focus, surfaces |
| Fluorescent cell volume | Simulated widefield or confocal fluorescence | Confocal-style stacks, volume rendering |
| Sub-diffraction structures (microtubule- and mitochondria-like) | Simulated fluorescence | Deconvolution against ground truth |
| Imported image or stack | Any | Your own data, kept out of git |

**Image formation.**

- Layers are blurred by a defocus PSF set by the NA and |z − z_layer|.
- Simulated confocal mode suppresses out-of-focus light.
- The sensor adds a Bayer mosaic, 12-bit quantisation, shot, read and dark noise, gain, exposure and saturation.
- Its modes have their own frame-rate table, a placeholder until M3 measures the real camera.

**Stage.** The simulated stage behaves like grblHAL: motion timed by the simulation clock (below), homing, limit switches, encoders (true versus commanded position), backlash, settling, and status reports at a set rate and latency. It also has a trigger output.

**The simulation clock and motion** (D30):

- **One clock** drives every simulated device, so the stage, camera and lights agree on what happened when. When the simulator is in use, OScope's own services take their time from the same clock (P02's `Clock`), so timestamps, position tracks and job timeouts stay consistent.
- **Rates:**
  - real time (1×, following the host's monotonic clock) is the default;
  - scaled (such as 10×) is for demos and long tests;
  - stepped means tests advance the clock explicitly, which makes them fully deterministic.
- **Moves take real time:**
  - a move of path length d at speed v takes d ÷ v;
  - with acceleration a switched on (the default, with grblHAL-like values), a move long enough to reach v takes d ÷ v + v ÷ a;
  - multi-axis moves follow the path at the set speed, as grblHAL's feed rate does;
  - jogs, focus moves and homing are timed the same way.
- **Position is a function of the clock,** never of how often it's read or how fast frames render.
- **The image follows the stage.**
  - The camera renders each frame at the stage's true positions during its exposure, blurred if the stage is moving.
  - It renders through a ground-truth camera mounting set in the simulator's configuration: rotation, flip and offset, with scale from the fitted objective.
  - So a jog at 1 mm/s under the 10× objective (0.24 µm per pixel) slides the image about 4,200 px/s, crossing the sensor's width in 1.3 s, in the direction the mounting dictates.
  - The camera-to-stage calibration (P32) measures that mounting, and tests check it, click-to-move and live stitching against the truth.
- **If rendering can't keep up,** the camera delivers fewer frames, each true to its moment, and counts the drops. Motion never slows to match rendering.

**Lights and slides.** Transmitted and reflected channels; slides can be loaded and unloaded.

**Performance.** Full-resolution rendering uses cached, pre-blurred levels of detail per z bucket, rayon, and an optional CUDA path. Tests use a scaled-down sensor.

### 5.11 Jobs

`oscope-jobs` (ADR-0016) holds every long-running piece of work.

**A job has:**

- an ID, kind and title;
- the account that submitted it;
- its parameters;
- a state: queued, running, paused, completed, failed, cancelled or interrupted;
- progress: a fraction, a stage text, counters and an ETA;
- its own captured log;
- references to its results;
- timestamps;
- a resource class.

**Resource classes.**

- **Hardware** jobs run one at a time from a FIFO queue that can be reordered, and own the hardware lock while they run. Manual moves are refused while one runs.
- **Compute** jobs run on a pool of N workers, with a semaphore guarding GPU memory.

**Persistence.** History is kept in SQLite. Jobs that a restart cut short are marked *interrupted*; resuming scans is a later feature.

The `jobs` Thing exposes the queue, and change events through teta-wot.

### 5.12 Data on disk

**Data root** (ADR-0021, ADR-0022). Configurable; the defaults are `%ProgramData%\OScope\` for the service, `/var/lib/oscope/` on Linux, and `./.oscope/` for development runs.

```text
<data_root>/
├─ projects/<project>/
│  ├─ project.json                 id, name, sample, tags, created, owner
│  ├─ acquisitions/<id>/raw.ome.zarr + acquisition.json (instrument, objective, calibration used, camera
│  │                                 settings, plan, per-frame positions and timestamps)
│  ├─ results/<id>/…  + result.json (provenance: recipe, node and extension versions, inputs, software version)
│  └─ reports/<id>/report.pdf, report.html
├─ calibrations/<objective>/<version>.json
├─ recipes/  extensions/  accounts/
└─ index.sqlite                    projects, acquisitions, results, jobs; rebuildable from the folders
<scratch_root>/                    frame logs, tile cache, temporary files; free space is watched
```

- **Metadata.** OME-NGFF 0.5 multiscales metadata keeps the stores readable in napari and Python. OScope's own JSON (versioned) holds everything else.
- **Thumbnails** are made when an acquisition or result is created.
- **Sizes to expect.** A 15 × 15 mm slide at 20× with 10 % overlap is about 950 tiles: roughly 38 GB raw before compression.

### 5.13 Calibration

Calibration (ADR-0023) is kept per objective, and per binning where it matters. Each set is versioned, with a date, the account, and the instrument state. Sets can be listed, activated, exported and imported.

| Calibration | How |
|---|---|
| Pixel size and the camera-to-stage affine (rotation, scale, flip) | Stage moves of known size, measured by registration |
| Stage | Backlash per axis, homing offsets, soft limits |
| Dark frame | Per exposure and gain range |
| Flat field | Per objective and light |
| White balance and colour-correction matrix | Per light |

**Objective plausibility check.** After a swap, a quick stage move compares the measured pixel size with the chosen objective's, and warns on a mismatch.

**The calibration wizard** is a state machine on the server: steps, prompts and actions. The M2 UI renders it, and the M1 example drives it from the command line.

### 5.14 Acquisition workflows

**Autofocus** (ADR-0024, spec-first):

- Focus measures are Brenner, Tenengrad and variance of Laplacian, on a binned ROI.
- A coarse sweep is followed by a fine sweep.
- A fast mode sweeps z continuously and matches frames to z through timestamps; this tests the clock model early.
- Reflected and transmitted light get different defaults.
- **Depth of field** sets the steps: about 31 µm at 5× and 1.9 µm at 50×.

**Auto exposure and white balance.** Auto exposure aims a histogram percentile at a target within the camera's limits; auto white balance follows. Both are jobs.

**Scans** (ADR-0025):

- **Planners:** raster, snake, z-scan, tiled z-stack, multi-position, and a focus map (focus at sample points, then a fitted surface).
- **Histo scan:** a fast binned overview pass, tissue detection, then full-resolution tiles over the tissue only.
- **Engine:** move, settle, focus (from the map, or by autofocus), capture (a frame or a stack), record, then place in the live mosaic.
- **Continuous scanning** (M3): the plan and frame model already carry exposure-midpoint positions and trigger positions.

### 5.15 Stitching

**Live** (ADR-0026):

- Each frame is placed at its exposure-midpoint position through the calibrated affine, into an in-memory multiscale canvas of 256-px tiles; the newest frame wins, or edges are feathered.
- Dirty tiles are announced on the preview WebSocket.
- **Free-roam:** the canvas takes binned frames from the live stream when the stage has moved far enough, the frame isn't motion-blurred, and it's sharp.
- **Scans:** tiles are placed as they're captured.

**Post-acquisition** (ADR-0027, spec-first):

1. Pairwise phase correlation over the overlaps (cuFFT or rustfft), searched near the expected offset, with a confidence score from the peak ratio.
2. A global weighted least-squares solve over the graph, rejecting outliers by iterative reweighting.
3. Optional flat-field estimation from the data, then feathered blending.
4. Out of core: the result goes into a multiscale OME-Zarr mosaic.
5. Z-stacks reuse the per-tile transforms.

**Deep zoom** (ADR-0028):

- DZI tiles (256 px) are made on demand from the multiscales, with the display transform applied, and cached on the scratch disk. Changing the levels needs no rebuild.
- **The cost of a tile made on demand:** read one OME-Zarr chunk, decompress it, apply the display transform, and encode JPEG. That's estimated at 1–3 ms for a cold tile, against well under 1 ms for a cached tile or a static file. P40 measures it.
- **Keeping viewing fast** (D33):
  - Zarr chunks are aligned to tiles: a 512-px chunk holds 2 × 2 tiles. Recently decompressed chunks stay in a memory cache, so a chunk's other tiles cost only the encode.
  - The coarse levels, which a whole-slide overview needs, are rendered into the cache when stitching finishes, so a slide always opens at once.
  - Tile work runs on the batch pool at low priority, with a thread cap while a hardware job runs, so viewing never starves acquisition.
  - Remote browsers over TLS use HTTP/2 (U03). Browsers allow only six HTTP/1.1 connections per server, which throttles tile loading; HTTP/2 multiplexes them.
- **Changing the display** (levels, gamma) makes tiles cold again, and the old ones are kept under their own cache key. For instant feedback while a slider moves, M2's viewer adjusts the cached 8-bit tiles in WebGL, then fetches exact tiles from the 16-bit data when the slider is released.
- An exporter writes a static DZI folder for sharing.
- The desktop app reads cached tiles straight from disk (§5.18).

### 5.16 3D

**Volumes** (ADR-0029):

- Z-stacks are stored as multiscale OME-Zarr with their real, anisotropic voxel size.
- The media plane serves metadata, orthogonal slices, and bricks at a level that fits the client's GPU (about 512³ by default).
- VTK.js renders them as MIP, composite with transfer functions, or isosurfaces.

**Depth from focus:**

- A focus measure is computed for each pixel through the stack.
- A parabolic fit around the peak gives a sub-step height map in µm, with a confidence map.
- The same peak picks each pixel's in-focus colour for an extended depth of field (all-in-focus) image.
- A surface mesh can be exported (PLY, STL or OBJ).
- VTK.js draws the height map as a surface textured with the all-in-focus image.

**Deconvolution:** Richardson–Lucy with a theoretical PSF from the NA, wavelength, pixel size and z step, on CPU or cuFFT. It's checked against the simulator's ground truth.

### 5.17 Analysis and reports

| Area | Detail |
|---|---|
| **Annotations** | Points, lines, polygons and ROIs in µm (GeoJSON), on mosaics and volumes |
| **Measurements** | Length, area, intensity, in calibrated units |
| **Cell detection and counting** | Threshold or Otsu, distance transform, watershed, connected components, per-object measurements, tables (CSV). Deep-learning models come through Python extensions |
| **Stain analysis** | Colour deconvolution (H&E, H-DAB), DAB positive-pixel quantification |
| **Reports** | Templates in Typst, giving PDF; HTML through minijinja. A report generator interface, which extensions can implement |

Results carry their provenance.

### 5.18 Security, control lease, desktop fast path

**Authentication** (ADR-0031):

- **Accounts.** Named accounts with argon2-hashed passwords, all with equal rights in M1; they record who did what in metadata.
- **Sessions** are bearer tokens. A WebSocket carries its token in its first message.
- **The local desktop app** proves itself with a per-boot token in a file only the service's group can read.
- **Network.** The server listens on 127.0.0.1 by default. Listening on the network needs at least one account, and TLS (rustls) with a generated or supplied certificate.

**Control lease:**

- One client at a time holds it, and it's needed for everything that changes state: hardware actions, property writes, every job (hardware or compute), recipes, annotations, settings, extensions and power (D32).
- A logged-in client without the lease can only watch and read: live view, projects and results, jobs, logs and status. Its own view parameters (preview viewport and ROI, tile display settings) aren't instrument state, so they stay free.
- Any client may ask for control. The holder can hand it over, and the lease expires after an idle timeout. The local desktop app can always take it.

**Sidecar and service** (ADR-0015):

- The server runs as a service, or is started by the desktop app.
- **The sidecar contract:**
  - a `--managed` flag;
  - a ready line on stdout, with the port and the local token's path;
  - stop by closing stdin;
  - exit code 75 asks to be restarted.
- **The fast path:** the server publishes preview frames to a shared-memory ring for the desktop app, which reads cached tiles from disk through a Tauri custom protocol. M2 measures whether raw RGBA or JPEG is faster through the webview, and picks.

### 5.19 Status, logging, power

**Status.** Each subsystem reports `ok`, `degraded`, `fault` or `offline`, with a message:

- camera, stage, lights;
- GPU (memory and temperature, through NVML);
- disks (free scratch space, write rate);
- jobs and extensions.

The `status` Thing rolls these up for the instrument status indicator.

**Logging** (ADR-0009):

- `tracing`, with daily files kept for 30 days and an in-memory ring.
- Levels are trace, debug, info, warn, error and critical (error with a `critical` mark).
- A live tail over WebSocket, and a log captured for each job.

**Power** (ADR-0032):

- **Restart service** exits with code 75, and the service manager restarts it.
- **Reboot and shutdown** use `shutdown.exe` on Windows, and `systemctl` on Linux with a polkit rule that the .deb installs.
- All of them need the lease and a confirmation, and refuse while a hardware job is running unless forced.

### 5.20 Performance budgets

These are targets on the owner's laptop (RTX 3070 Ti laptop GPU). Each is measured in the phase named and recorded in its notes; the nightly run gates on them from P53.

| Path | Budget |
|---|---|
| Preview: a 20 MP frame to a JPEG sent at 1920 px | ≤ 30 ms with CUDA, ≤ 60 ms on the CPU (P17) |
| Demosaic, 20 MP | ≤ 10 ms with CUDA (P08c), ≤ 80 ms on 8 CPU threads (P08) |
| Scratch recording | 580 MB/s for 10 minutes with no dropped frames (P29). That is the design rate; the MIChrome 20 delivers 289 MB/s |
| Simulated camera at full resolution | ≥ 14.5 fps with CUDA, ≥ 5 fps on the CPU (P15) |
| Phase correlation of a 2048² pair | ≤ 15 ms with CUDA (P31) |
| Post-stitching | ≥ 10 tiles/s for 20 MP tiles, at bounded memory (P39) |
| Deep zoom | A cold tile ≤ 5 ms of server time, a cached tile ≤ 1 ms; a full-screen view of a region never seen before ≤ 150 ms on a LAN (P40) |

### 5.21 Testing and CI

**CI** runs on GitHub Actions on `windows-latest` and `ubuntu-24.04` for every PR:

- `cargo fmt --check`, `clippy -D warnings`, `test`, `doc -D warnings`;
- `cargo-deny` for licences and advisories;
- the layering test;
- each example's `--smoke` run, which exercises it headless for a few seconds.

**Tests that CI can't run:**

- CUDA tests, with `OSCOPE_CUDA=1`;
- hardware tests in M3, with `OSCOPE_GRBL_PORT` and `OSCOPE_TUCSEN=1`;
- benchmarks and the nightly soak.

**Lessons carried over** from `microscope`'s retrospective:

- wait for conditions, never for fixed times;
- give each test its own data root, port and build directory;
- test on the default sample and over real distances;
- break code on purpose to check that anything that supervises or detects actually fails its test.

## 6. How phases work

**Size.** Each phase has a budget of hand-written changed lines, counting source, tests, examples, HTML, comments and blank lines, but not lock files, generated files, vendored code or docs: **S** ≤ 300, **M** ≤ 600, **L** ≤ 900.

- **Estimate in four parts:** code, tests (often a third or more), examples and glue.
- **Split before starting** any phase estimated near its budget; the split goes in [Appendix C](#appendix-c-change-log).

**Branch and pull request.**

- OScope phases use the branch `m1/pNN-short-name` and the PR title `PNN: <title>`.
- Upstream phases use `u/uNN-short-name` in the owner's `teta-wot` clone, a sibling folder of OScope (`../teta-wot`), with the PR title `UNN: <title>` on `lucaswewa/teta-wot`, under the same approval rules.
  - Each phase runs in a fresh session opened in `teta-wot`, with OScope added (`/add-dir`) for the plan and specs.
  - User-facing changes go into `teta-wot`'s CHANGELOG (*Unreleased*) and guide. The phase notes, with their design decisions, go into OScope as `docs/milestone-1/notes/UNN-short-name.md`.
  - Only phase branches are pushed; v0.2.0 is tagged after the owner approves U06.

**Order.** P00 comes first, then U01–U06 one at a time, then P01 onwards. P02b runs whenever the owner is at the instrument, before P03. Milestone 2 follows, from W0.1, in [§8](#8-milestone-2-phase-overview)'s order.

**Milestone 2's phases** keep these rules, with their own IDs, estimates in place of the size budgets, and the app itself in place of examples: [milestone-2/phases.md](milestone-2/phases.md#how-milestone-2s-phases-work) says how.

**Approval and merge.** One phase at a time:

1. The PR opens with CI green, and the phase marked *In review*.
2. The owner reviews it.
3. It's merged only after the owner approves; it's then *Done*.
4. The next phase starts from the updated `main`.

**Every PR contains:**

- the code and its tests;
- its examples, with a README "How to try it";
- the phase's notes, `docs/milestone-1/notes/PNN-short-name.md`;
- new ADRs (*Accepted*), with their rows in `docs/adr/README.md`;
- the phase's status in [§7](#7-milestone-1-phase-overview).

**Definition of Done** (the PR template, from P00):

- [ ] The scope matches the phase spec, and any deviation is explained in the notes.
- [ ] CI is green on Windows and Ubuntu.
- [ ] Tests cover the new behaviour, and CUDA paths have passed `OSCOPE_CUDA=1` on the owner's machine.
- [ ] The example runs from a clean checkout as its README says.
- [ ] The notes and ADRs are written, and the indexes updated.
- [ ] Clean room: nothing is copied from `microscope` or any other codebase.
- [ ] New dependencies' licences are checked, and each is at least two weeks old.
- [ ] Within the size budget, or the overrun is explained.

**Spec-first phases** open their notes with a behaviour spec and its literature sources, written before the code: P31, P32, P34, P37, P39, P42, P43, P45 and P46.

**Checkpoints.** Each stage ending in a checkpoint includes a demo script in its notes.

## 7. Milestone 1 phase overview

The full specs are in [milestone-1/phases.md](milestone-1/phases.md). Status runs Planned → In progress → In review → Done. The milestone has 59 OScope phases (P00–P54, plus P02b, P08b, P08c and P09b) and 6 upstream phases (U01–U06).

### Stage 0: `teta-wot` v0.2.0 (upstream, in the `teta-wot` repository)

| ID | Phase | Depends on | Size | Status |
|---|---|---|---|---|
| U01 | Linux support, Ubuntu CI, licence metadata | — | M | Done |
| U02 | Path parameters for endpoints | U01 | L | Done |
| U03 | Router composition, custom listeners, HTTP/2 | U01 | L | Done |
| U04 | Binary WebSocket streams | U03 | L | Done |
| U05 | Pluggable authentication | U03 | L | Done |
| U06 | Release v0.2.0 | U02–U05 | S | Done |

### Stage 1: Groundwork

| ID | Phase | Depends on | Size | ADRs | Status |
|---|---|---|---|---|---|
| P00 | Docs scaffolding, working rules, foundational ADRs | — | S | 0001–0004 | Done |
| P01 | Rust workspace, layering test, CI | P00, U06 | L | 0005 | Done |
| P02 | Core types: units, geometry, time, IDs | P01 | L | 0007 | Done |
| P02b | Hardware capability survey (docs only) | P00 | S | — | Done |
| P03 | Frame model, buffer pool, frame bus | P02, P02b | L | 0008 | Done |
| P04 | Server skeleton, `system` Thing, media plane, example helper | P03 | M | 0006 | Done |
| P05 | Logging and the `logs` Thing | P04 | M | 0009 | Done |

### Stage 2: Compute

| ID | Phase | Depends on | Size | ADRs | Status |
|---|---|---|---|---|---|
| P06 | Compute abstraction, CPU backend, benchmark harness | P03 | M | 0010 | Done |
| P07 | CUDA backend | P06 | L | 0011 | Done |
| P08 | Demosaicing on the CPU | P07 | L | — | Done |
| P08b | Colour, display transform, crop and binning, writers | P08 | L | — | Done |
| P08c | CUDA twins of the colour pipeline, the budgets, the comparison page | P08b | L | — | Done |

### Stage 3: Hardware abstraction and simulator

| ID | Phase | Depends on | Size | ADRs | Status |
|---|---|---|---|---|---|
| P09 | HAL traits, capabilities, objective catalogue, manual implementations | P03 | L | 0012 | Done |
| P09b | HAL fakes and conformance suite | P09 | L | — | Done |
| P10 | Simulated world, optics, H&E tissue | P09 | L | 0013 | Done |
| P11 | Simulation clock and grblHAL-like stage | P09b | L | — | Done |
| P12 | Simulated camera: sensor, modes, timing, triggers | P10, P11 | L | — | Done |
| P13 | Samples II: reflective surface, fluorescent cell volume | P12 | L | — | Done |
| P14 | Samples III: sub-diffraction structures, imports, load and unload | P13 | M | — | Done |
| P15 | Simulator at full resolution | P14, P07 | M | — | Done |

### Stage 4: Live microscope (ends with Checkpoint A)

| ID | Phase | Depends on | Size | ADRs | Status |
|---|---|---|---|---|---|
| P16 | Device Things: camera, stage, illumination, objective, simulation | P05, P15 | L | 0034 | Done |
| P17 | Preview service and binary WebSocket protocol | P16, P08c | L | 0014 | Done |
| P18 | Live statistics and overlay data | P17 | M | 0035 | Done |
| P19 | Sidecar contract, service mode, shared-memory preview | P17 | M | 0015 | Done |

**Checkpoint A:** a simulated 20 MP microscope, live in a browser page with crosshair, ROI, profile, histogram, false colour and saturation, driven over HTTP.

### Stage 5: Jobs and pipelines

| ID | Phase | Depends on | Size | ADRs | Status |
|---|---|---|---|---|---|
| P20 | Job service and the `jobs` Thing | P16 | L | 0016 | Done |
| P21 | Pipeline engine | P20, P07 | L | 0017 | Done |
| P22 | Stock nodes I: correction and display; live recipes | P21, P08c | M | — | Done |
| P23 | Stock nodes II: filters, enhancement, threshold, morphology | P22 | L | — | Done |

### Stage 6: Extensions (ends with Checkpoint B)

| ID | Phase | Depends on | Size | ADRs | Status |
|---|---|---|---|---|---|
| P24 | `.omext` packages, registry, `extensions` Thing | P21 | M | 0018 | Done |
| P25 | Rust extension ABI and SDK | P24 | L | 0019 | Done |
| P26 | Python extension runtime and SDK | P24, P19 | L | 0020 | Done |
| P27 | Driver and panel extensions | P25, P26 | L | — | Done |

**Checkpoint B:** one recipe mixing stock, Rust and Python nodes runs live on the preview and as a batch job.

### Stage 7: Data, calibration and focus (ends with Checkpoint C)

| ID | Phase | Depends on | Size | ADRs | Status |
|---|---|---|---|---|---|
| P28 | Projects, OME-Zarr store, SQLite index | P20 | L | 0021 | Done |
| P29 | Scratch recorder and ingest | P28 | M | 0022 | Done |
| P30 | Capture, downloads, importers and exporters | P29, P27 | M | 0037 | Done |
| P31 | Registration: phase correlation on CPU and CUDA | P23 | M | — | Done |
| P32 | Calibration I: pixel size, camera-to-stage affine, backlash, store | P30, P31 | L | 0023 | Done |
| P33 | Calibration II: dark, flat, colour; the wizard | P32 | L | — | Done |
| P34 | Autofocus, auto exposure, auto white balance | P33 | L | 0024 | Done |

**Checkpoint C:** calibrate a simulated objective, autofocus, auto-expose, and capture a raw frame into a project; reopen it in napari.

### Stage 8: Scanning and stitching (ends with Checkpoint D)

| ID | Phase | Depends on | Size | ADRs | Status |
|---|---|---|---|---|---|
| P35 | Scan planners and focus maps | P34 | M | — | Done |
| P36 | Scan engine, the `scan` Thing, workflow extensions | P35 | L | 0025 | Done |
| P37 | Histo scan: overview and tissue detection | P36 | M | — | Done |
| P38 | Live stitching: in scans and free-roam | P36 | L | 0026 | Done |
| P39 | Post-acquisition stitching | P38 | L | 0027 | Done |
| P40 | Deep-zoom serving, tile cache, DZI export | P39 | M | 0028 | Done |

**Checkpoint D:** histo-scan a simulated H&E slide while the mosaic builds live, stitch it, and browse it in OpenSeadragon.

### Stage 9: 3D (ends with Checkpoint E)

| ID | Phase | Depends on | Size | ADRs | Status |
|---|---|---|---|---|---|
| P41 | Z-stacks, volume store, slice and brick routes | P36 | L | 0029 | Done |
| P42 | Depth from focus and extended depth of field | P41 | L | — | Done |
| P43 | Deconvolution and volume rendering | P42 | L | — | Done |

**Checkpoint E:** a height-map surface of the reflective sample, and a deconvolved fluorescent volume, both in VTK.js.

### Stage 10: Analysis and reports (ends with Checkpoint F)

| ID | Phase | Depends on | Size | ADRs | Status |
|---|---|---|---|---|---|
| P44 | Annotations and measurements | P40 | M | — | Done |
| P45 | Stain analysis | P44 | M | — | Done |
| P46 | Cell detection and counting | P45 | L | — | Done |
| P47 | Reports, and report extensions | P46 | L | 0030 | Done |
| P48 | The `workspace` Thing: analyses on datasets, results, provenance | P47 | M | — | Done |

**Checkpoint F:** count cells and quantify DAB on a stitched slide, then produce a PDF report.

### Stage 11: System and delivery (ends with Milestone 1)

| ID | Phase | Depends on | Size | ADRs | Status |
|---|---|---|---|---|---|
| P49 | Accounts, sessions, control lease, TLS | P48 | L | 0031 | Done |
| P50 | Instrument status and health | P49 | M | — | Done |
| P51 | Power actions | P50 | S | 0032 | Done |
| P52 | Settings and configuration migration | P51 | M | — | Done |
| P53 | Performance budgets and nightly soak | P52 | M | — | Done |
| P54 | Packaging, release workflow, M1 retrospective | P53 | M | 0033 | Done |

## 8. Milestone 2 phase overview

The full specs are in [milestone-2/phases.md](milestone-2/phases.md). Milestone 2 is the app: one Vue 3 codebase in `web/`, built for the browser and, in `desktop/`, for Tauri 2, on the simulated instrument throughout. It was specified on 2026-11-17, as 57 phases in 11 stages, where the outline before it foresaw 40 to 45. Status runs Planned → In progress → In review → Done.

**What differs from Milestone 1** ([the specs say how](milestone-2/phases.md#how-milestone-2s-phases-work)):

- **IDs** name the stage and the phase in it, as W3.2. Branches are `m2/w3.2-short-name`.
- **Estimates stand where S, M and L did.** Each spec has a first one, below. A second, in five parts, goes in [Appendix C](#appendix-c-change-log) before the phase's code. The owner is asked first when the second is over 2,500 lines, or a third over the first. The first estimates total about 83,000 lines.
- **Each phase is tried in the app itself,** against the simulated instrument. Its notes carry screenshots of both themes.
- **Server work** that the app needs is in the phase that needs it:
  - W0.1, serving the app;
  - W1.3, the origins a browser may call from;
  - W6.3, uploads;
  - W7.1, the `pipeline` Thing;
  - W8.2, the look's corrections;
  - W8.6, accounts over the API.

### Stage W0: Foundations

The workspace, the look, and the components every tab is made of.

| ID | Phase | Depends on | Estimate | ADRs | Status |
|---|---|---|---|---|---|
| [W0.1](milestone-2/phases.md#w0-1) | The web workspace and its checks | — | 900 | 0039, 0040 | Done |
| [W0.2](milestone-2/phases.md#w0-2) | Design tokens, themes and icons | W0.1 | 1,100 | 0041 | Done |
| [W0.3](milestone-2/phases.md#w0-3) | Components I: actions and inputs | W0.2 | 2,000 | 0042 | Done |
| [W0.4](milestone-2/phases.md#w0-4) | Components II: overlays and feedback | W0.3 | 1,500 | — | Done |
| [W0.5](milestone-2/phases.md#w0-5) | Components III: layout and data | W0.4 | 2,000 | — | Done |
| [W0.6](milestone-2/phases.md#w0-6) | Forms from JSON Schema | W0.5 | 1,500 | — | Done |
| [W0.7](milestone-2/phases.md#w0-7) | Charts | W0.5 | 1,200 | — | Done |

### Stage W1: Client plumbing

How the app talks to the server: typed calls, live state, the connection and the lease, the platform, and the preview stream.

| ID | Phase | Depends on | Estimate | ADRs | Status |
|---|---|---|---|---|---|
| [W1.1](milestone-2/phases.md#w1-1) | Generated API types and the typed client | W0.1 | 1,300 | 0043 | Done |
| [W1.2](milestone-2/phases.md#w1-2) | Things, live properties and events | W1.1 | 1,500 | 0044 | Done |
| [W1.3](milestone-2/phases.md#w1-3) | Connection, login and the control lease | W1.2 | 1,900 | 0045 | Done |
| [W1.4](milestone-2/phases.md#w1-4) | The host adapter | W1.3 | 800 | 0046 | Done |
| [W1.5](milestone-2/phases.md#w1-5) | The preview client | W1.3 | 1,800 | 0047 | Done |

### Stage W2: Shell

The rail, the routes, and what the rail says of the instrument.

| ID | Phase | Depends on | Estimate | ADRs | Status |
|---|---|---|---|---|---|
| [W2.1](milestone-2/phases.md#w2-1) | The shell and navigation | W0.5, W1.4 | 1,400 | 0048 | Done |
| [W2.2](milestone-2/phases.md#w2-2) | Status, connection and lease in the shell | W2.1 | 1,100 | — | Done |

### Stage W3: Control and Live View (ends with Checkpoint G)

The camera, the stage, the lights, the live image and what is drawn over it.

| ID | Phase | Depends on | Estimate | ADRs | Status |
|---|---|---|---|---|---|
| [W3.1](milestone-2/phases.md#w3-1) | The live viewer | W1.5, W2.2 | 1,500 | 0049 | Done |
| [W3.2](milestone-2/phases.md#w3-2) | Camera controls | W3.1 | 1,400 | — | Done |
| [W3.3](milestone-2/phases.md#w3-3) | Stage controls | W3.1 | 1,700 | — | Done |
| [W3.4](milestone-2/phases.md#w3-4) | Illumination and the objective | W3.2 | 900 | — | Done |
| [W3.5](milestone-2/phases.md#w3-5) | Overlays I: crosshair, ROI, grid and profile | W3.1, W0.7 | 1,600 | — | Done |
| [W3.6](milestone-2/phases.md#w3-6) | Overlays II: histogram, saturation, false colour and the look | W3.5 | 1,600 | — | Done |
| [W3.7](milestone-2/phases.md#w3-7) | Autofocus, capture and the live recipe | W3.6 | 1,400 | — | Done |
| [W3.8](milestone-2/phases.md#w3-8) | The free-roam mosaic and the tile viewer | W3.3 | 1,500 | 0050 | Done |

**Checkpoint G:** a live microscope in the app: jog, zoom to full resolution, watch the histogram and the saturation, autofocus, capture, and build a mosaic by jogging.

### Stage W4: Operations (ends with Checkpoint H)

Jobs, logs, power, and the instrument's state in full.

| ID | Phase | Depends on | Estimate | ADRs | Status |
|---|---|---|---|---|---|
| [W4.1](milestone-2/phases.md#w4-1) | Jobs | W2.2 | 1,500 | — | Done |
| [W4.2](milestone-2/phases.md#w4-2) | Logging | W2.2 | 1,300 | — | Done |
| [W4.3](milestone-2/phases.md#w4-3) | About, Power and the status in full | W4.1 | 1,600 | 0060 | Done |

**Checkpoint H:** queue and steer jobs, search and follow the log, inject and clear a fault, and restart the service.

### Stage W5: Scan Workflow (ends with Checkpoint I)

A scan planned over a picture of the slide, run, and watched.

| ID | Phase | Depends on | Estimate | ADRs | Status |
|---|---|---|---|---|---|
| [W5.1](milestone-2/phases.md#w5-1) | Scan plans | W3.8, W0.6 | 1,800 | — | Done |
| [W5.2](milestone-2/phases.md#w5-2) | Running a scan | W5.1, W4.1 | 1,500 | — | Done |
| [W5.3](milestone-2/phases.md#w5-3) | The histo scan | W5.2 | 1,200 | — | Done |
| [W5.4](milestone-2/phases.md#w5-4) | Z-scans, stacks and positions | W5.2 | 1,200 | — | Done |

**Checkpoint I:** an overview, a histo scan with its live mosaic, and its stitch.

### Stage W6: Gallery (ends with Checkpoint J)

Projects as cards, what each holds, and its images at any zoom.

| ID | Phase | Depends on | Estimate | ADRs | Status |
|---|---|---|---|---|---|
| [W6.1](milestone-2/phases.md#w6-1) | Projects | W2.2 | 1,500 | — | Done |
| [W6.2](milestone-2/phases.md#w6-2) | A project's records | W6.1 | 1,400 | — | Done |
| [W6.3](milestone-2/phases.md#w6-3) | Imports, exports and uploads | W6.2, W4.1 | 1,500 | 0051 | Done |
| [W6.4](milestone-2/phases.md#w6-4) | The deep-zoom viewer | W6.2, W3.8 | 1,600 | — | Done |

**Checkpoint J:** find the scanned slide, read its records, view it at every zoom, export and import.

### Stage W7: Workspace (ends with Checkpoint K)

Recipes drawn as graphs, analyses with their provenance, annotations, cells and stains, 3D, and reports.

| ID | Phase | Depends on | Estimate | ADRs | Status |
|---|---|---|---|---|---|
| [W7.1](milestone-2/phases.md#w7-1) | Recipes over the API | W1.1 | 1,200 | 0052 | Done |
| [W7.2](milestone-2/phases.md#w7-2) | The graph editor: the canvas | W7.1, W2.2 | 2,000 | 0053 | Done |
| [W7.3](milestone-2/phases.md#w7-3) | The graph editor: parameters and the library | W7.2, W0.6 | 1,600 | — | Planned |
| [W7.4](milestone-2/phases.md#w7-4) | Analyses and results | W7.3, W6.2 | 1,500 | — | Planned |
| [W7.5](milestone-2/phases.md#w7-5) | Annotations and measurements | W6.4 | 1,800 | — | Planned |
| [W7.6](milestone-2/phases.md#w7-6) | Cells and stains | W7.4, W7.5 | 1,600 | — | Planned |
| [W7.7](milestone-2/phases.md#w7-7) | 3D: surfaces | W7.4 | 1,600 | 0054 | Planned |
| [W7.8](milestone-2/phases.md#w7-8) | 3D: volumes and slices | W7.7 | 1,700 | — | Planned |
| [W7.9](milestone-2/phases.md#w7-9) | Reports | W7.4 | 1,300 | — | Planned |

**Checkpoint K:** a recipe drawn and run live; cells counted and DAB measured; annotations; a surface and a volume; the PDF report.

### Stage W8: Settings (ends with Checkpoint L)

The instrument's settings, its calibration, the simulator's samples, extensions and accounts.

| ID | Phase | Depends on | Estimate | ADRs | Status |
|---|---|---|---|---|---|
| [W8.1](milestone-2/phases.md#w8-1) | System, display and devices | W2.2 | 1,800 | — | Planned |
| [W8.2](milestone-2/phases.md#w8-2) | Calibration sets | W8.1, W4.1 | 1,600 | — | Planned |
| [W8.3](milestone-2/phases.md#w8-3) | The calibration wizard | W8.2, W3.3 | 1,500 | — | Planned |
| [W8.4](milestone-2/phases.md#w8-4) | Simulated samples | W8.1, W6.3 | 1,400 | — | Planned |
| [W8.5](milestone-2/phases.md#w8-5) | Extensions and their panels | W8.1, W0.6 | 1,700 | — | Planned |
| [W8.6](milestone-2/phases.md#w8-6) | Accounts | W8.1, W1.3 | 1,400 | 0055 | Planned |

**Checkpoint L:** an objective calibrated by the wizard, one's own sample, an extension's panel, and an account.

### Stage W9: Desktop (ends with Checkpoint M)

The same app in Tauri 2, with the server as its sidecar and the fast path for frames and tiles.

| ID | Phase | Depends on | Estimate | ADRs | Status |
|---|---|---|---|---|---|
| [W9.1](milestone-2/phases.md#w9-1) | The Tauri shell | W2.2 | 1,300 | 0056 | Planned |
| [W9.2](milestone-2/phases.md#w9-2) | The sidecar | W9.1 | 1,400 | — | Planned |
| [W9.3](milestone-2/phases.md#w9-3) | The shared-memory preview | W9.2, W1.5 | 1,600 | 0057 | Planned |
| [W9.4](milestone-2/phases.md#w9-4) | Tiles from disk | W9.2, W6.4 | 1,100 | — | Planned |
| [W9.5](milestone-2/phases.md#w9-5) | Installers | W9.4 | 1,400 | 0058 | Planned |

**Checkpoint M:** the desktop app installed, starting its own server, with its live view through shared memory and its tiles from disk.

### Stage W10: Quality

The app tested as a whole, measured, packaged, and looked back on.

| ID | Phase | Depends on | Estimate | ADRs | Status |
|---|---|---|---|---|---|
| [W10.1](milestone-2/phases.md#w10-1) | Workflows end to end | W8.6 | 1,500 | — | Planned |
| [W10.2](milestone-2/phases.md#w10-2) | Visual baselines and accessibility | W10.1 | 1,200 | — | Planned |
| [W10.3](milestone-2/phases.md#w10-3) | Front-end budgets | W10.1 | 1,100 | 0059 | Planned |
| [W10.4](milestone-2/phases.md#w10-4) | The app in the packages, and the look back | W10.3, W9.5 | 1,000 | — | Planned |

**Milestone 2** ends with W10.4, as 0.2.0.

## 9. Milestone 3 outline: real hardware and performance

Specified when M2 nears its end. Every driver phase is Done only after the owner has run its "How to try it" on the instrument.

| Stage | Scope |
|---|---|
| **H0 Bring-up** | Bring `docs/hardware.md` (from P02b) up to date with what bring-up finds: wiring, axis travel, steps per mm, encoders, the Tucsen SDK version. Hardware tests gated by environment variables |
| **H1 grblHAL driver** | `oscope-grbl`: line protocol and real-time commands (`?` status, `!` hold, `~` resume, jog cancel, soft reset), `$J=` jogging, `$H` homing, alarms and settings, encoder positions, aux outputs (M62–M65) for triggers. A protocol fake, recorded sessions, and calibration of report latency |
| **H2 Tucsen driver** | `oscope-tucsen`: the TUCam SDK loaded at run time (user-installed); modes and the real frame-rate table; raw Bayer, which the SDK delivers already processed (P02b); host timestamps; the SDK's defaults (automatic exposure and white balance, a vertical flip) set explicitly; broken frames detected; software triggers, and hardware triggers on cameras that have them; reconnecting; a recorded-session fake |
| **H3 Lights** | The illumination driver, once the controller is chosen (D19) |
| **H4 Real microscope** | Calibrating each objective on real optics, flat fields, autofocus tuning, scans of real slides and surfaces, objective plausibility checks |
| **H5 Continuous scanning** | On-the-fly scans. With a trigger-capable camera, grblHAL fires it at positions and frames are matched by trigger. With the MIChrome 20, frames are matched by host timestamps and the position track, allowing for the rolling shutter. Motion-blur limits; checked against stop-and-go |
| **H6 Performance** | End-to-end latency, the raw path at the full sensor rate, GPU tuning, memory, multiple clients, remote bandwidth (with H.264 only if needed) |
| **H7 Release 1.0** | Signed installers, the user guide, the M3 retrospective |

## 10. Planned ADRs

ADRs are numbered across the project. Upstream phases record their design decisions in their notes in OScope (`docs/milestone-1/notes/UNN-…`), and their user-facing changes in `teta-wot`'s CHANGELOG and guide.

| ADR | Title | Phase |
|---|---|---|
| 0001 | Record architecture decisions, and the delivery process | P00 |
| 0002 | Clean room: what OScope may take from `microscope`, and the dependency licence policy | P00 |
| 0003 | Platforms: Windows 11 and Ubuntu LTS on x86_64 | P00 |
| 0004 | Depend on `teta-wot` v0.2.0, extended upstream first | P00 |
| 0005 | Layered crates and their dependency rules | P01 |
| 0006 | Control plane and media plane | P04 |
| 0007 | Units, coordinates and the clock | P02 |
| 0008 | The frame model, buffer pool and frame bus | P03 |
| 0009 | Logging | P05 |
| 0010 | Compute backends: the CPU reference and CUDA through `cudarc` | P06 |
| 0011 | Building, loading and redistributing CUDA kernels | P07 |
| 0012 | The hardware abstraction layer | P09 |
| 0013 | The simulator: its clock, camera mounting and 3D samples | P10 |
| 0014 | The preview protocol: JPEG over a binary WebSocket | P17 |
| 0015 | The sidecar contract, service mode and the desktop fast path | P19 |
| 0016 | The job service | P20 |
| 0017 | The pipeline model | P21 |
| 0018 | The `.omext` package format | P24 |
| 0019 | The Rust extension ABI | P25 |
| 0020 | The Python extension runtime | P26 |
| 0021 | Projects, OME-Zarr v3 and the SQLite index | P28 |
| 0022 | Scratch recording and ingest | P29 |
| 0023 | The calibration model | P32 |
| 0024 | Autofocus | P34 |
| 0025 | Scan planning and the scan engine | P36 |
| 0026 | Live stitching | P38 |
| 0027 | Post-acquisition stitching | P39 |
| 0028 | Deep-zoom serving | P40 |
| 0029 | Volumes and 3D data on the media plane | P41 |
| 0030 | Reports | P47 |
| 0031 | Authentication and the control lease | P49 |
| 0032 | Power actions and their privileges | P51 |
| 0033 | Packaging and release | P54 |

**Milestone 1 added five** that weren't planned, 0034 to 0038: they are in [the index](adr/README.md).

**Milestone 2's:**

| ADR | Title | Phase |
|---|---|---|
| 0039 | The web app: its layers, state and routing | W0.1 |
| 0040 | Testing the app: unit, contract, end to end and visual | W0.1 |
| 0041 | Design tokens and themes | W0.2 |
| 0042 | The component library on reka-ui | W0.3 |
| 0043 | Generated API types and the typed client | W1.1 |
| 0044 | Things in the app: live properties and events | W1.2 |
| 0045 | The app's connection, its sessions, and media under security | W1.3 |
| 0046 | The host adapter | W1.4 |
| 0047 | The preview client: a worker and WebGL | W1.5 |
| 0048 | The shell, and the lease's read-only mode | W2.1 |
| 0049 | The live viewer and its overlays | W3.1 |
| 0050 | The tile viewer | W3.8 |
| 0051 | Uploads | W6.3 |
| 0052 | Recipes over the API: the `pipeline` Thing | W7.1 |
| 0053 | The graph editor on Vue Flow | W7.2 |
| 0054 | 3D in the app | W7.7 |
| 0055 | Accounts over the API | W8.6 |
| 0056 | The desktop app: Tauri, its permissions and the sidecar | W9.1 |
| 0057 | The desktop fast path: frames and tiles | W9.3 |
| 0058 | Installers | W9.5 |
| 0059 | Front-end budgets | W10.3 |

## 11. Documentation conventions

```text
docs/
├─ goal.md                     the owner's goals, in the owner's words (unchanged)
├─ README.md                   index of all documentation
├─ implementation-plan.md      this document
├─ hardware.md                 the real instrument: camera, SDK, controller, lights, host   (P02b)
├─ adr/                        README.md (index and process), template.md, NNNN-short-title.md   (P00)
├─ milestone-1/
│  ├─ phases.md                phase specifications
│  └─ notes/                   README.md, template.md, PNN-short-name.md, UNN-short-name.md   (P00)
├─ milestone-2/
│  ├─ phases.md                phase specifications
│  ├─ look.md                  the app's look, in our own words   (W0.2)
│  └─ notes/                   README.md, template.md, W3.2-short-name.md, and shots/ for their screenshots
└─ milestone-3/                phases.md and notes/, written near the end of M2
```

- **ADRs** are never reused or edited after acceptance, except for their status. A later ADR supersedes an earlier one.
- **Notes** are written for the reviewer and for whoever works on the code next ([Appendix B](#appendix-b-implementation-notes-template)).
- **This plan is a living document.** Splits, merges and reorderings go into the same PR, with a line in [Appendix C](#appendix-c-change-log).

## 12. Risks

| Risk | Impact | Mitigation |
|---|---|---|
| **Scope.** M1 is 56 phases plus 6 upstream, and estimates ran low in `microscope` | A long milestone, or phases over budget | Six checkpoints give usable increments. Estimate tests and glue explicitly and split early. If time runs short, the cut line is P45 (stain analysis), P37 (histo scan) and P14's imports, which can move to M1.5 |
| **The 20 MP data rate:** 580 MB/s raw by design (the MIChrome 20 delivers 289 MB/s), plus processing | Dropped frames, a slow preview | A pooled, pinned frame model from P03; latest-wins subscribers; binning and ROIs for preview; budgets measured from P08, P08c and P17; NVMe scratch with no compression on the hot path |
| **Upstream `teta-wot` work delays OScope** | Stage 0 blocks P01 | Small, separate PRs. P00 can proceed in parallel. If a feature stalls, OScope can mount its own routes temporarily, recorded in an ADR |
| **CUDA builds, drivers and CI without a GPU** | Fragile builds, untested GPU paths | Runtime loading with CPU fallback; a CPU twin for every kernel; `OSCOPE_CUDA=1` runs on the owner's machine before merge; a weekly CUDA build in CI |
| **Extension ABI stability** | Old extensions break, or crash the server | A versioned C ABI with compatibility checks at install; panics caught at the boundary; ABI tests built against older SDK versions; Python workers isolated in their own processes |
| **Python environments on locked-down lab PCs** | Extensions fail to install offline | uv bundled with the installers, offline wheelhouses, clear errors |
| **The camera's real capabilities.** P02b found a rolling shutter, a raw mosaic the SDK has already processed, no camera timestamps and no trigger; the raw rate at full resolution is still unmeasured | The frame model or preview targets need rework in M3 | Capabilities are reported, not assumed. `docs/hardware.md` records them, and the frame model carries the line time and an optional sensor timestamp |
| **Matching positions to timestamps** in live stitching | Misplaced tiles before refinement | Exposure-midpoint interpolation, latency estimation, encoders, then hardware triggers in M3; the post-acquisition stitch always refines |
| **Simulator speed at 20 MP** | Slow examples and tests | Cached, pre-blurred levels of detail, rayon, the CUDA path; a scaled-down sensor in tests (P15) |
| **Browser rendering at 30–60 fps at high resolution** | Jank in M2's live view | Worker decoding and WebGL; viewport-sized frames; the desktop fast path |
| **Deep zoom on demand is slow on cold regions,** or while a scan runs | A sluggish viewer | Tiles aligned to chunks, a chunk cache, coarse levels rendered eagerly, low priority with a thread cap, HTTP/2; budgets measured in P40; the static DZI export as a fallback |
| **Licences** (the CUDA EULA, the Tucsen SDK, JS libraries) | Shipping something we may not | `cargo-deny`, licence checks per phase, users install the SDK, P07 records the CUDA list |
| **`teta-wot`'s licence metadata is inconsistent:** its LICENSE file says MIT, but Cargo.toml and the guide say it has no licence | Confusion for an MIT product depending on it | Fixed in U01 |
| **Milestone 2's scope:** 57 phases, about 83,000 lines by their first estimates | A long milestone | Seven checkpoints give usable increments. Each phase has a second estimate before its code, and the owner is asked when one runs over. The cut line is W5.4 (z-scans and positions), W7.8 (volumes), W7.9's template editor and W9.4 (tiles from disk), which can move to M2.5 |
| **Two webviews in the desktop app:** WebView2 on Windows, and WebKitGTK on Ubuntu, which lags the browsers | The desktop app fails where the browser works | WebKit runs the main path's tests from W0.1, and both shells do from W9.1 |
| **The app's tests under the replays' load** | Replays that take much longer, or fail for no fault | Each end-to-end test has a server of its own and waits for conditions; touched suites run five times on two cores; the visual baselines are made on one system |
| **The server changes while the app is built:** the `pipeline` Thing, uploads, accounts over the API | The API moves under the app | The generated types fail the type check wherever a change reaches. Each change is in the phase that needs it, with its Rust tests |
| **VTK.js and Vue Flow are large** | A slow first load | Each is a chunk of its own, loaded when its tab first opens; W10.3 budgets every chunk |
| **npm's packages:** hundreds, each with a licence and an age | A licence OScope can't ship, or a version too new | W0.1's script checks every installed package; a licence outside ADR-0002's list goes to the owner |
| **GitHub Actions is still blocked** by its billing | No CI on pull requests | The local replays stay the merge gate (ADR-0038), and gain the web checks in W0.1 |

## 13. Points for your review

The first draft's points, with the owner's answers from 2026-11-10. All are resolved; new points will be added here as they arise.

| # | Point | Answer | Effect on the plan |
|---|---|---|---|
| 1 | **Granularity:** 55 OScope phases (now 56) and 6 upstream ones, each within a 900-line budget | Yes | — |
| 2 | **Three milestones,** where goal.md says "two" | "Two" was a typo | — |
| 3 | **An early, docs-only hardware survey** before P03 fixes the frame model | Yes | P02b added (D31) |
| 4 | **Drivers stay in M3,** though the hardware is available | OK | — |
| 5 | **CI on Windows and Ubuntu** for every PR | Yes | D35 |
| 6 | **Without the lease,** may a logged-in viewer run compute-only jobs? | No: without the lease, nothing that changes state | D32; §5.18 |
| 7 | **Deep zoom on demand,** from OME-Zarr with a cache | Yes; how fast is it to view? | The answer is in §5.15, with budgets in §5.20, the design in P40, and HTTP/2 in U03 (D33) |
| 8 | **Reports** in Typst (PDF) and minijinja (HTML) | OK | — |
| 9 | **Configuration in TOML** | OK | — |
| 10 | **Default port** | 8210 | D34; P00, P04. Data lives in `%ProgramData%\OScope\` or `/var/lib/oscope/` for the service, and in `./.oscope/` for development |
| 11 | **Units:** µm as `f64` at every API | OK | — |
| 12 | **Target workstation:** 8 or more cores, 32–64 GB of RAM, an NVIDIA GPU with 8 GB or more, a 1 TB or larger NVMe scratch disk | Yes | — |
| 13 | **Fluorescence and confocal modes** exist only in the simulator | OK | — |
| 14 | **goal.md stays** at `docs/goal.md`; P00 rewrites the root README | OK | — |
| — | **Owner's note:** the simulated hardware needs its own clock, so moves take real time and the live image moves accordingly | Added | D30; §5.10; P10, P11, P12, P17, P31 |

**Milestone 2's points,** from its specification on 2026-11-17. Each has what the specs assume; the owner's answers go here.

| # | Point | Assumed |
|---|---|---|
| 15 | **57 phases,** where the outline said 40 to 45, and about 83,000 lines | Kept, with §12's cut line |
| 16 | **IDs by stage,** as W3.2, and branches `m2/w3.2-short-name` | Yes |
| 17 | **Estimates in place of S, M and L,** with the owner asked when a second estimate is over 2,500 lines or a third over the first | Yes: CLAUDE.md says so from this change |
| 18 | **Each phase is tried in the app,** and its notes' screenshots are committed, some 100 to 300 kB each | Committed; they could go on the pull request instead |
| 19 | **Server work inside six of M2's phases** (§8) | Yes |
| 20 | **Media under security** by the token and `fetch`, with no cookies and no tokens in URLs | Yes: ADR-0045, at W1.3 |
| 21 | **Browsers:** Chromium for every test; Firefox and WebKit for the main path; both desktop webviews from W9.1 | Yes |
| 22 | **Charts and forms from JSON Schema are written here,** with no dependency | Yes; uPlot and Ajv, both MIT, are the alternatives |
| 23 | **WSL gains** Node 24 and Playwright's browsers at W0.1, and WebKitGTK at W9.1 | Asked again as each is installed |
| 24 | **CUDA in the installers** | Decided at W9.5 |
| 25 | **A checklist for the owner's own run** at each checkpoint, G to M | Yes |
| 26 | **Milestone 2 ends as 0.2.0** | Yes, by `docs/releasing.md` |

## 14. Appendices

### Appendix A: ADR template

```markdown
# ADR-NNNN: <Title>

- Status: Proposed | Accepted | Superseded by ADR-NNNN
- Date: YYYY-MM-DD
- Phase: PNN

## Context
## Decision
## Consequences
## Alternatives considered
```

### Appendix B: implementation notes template

```markdown
# PNN: <Title>

- Status: In review | Done
- Branch / PR: m1/pNN-short-name / #NN
- Size: estimated <S|M|L> (code N, tests N, examples N, glue N) · measured N lines

## Summary
## Behaviour spec (spec-first phases only, with literature sources)
## What was built
## How to try it (the examples)
## Performance (measured against §5.20, where relevant)
## Design notes and deviations from the plan
## Tests
## Follow-ups and known gaps
## Clean room
```

### Appendix C: change log

| Date | Change |
|---|---|
| 2026-11-10 | First draft, from goal.md and the planning Q&A |
| 2026-11-10 | Revised after the owner's review. Added P02b (hardware survey) and decisions D30–D35: the simulation clock and real-time motion; access without the lease; deep-zoom speed; port 8210; CI on both systems. P11 became "Simulation clock and grblHAL-like stage" (M → L). U03 gained HTTP/2 over TLS. The §13 points are resolved |
| 2026-11-10 | Approved by the owner. U06 follows `teta-wot`'s release checklist, including its 24-hour soak |
| 2026-11-10 | How upstream work runs (§6). Phases run in the owner's `teta-wot` clone; their notes live in OScope, and their user-facing docs in `teta-wot`. U01 adds a `CLAUDE.md` to `teta-wot`. Order: P00, then U01–U06, then P01; P02b when the owner is at the instrument |
| 2026-11-11 | P00: §6 names the `teta-wot` clone as a sibling folder (`../teta-wot`) rather than a Windows path |
| 2026-11-11 | U02 re-sized from M to L by the owner, before it started: the estimate was about 540 lines, near M's 600 |
| 2026-11-11 | U03 re-sized from M to L by the owner, before it started, rather than split: the estimate was about 725 lines |
| 2026-11-11 | U05 re-sized from M to L by the owner, before it started, rather than split: the estimate was about 840 lines |
| 2026-11-11 | P01 re-sized from M to L by the owner, before it started, rather than split: the estimate was about 695 lines, of which about 200 are the 17 crates' manifests |
| 2026-11-11 | P02 re-sized from M to L by the owner, before it started, rather than split: the estimate was about 765 lines |
| 2026-11-11 | P02 measured 1,648 lines, over L's 900; the estimate had left out doc comments, blank lines and formatting. The owner chose to take it in one pull request, with the overrun explained, rather than split it |
| 2026-11-12 | P02b, the hardware survey. The MIChrome 20 has a rolling shutter, no camera timestamps and no trigger input, and its SDK delivers the raw mosaic already processed. A camera with an external trigger is to replace it, and the lights aren't chosen yet. Updated: §1 and D20; §5.5's frame metadata (line time, optional sensor timestamp, black level); §5.9's camera capabilities and illumination; P03, P09 and P12; M3's H2 and H5; the camera risk |
| 2026-11-12 | P03 was estimated at about 1,275 lines before it started, counting documentation at P02's measured density: over L's 900. The owner chose one pull request, with the overrun explained, rather than a split |
| 2026-11-12 | P04 was estimated at about 1,820 lines before it started: about 1,460 at the measured density, plus a quarter, since P03 measured that much over its estimate. That is three times M's 600, and twice L's 900. The owner chose one pull request, with the overrun explained, rather than a split into the control plane (~1,090) and the media plane with the examples' helper (~730) |
| 2026-11-12 | P05 was estimated at about 1,760 lines before it started: about 1,410 at the measured density, plus a quarter. That is about three times M's 600. The owner chose one pull request, with the overrun explained, rather than a split into the logging core (~960) and the `logs` Thing with the live tail and `ex-logs` (~800). The owner also chose that queries cover the in-memory ring only, with older history in the downloadable daily files |
| 2026-11-12 | P06 was estimated at about 1,480 lines before it started: about 1,190 at the measured density, plus a quarter. That is over L's 900. The owner chose one pull request, with the overrun explained, rather than a split into the backends, `ComputeInfo` and the pools (~590) and the kernels, the equivalence helper and the benchmarks (~900) |
| 2026-11-12 | P07 was estimated at about 1,550 lines before it started: about 1,240 at the measured density, plus a quarter. That is over L's 900. The owner chose one pull request, with the overrun explained, rather than a split into the CUDA foundation (~800) and the kernels with their twins and example (~750). The owner had the CUDA 13.4 toolkit installed on their PC, which stands in for the laptop that P07's *Done when* names |
| 2026-11-12 | P08 was estimated at about 2,500 lines before it started: about 2,030 at the measured density, plus a quarter. That is nearly three times L's 900. With the owner's agreement it was split into three, each within L:<ul><li>**P08:** demosaicing on the CPU, the PSNR tests, the benchmarks and the CPU budget (~900);</li><li>**P08b:** the colour steps, the display transform, crop and binning, and the TIFF and PNG writers (~800);</li><li>**P08c:** the CUDA twins, equivalence, the CUDA budget and `ex-demosaic`'s comparison page (~900).</li></ul>P17 and P22 now depend on P08c |
| 2026-11-12 | P08c was estimated at about 1,100 lines before it started, over L's 900. The owner chose one pull request, with the overrun explained, rather than a split into the CUDA twins (~700) and the comparison page (~400) |
| 2026-11-12 | The MIChrome 20 was connected and measured (P02b's steps 1–3, `hardware.md`). In 16 bits it gives 7.24 fps at full resolution, 289 MB/s, half the 580 MB/s that §5.5 assumed; 14.5 fps holds only in 8 bits. The owner kept 580 MB/s as the design budget (§5.5, §5.20, P29), as headroom for the camera that replaces it. So P12's simulated camera runs its modes at the measured rates for each bit depth, plus a design rate for P29. An ROI is accepted but doesn't raise the rate, so §5.5's preview rate no longer counts on ROIs, and P12's ROIs crop without speeding up |
| 2026-11-12 | P09 was estimated at about 1,900 lines before it started, bottom-up from its API surface at the measured density: about three times M's 600. With the owner's agreement it was split into two:<ul><li>**P09:** the four traits and their capability types, `PositionTrack`, the objective catalogue, the manual implementations, ADR-0012 and `ex-objectives` (~1,150, so sized L);</li><li>**P09b:** the scripted fakes, the conformance suite and `ex-hal-fakes` (~850).</li></ul>The trait design and ADR-0012 are reviewed before the fakes and the suite build on them. P11 now depends on P09b, whose conformance suite it runs |
| 2026-11-12 | P09b was estimated again before it started, from P09's measured sizes (P09 came in at 2,157 lines against ~1,150): about 2,000, over twice L's 900. The owner chose one pull request, with the overrun explained, rather than a split into the fakes with `ex-hal-fakes` (~1,000) and the conformance suite (~1,000) |
| 2026-11-12 | P10 was estimated at about 2,200 lines before it started, from P09's measured densities: over twice L's 900. The owner chose one pull request, with the overrun explained, rather than a split into the renderer with a test sample (~1,200) and the H&E tissue (~1,000) |
| 2026-11-12 | P11 was estimated at about 2,100 lines before it started, from P10's measured densities: over twice L's 900. The owner chose one pull request, with the overrun explained, since the stage's tests need the clock and the motion, and a split's second half (~1,300) would still be over L |
| 2026-11-12 | The owner auto-approves P12–P23: each pull request is merged once CI passes, and the owner reviews them together after P23. Decided up front: one pull request for each phase, with any overrun explained; a §5.20 budget not met is recorded and followed up; P17 encodes JPEG with `jpeg-encoder`; a change teta-wot would need is worked around in OScope and recorded in an ADR; P17's visual check is measured, and watched at review; a conflict with a spec is decided, recorded and flagged in the pull request. P12 was estimated at about 2,000 lines |
| 2026-11-12 | P13 was estimated at about 1,900 lines before it started (code ~1,000, tests ~500, `ex-sim-3d` ~450), from P12's measured densities: over twice L's 900. One pull request, with the overrun explained, as the owner decided for P12–P23 |
| 2026-11-12 | P14 was estimated at about 1,700 lines before it started (code ~900, tests ~550, `ex-sim-samples` ~250), from P13's measured densities (P13 came in at 2,511 against ~1,900): nearly three times M's 600. One pull request, with the overrun explained |
| 2026-11-12 | P15 was estimated at about 1,500 lines before it started (code ~850, of it a CUDA kernel and its wrapper ~300; tests ~400; `ex-sim-bench` ~250), from P14's measured densities (P14 came in at 2,246 against ~1,700): over twice M's 600. One pull request, with the overrun explained. Measured first: at 20 MP, rendering the world takes 3.2 s of a frame's 3.4 s, so the cache of rendered tiles is what the budget needs |
| 2026-11-12 | P16 was estimated at about 2,500 lines before it started (the five Things ~1,000; the driver registry, the simulator's lights and acquisition ~400; tests ~700; `ex-http-walkthrough` ~400): nearly three times L's 900. One pull request, with the overrun explained, as the owner decided for P12–P23, rather than the split into P16 and P16b that the phase's spec allows. `teta-wot` answers every failed property with 500, so the HTTP statuses ADR-0012 gives driver errors need a workaround in OScope: ADR-0034, the next number above those reserved |
| 2026-11-13 | P17 was estimated at about 2,900 lines before it started (the preview chain ~350, a JPEG encoder ~400, the wire format ~150, the `preview` Thing and its sessions ~350, tests ~800, `ex-live-view` ~500): over three times L's 900. One pull request, with the overrun explained. `jpeg-encoder`, which the owner chose for P17, turned out to carry the Independent JPEG Group's conditions in every version, in its DCT ported from mozjpeg. IJG isn't among the licences ADR-0002 allows, so P17 writes OScope's own baseline encoder from ITU-T T.81 instead, and drops P14's test-only use of `jpeg-encoder` |
| 2026-11-13 | P17's preview stream is the `preview` Thing's binary stream, `/api/v1/preview/stream`, not the media plane's `/api/v1/media/preview` (§5.4): it takes `teta-wot`'s authentication, shutdown and Thing Description. `teta-wot`'s streams send only binary messages, so statistics are in each message's header and in the Thing's `sessions` property, not JSON messages from the server. Socket buffers held about 800 KB of stale previews for a slow client on loopback before the server noticed it, so clients may acknowledge what they've shown, and then have at most two frames they haven't shown. ADR-0014 records all three |
| 2026-11-13 | P18 was estimated at about 1,400 lines before it started (the statistics ~370; their blocks on the preview stream and the sessions ~120; tests ~350; `ex-live-overlays` with its page ~550): over twice M's 600. One pull request, with the overrun explained. The statistics travel inside the preview's frame messages, as blocks after the JPEG, so that they don't take the one-slot sink from a frame. That extends ADR-0014, and accepted ADRs aren't edited, so it is ADR-0035, the next free number |
| 2026-11-13 | P19 was estimated at about 2,300 lines before it started (shared memory, the preview ring and the private token file ~450; the server's managed and service modes ~250; the restart action, the service and the ring's publisher in `oscope-api` ~170; packaging and its test scripts ~185; tests ~500; `ex-sidecar` and `ex-shm-reader` ~330; and about a third on top, from P18's measured density): nearly four times M's 600. One pull request, with the overrun explained |
| 2026-11-13 | P19: the `system` Thing gains a `restart` action, which the sidecar contract's exit code 75 needs. `teta-wot`'s Windows service `install` sets no recovery actions, so OScope sets them with `sc.exe` (ADR-0015), with the upstream change as a follow-up. A service's local token and preview ring are its own account's, and its ring is in session 0's namespace on Windows, so the desktop app's fast path to a service waits for the installers (W9): a group for both, and the ring in `Global\` |
| 2026-11-13 | P20 was estimated at about 2,600 lines before it started (`oscope-jobs`: the model, the SQLite store, the hardware queue, the compute pool and the job's context ~1,000; the `jobs` Thing, the hardware lock in the device Things and the server ~380; tests ~700; `ex-jobs` with its page ~550): nearly three times L's 900. One pull request, with the overrun explained, rather than the spec's split of persistence into P20b, as the owner decided for P12–P23. SQLite is `rusqlite` 0.41.1 with its bundled SQLite: 0.42.0 is under two weeks old |
| 2026-11-13 | P20: `teta-wot` gives each event subscriber a buffer of 5, so the `jobs` Thing sends a job's progress at most every 100 ms, and its changes of state at once (ADR-0016); clients treat job events as hints. The manual-check ports, 8211–8219, are used up: `ex-jobs` shares 8211 with `ex-server-hello` |
| 2026-11-13 | GitHub CI's runs of the examples' `--smoke` are switched off for now, to save CI minutes, by `RUN_EXAMPLES` in `ci.yml` and `cuda.yml` (the owner's decision, from P21), until the owner turns them back on. Clippy and the tests still build every example there, and the local replays (`scripts/ci-local.sh`, `scripts/ci-clone.sh`) still run them before each pull request |
| 2026-11-13 | P21 was estimated at about 3,100 lines before it started (`oscope-pipeline`: values, nodes, the registry and the parameters' JSON Schema validator ~650; recipes and their validation ~330; the plan, with its order, placement and halos ~250; running it, with the memo cache ~400; tiles, live mode and the batch job ~420; trivial nodes ~250; tests ~800; `ex-pipeline` ~350): more than three times L's 900. One pull request, with the overrun explained, rather than the spec's split of tiled execution into P21b, as the owner decided for P12–P23. Parameters are checked by OScope's own validator for the subset of JSON Schema that nodes use, rather than the `jsonschema` crate, which brings about 30 crates |
| 2026-11-13 | P21's pull request, #34, was squash-merged as `37e1d69`. GitHub answered 502 during the merge and left the pull request open, so it was closed by hand, with a comment naming the commit |
| 2026-11-13 | P22 was estimated at about 3,200 lines before it started (fifteen stock nodes over P08–P08c's functions, a reference reader and a resampler ~1,000; live recipes in the preview chain and the `preview` Thing, with references captured from the camera ~450; tests ~700; `ex-live-recipe` with its page ~650; glue ~50; and a tenth on top, after P21): more than five times M's 600. One pull request, with the overrun explained, as the owner decided for P12–P23 |
| 2026-11-13 | P23 was estimated at about 3,400 lines before it started (the CPU references of the Gaussian, median and bilateral filters and of CLAHE ~450, their CUDA kernels and launches ~500; twelve stock nodes, with thresholds, morphology, the distance transform and labelling, ~800; tests ~700; `ex-filters` with its page ~550; glue ~50; and a tenth on top): nearly four times L's 900. One pull request, with the overrun explained, as the owner decided for P12–P23 |
| 2026-11-14 | The owner's manual tests of P16–P23 ([the checklist](milestone-1/manual-tests-P16-P23.md)) found three faults, fixed in one pull request: `ex-http-walkthrough`'s page read the objective catalogue's list under the wrong name and failed to start; the stage's `moved` event said nothing for jogs, or for a move or homing halted on the way, and now fires whenever the stage comes to rest, with a report taken after it is still; and `ex-live-view` and `ex-live-overlays` started the camera only in their `--smoke` runs. The examples' smoke checks had served each page, but never run its script |
| 2026-11-14 | The owner gave a standing approval for P24–P34, with a manual test after P34, and answered the run's questions up front: uv is installed locally and set up in CI with Astral's `setup-uv`, the Python SDK's tests running in CI on Ubuntu only; an extension's Python packages are its own, outside OScope's licence list, but held to the two-week rule through uv's `exclude-newer`; P28 validates NGFF metadata with the `jsonschema` crate in tests only; P29's 10-minute run records in rolling segments, at most about 100 GB on disk; spec-first phases go on to their code without stopping for review; and examples get the manual-check ports 8230–8249 too |
| 2026-11-14 | P24 was estimated at about 2,050 lines before it started (the manifest and its checks ~250; the zip reader and writer, guarded against traversal ~200; installing, enabling and the load order, with a registry for each kind ~300; the `extensions` Thing and its upload route ~250; `oscope-server ext pack` ~60; tests ~400; `ex-omext` with its page ~400; and a tenth on top): more than three times M's 600. One pull request, with the overrun explained, as the owner decided for P24–P34 |
| 2026-11-14 | P25 was estimated at about 2,300 lines before it started (the C ABI's tables and views, the safe `ProcessingNode` trait, the `export!` macro and its glue, with panics caught, ~450; the loader with its version check, the host's table and the nodes it gives the pipeline, loaded at each start and offered to live recipes, ~450; tests ~550; the sample `ext-rust-node` and `ex-rust-ext`, which builds, packs, installs, loads and runs it, ~450; and a fifth on top, after P24 came in 38 % over): more than twice L's 900. One pull request, with the overrun explained, as the owner decided for P24–P34 |
| 2026-11-14 | P26 was estimated at about 3,000 lines before it started (the Python SDK, its node decorator, schemas from type hints, shared-memory views and message loop, with its pytest suite, ~650; finding uv and building each extension's environment at install, online or from a wheelhouse, ~250; the worker supervisor and the shared-memory transport, ~650; Rust tests ~450; the scikit-image sample and `ex-python-node` ~350; CI, scripts and the server's settings ~150; and a fifth on top): more than three times L's 900. One pull request, with the overrun explained, as the owner decided for P24–P34 |
| 2026-11-14 | P27 was estimated at about 3,700 lines before it started (driver traits in both SDKs, with the Rust ABI's second version, ~650; the host's adapters that make extension drivers the HAL's, and the drivers' registry, ~650; panels described by extensions and served by the `extensions` Thing ~200; a batch job of a recipe over the camera's frames, for Checkpoint B, ~200; the fake-camera sample, the Python panel, `ex-ext-camera` and the Checkpoint B script ~450; tests ~600; and a fifth on top): more than four times L's 900. One pull request, with the overrun explained, as the owner decided for P24–P34. ADR-0036 records the drivers' ABI and the panels, which no planned ADR covered |
| 2026-11-14 | P28 was estimated at about 3,600 lines before it started (project, acquisition, result and report records and their folders ~400; OME-Zarr v3 through `zarrs`, NGFF 0.5 multiscales, tile-aligned chunks and zstd ~450; the SQLite index, its search and paging, and its rebuild from the folders ~350; thumbnails ~100; the `projects` Thing ~350; the server's wiring ~50; tests, the NGFF schema checks among them, ~800; `ex-projects`, its Gallery page and its README on napari and Python ~450; and a fifth on top): four times L's 900. One pull request, with the overrun explained, as the owner decided for P24–P34 |
| 2026-11-14 | P29 was estimated at about 2,900 lines before it started (the frame log: segments of records with their metadata and checksums, index footers, preallocation, rolling and recovery by scanning, ~520; scratch space, its health and its clean-up ~170; ingest into an acquisition, in parallel, its pyramid too, ~210; the recorder, a lossless subscriber on a thread of its own, ~260; the ingest job and the scratch's health over HTTP ~130; the server's wiring ~25; tests ~790; `ex-record` ~300; and a fifth on top): almost five times M's 600. One pull request, with the overrun explained, as the owner decided for P24–P34 |
| 2026-11-14 | P30 was estimated at about 4,200 lines before it started (the `capture` Thing: single captures and bursts as acquisitions, a burst as a job through the recorder and ingest, and the display rendition, ~400; the data route, with Range requests and content types, ~200; importers of TIFF, multi-page TIFF, PNG and JPEG ~300; exporters of OME-TIFF with its OME-XML, 16-bit TIFF and PNG ~350; importer and exporter extension kinds, with the Rust ABI's third version, the Python SDK and worker, and the host's registry, ~800; tests ~1,000; `ex-capture` and the sample extensions' importer and exporter ~450; and a fifth on top): seven times M's 600. One pull request, with the overrun explained, as the owner decided for P24–P34. ADR-0037 records the importers, exporters and the ABI's third version, which no planned ADR covered |
| 2026-11-14 | P31 was estimated at about 1,900 lines before it started (phase correlation on the CPU: the Hann window, FFTs through `rustfft`, the normalised cross-power spectrum, the peak in a window around the expected offset, Foroosh's sub-pixel estimate and the peak-ratio confidence, ~450; on CUDA: cuFFT's plans and the kernels for the window, the cross-power spectrum and the search window's download, ~300; the batch API on both ~100; tests, the simulator's jog among them, ~500; `ex-register` ~200; glue ~30; and a fifth on top): three times M's 600. One pull request, with the overrun explained, as the owner decided for P24–P34. The behaviour spec opens the notes, before the code |
| 2026-11-14 | P32 was estimated at about 3,000 lines before it started (the calibration sets, versioned per kind and key, and their store of JSON files with the active ones, export and import, ~400; the least-squares fit of the camera-to-stage matrix and its decomposition into turn, mirror, scales and skew ~150; the procedures: frames to grey, moves reached one way, registration, the 3 × 3 pattern, backlash from both sides, the soft limits and homing, and the objective check, ~450; the `calibration` Thing and its jobs ~350; the simulator's truth (mounting, scale, backlash) ~60; tests ~920; `ex-calibrate` ~280; glue ~60; and about a tenth on top): three times L's 900. One pull request, with the overrun explained, as the owner decided for P24–P34. ADR-0023 records the calibration model. The behaviour spec opens the notes, before the code |
| 2026-11-15 | P33 was estimated at about 4,150 lines before it started (the simulator's vignetting, in the CPU's exposure and the CUDA kernel's, its sensor's colour response and a colour target, ~300; the colour target's layout and the colour science, the matrix's fit, CIELAB and ΔE, ~250; dark, flat and colour sets, with their images beside them, ~300; the procedures ~450; their jobs ~120; the wizard, a state machine of steps, prompts, required actions, validation and resume, ~400; the `calibration` Thing's additions ~300; tests ~1,300; `ex-wizard` ~300; glue ~60; and about a tenth on top): more than four times L's 900. One pull request, with the overrun explained, as the owner decided for P24–P34 |
| 2026-11-15 | P34 was estimated at about 4,200 lines before it started (the focus measures, Brenner, Tenengrad and the variance of the Laplacian, on the CPU ~150 and with CUDA ~200; the autofocus procedure, stepped and continuous sweeps, ~600; auto exposure ~200; auto white balance ~120; their jobs ~150; the `autofocus` and `exposure` Things ~350; the simulator's truth ~50; tests ~1,300; `ex-autofocus` ~400; Checkpoint C's `ex-checkpoint-c` ~250; glue ~60; and about a tenth on top): more than four times L's 900. One pull request, with the overrun explained, as the owner decided for P24–P34 (and, on 2026-11-15, extended to P43) |
| 2026-11-15 | P35 was estimated at about 2,500 lines before it started (regions, rectangles, polygons and masks, and which tiles each touches, ~220; the plan model, tiles, the z policy and stacks, and the field and its overlaps from the calibrated geometry, ~220; the planners, raster, snake, z-scan, tiled z-stack and multi-position, ~200; the focus map, its sample points, a plane and a thin-plate spline with outliers rejected, and a small linear solver, ~330; estimates of time and data size ~130; measuring a map by autofocus ~80; the simulator's tilted slide ~40; tests ~600; `ex-plan` and its page ~550; glue ~30; and a little on top): about four times M's 600. One pull request, with the overrun explained, as the owner decided for P24–P43 |
| 2026-11-15 | P36 was estimated at about 3,600 lines before it started (the engine, each tile moved to, settled, focused from the map or by autofocus, and its frame or stack taken with its position into one recording, then ingested, ~550; the scan job, its retries, cancel and end position, ~200; continuous-scan fields reserved ~40; the `scan` Thing, its planners, preview and start, ~350; the `workflow` extension kind, through the SDK's drivers, the host and the Python SDK, ~460; the server's wiring ~40; tests ~850; `ex-scan` and its page ~500, and two sample workflows, Rust and Python, ~200; glue ~60; and about a tenth on top): four times L's 900. One pull request, with the overrun explained, as the owner decided for P24–P43; the spec's split candidate stays in it |
| 2026-11-15 | P37 was estimated at about 2,000 lines before it started (the overview, frames a field apart reduced to colour and placed, ~220; tissue detection, saturation, Otsu's threshold, closing and opening, components and their areas, and the margin, ~300; the engine's map measured at the tiles before the first, ~80; the `scan` Thing's `overview` and the `tissue` pattern ~150; the simulator's tissue truth ~20; tests ~500; `ex-histo-scan` and its page ~450; glue ~30; and about a tenth on top): more than three times M's 600. One pull request, with the overrun explained, as the owner decided for P24–P43 |
| 2026-11-15 | P38 was estimated at about 2,100 lines before it started (the canvas, its 256-pixel tiles and their pyramid, frames placed through an affine, the newest winning or edges feathered, a memory budget with eviction to scratch, and the tiles changed since a generation, ~340; the live mosaic, frames drawn as the preview draws them and placed at their exposure midpoint, free roaming's gates of motion blur, place and sharpness, and the scan's tiles, ~280; the `live_mosaic` Thing, its tile route, the preview stream's block of changed tiles and the wiring, ~260; the engine's hook ~25; the simulator's stage latency ~10; the examples' vendored folder served ~25; tests ~550; `ex-live-mosaic` and its page ~420; glue ~40; and about a tenth on top): more than twice L's 900. One pull request, with the overrun explained, as the owner decided for P24–P43 |
| 2026-11-16 | P39 was estimated at about 2,500 lines before it started (the overlap graph and the pairs' parts ~120; their registration in batch, binned where long, ~120; the global solve by conjugate gradients with reweighting and rejection ~200; the flat field ~100; the blend and the out-of-core writer ~220; the stitch job, from the acquisition to the result, ~220; the simulator's encoders and lost steps as settings ~25; the job's wiring ~20; tests ~600; `ex-stitch`, its page and its throughput bench ~450; glue ~40; and about a tenth on top): nearly three times L's 900. One pull request, with the overrun explained, as the owner decided for P24–P43 |
| 2026-11-15 | Outside the phases, at the owner's request before P40: the local replays are the merge gate, and they run the test suites and smoke runs eight at a time, each on two CPUs; `scripts/two-core.sh` runs the suites a change touches five times each, side by side; test builds optimise the dependencies, the simulator, imaging and compute. ADR-0038 records it, the next number free above the reserved ones, and supersedes ADR-0003's CI section |
| 2026-11-15 | P40 was estimated at about 2,750 lines before it started (in `oscope-imaging`, the DZI geometry and tiles rendered from a pyramid's levels through a display transform, raw frames demosaiced, ~320; in `oscope-store`, tiles read through a cache of decompressed chunks, a colour overview for raw images, the disk tile cache on scratch and the DZI export, ~440; the pipeline's DZI node ~70; in `oscope-api`, the routes with `Server-Timing`, the tile pool capped while a hardware job runs, the cache warmed when a stitch ends and the export format, ~320; tests ~650; `ex-deep-zoom` and its bench ~410; `ex-checkpoint-d` ~250; glue ~50; and about a tenth on top): more than four times M's 600. One pull request, with the overrun explained, as the owner decided for P24–P43. Raw images get a colour overview, at the owner's choice: deep zoom would otherwise show their coarse levels in grey |
| 2026-11-16 | P41 was estimated at about 1,760 lines before it started (in `oscope-store`, a volume's pyramid halving z as well as x and y, as its voxels near a cube, its levels' planes and the OME metadata's z scale, ~220; a z-scan's voxel size from its plan at ingest ~30; in `oscope-api`, the volume routes: metadata, orthogonal slices and bricks with a binary header, uint8 or uint16, at a level chosen to fit, a raw image from its first level, ~320; tests ~550; `ex-volume` and its VTK.js page ~440; glue ~40; and about a tenth on top): about twice L's 900. One pull request, with the overrun explained, as the owner decided for P24–P43. VTK.js is vendored, as the owner chose, and not counted |
| 2026-11-16 | P42 was estimated at about 2,650 lines before it started (in `oscope-compute`, the sum-modified-Laplacian focus measure and its CUDA twin, to the bit, ~170; in `oscope-volume`, the focus measures streamed plane by plane, the height from the peak and its parabola, the confidence, the median and the all-in-focus image ~300, and the PLY, STL and OBJ meshes ~200; the pipeline's nodes for each step ~240; in `oscope-workflows`, the depth-from-focus job, from a volume acquisition to its results and mesh, ~250; in `oscope-api`, the job's wiring and float bricks for the heightfield ~50; tests ~700; `ex-depth-from-focus` and its VTK.js page ~450; glue ~50; and about a tenth on top): nearly three times L's 900. One pull request, with the overrun explained, as the owner decided for P24–P43 |
| 2026-11-16 | P43 was estimated at about 3,000 lines before it started (in `oscope-volume`, the Born–Wolf widefield PSF with its Bessel function and integral ~150, Richardson–Lucy on rustfft's 3D FFTs with mirrored padding, total-variation regularisation and its iterations ~280; in `oscope-compute`, its CUDA twin on cuFFT with its kernels ~230; the pipeline's nodes ~150; in `oscope-workflows`, the deconvolution job, from a volume acquisition to a result volume, ~200; tests ~750; `ex-deconvolve` and its VTK.js page with MIP, composite, isosurface and a before-and-after toggle ~500; `ex-checkpoint-e` ~200; glue ~60; and about a tenth on top): more than three times L's 900. One pull request, with the overrun explained, as the owner decided for P24–P43. The PSF is Born–Wolf's alone, as the owner chose |
| 2026-11-16 | P44 was estimated at about 2,000 lines before it started (in `oscope-imaging`, shapes' lengths, areas and centres, and the pixels a polygon holds, row by row, ~170; in `oscope-store`, annotations in layers with their validation and their keeping ~260, and measurements read tile by tile, with the CSV, ~230; the `projects` Thing's routes ~130; tests ~560; `ex-annotate` and its OpenSeadragon drawing page ~490; `ex-common`'s helpers ~60; glue ~30): more than three times M's 600. One pull request, with the overrun explained, as the owner decided for P24–P54. A scan's images now record their pixel size from the plan's field, which P41 did for z-scans only, so that measurements on a stitched mosaic are in µm |
| 2026-11-16 | P45 was estimated at about 2,500 lines before it started (in `oscope-imaging`, optical density, stain vectors and colour deconvolution, Macenko's estimate with a 3 × 3 eigensolver, and the DAB bins, ~350; in `oscope-sim`, the H-DAB section and the stains' amounts as truth, ~120; the pipeline's four nodes ~220; in `oscope-workflows`, the job over a stored slide, tile by tile, ~320; tests ~850; `ex-stain` and its page ~600; glue ~40): more than four times M's 600. One pull request, with the overrun explained, as the owner decided for P24–P54 |
| 2026-11-16 | P46 was estimated at about 2,750 lines before it started (in `oscope-imaging`, an adaptive threshold, the distance transform and labelling moved from the pipeline, a seeded watershed, each object's measures and outline, ~550; the pipeline's nodes ~200; in `oscope-workflows`, the counting job over a stored slide, tile by tile with a halo, its table, outlines and annotation layer, ~400; a stub Python extension ~80; tests ~900; `ex-count-cells` and its page ~550; glue and `ex-checkpoint-d`'s wait ~70): about three times L's 900. One pull request, with the overrun explained, as the owner decided for P24–P54 |
| 2026-11-16 | P47 was estimated at about 2,600 lines before it started: the report crate, with its model, Typst and minijinja renderers, charts and templates; the report generator kind in both SDKs and the loader; the job and OScope's generator; the `projects` Thing's templates; tests; and `ex-report`. Nearly three times L's 900. One pull request, with the overrun explained, as the owner decided for P24–P54. Typst's tree comes with one licence exception and eight advisories ignored, as the owner chose (ADR-0030) |
| 2026-11-16 | P48 was estimated at about 2,200 lines before it started (in `oscope-store`, the inputs' hashes in the provenance and deleting a result ~60; in `oscope-workflows`, the analysis job over a stored image or a region of it, its outputs kept as a result, its provenance, made again and compared ~380; the `workspace` Thing, with P44's annotation routes moved to it ~350; tests ~660; `ex-workspace` and its page, and `ex-checkpoint-f` ~550; and about a tenth on top): more than three times M's 600. One pull request, with the overrun explained, as the owner decided for P24–P54 |
| 2026-11-16 | P49 was estimated at about 2,200 lines before it started (in `oscope-api`, accounts with argon2, sessions, the lease, the authenticator and authoriser, and the `auth` Thing ~720; in `oscope-server`, the account commands, TLS with a made or given certificate, and the network's rules ~280; tests, among them one made from the Thing Descriptions ~610; `ex-two-clients` and its page ~280; and about a tenth on top): more than twice L's 900. One pull request, TLS within it, with the overrun explained, as the owner decided for P24–P54 |
| 2026-11-16 | P50 was estimated at about 1,450 lines before it started (in `oscope-sim`, the faults: a camera unplugged, a stage alarm, a scratch disk full ~80; the GPU through NVML ~80; the disk monitor ~60; the `status` Thing with its reporters, roll-up and event ~300; the `simulation` Thing's fault actions ~50; tests ~400; `ex-status` and its page ~300; glue ~30; and about a tenth on top): more than twice M's 600. One pull request, with the overrun explained, as the owner decided for P24–P54 |
| 2026-11-16 | P51 was not estimated in lines before it started, an oversight: its spec's S stood. It measured 738 hand-written lines added (its notes): 438 over S's 300. One pull request, with the overrun explained, as the owner decided for P24–P54 |
| 2026-11-16 | P52 was estimated at about 1,550 lines before it started (the `settings` Thing: each account's display preferences and their defaults, as teta-wot settings; the configuration file's system and simulator settings behind a trait the server implements; their validation; export and import ~350; the control lease's exemption for one's own display, and the builder ~20; in `oscope-sim`, the simulator's checks taken out of its construction ~30; in `oscope-server`, the file's version and its migration from version 1, keeping comments and order, loading through it, writing with a backup, `config migrate`, and `[server]` renamed `[network]` ~260; tests with version-1 fixtures ~460; `ex-settings` and its page ~280; glue ~30; and about a tenth on top): more than twice M's 600. One pull request, with the overrun explained, as the owner decided for P24–P54 |
| 2026-11-17 | P53 was estimated at about 1,050 lines before it started (`cargo xtask` and its commands ~70; `perf`: §5.20's benchmarks run and read ~170, and their HTML report ~120; a `perf:` line in each benchmark ~40; the soak, against the release server: its configuration, a preview client, scans and stitches, memory and handles sampled on each platform ~260, and its leak detector, by Theil–Sen's slope, ~50; tests ~200; the nightly workflow, its schedule off ~35; glue ~20; and about a tenth on top): nearly twice M's 600. One pull request, with the overrun explained, as the owner decided for P24–P54 |
| 2026-11-17 | RUSTSEC-2026-0543 marks `chrono` unmaintained, not vulnerable, so `cargo deny` failed on every branch. At the owner's choice, `deny.toml` ignores it, with its reason, and a move to `jiff` or `time` is planned: teta-wot's core and HTTP crates first, then OScope's nine crates. P53's notes record it |
| 2026-11-17 | P54 was estimated at about 720 lines before it started (the .deb, built by a script with `dpkg-deb`, its maintainer scripts, the polkit rule in both of Ubuntu's forms, and the unit's changes ~200; the Windows zip, its configuration and its install and uninstall scripts ~145; the notices with cargo-about ~70; the release workflow ~110; the example, the .deb installed in clean Ubuntu containers, and a check of the zip ~105; tests ~15; glue ~10; and about a tenth on top): a little over M's 600. One pull request, with the overrun explained, as the owner decided for P24–P54 |
| 2026-11-17 | **Milestone 2 specified** ([milestone-2/phases.md](milestone-2/phases.md), §8): 57 phases in 11 stages, W0.1 to W10.4, where the outline foresaw 40 to 45. IDs name the stage. First estimates, about 83,000 lines in all, stand where S, M and L did, by Milestone 1's look back; a second estimate comes here before each phase's code, and the owner is asked when it is over 2,500 lines or a third over the first. Each phase is tried in the app, with screenshots in its notes. Checkpoints G to M. ADRs 0039 to 0059 planned (§10). Server work named in W0.1, W1.3, W6.3, W7.1, W8.2 and W8.6. Risks and points 15 to 26 added (§12, §13). P54 marked Done |
| 2026-11-17 | W0.1 was estimated at about 1,420 lines before it started (code ~430: the app's first page, with its router, i18n and the version line ~220, the packages' licence check ~120, and the server's `--web` and `[web] root` ~90; tests ~290: the version line's states ~70, the lint rules' fixtures ~70, the licence check's ~60, and the server's ~90; styles ~40; the try-it's end-to-end test, with the server it starts ~140; glue ~520: `package.json` and the TypeScript, Vite, Vitest, Playwright, ESLint and Prettier configurations ~390, and the web steps in the replays and CI ~130). That is 58 % over the spec's 900, in the configurations and the replays' steps. At the owner's standing choice for W0 and W1 (2026-11-17: go on, and record it), it is one pull request |
| 2026-11-17 | W0.1 adds a check the spec doesn't name: npm's advisories, judged by tier as licences are ([ADR-0040](adr/0040-testing-the-app.md)). None is allowed in what the app ships. In development tools, an advisory fails the check until it is listed in `web/advisories.json` with its reason, as `deny.toml` lists the crates'. Three were listed on the first day, each a slow pattern match on a stranger's glob, in tools that match our own files: `picomatch` (no fixed version exists), `minimatch` (its fix is a new major, which ESLint doesn't take yet) and `glob` (its fix is taken once two weeks old, on 2026-11-20). Decided under the owner's standing approval for W0 and W1, and for the owner's review |
| 2026-11-17 | W0.2 was estimated at about 1,600 lines before it started (code ~420: the theme's and the accents' names and how they reach the root ~45, the pairs the tokens allow ~70, the `Icon` component and its registry ~135, the store and the browser's storage ~100, the words and the route ~70; tests ~460: a reader of the tokens that works their values out as a browser does ~140, the contrast of every pair and the tokens' other rules ~150, the store's ~95, the icons' ~45, the storage's ~30; styles ~430: the tokens ~230, the base styles ~85, the gallery's ~100, the first page's ~15; the try-it's gallery page ~170 and its end-to-end test, with the first visual baselines ~85; glue ~35). That is 45 % over the spec's 1,100: the tokens' reader, nine accents where the spec named none, and the gallery's size. At the owner's standing choice for W0 and W1 (2026-11-17: go on, and record it), it is one pull request |
| 2026-11-17 | W0.3 was estimated at about 2,800 lines before it started (code ~910: sixteen components on reka-ui's primitives and their own words, the number field ~150 of it, since it is written here for its own rules of committing and of saying why a value is refused; tests ~560: each component's keys, states and events, the number field's ~140; styles ~670: each component's, and what the fields share; the try-it ~600: the gallery's page of inputs ~300, and the end-to-end tests that use every control from the keyboard, run axe and compare the baselines ~300; glue ~40). That is 40 % over the spec's 2,000, and over the 2,500 at which the owner is asked: sixteen components came to more than the spec allowed each. At the owner's standing choice for W0 and W1 (2026-11-17: go on, and record it), it is one pull request |
| 2026-11-18 | W0.4 was estimated at about 2,900 lines before it started (code ~810: twelve components, the dialog and its two confirmations, the two menus, and the toasts with their queue ~170 of it; tests ~520: the queue's ~140, each component's states, and the confirmation that is typed; styles ~460, most of them shared stylesheets, since an overlay is drawn at the page's root; the try-it ~1,050: the gallery's page of overlays and its words ~420, and the end-to-end tests ~630, which open each overlay from the keyboard, follow the focus, run axe on each and compare each with its baseline; glue ~10). That is nearly twice the spec's 1,500, and over the 2,500 at which the owner is asked. W0.3 taught the measure: a control's tests are as long as the control, and its keys are tested in a real browser. At the owner's standing choice for W0 and W1 (2026-11-17: go on, and record it), it is one pull request |
| 2026-11-18 | W0.5 was estimated at about 4,000 lines before it started (code ~1,250: the data table ~380 and its rules apart from it ~170 (sorting, the virtual list's window, the grid's keys, selection), the tree, tabs, splitter, toolbar, pagination, collapsible, card, scroll area and key-and-value list ~620, a kept value in the browser's storage ~40, the words ~40; tests ~1,000: the table's rules ~300, the table ~250, the others' ~450; styles ~650; the try-it ~1,090: the gallery's page of data with its 100,000 rows ~420, its words ~100, and the end-to-end tests ~570, which sort and scroll the rows, walk the tree and the grid from the keyboard, resize and keep the panes, measure the frames while scrolling, run axe and compare the baselines; glue ~10). That is twice the spec's 2,000, by the measure W0.3 and W0.4 set: a component's tests are as long as it is. At the owner's standing choice for W0 and W1 (2026-11-17: go on, and record it), it is one pull request |
| 2026-11-18 | W0.6 was estimated at about 2,400 lines before it started (code ~900: reading a schema into the shapes the form draws ~300 (objects, maps, numbers with their ranges and units, strings and their formats, enums written either way, booleans, arrays, optional values written three ways, unions tagged inside their objects or outside them, `$ref` to the document's `$defs`, values of any kind, and anything else as JSON), checking a value against them ~150, defaults ~50, the server's refusals ~30, the form and its fields ~330, the words ~40, reading every schema the server publishes ~80; tests ~700: the shapes ~300, the checks and defaults ~200, the form ~200; styles ~150; the try-it ~640: the gallery's page of forms, with every schema the server publishes drawn at once for the contract, ~300, its words ~60, and the end-to-end tests ~280, which draw every schema, fill one in, have the server refuse one, run axe and compare the baselines; glue ~10). That is 60 % over the spec's 1,500. A survey of the server's 272 schemas before the estimate found what the spec's subset doesn't name: maps, unions tagged outside their objects, enums written as `oneOf` of `const`s, and values of any kind. W0.5 measured 31 % under its estimate, so tests are no longer counted twice. At the owner's standing choice for W0 and W1 (2026-11-17: go on, and record it), it is one pull request |
| 2026-11-18 | W0.7 was estimated at about 2,230 lines before it started (code ~820: the scales and their ticks, by Heckbert's nice numbers and in decades, ~160, the nearest point ~40, drawing on a canvas ~260, the chart, with its cursor, zoom, pan, keys and table, ~300, the charts' colour tokens ~40, the words ~20; tests ~600: the scales, ticks and nearest point ~280, the drawing, on a canvas that records what it is told, ~150, the chart ~170; styles ~80; the try-it ~720: the gallery's page of charts ~200, its words ~40, a script that records its data from the simulated instrument through the API ~230, and the end-to-end tests ~250, which zoom, pan and read the charts from the keyboard, measure the histogram's redraw, run axe and compare the baselines; glue ~10). That is 86 % over the spec's 1,200: the spec asks for recorded data, and the simulated instrument gives them only through its API, captured, exported and read back. The data themselves are generated, and not counted. At the owner's standing choice for W0 and W1 (2026-11-17: go on, and record it), it is one pull request |
| 2026-11-18 | W1.1 was estimated at about 2,300 lines before it started (code ~770: the types' generator and its stale check ~130, the typed client over `fetch`, with its time limits and aborts, ~110, the error model, ADR-0034's refusals among its cases, ~140, actions started, followed to their end and cancelled ~130, jobs followed ~80, and the media plane's wrappers, written by hand, ~180; tests ~930: the error model from recorded answers ~220, the client ~110, the actions ~150, the jobs ~60, the stale check ~60, and contract tests against a real server ~330, for each media wrapper and an action followed to completion, to a failure and cancelled; styles ~50; the try-it ~480: the gallery's page of calls ~220, its words ~50, its end-to-end test ~130, and a script that records the server's answers ~80; glue ~70: the scripts, the step in the replays and CI, the configurations and the README). That is 77 % over the spec's 1,300, which didn't count the tests against a real server or the recorded answers. The generated types, from 1 MB of OpenAPI, and the recorded answers are generated, and not counted. W1.1 was begun in a sandbox of its own while W0.7's gate ran, at the owner's trial of 2026-11-18. At the owner's standing choice for W0 and W1 (2026-11-17: go on, and record it), it is one pull request |
| 2026-11-18 | W1.2 was estimated at about 2,230 lines before it started (code ~850: the Thing Descriptions, read once, and what each Thing offers ~130, the Thing's WebSocket, one a Thing in use, shared, its first message under security, its observations and its back-off ~260, staleness ~40, `useProperty`, read at its first use, kept by the socket, read again at intervals without it, and written at once, taken back with its reason if refused, ~200, `useAction` ~60, `useEvent` ~70, and the Pinia stores, a Thing each, ~90; tests ~850: with a WebSocket and a server stood in for ~600, the socket's messages, its reopening and staleness, a property followed, read again without the socket and its refused write taken back, events in order and the stores, and contract tests against a real server ~250, a property followed through another client's writes, events in order, and the socket reopened after the server restarts; styles ~50; the try-it ~450: the gallery's page of Things ~230, its words ~40, and its end-to-end test ~180, which follows a move made by another client, sees the values marked stale while the server is stopped and live again when it returns, runs axe and compares the baselines; glue ~30). That is 49 % over the spec's 1,500, W1.1's measure having been 10 % over its own second estimate. It was begun in the sandbox while W1.1's gate ran. At the owner's standing choice for W0 and W1 (2026-11-17: go on, and record it), it is one pull request |
| 2026-11-18 | W1.3 was estimated at about 2,960 lines before it started (code ~1,250: the connection, `/health` watched, its states and the server's version checked ~180, the session, its login, its token kept for the session through the host and a session that ended taken back to the login, then to where the user was, ~200, the lease, who holds it, taking it, asking for it, handing it over, giving it up and the warning before it lapses, ~200, `useCanControl` ~40, the token on every call and fetch, media shown as blobs, ~100, uploads and downloads with progress ~160, the login page ~150, and the server's `[network] origins`, a layer of OScope's own before teta-wot's, which allows every origin, ~220; tests ~900: unit, the connection's states, the session's ends, the lease and progress, ~450, the server's tests of the origins ~150, contract tests, an image fetched with the token and refused without it, and the lease over the API, ~200, and the end-to-end servers given accounts ~100; styles ~100; the try-it ~650: the gallery's page of the connection and the lease ~250, its words ~80, and end-to-end tests with two browsers, the login, a wrong password, the lease taken, asked for and handed over, and a session that ends, ~320; glue ~60). That is 56 % over the spec's 1,900, which didn't count the server's origins layer, needed since teta-wot allows every origin. It was begun in the sandbox while W1.2's gate ran, its Rust built only between gates. At the owner's standing choice for W0 and W1 (2026-11-17: go on, and record it), it is one pull request |
| 2026-11-18 | W1.4 was estimated at about 1,110 lines before it started (code ~330: the `HostAdapter` interface ~70; the browser's implementation ~170, its storage, a file saved as a download and one opened through the file chooser, the clipboard, with a fallback for a page served over plain HTTP, notifications, the window's title and full screen, and links opened outside the app; choosing the host ~20; and the theme, the session's token and the values kept moved onto it, the old storage helpers removed, ~70; tests ~390: the browser's implementation against a faked window ~290, `kept` and the storage tests moved ~50, and the lint rules' tests ~50; styles ~40; the try-it ~310: the gallery's page of the host ~140, its words ~45, and its end-to-end test ~125, a saved file read back from the disk and a value kept through a reload in Chromium, Firefox and WebKit, the clipboard, the title and full screen in Chromium, axe and the baselines; glue ~40: the lint rules, which now name the adapter, the route and the gallery's link). That is 39 % over the spec's 800, which didn't count the fallbacks, the tests against a faked window or the lint rules' tests. It was begun in the sandbox while W1.3's gate ran. At the owner's standing choice for W0 and W1 (2026-11-17: go on, and record it), it is one pull request |
| 2026-11-18 | W1.5 was estimated at about 2,090 lines before it started (code ~820: ADR-0014's header and ADR-0035's blocks, the statistics and the saturation mask, with their types, ~130; the parameters, sent whole each time, ~40; the worker's stream, its WebSocket, its first message under security, its acknowledgements, decoding with `createImageBitmap`, and reopening with back-off, with the worker's shell, ~200; the main thread's client, the newest frame replacing one not yet drawn, drawing at the display's refresh, its numbers (frames drawn a second, latency from the exposure's end to the draw, frames replaced), resting while the tab is hidden, and P18's data published to the app, ~220; the renderer, WebGL2's textured quad with pan and zoom as a transform, and a 2D canvas without WebGL2, ~160; and the view, fitting, zooming about a point, panning and the ROI asked for when zoomed in, ~70; tests ~700: the header from recorded messages, a version it doesn't know and the blocks ~130, the stream with a socket stood in for, its first message, its acknowledgements, a decode that fails and reopening, ~200, the client, the newest frame winning, the numbers, resting while hidden and the parameters sent whole, ~170, the view ~70, and contract tests against a real server, frames arriving, an ROI given the pixels asked for, statistics when asked and the parameters as the server describes them, ~130; styles ~50; the try-it ~480: the gallery's page of the live preview, its readout, its controls and the histogram, ~200, its words ~50, a script that records messages from the server ~70, and its end-to-end tests ~160, frames drawn in Chromium, Firefox and WebKit, the rate drawn against the rate sent at 1920 pixels, the frames' size following the window, resting while hidden, axe and the baselines; glue ~40). That is 16 % over the spec's 1,800, which didn't count the contract tests or the recording script. It was begun in the sandbox while W1.4's gate ran. At the owner's standing choice for W0 and W1 (2026-11-17: go on, and record it), it is one pull request |
| 2026-11-19 | W2.1 was estimated at about 1,850 lines before it started (code ~900: the shell, the rail as a toolbar with the status indicator's place, and the layout of rail, control pane and view with a splitter whose place is kept, ~280; the routes, nine tabs each a chunk, the component gallery kept out of the rail, the window's title following the tab and the focus moved to its heading, ~150; the shortcuts' registry and its list under `?` ~120; the read-only mode, a scope that switches off every control inside it unless freed, through some fifteen components, and its line with the way to ask for control, ~170; each tab's error boundary with its reload, and a refused action raised as a notice, ~90; and the tabs' placeholders, Settings with the theme and the accent and About with the server's version, ~90; tests ~300: the shortcuts, the boundary, the scope and the refusals' notices; styles ~180; the try-it ~420: the shell's words ~120, and its end-to-end tests ~300, each route opened, the focus after navigation, the skip link and the rail from the keyboard, the read-only mode between two browsers, axe and the baselines; glue ~50). That is 32 % over the spec's 1,400, which didn't count the scope's reach into the components. It was begun in the sandbox while PR #83's gate ran. At the owner's standing choice for W2 to W4 (2026-11-19: go on, and record it), it is one pull request |
| 2026-11-19 | W2.2 was estimated at about 1,350 lines before it started (code ~620: the instrument's status indicator, its state from `status/instrument` kept by `changed`, its subsystems and their messages in a popover, ~170; the page in place of the tabs while there is no server ~80; the connection's state and the lease's holder in the rail ~90; another's request for the lease raised as a notice that hands over ~80; the notices' history, newest first, and its drawer ~150; and the shell's wiring ~50; tests ~250: the indicator's colour from the state, the history and its limit, a request's notice raised once and handed over, and the drawer; styles ~150; the try-it ~310: its words ~60, and its end-to-end tests ~250, the indicator following a fault injected and cleared and the stage homed, the reconnecting page coming and going, a request handed over between two browsers, the drawer, axe and the baselines; glue ~20). That is 23 % over the spec's 1,100. It was begun in the sandbox while W2.1's gate ran. At the owner's standing choice for W2 to W4 (2026-11-19: go on, and record it), it is one pull request |
| 2026-11-19 | W3.1 was estimated at about 1,800 lines before it started (code ~750: the viewer in Control's view, its canvas on W1.5's client, fitted, zoomed about the pointer and panned by dragging, with the keys for each, ~220; the sensor's pixels drawn as squares past one to one ~20; the transforms between the pane, the image and the sensor, and the place under the pointer in the sensor's pixels and in µm at the sample, ~80; streaming started and stopped, and its four states, not streaming, waiting for a frame, live and stale, ~80; the readouts, the rate, the latency, the zoom and the place, ~60; the scale bar and its round lengths ~90; the toolbar, full screen and the view saved as a PNG through the host ~120; the simulator's clock rate made writable on the server, for the tests' fast clock, ~30; and Control's wiring ~50; tests ~350: the transforms, the scale bar's lengths, the streaming states, the renderer's squares, and the rate written on the server; styles ~200; the try-it ~450: its words ~60, the tests' shared fast clock and W1.5's tests moved onto it ~70, and its end-to-end tests ~320, frames drawn, a zoom asking for a region and that region coming back, the states, the readouts, the scale bar shortening, the PNG saved, axe and the baselines on a masked frame; glue ~50). That is 20 % over the spec's 1,500, which didn't count the server's clock or the tests' helper. It was begun in the sandbox while W2.2's gate ran. It is one pull request |
| 2026-11-19 | W3.2 was estimated at about 1,280 lines before it started (code ~560: the camera's pane in Control, its exposure and gain within the camera's ranges, its mode and pixel format applied together and kept from while streaming, the region read out, the trigger and its edge, and the temperature, ~320; the fields built from `capabilities`, the formats each mode gives and the settings that differ from those saved, ~110; an action that hands its work to a job, followed to its end, for auto exposure and white balance, ~45; what they found, and the saved settings put back, ~60; and Control's wiring ~25; tests ~200: the fields from a recorded `capabilities`, the differences, and the job's action; styles ~90; the try-it ~400: its words ~70, the recording's script ~25, and its end-to-end tests ~300, each setting changing the camera, a value refused at its field by the field and by the camera, the mode applied and kept from while streaming, auto exposure's job and its value taken, the saved settings put back, the recording still the server's, axe and the baselines; glue ~30). That is 9 % under the spec's 1,400. It was begun in the sandbox while W3.1's gate ran. It is one pull request |
| 2026-11-19 | W3.3 was estimated at about 1,700 lines before it started (code ~760: the stage's pane in Control, its position in the display's unit, its status, and an alarm's reason with clearing it, homing and the wait to be homed, ~180; moves to a place and by a step, refused before they are sent past the limits, the jog pad with its steps and continuous jogging, and the speed, ~200; Halt, a button that never scrolls away, and Escape, ~50; the keys on the focused image, the arrows and Page Up and Page Down, ~60; pointing, a double click centring the place and a drag panning the stage through the active calibration's matrix, and what is missing without one, ~170; and the limits' check, the keys' map and the arithmetic from the sensor's pixels to the stage's µm, ~100; tests ~250: the limits' check, the keys' map, the pointing's arithmetic, a step refused before it is sent, and a jog halted when the page loses the keys; styles ~120; the try-it ~520: its words ~90, and its end-to-end tests ~430, at real distances, a move arriving, steps from the pad and the keys, a step past a limit refused unsent, a jog moving while held and stopping on release, a long move halted by Escape and by the button, a double click centring the place within the calibration's error, found again in the image, a drag panning, what is missing without a calibration, an alarm said and cleared, axe and the baselines; glue ~50). That is the spec's 1,700. It was begun in the sandbox while W3.2's gate ran. It is one pull request |
| 2026-11-19 | W3.4 was estimated at about 910 lines before it started (code ~380: the lights' pane in Control, each channel from `capabilities`, on or off and dimmed, set with `set_light`, and those that differ from the saved ones put back, ~150; the lights' list and their differences, ~40; the objective's pane, the catalogue and the one fitted, a change fitted at once and the pixel's size read again, ~110; and the objective's check offered after a change, run as a job and what it found said, ~80; tests ~120: the lights' list from a recorded `capabilities`, and the differences from the saved lights; styles ~60; the try-it ~330: its words ~50, the recording's script ~20, and its end-to-end tests ~260, a light's intensity changing the image's mean, measured in the screenshots with W3.3's helper, now shared, fitting an objective changing the scale bar and the place's µm, the check offered and run, the recording still the server's, axe and the baselines; glue ~20). That is 1 % over the spec's 900. It was begun in the sandbox while W3.3's gate ran. It is one pull request |
| 2026-11-19 | W3.5 was estimated at about 1,600 lines before it started (code ~670: the layers over the live image, drawn as SVG in the sensor's pixels and placed by the view, the crosshair, a grid and the scale bar, each switched on and off and kept, ~200; the ROI drawn, moved and resized on the image, its handles from the keyboard, and made the camera's region or the statistics' ~180; the line drawn and its ends moved, and its profile from the frames' statistics charted as it changes ~110; the ROI's statistics, each channel's mean, deviation, least and greatest ~60; the tools on the image, the preview asked for the statistics as they change, and Control's wiring ~120; tests ~250: the geometry under zoom and pan, the handles' keys, an ROI on the camera's grid, and the statistics and the profile read; styles ~150; the try-it ~500: its words ~60, and its end-to-end tests ~440, an ROI drawn being the region the server measures, moved from the keyboard and made the camera's, the profile's ends the line's, the layers switched and kept, axe, and each layer's baseline on a frozen frame; glue ~30). That is the spec's 1,600. It was begun in the sandbox while W3.4's gate ran. It is one pull request |
| 2026-11-19 | W3.6 was estimated at about 1,500 lines before it started (code ~640: the histogram, a channel each, linear or logarithmic, its percentiles marked, live, with the black and white points dragged on it, ~180; the saturated pixels marked on the image from the mask, in the renderer, ~90; false colour in the fragment shader, a colour map's table as a texture, chosen and kept, ~110; the look, the white balance's gains with auto white balance's offered, gamma, and reset, ~150; and the focus value and its trace ~60; and the seven maps' tables, generated from their published data and left out of the count as data; tests ~220: the histogram's bins and percentiles from recorded messages, the black and white points from a drag, the colour maps' ends and steps, and the mask's bits placed; styles ~120; the try-it ~480: its words ~70, the tables' script ~40, and its end-to-end tests ~370, the mask's pixels where the raw frame is saturated, a look's change reaching the preview, false colour chosen and kept, axe, and the histogram's and false colour's baselines; glue ~40). That is 6 % under the spec's 1,600. It was begun in the sandbox while W3.5's gate ran. It is one pull request |
| 2026-11-19 | W3.7 was estimated at about 1,300 lines before it started (code ~700: autofocus's pane, its measure, sweep and range from its defaults, run as a job and followed, the frame's measure now, and the result said, ~170; its curve from `last`, the coarse and fine sweeps with the focus found marked, ~50; capture's pane, into a project chosen or made there, a frame or a burst, named, with a rendition, and the record shown with a link, ~200; the live recipe's pane, none, a recipe that comes with the app or one opened from a file, set and cleared, its cost against `recipe_budget_ms`, and the references listed and captured, ~180; the recipes that come with the app until W7.1's library, and the references each needs, ~70; and the tab, ~30; tests ~110: the curve from a recorded `last`, and the recipes' references and a file's checks; styles ~70; the try-it ~380: its words ~100, and its end-to-end tests ~280, autofocus to the simulator's true focus within a quarter of the depth of field, a capture making a record in a project made there, a recipe set and cleared, a reference captured for one, axe, and the panes' baseline; glue ~40). That is 7 % under the spec's 1,400. It was begun in the sandbox while W3.6's gate ran. It is one pull request |
| 2026-11-20 | W3.8 was estimated at about 1,450 lines before it started (code ~700: the `TileViewer`, which wraps OpenSeadragon once (ADR-0050), its tiles fetched with the session's token, its controls the app's own, a scale bar and a navigator, ~260; the coordinates between the viewer, the image and the stage's µm, ~70; the live mosaic's view, begun, halted and cleared, its state followed and its tiles refreshed as they are drawn, the field of view marked, and a double click sending the stage there, ~250; the split beside the live view, ~60; and the mosaic's tile source, its levels in OpenSeadragon's order and its tiles' addresses, ~60; tests ~150: the coordinates both ways, the levels' order and the tiles' addresses, and the mosaic's state; styles ~90; the try-it ~450: its words ~60, and its end-to-end tests ~390, jogging adding tiles where the stage went, a double click moving the stage there, axe, and the viewer's baseline; glue ~60: the package and its notice). That is 3 % under the spec's 1,500. Checkpoint G's demo script and checklist are in its notes, which aren't counted. It was begun in the sandbox while W3.7's gate ran. It is one pull request |
| 2026-11-20 | W4.1 was estimated at about 1,500 lines before it started (code ~750: the jobs tab, the queue and the history side by side with a job's detail, ~180; the jobs kept live, the queue from `jobs/queue` and each change from `jobs/changed`, the history by pages of `jobs/list`, ~120; a job's row and its detail, its kind, title, account, state, progress with its stage and when it should end, its results as links, its error and its own log from `logs/records`, ~230; steering with the lease, cancel, pause and resume, and the queue's order by dragging or from the keyboard, ~120; the shell's running hardware job, and each job's end as a notice and, the page hidden, a system notification, ~80; and submitting by kind in development builds, ~60; tests ~180: progress and its ETA, the history's paging, a change merged into the queue, and the order's keys; styles ~110; the try-it ~420: its words ~90, and its end-to-end tests ~330, the queue following jobs submitted elsewhere, cancel, pause and reorder reaching the server, an end's notification, axe, and the tab's baseline; glue ~40). That is the spec's 1,500. It was begun in the sandbox while W3.8's gate ran. It is one pull request |
| 2026-11-20 | W4.2 was estimated at about 1,280 lines before it started (code ~650: the logging tab, its filters, the records in the virtual table and a record's fields, ~260; the filters as `logs/records`' query, ~50; the records kept, a page from `logs/records`, older pages by its `next`, and the live tail from the media plane merged in, ~150; the tail held while the user scrolls up, and following again at the end, ~70; each level's colour from the tokens' states, and its count, ~40; and the daily files, listed and downloaded through the host adapter, ~80; tests ~150: the filters' query, the records' window over 100,000, the tail's merging and holding; styles ~100; the try-it ~350: its words ~70, and its end-to-end tests ~280, a record emitted appearing at the tail and under its filter, scrolling up holding the tail, a daily file downloaded, axe, and the tab's baseline; glue ~30). That is 2 % under the spec's 1,300. It was begun in the sandbox while W4.1's gate ran. It is one pull request |
| 2026-11-20 | W4.3 was estimated at about 1,650 lines before it started (code ~800: About, from `system`, the version and commit, the operating system, the processor, the memory and the uptime, ~120; the licences, the server's notices listed and opened, and the app's own, made from the bundle's packages at build, ~110; Power, its three actions each confirmed by typing its name, refused while a hardware job runs unless forced, the dry run shown, and the app back by itself after a restart, ~200; the status in full, each subsystem's state, message and details, on a page of its own, ~150; the simulator's faults, ~60; and the server's part, ~160: the service's restart done while the host's actions stay dry runs, for the tests, and its notices served; tests ~230: the confirmation's typing, the notices' list, the details' words, and the server's, ~110 of them; styles ~200; the try-it ~390: its words ~100, the test's server started again on its exit code 75, ~30, and its end-to-end tests ~260, a restart with the app back, reboot and shutdown against the dry run with their confirmations, a refusal while a job runs, About and the status's page, a fault injected and cleared, axe, and the baselines; glue ~30). That is 3 % over the spec's 1,600. It was begun in the sandbox while W4.2's gate ran. It is one pull request |
| 2026-11-20 | **ADR-0060, *Restarts in a dry run*,** wasn't planned: it takes the next free number above the reserved 0059, and extends ADR-0032. W4.3's tests restart their own server for real while rebooting and shutting down stay dry runs, with the new `[power] dry_run_restart`. W4.3 also serves the server's third-party notices, from the `system` Thing, for About. Neither server change was named among Milestone 2's server work |
| 2026-11-21 | Outside the phases, at the owner's request before W5: **a phase's gate runs the tests its change reaches** ([ADR-0061](adr/0061-gates-test-what-a-change-reaches.md), extending ADR-0038). `web/scripts/changed.mjs` plans them from the files changed: the Rust packages changed and those depending on them, their examples, the unit tests related to the app's changed files, and the end-to-end specs covering what changed, by each spec's `// Covers:` line. The replays run that plan with `--changed`, and every other check in full; the two-core runs, the phase's own specs. Both full replays still run for each stage's checkpoint phase, W5.4's and W6.4's first. ADR-0061 wasn't planned, and takes the next free number |
| 2026-11-21 | W5.1 was estimated at about 1,830 lines before it started (code ~950: the scan tab, the overview beside the plan, ~120; the overview, its region typed or from the stage's corners, the frames it will take, and the job followed, ~150; the plan's pane, the pattern, a workflow's form, the overlap, the region drawn, typed or from the corners, its estimate and warnings, ~240; the plan drawn over the overview, each tile's frame in order, with the rows, the columns and the overlap, ~170; the overview made a tiled image, and its tissue, ~80; the warnings, tiles past the stage's limits, no calibration and too little room, ~70; and the tile viewer's overlay of rectangles, and a region drawn on it with a drag, ~120; tests ~250: the tiles' drawing from a recorded plan, the warnings, the region from corners, the frames an overview takes; styles ~120; the try-it ~450: its words ~90, and its end-to-end tests ~360, an overview made, a region drawn giving the server's plan for it, each pattern's form submitting, axe, and a plan over the overview's baseline; glue ~60). That is 2 % over the spec's 1,800. It was begun in the sandbox while the gate change's gate ran. It is one pull request |
| 2026-11-21 | Outside the phases, at the owner's choice during W5.1's gate: **the Linux replay runs the end-to-end tests with six workers.** Since #106, its end-to-end step had failed two to six of them a run under WSL, a different set each time, all on load-sensitive waits; Playwright's own choice, half the CPUs, was twelve. A fresh WSL VM failed four as well; six workers failed one, in 4.1 min instead of 2.6. Windows keeps Playwright's choice. It is one pull request |
| 2026-11-21 | W5.2 was estimated at about 1,800 lines before it started (code ~880: the run's pane, the project and the name, how to focus, fixed, by autofocus every so many tiles, or by a focus map measured first, and the settle time, ~190; the progress on the plan, each tile done marked, a fault's tile, and when it should end, ~130; the scan's pane, its progress, pause, resume and cancel, its fault, and at its end the acquisition's record, stitching it as a job, and opening it in the Gallery, ~200; the live mosaic begun over the plan's region as the scan starts, and the view showing it under the plan, ~90; the tile viewer's look for tiles done and a fault, ~20; the progress recorded for the unit tests, ~120; and **server** work the spec didn't name, ~130: the scan made pausable, at each tile's start, and the moves to where the stage reads itself kept within its limits, which W5.1 found refused just after homing; tests ~330: the progress on the plan from recorded messages, and the run's request; in Rust, a pause holding the stage, and an overview and a scan just after homing; styles ~100; the try-it ~430: its words ~80, the recorder ~60, and its end-to-end tests ~290, a scan of the default sample run to its end, a pause holding the stage, a cancel leaving what was taken, the stitch job queued, axe, and a scan's baseline; glue ~60). That is 20 % over the spec's 1,500, under a third over it: the server's part wasn't in the spec. It was begun in the sandbox while W5.1's gate ran. It is one pull request |
| 2026-11-21 | W5.3 was estimated at about 1,580 lines before it started (code ~820: the steps, in order, each kept when going back, ~130; the tissue's islands from the overview's runs, proposed as regions, ~90; the regions' list, each accepted, dropped, edited or redrawn, ~170; the regions kept as one mask, which the plan scans, ~40; the focus map's step, its grid and its points on the overview, ~120; the map as the run measures it, each point's focus and the surface fitted through them, ~110; the tab's two flows, and the view's islands, regions and points, ~100; the planning's mask, ~60; tests ~280: the steps' state, the islands from runs, the mask, the map's points, and the surface from recorded points; styles ~80; the try-it ~360: its words ~70, the recorder ~50, and its end-to-end tests ~240, the regions proposed covering the simulator's tissue, an edited region being what is scanned, the map measured and shown, axe, and a baseline; glue ~40). The focus map is measured by the run, before its first tile, as P37's scan does: no server work. That is 32 % over the spec's 1,200, under a third over it. It was begun in the sandbox while W5.2's gate ran. It is one pull request |
| 2026-11-21 | W5.4 was estimated at about 800 lines (code ~300: the positions' list, each added with the stage's z and a name of its own, renamed, moved and removed, ~150; the stage's place beside *Add where the stage is*, ~20; a stack's planes in the estimate, as the server counts them, ~40; the positions as the server takes them, ~40; the plan's and the planning's types, ~50; tests ~120: the list's editing, the sizes' arithmetic; styles ~20; the try-it ~350: its words ~20, and its end-to-end tests ~330, a z-stack's planes at the heights asked for, the positions visited in their order, and axe; glue ~10). Checkpoint I's demo and checklist are docs. That is 33 % under the spec's 1,200: W5.1 made the z-scan's, the tiled stack's and the positions' forms. The estimate was written in the sandbox with the code's first draft, during W5.3's gate, and committed before it. It is one pull request |
| 2026-11-21 | W6.1 was estimated at about 1,740 lines before it started (code ~850: the Gallery's tab, its cards in a grid by pages, each with its name, thumbnail, tags, sample, dates and what it holds, ~260; finding by text and by tag, sorted by name or date, ~110; changing with the lease, create, rename, tag, describe, the sample, and delete confirmed with what would go, ~260; the grid walked with the arrow keys, empty, and loading, ~120; kept live by `projects/changed`, ~50; and **server** work the spec didn't name, ~50: the projects listed by name as well as by date; tests ~300: the grid's keys, the paging, and in Rust, the list sorted by name; styles ~150; the try-it ~400: its words ~100, and its end-to-end tests ~300, a project made elsewhere appearing, search, tag and rename reaching the server, delete asking first, axe, and the grid's and its empty state's baselines; glue ~40). That is 16 % over the spec's 1,500. It was begun in the sandbox while W5.4's gate ran. It is one pull request |
| 2026-11-21 | W6.2 was estimated at about 1,630 lines before it started (code ~800: the project's page, its acquisitions, results and reports, opened from its card and from a result's link, ~150; a record, its thumbnail, its metadata, the camera's settings, the objective, the stage's place, the calibration, the account and the time, its size and its pyramid's levels, ~250; a result's provenance, its recipe, its inputs and their hashes, ~100; downloads through the host adapter, stopped and resumed by ranges, ~180; deleting a record, and rebuilding the index, ~70; and the route, ~50; tests ~250: the provenance's display from a recorded result, and the downloads' ranges; styles ~120; the try-it ~420: its words ~100, and its end-to-end tests ~320, each kind of record listed with its metadata, a download's bytes the server's, a download stopped and resumed, axe, and a record's baseline; glue ~40). That is 16 % over the spec's 1,400; no server work, as the data's route answers ranges (P30). It was begun in the sandbox while W6.1's gate ran. It is one pull request |
| 2026-11-21 | W6.3 was estimated at about 1,850 lines before it started (code ~850: **server** work the spec names, ~250: the upload route on the media plane, `POST /api/v1/upload/{name}`, its name checked, its size limited, room kept on the scratch disk, the file staged in the scratch space and one cut short removed, ~150, a staged file removed once imported or after a day, ~50, an import from an upload, ~30, and the recordings waiting listed by `system`, ~20; and the app's, ~600: imports, a file dropped on a project's card or page, or chosen, uploaded with its progress, then imported as a job followed to its end, ~250, exports, a record's formats, the job, then its download, ~120, the recordings waiting, each ingested into a project chosen, ~150, and the API's calls, ~80; tests ~400: in Rust ~250, the route's limits, the lease, a staged file removed once used and after its lifetime, a name that tries to leave the staging folder, and the contract test, an upload cut short leaving nothing staged; unit ~150, the imports' states, the files the formats accept, and an export's file from its job's result; styles ~100; the try-it ~450: its words ~100, and its end-to-end tests ~350, a file dropped on a project uploaded and imported, its acquisition appearing, an export then its download, the server's bytes, a recording waiting ingested, axe, and the baselines; glue ~50). That is 23 % over the spec's 1,500. It was begun in the sandbox while W6.2's gate ran. It is one pull request |
| 2026-11-21 | W6.4 was estimated at about 2,000 lines before it started (code ~950: the viewer's page in the Gallery, opened from a record and from a scan's stitched image, ~150; a record's image placed on the stage, an acquisition's planes by their frames, a result by what it was made from, and its tiles from the deep zoom routes with its look, ~160; the display, the black and white points and gamma as the tiles' query, and a colour map for one channel applied to the tiles in the app, ~200; a stack's slider through its planes, ~60; two records side by side, their views linked, the tile viewer saying its view and following another's, ~180; full screen, ~40; the readout and the scale in µm, ~60; and the time a region takes to be sharp, measured, ~100; tests ~300: unit, the display's transform, the colour map on a tile's pixels, a record's placement and the readout, ~250, and the viewer's view said, ~50; styles ~150; the try-it ~550: its words ~80, its end-to-end tests ~350, tiles for each level, the stack's slider, linked views moving together, the colour map, axe and the baselines, and the measurement's script ~120; glue ~50). That is 25 % over the spec's 1,600. It is Checkpoint J: its demo and checklist go in its notes, and both replays run in full. It was begun in the sandbox while W6.3's gate ran. It is one pull request |
| 2026-11-22 | RUSTSEC-2026-0592, a stack overflow in `url` 2.5.8's `Url::origin()` on nested `blob:` URLs, made `cargo deny` fail on every branch. `url` reaches OScope only through `hayagriva`, the reports' bibliography crate, and nothing calls `Url::origin()`. At the owner's choice, `url` 2.5.9 and `form_urlencoded` 1.2.3, which it needs, are taken now, released 2026-11-20 at 23:03 UTC, under the two-week rule. They are checked again on 2026-12-04, once two weeks old: not yanked, the same licence, and `cargo deny check` passing |
| 2026-11-22 | Outside the phases, at the owner's request: **the live view's performance**, in three steps on one pull request, #113. [Its baseline](performance/live-view-baseline.md) measured each step from the simulated camera to the page; [the simulated camera's image on the GPU](performance/gpu-simulator.md) moved the camera's H&E tiles and exposure there; and [the live view at 14.5 fps](performance/live-view-fps.md) streams `ex-live-view` in mode `design` and takes the stage's lock out of each frame. **ADR-0062, *The simulated camera's image on the GPU*,** wasn't planned: it takes the next free number above the reserved 0059, after 0060 and 0061, and extends ADR-0013. |
| 2026-11-23 | W7.1 was estimated at about 1,250 lines before it started (code ~600: the recipe library in `oscope-pipeline`, stock recipes read-only beside the user's in the data root, names that would leave its folder refused, ~150; `check`, each fault with the node and the port it is at, ~120; the `pipeline` Thing, its catalogue, `check`, the library's routes and actions, `run` as `workspace.run` jobs, and `check` free of the lease, ~250; the client's wrappers, ~80; tests ~450: the library's round trip and its names, ~120, `check` on a cycle, a type mismatch and a missing input together, ~150, the Thing over HTTP, ~140, and the contract's wrappers, ~40; styles none; the try-it ~60, its `curl` session in the notes; glue ~140: the stock recipes' JSON, the server's wiring and the generated types' script). That is 4 % over the spec's 1,200. It is one pull request |
| 2026-11-23 | W7.2 was estimated at about 2,380 lines before it started (code ~1,150: the editor's page and its canvas on Vue Flow, ~250; a node, its ports coloured by type and its faults, ~150; the palette, searched, ~120; the recipe and the graph as one, each loading from the other, and the nodes' places kept in the recipe, ~180; undo, redo, copy and paste, ~100; the keyboard's path, the focus between nodes and ports and a connection made from it, ~150; `check` as the graph changes, its faults on their nodes and ports, ~60; a recipe's file imported and exported, ~50; the automatic layout, ~40; and **server** work the spec didn't name, ~50: a recipe's optional `layout`, its nodes' places, which the engine ignores; tests ~450: unit, the graph to the recipe and back for every stock recipe, undo and redo, the types' rule and the keyboard's moves, ~380, and in Rust, the layout kept, ~70; styles ~200; the try-it ~500: its words ~100, and its end-to-end tests ~400, a graph built by hand passing `check`, a broken one's faults shown, the keyboard's path, axe and the baselines; glue ~80: the packages, ADR-0053's lint rule and the tab). That is 19 % over the spec's 2,000. Vue Flow 1.48.2, its minimap 1.5.4, and `@dagrejs/dagre` 3.1.3 are MIT, each over two weeks old. It was begun while W7.1's gate ran. It is one pull request |
| 2026-11-23 | W7.3 was estimated at about 1,830 lines before it started (code ~890: a node's form in the control pane, SchemaForm on its parameters' schema, its defaults, ~180; what differs from the recipe saved, by node and parameter, ~80; the library: open, save, save under another name and delete, stock recipes opened as copies, names checked as the server checks them, ~260; leaving unsaved changes asks first, on New, Open, Import and leaving the tab, ~80; live: the recipe set as the live recipe and cleared, the live image beside the graph, its refusal said, ~150; each node's cost against the live budget, on its node, ~60; and **server** work the spec didn't name, ~80: `set_live_recipe`'s check gives each step's time from the run it measures, not only the whole recipe's; tests ~250: unit, what counts as changed, the costs' display and the library's names, ~150, in Rust, each step timed, ~70, and the contract's, ~30; styles ~180; the try-it ~450: its words ~100, and its end-to-end tests ~350, a parameter changed reaching the live image, save, reload and open giving the same recipe, unsaved changes asked about, axe and the baselines; glue ~60: the generated types and the wiring). That is 14 % over the spec's 1,600. It is one pull request |
