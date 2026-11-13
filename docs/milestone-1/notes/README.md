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
| P16 | Device Things: camera, stage, illumination, objective, simulation | In review | [P16-device-things.md](P16-device-things.md) |
