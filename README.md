[![CI](https://github.com/lucaswewa/OScope/actions/workflows/ci.yml/badge.svg)](https://github.com/lucaswewa/OScope/actions/workflows/ci.yml)

# OScope

OScope (OpenScope) is control and analysis software for a motorised brightfield microscope. It runs the instrument, from live view and stage control to scans, stitching, 3D and analysis, through a Rust service and a Vue 3 app that works in a browser or as a Tauri 2 desktop application.

## Status

**Planning is complete; Milestone 1 has started.**

- The plan was approved on 2026-11-10.
- Work proceeds one reviewed phase at a time, as the [implementation plan](docs/implementation-plan.md#7-milestone-1-phase-overview) shows.
- The Rust workspace is in place (P01): a skeleton of every Milestone 1 crate, with the layering rules checked and CI on Windows and Ubuntu. The crates fill in from P02.

## Building

Install [rustup](https://rustup.rs/): on Windows, with the MSVC toolchain; on Ubuntu, with `build-essential` and `pkg-config` first. Cargo then uses the toolchain that `rust-toolchain.toml` pins.

```sh
cargo build --workspace
cargo test --workspace                     # the layering test among them
cargo run -p oscope-server -- --version
```

The checks that CI runs are listed in [`CLAUDE.md`](CLAUDE.md#checks-before-opening-a-pull-request).

## The instrument

| Part | What |
|---|---|
| Camera | Tucsen MIChrome 20: colour, 5472 × 3648 (20 MP), 2.4 µm pixels, rolling shutter, 14.5 fps at full resolution in 8 bits and 7.2 fps in 16, USB only. A camera with an external trigger is to replace it |
| Stage | Motorised XYZ on a grblHAL controller over a serial port, with homing switches, linear encoders, and an aux output for a camera trigger |
| Objectives | Mitutoyo long-working-distance 5×, 10×, 20× and 50×, in a single mount |
| Light | Transmitted brightfield and reflected (coaxial); the sources aren't chosen yet, so they are simulated for now |

Everything also runs **without hardware.** A simulator stands in for every device, with procedural 3D samples (H&E tissue, a reflective surface, fluorescent cells, sub-diffraction structures) and real-time motion.

## How it's built

- **A layered Rust backend.**
  - It serves the instrument through [`teta-wot`](https://github.com/lucaswewa/teta-wot) (W3C Web of Things) for control, and its own routes for media.
  - It keeps raw 16-bit frames in memory or on an NVMe scratch disk, and sends the UI light, viewport-sized JPEG previews.
- **Processing** runs as graphs of nodes, on a CUDA GPU when one is present and on the CPU otherwise. Third parties extend it with `.omext` packages, in Rust or Python.
- **Data** is kept as projects in OME-Zarr, with deep zoom (DZI) and 3D volumes served to the app.
- **A Vue 3 + TypeScript frontend** (Vite, Pinia, Reka UI, Material Symbols), shared by the web build and the Tauri 2 desktop app, with OpenSeadragon for deep zoom and VTK.js for 3D.
- **Platforms:** Windows 11 and Ubuntu 22.04/24.04 LTS, both x86_64.

## Milestones

| Milestone | Delivers |
|---|---|
| 1: backend | The whole Rust backend against simulated hardware, with a runnable example in every phase |
| 2: frontend | The web and desktop app, against simulated hardware |
| 3: hardware | Real camera, stage and light drivers; continuous scanning; performance; release 1.0 |

## Documentation

- [Documentation index](docs/README.md)
- [Goals](docs/goal.md), in the owner's words
- [Implementation plan](docs/implementation-plan.md)
- [Milestone 1 phase specifications](docs/milestone-1/phases.md)
- [Architecture decision records](docs/adr/README.md)
- [Working rules](CLAUDE.md), for anyone contributing a phase

## Licence

[MIT](LICENSE). OScope is written independently: it reuses ideas and visual style from the owner's earlier [`microscope`](https://github.com/lucaswewa/microscope) project, but no code ([ADR-0002](docs/adr/0002-clean-room-and-dependency-licences.md)).
