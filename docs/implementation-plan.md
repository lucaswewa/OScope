# OScope implementation plan

| | |
|---|---|
| Status | Approved 2026-11-10; a living document, updated as phases land (Appendix C) |
| Covers | The whole product: architecture for milestones 1–3, Milestone 1 phase by phase, and outlines of Milestones 2 and 3 |
| Inputs | [`goal.md`](goal.md), the planning Q&A (recorded in [§3](#3-decisions-made-while-planning)), a study of `teta-wot` v0.1.0, and of `microscope` (ideas and look only, [D1](#3-decisions-made-while-planning)) |
| Companion | [milestone-1/phases.md](milestone-1/phases.md): the specification of every Milestone 1 phase |

## Contents

1. [OScope in one page](#1-oscope-in-one-page)
2. [In and out of scope](#2-in-and-out-of-scope)
3. [Decisions made while planning](#3-decisions-made-while-planning)
4. [Ground rules](#4-ground-rules)
5. [Architecture](#5-architecture)
6. [How phases work](#6-how-phases-work)
7. [Milestone 1 phase overview](#7-milestone-1-phase-overview)
8. [Milestone 2 outline: the web and desktop app](#8-milestone-2-outline-the-web-and-desktop-app)
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
| `preview` | Defaults for the preview and the live recipe; the stream itself is media plane |
| `live_mosaic` | Free-roam start, stop and clear, plus the mosaic's extent |
| `calibration` | Per-objective calibration sets, routines and the wizard's steps |
| `autofocus`, `exposure` | Autofocus, auto exposure and auto white balance (each a job) |
| `capture` | Single captures and bursts (jobs) |
| `scan` | Planners, plan preview, start (a job), live progress |
| `jobs` | Queue, history, progress, cancel, pause, reorder |
| `projects` | The Gallery: projects, acquisitions, results, thumbnails, metadata |
| `pipeline` | Node catalogue, recipes (CRUD), run (a job) |
| `workspace` | Analyses on datasets, results with provenance, reports |
| `extensions` | Installed packages; install, enable, disable |

**The media plane** is OScope's own axum routes, mounted beside the Things through teta-wot v0.2.0's router composition (U03), so they get the same authentication, CORS and shutdown handling.

| Route | Carries |
|---|---|
| `GET /api/v1/media/preview` (WebSocket) | Binary JPEG frames and binary masks; JSON messages for viewport and ROI control, statistics and dirty mosaic tiles |
| `GET /api/v1/media/dzi/{dataset}.dzi`, `…/{dataset}_files/{level}/{col}_{row}.jpg` | Deep-zoom tiles, made on demand from OME-Zarr multiscales and cached on disk |
| `GET /api/v1/media/mosaic/live/…` | The live mosaic's tiles |
| `GET /api/v1/media/volume/{dataset}/meta`, `/slice`, `/brick` | Volume metadata, orthogonal slices and bricks (binary, with a small header) for VTK.js |
| `GET /api/v1/data/{project}/{path}` | Downloads, with Range support |
| `POST /api/v1/upload/…` | `.omext` packages and sample images or stacks |

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
- **Falling behind.** Each client has a one-slot mailbox, so a slow client always gets the newest frame.
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

**Order.** P00 comes first, then U01–U06 one at a time, then P01 onwards. P02b runs whenever the owner is at the instrument, before P03.

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
| P13 | Samples II: reflective surface, fluorescent cell volume | P12 | L | — | In review |
| P14 | Samples III: sub-diffraction structures, imports, load and unload | P13 | M | — | Planned |
| P15 | Simulator at full resolution | P14, P07 | M | — | Planned |

### Stage 4: Live microscope (ends with Checkpoint A)

| ID | Phase | Depends on | Size | ADRs | Status |
|---|---|---|---|---|---|
| P16 | Device Things: camera, stage, illumination, objective, simulation | P05, P15 | L | — | Planned |
| P17 | Preview service and binary WebSocket protocol | P16, P08c | L | 0014 | Planned |
| P18 | Live statistics and overlay data | P17 | M | — | Planned |
| P19 | Sidecar contract, service mode, shared-memory preview | P17 | M | 0015 | Planned |

**Checkpoint A:** a simulated 20 MP microscope, live in a browser page with crosshair, ROI, profile, histogram, false colour and saturation, driven over HTTP.

### Stage 5: Jobs and pipelines

| ID | Phase | Depends on | Size | ADRs | Status |
|---|---|---|---|---|---|
| P20 | Job service and the `jobs` Thing | P16 | L | 0016 | Planned |
| P21 | Pipeline engine | P20, P07 | L | 0017 | Planned |
| P22 | Stock nodes I: correction and display; live recipes | P21, P08c | M | — | Planned |
| P23 | Stock nodes II: filters, enhancement, threshold, morphology | P22 | L | — | Planned |

### Stage 6: Extensions (ends with Checkpoint B)

| ID | Phase | Depends on | Size | ADRs | Status |
|---|---|---|---|---|---|
| P24 | `.omext` packages, registry, `extensions` Thing | P21 | M | 0018 | Planned |
| P25 | Rust extension ABI and SDK | P24 | L | 0019 | Planned |
| P26 | Python extension runtime and SDK | P24, P19 | L | 0020 | Planned |
| P27 | Driver and panel extensions | P25, P26 | L | — | Planned |

**Checkpoint B:** one recipe mixing stock, Rust and Python nodes runs live on the preview and as a batch job.

### Stage 7: Data, calibration and focus (ends with Checkpoint C)

| ID | Phase | Depends on | Size | ADRs | Status |
|---|---|---|---|---|---|
| P28 | Projects, OME-Zarr store, SQLite index | P20 | L | 0021 | Planned |
| P29 | Scratch recorder and ingest | P28 | M | 0022 | Planned |
| P30 | Capture, downloads, importers and exporters | P29, P27 | M | — | Planned |
| P31 | Registration: phase correlation on CPU and CUDA | P23 | M | — | Planned |
| P32 | Calibration I: pixel size, camera-to-stage affine, backlash, store | P30, P31 | L | 0023 | Planned |
| P33 | Calibration II: dark, flat, colour; the wizard | P32 | L | — | Planned |
| P34 | Autofocus, auto exposure, auto white balance | P33 | L | 0024 | Planned |

**Checkpoint C:** calibrate a simulated objective, autofocus, auto-expose, and capture a raw frame into a project; reopen it in napari.

### Stage 8: Scanning and stitching (ends with Checkpoint D)

| ID | Phase | Depends on | Size | ADRs | Status |
|---|---|---|---|---|---|
| P35 | Scan planners and focus maps | P34 | M | — | Planned |
| P36 | Scan engine, the `scan` Thing, workflow extensions | P35 | L | 0025 | Planned |
| P37 | Histo scan: overview and tissue detection | P36 | M | — | Planned |
| P38 | Live stitching: in scans and free-roam | P36 | L | 0026 | Planned |
| P39 | Post-acquisition stitching | P38 | L | 0027 | Planned |
| P40 | Deep-zoom serving, tile cache, DZI export | P39 | M | 0028 | Planned |

**Checkpoint D:** histo-scan a simulated H&E slide while the mosaic builds live, stitch it, and browse it in OpenSeadragon.

### Stage 9: 3D (ends with Checkpoint E)

| ID | Phase | Depends on | Size | ADRs | Status |
|---|---|---|---|---|---|
| P41 | Z-stacks, volume store, slice and brick routes | P36 | L | 0029 | Planned |
| P42 | Depth from focus and extended depth of field | P41 | L | — | Planned |
| P43 | Deconvolution and volume rendering | P42 | L | — | Planned |

**Checkpoint E:** a height-map surface of the reflective sample, and a deconvolved fluorescent volume, both in VTK.js.

### Stage 10: Analysis and reports (ends with Checkpoint F)

| ID | Phase | Depends on | Size | ADRs | Status |
|---|---|---|---|---|---|
| P44 | Annotations and measurements | P40 | M | — | Planned |
| P45 | Stain analysis | P44 | M | — | Planned |
| P46 | Cell detection and counting | P45 | L | — | Planned |
| P47 | Reports, and report extensions | P46 | L | 0030 | Planned |
| P48 | The `workspace` Thing: analyses on datasets, results, provenance | P47 | M | — | Planned |

**Checkpoint F:** count cells and quantify DAB on a stitched slide, then produce a PDF report.

### Stage 11: System and delivery (ends with Milestone 1)

| ID | Phase | Depends on | Size | ADRs | Status |
|---|---|---|---|---|---|
| P49 | Accounts, sessions, control lease, TLS | P48 | L | 0031 | Planned |
| P50 | Instrument status and health | P49 | M | — | Planned |
| P51 | Power actions | P50 | S | 0032 | Planned |
| P52 | Settings and configuration migration | P51 | M | — | Planned |
| P53 | Performance budgets and nightly soak | P52 | M | — | Planned |
| P54 | Packaging, release workflow, M1 retrospective | P53 | M | 0033 | Planned |

## 8. Milestone 2 outline: the web and desktop app

Specified phase by phase when M1 reaches Checkpoint E. About 40–45 phases, simulated hardware throughout.

| Stage | Scope |
|---|---|
| **W0 Foundations** | `web/`: Vue 3, TypeScript, Vite, Pinia, Vue Router (hash routing), reka-ui used only inside `src/ui/`, Material Symbols as bundled SVGs, vue-i18n, ESLint rules for a Tauri-ready app, Vitest and Playwright. Design tokens recreating `microscope`'s look (D1) in light, dark and system themes, with accent colours. A component library on reka-ui, including what `microscope` lacked: tabs, data tables, splitters, trees, context menus |
| **W1 Client plumbing** | The WoT client from Thing Descriptions, generated OpenAPI types checked in CI, the connection model (local or remote), login and the control lease, a `HostAdapter` (browser and Tauri), and the preview client: a binary WebSocket in a Web Worker, `createImageBitmap`, WebGL rendering at the display's refresh rate |
| **W2 Shell** | The NavRail: Control and Live View, Scan Workflow, Workspace, Gallery, Settings, Jobs, Logging, About, Power, and the instrument status indicator |
| **W3 Control and Live View** | Camera controls (exposure, gain, pixel format, binning, ROI), stage controls (position, moving, move, jog, click-to-move, keyboard), and the live viewer: zoom with full-resolution ROI, overlay layers (crosshair, ROI, line profile, false colour, saturation, histogram, scale bar), and the free-roam mosaic |
| **W4 Operations** | Jobs (queue, progress, logs, cancel, reorder), Logging (live tail, filters), About, Power, status details |
| **W5 Scan Workflow** | Histo, snake, raster, z-scan, tiled z-stack and multi-position; plan preview over an overview image; live mosaic; progress |
| **W6 Gallery** | Project cards, acquisition and result browser, metadata, the OpenSeadragon deep-zoom viewer with display controls, downloads |
| **W7 Workspace** | The node-graph editor (Vue Flow, MIT) with node forms from JSON Schema; running recipes; results; annotations and measurements; cell counting and stain analysis views; the VTK.js 3D viewer (surfaces, volumes, slices); reports |
| **W8 Settings** | System, display, camera, stage, calibration management and the calibration wizard, simulated samples (settings, load, unload, import), extensions, accounts |
| **W9 Desktop** | Tauri 2 in `desktop/`: the Tauri `HostAdapter`, sidecar management, the shared-memory preview path, tiles read from disk through a custom protocol, installers (MSI/NSIS and .deb) bundling the service |
| **W10 Quality** | End-to-end workflows, visual baselines in both themes, accessibility (axe), front-end performance budgets, M2 retrospective |

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
├─ milestone-2/                phases.md and notes/, written near the end of M1
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
