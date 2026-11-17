# Milestone 1 implementation notes

One file per phase, written for the reviewer and for whoever works on the code next: what was built, how to try it, deviations from the plan, tests, follow-ups and the clean-room check.

- **OScope phases** write `PNN-short-name.md` in the phase's own pull request.
- **Upstream `teta-wot` phases** write `UNN-short-name.md` in a small OScope pull request of their own.
- **Every file starts from [the template](template.md).**
- **Status runs** In review → Done. The next phase's pull request marks the previous one *Done*, both here and on its notes' Status line.

| Phase | Title | Status | Notes |
|---|---|---|---|
| P00 | Docs scaffolding, working rules, foundational ADRs | Done | [P00-docs-scaffolding.md](P00-docs-scaffolding.md) |
| U01 | Linux support, Ubuntu CI, licence metadata (`teta-wot`) | Done | [U01-linux-ci.md](U01-linux-ci.md) |
| U02 | Path parameters for endpoints (`teta-wot`) | Done | [U02-path-params.md](U02-path-params.md) |
| U03 | Router composition, custom listeners, HTTP/2 (`teta-wot`) | Done | [U03-router-composition.md](U03-router-composition.md) |
| U04 | Binary WebSocket streams (`teta-wot`) | Done | [U04-binary-streams.md](U04-binary-streams.md) |
| U05 | Pluggable authentication (`teta-wot`) | Done | [U05-pluggable-auth.md](U05-pluggable-auth.md) |
| U06 | Release v0.2.0 (`teta-wot`) | Done | [U06-release-v0.2.0.md](U06-release-v0.2.0.md) |
| P01 | Rust workspace, layering test, CI | Done | [P01-workspace.md](P01-workspace.md) |
| P02 | Core types: units, geometry, time, IDs | Done | [P02-core-types.md](P02-core-types.md) |
| P02b | Hardware capability survey (docs only) | Done | [P02b-hardware-survey.md](P02b-hardware-survey.md) |
| P03 | Frame model, buffer pool, frame bus | Done | [P03-frame-model.md](P03-frame-model.md) |
| P04 | Server skeleton, `system` Thing, media plane, example helper | Done | [P04-server-skeleton.md](P04-server-skeleton.md) |
| P05 | Logging and the `logs` Thing | Done | [P05-logging.md](P05-logging.md) |
| P06 | Compute abstraction, CPU backend, benchmark harness | Done | [P06-compute.md](P06-compute.md) |
| P07 | CUDA backend | Done | [P07-cuda.md](P07-cuda.md) |
| P08 | Demosaicing on the CPU | Done | [P08-demosaic.md](P08-demosaic.md) |
| P08b | Colour, display transform, crop and binning, writers | Done | [P08b-colour.md](P08b-colour.md) |
| P08c | CUDA twins of the colour pipeline, the budgets, the comparison page | Done | [P08c-cuda-colour.md](P08c-cuda-colour.md) |
| P09 | HAL traits, capabilities, objective catalogue, manual implementations | Done | [P09-hal-traits.md](P09-hal-traits.md) |
| P09b | HAL fakes and conformance suite | Done | [P09b-hal-fakes.md](P09b-hal-fakes.md) |
| P10 | Simulated world, optics, H&E tissue | Done | [P10-sim-world.md](P10-sim-world.md) |
| P11 | Simulation clock and grblHAL-like stage | Done | [P11-sim-stage.md](P11-sim-stage.md) |
| P12 | Simulated camera: sensor, modes, timing, triggers | Done | [P12-sim-camera.md](P12-sim-camera.md) |
| P13 | Samples II: reflective surface, fluorescent cell volume | Done | [P13-sim-3d-samples.md](P13-sim-3d-samples.md) |
| P14 | Samples III: sub-diffraction structures, imports, load and unload | Done | [P14-sim-samples.md](P14-sim-samples.md) |
| P15 | Simulator at full resolution | Done | [P15-sim-full-resolution.md](P15-sim-full-resolution.md) |
| P16 | Device Things: camera, stage, illumination, objective, simulation | Done | [P16-device-things.md](P16-device-things.md) |
| P17 | Preview service and binary WebSocket protocol | Done | [P17-preview.md](P17-preview.md) |
| P18 | Live statistics and overlay data | Done | [P18-live-stats.md](P18-live-stats.md) |
| P19 | Sidecar contract, service mode, shared-memory preview | Done | [P19-sidecar-service-shm.md](P19-sidecar-service-shm.md) |
| P20 | Job service and the `jobs` Thing | Done | [P20-jobs.md](P20-jobs.md) |
| P21 | Pipeline engine | Done | [P21-pipeline.md](P21-pipeline.md) |
| P22 | Stock nodes I: correction and display; live recipes | Done | [P22-stock-nodes.md](P22-stock-nodes.md) |
| P23 | Stock nodes II: filters, enhancement, threshold, morphology | Done | [P23-filters.md](P23-filters.md) |
| P24 | `.omext` packages, registry, `extensions` Thing | Done | [P24-omext.md](P24-omext.md) |
| P25 | Rust extension ABI and SDK | Done | [P25-rust-ext.md](P25-rust-ext.md) |
| P26 | Python extension runtime and SDK | Done | [P26-python-ext.md](P26-python-ext.md) |
| P27 | Driver and panel extensions | Done | [P27-drivers-panels.md](P27-drivers-panels.md) |
| P28 | Projects, OME-Zarr store, SQLite index | Done | [P28-projects-store.md](P28-projects-store.md) |
| P29 | Scratch recorder and ingest | Done | [P29-scratch-recorder.md](P29-scratch-recorder.md) |
| P30 | Capture, downloads, importers and exporters | Done | [P30-capture-io.md](P30-capture-io.md) |
| P31 | Registration: phase correlation on CPU and CUDA | Done | [P31-registration.md](P31-registration.md) |
| P32 | Calibration I: pixel size, camera-to-stage affine, backlash, store | Done | [P32-calibration.md](P32-calibration.md) |
| P33 | Calibration II: dark, flat, colour; the wizard | Done | [P33-calibration-ii.md](P33-calibration-ii.md) |
| P34 | Autofocus, auto exposure, auto white balance | Done | [P34-autofocus.md](P34-autofocus.md) |
| P35 | Scan planners and focus maps | Done | [P35-scan-planners.md](P35-scan-planners.md) |
| P36 | Scan engine, the `scan` Thing, workflow extensions | Done | [P36-scan-engine.md](P36-scan-engine.md) |
| P37 | Histo scan: overview and tissue detection | Done | [P37-histo-scan.md](P37-histo-scan.md) |
| P38 | Live stitching: in scans and free-roam | Done | [P38-live-stitching.md](P38-live-stitching.md) |
| P39 | Post-acquisition stitching | Done | [P39-post-stitching.md](P39-post-stitching.md) |
| P40 | Deep-zoom serving, tile cache, DZI export | Done | [P40-deep-zoom.md](P40-deep-zoom.md) |
| P41 | Z-stacks, volume store, slice and brick routes | Done | [P41-volumes.md](P41-volumes.md) |
| P42 | Depth from focus and extended depth of field | Done | [P42-depth-from-focus.md](P42-depth-from-focus.md) |
| P43 | Deconvolution and volume rendering | Done | [P43-deconvolution.md](P43-deconvolution.md) |
| P44 | Annotations and measurements | Done | [P44-annotations.md](P44-annotations.md) |
| P45 | Stain analysis | Done | [P45-stain.md](P45-stain.md) |
| P46 | Cell detection and counting | Done | [P46-cells.md](P46-cells.md) |
| P47 | Reports, and report extensions | Done | [P47-reports.md](P47-reports.md) |
| P48 | The `workspace` Thing: analyses on datasets, results, provenance | Done | [P48-workspace.md](P48-workspace.md) |
| P49 | Accounts, sessions, control lease, TLS | Done | [P49-auth.md](P49-auth.md) |
| P50 | Instrument status and health | Done | [P50-status.md](P50-status.md) |
| P51 | Power actions | In review | [P51-power.md](P51-power.md) |
