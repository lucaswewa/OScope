# Architecture decision records

One record per significant decision, so that later phases, and later readers, can see what was decided and why.

## Index

| ADR | Title | Status | Phase |
|---|---|---|---|
| [0001](0001-record-architecture-decisions-and-the-delivery-process.md) | Record architecture decisions, and the delivery process | Accepted | P00 |
| [0002](0002-clean-room-and-dependency-licences.md) | Clean room: what OScope may take from `microscope`, and the dependency licence policy | Accepted; one licence's exception in 0030 | P00 |
| [0003](0003-platforms-windows-and-ubuntu-x86-64.md) | Platforms: Windows 11 and Ubuntu LTS on x86_64 | Accepted; its CI superseded by 0038 | P00 |
| [0004](0004-teta-wot-v0-2-0-extended-upstream-first.md) | Depend on `teta-wot` v0.2.0, extended upstream first | Accepted | P00 |
| [0005](0005-layered-crates-and-their-dependency-rules.md) | Layered crates and their dependency rules | Accepted | P01 |
| [0006](0006-control-plane-and-media-plane.md) | Control plane and media plane | Accepted | P04 |
| [0007](0007-units-coordinates-and-the-clock.md) | Units, coordinates and the clock | Accepted | P02 |
| [0008](0008-the-frame-model-buffer-pool-and-frame-bus.md) | The frame model, buffer pool and frame bus | Accepted | P03 |
| [0009](0009-logging.md) | Logging | Accepted | P05 |
| [0010](0010-compute-backends-the-cpu-reference-and-cuda.md) | Compute backends: the CPU reference and CUDA through `cudarc` | Accepted | P06 |
| [0011](0011-building-loading-and-redistributing-cuda-kernels.md) | Building, loading and redistributing CUDA kernels | Accepted | P07 |
| [0012](0012-the-hardware-abstraction-layer.md) | The hardware abstraction layer | Accepted | P09 |
| [0013](0013-the-simulator-clock-camera-mounting-and-3d-samples.md) | The simulator: its clock, camera mounting and 3D samples | Accepted; extended by 0062 | P10 |
| [0014](0014-the-preview-protocol-jpeg-over-a-binary-websocket.md) | The preview protocol: JPEG over a binary WebSocket | Accepted; extended by 0035, 0026 | P17 |
| [0015](0015-the-sidecar-contract-service-mode-and-the-desktop-fast-path.md) | The sidecar contract, service mode and the desktop fast path | Accepted; its desktop app dropped by 0063 | P19 |
| [0016](0016-the-job-service.md) | The job service | Accepted | P20 |
| [0017](0017-the-pipeline-model.md) | The pipeline model | Accepted | P21 |
| [0018](0018-the-omext-package-format.md) | The `.omext` package format | Accepted | P24 |
| [0019](0019-the-rust-extension-abi.md) | The Rust extension ABI | Accepted; extended by 0036, 0037, 0025 | P25 |
| [0020](0020-the-python-extension-runtime.md) | The Python extension runtime | Accepted; extended by 0036, 0037, 0025 | P26 |
| [0021](0021-projects-ome-zarr-and-the-index.md) | Projects, OME-Zarr v3 and the SQLite index | Accepted; extended by 0027, 0029 | P28 |
| [0022](0022-scratch-recording-and-ingest.md) | Scratch recording and ingest | Accepted; extended by 0029 | P29 |
| [0023](0023-the-calibration-model.md) | The calibration model | Accepted | P32 |
| [0024](0024-autofocus.md) | Autofocus | Accepted | P34 |
| [0025](0025-scan-planning-and-the-scan-engine.md) | Scan planning and the scan engine | Accepted; extended by 0026 | P36 |
| [0026](0026-live-stitching.md) | Live stitching | Accepted | P38 |
| [0027](0027-post-acquisition-stitching.md) | Post-acquisition stitching | Accepted | P39 |
| [0028](0028-deep-zoom-serving.md) | Deep-zoom serving | Accepted | P40 |
| [0029](0029-volumes-and-3d-data-on-the-media-plane.md) | Volumes and 3D data on the media plane | Accepted | P41 |
| [0030](0030-reports.md) | Reports: the model, Typst and minijinja, templates and report generators | Accepted | P47 |
| [0031](0031-authentication-and-the-control-lease.md) | Authentication and the control lease: accounts, sessions, the lease and TLS | Accepted | P49 |
| [0032](0032-power-actions-and-their-privileges.md) | Power actions and their privileges | Accepted; extended by 0060 | P51 |
| [0033](0033-packaging-and-release.md) | Packaging and release: the Windows zip, the .deb, the notices and the release workflow | Accepted; its Tauri installer dropped by 0063 | P54 |
| [0034](0034-driver-errors-over-teta-wot.md) | Driver errors' HTTP statuses, and other names, over teta-wot | Accepted | P16 |
| [0035](0035-live-statistics-on-the-preview-stream.md) | Live statistics on the preview stream | Accepted; extended by 0026 | P18 |
| [0036](0036-extension-drivers-and-panels.md) | Extension drivers and panels | Accepted | P27 |
| [0037](0037-importers-exporters-and-the-abi-s-third-version.md) | Importers, exporters, and the ABI's third version | Accepted | P30 |
| [0038](0038-local-ci-replays-side-by-side.md) | Local CI replays, side by side on two CPUs | Accepted; extended by 0061 | — |
| [0039](0039-the-web-app-its-layers-state-and-routing.md) | The web app: its layers, state and routing | Accepted | W0.1 |
| [0040](0040-testing-the-app.md) | Testing the app: unit, contract, end to end and visual | Accepted | W0.1 |
| [0041](0041-design-tokens-and-themes.md) | Design tokens and themes | Accepted | W0.2 |
| [0042](0042-the-component-library-on-reka-ui.md) | The component library on reka-ui | Accepted | W0.3 |
| [0043](0043-generated-api-types-and-the-typed-client.md) | Generated API types and the typed client | Accepted | W1.1 |
| [0044](0044-things-in-the-app-live-properties-and-events.md) | Things in the app: live properties and events | Accepted | W1.2 |
| [0045](0045-the-app-s-connection-sessions-and-media-under-security.md) | The app's connection, its sessions, and media under security | Accepted | W1.3 |
| [0046](0046-the-host-adapter.md) | The host adapter | Accepted; its desktop host dropped by 0063 | W1.4 |
| [0047](0047-the-preview-client-a-worker-and-webgl.md) | The preview client: a worker and WebGL | Accepted | W1.5 |
| [0048](0048-the-shell-and-the-lease-s-read-only-mode.md) | The shell, and the lease's read-only mode | Accepted | W2.1 |
| [0049](0049-the-live-viewer-and-its-overlays.md) | The live viewer and its overlays | Accepted | W3.1 |
| [0050](0050-the-tile-viewer.md) | The tile viewer | Accepted | W3.8 |
| [0051](0051-uploads.md) | Uploads | Accepted | W6.3 |
| [0052](0052-recipes-over-the-api-the-pipeline-thing.md) | Recipes over the API: the `pipeline` Thing | Accepted | W7.1 |
| [0053](0053-the-graph-editor-on-vue-flow.md) | The graph editor on Vue Flow | Accepted | W7.2 |
| [0054](0054-3d-in-the-app-on-vtk-js.md) | 3D in the app, on VTK.js | Accepted | W7.7 |
| [0055](0055-accounts-over-the-api.md) | Accounts over the API | Accepted | W8.6 |
| [0059](0059-front-end-budgets.md) | Front-end budgets | Accepted | W10.3 |
| [0060](0060-restarts-in-a-dry-run.md) | Restarts in a dry run | Accepted | W4.3 |
| [0061](0061-gates-test-what-a-change-reaches.md) | Gates test what a change reaches | Accepted | — |
| [0062](0062-the-simulated-camera-s-image-on-the-gpu.md) | The simulated camera's image on the GPU | Accepted | The GPU simulator |
| [0063](0063-no-desktop-app.md) | No desktop app | Accepted | — |

The ADRs still to come, and the phase that writes each one, are listed in [plan §10](../implementation-plan.md#10-planned-adrs).

## How ADRs work

- **When.** Write an ADR when a phase makes a decision that later phases will build on or that would be costly to reverse. Examples: a data format, an interface, a protocol, a dependency, a policy.
- **Where.** In the phase that makes the decision, in the same pull request. Approving the pull request accepts the ADR, so it is merged with the status *Accepted*.
- **Numbers** run across the whole project (`NNNN`) and are never reused. The plan's §10 reserves numbers for the planned ADRs. An ADR that wasn't planned takes the next free number above the reserved ones, and the plan's change log records it.
- **Files** are named `NNNN-short-title.md`, from [the template](template.md).
- **After acceptance,** an ADR is never edited except for its status. A decision that changes gets a new ADR, and the old one is marked "Superseded by ADR-NNNN".
- **Upstream `teta-wot` phases** record their design decisions in their notes (`docs/milestone-1/notes/UNN-…`), not here, unless the decision is OScope's own.
