# R04 Simulated devices and acquisition

- Status: Complete locally; owner review pending
- Date: 2026-11-22
- Source: `44bc08a53f98dc29db1fa6b488a4f6269732a71a`
- Owner approval: Pending
- Scope: HAL, simulator, frame bus and live frame metadata

## Port specification

Port `oscope-hal` and `oscope-sim` with their existing conformance, physics,
rendering, motion and camera tests. Preserve algorithms and tolerances. Add CUDA
defaults and explicit feature forwarding without removing CPU reference paths.
Retain `configs/objectives.toml`, the HAL catalogue's reference fixture, and route
configuration changes through the backend gate. Reuse the source's Tokio/TOML
versions; keep only acquisition's actual dependencies in its reduced manifest.
The composed `Simulator` and camera example prefer CUDA, with CPU fallback.
Low-level `SimCamera::new` stays a controllable CPU reference until the caller
explicitly requests GPU preference or installs a specific GPU.

Port only `acquisition.rs`, `bus.rs` and their tests from `oscope-acquire`.
Its preview, statistics, recording, capture and mosaic modules remain deferred:
they require the pipeline/job/storage/shared-memory services in later phases.
Do not add those dependencies or empty placeholders to R04.

Port standalone HAL-fake, simulated-camera and frame-bus examples. Add one small
end-to-end acquisition checkpoint: simulated tissue/camera/stage through metadata
completion and the bus, verifying timestamps, position, objective, lights, frame
ordering, cancellation and returned frame slabs. Required CUDA must fail when
unavailable, rather than silently falling back.

## Implementation

The complete HAL and simulator are ported, along with the dependency-closed
acquisition bus and metadata modules. All original tests and algorithm/kernel
implementations retain their source contents and tolerances. The HAL catalogue
fixture is also retained. Recording and preview services are deferred until
their dependencies are available; the bus example's `recorder` is an in-memory
lossless subscriber, not a file recorder.

`SimCamera::prefer_gpu` initializes device zero and the existing 512-slot GPU
tile atlas, retaining CPU fallback on initialization or rendering failure. The
composed `Simulator` and normal camera/acquisition examples call it. Low-level
camera construction remains available as a CPU reference for equivalence tests.
CUDA defaults and feature forwarding are checked for all eight feature-bearing
members, including the simulator and its two runnable examples. A new test checks
the composed simulator's backend; the required-device gate rejects CPU selection.

`ex-acquisition` connects a seeded tissue camera and stage to acquisition and
lossless/latest-frame subscribers. Its smoke captures six quarter-size frames,
checks metadata and order, stops the camera, cancels the stage actor and verifies
all eight frame slabs return. `--require-cuda` rejects both unavailable CUDA and
a rendering-time fallback. This is a software checkpoint without a server,
physical driver or persistent recording.

## Manual test

### Prepare the environment

From PowerShell, open the existing Linux checkout:

```powershell
wsl -d Ubuntu-24.04 --cd /home/lucas/src/OScope-container
```

Check that `git branch --show-current` reports `codex/linux-container-rebuild`.
Run `sudo bash scripts/dev.sh build` if the development image is absent. The GPU
steps require the NVIDIA Windows driver and working WSL/container GPU access.
Allow at least 1 GB free RAM for the frame-bus example, in addition to compiler,
container and simulator memory. No microscope or USB attachment is needed.

### Inspect device behavior and frame fan-out

From WSL, enter the container without GPU injection:

```sh
sudo bash scripts/dev.sh shell-cpu
```

Inside the container:

```sh
cargo run -p ex-hal-fakes --locked -- --smoke
echo $?
cargo run -p ex-frame-bus --locked -- --smoke
echo $?
cargo run -p ex-sim-camera --locked -- --smoke
echo $?
```

Each must end with `smoke: ok` and exit `0`. The HAL example prints capabilities,
five ordered fake-camera frames, stage moves, lights and objective changes. Its
scripted failing camera is expected to fail conformance; the example verifies
that failure was detected and still ends successfully.

The bus example sends synthetic 20 MP frames for three seconds. Its lossless
recorder must receive every frame in order, the slower latest-frame preview must
drop frames, the pool must never exhaust, and measured throughput must stay above
90% of the synthetic 14.5 fps target. This exercises memory fan-out, not physical
camera throughput or persistence.

The simulated camera must report `renderer: CPU` here, then exercise full, bin2,
bin3 and design modes across their formats. Each row needs at least two frames,
delivered-plus-dropped rate within 2% of its mode rate, and position error below
0.5 micrometers. Drops are expected when rendering is slower than the sensor.

### Inspect live acquisition and runtime CPU fallback

Still in the same container, with CUDA compiled in but no GPU injected:

```sh
cargo run -p ex-acquisition --locked -- --smoke
echo $?
cargo run -p ex-acquisition --locked -- --require-cuda --smoke
echo $?
```

The first must report `renderer: CPU`, six ordered 1368 × 912 frames, zero
unplaced/refused frames, cancelled stage, stopped camera, pool `in_use: 0` and
`exhausted: 0`, then `smoke: ok` and exit `0`. Rendering may take longer on CPU.
The second must say `CUDA required but unavailable` and exit nonzero. That is
the expected rejection, not a successful GPU test.

### Inspect actual GPU acquisition

Exit to WSL and enter the GPU container:

```sh
exit
sudo bash scripts/dev.sh shell
```

Inside it:

```sh
cargo run -p ex-acquisition --locked -- --require-cuda --smoke
echo $?
OSCOPE_CUDA=1 cargo test -p oscope-sim --test backend --test gpu --locked -- --test-threads=1 --nocapture
echo $?
```

The checkpoint must report `renderer: CUDA` and the same successful metadata and
cleanup results. Required CUDA also checks the camera remains on GPU after
capture. The backend-selection test and two simulator GPU tests must pass without
`skipped:` messages. The numerical tests compare actual rendered frames against
CPU references, including binning, ROI, mounting, vignetting, colour response,
motion and noise statistics, with unchanged source tolerances.

### Run the complete software gates

Exit to WSL, then run:

```sh
exit
sudo bash scripts/dev.sh backend
echo $?
sudo bash scripts/dev.sh cpu-tests
echo $?
sudo bash scripts/dev.sh gpu-tests
echo $?
```

All exit codes must be `0`. The backend gate runs default CUDA-enabled builds
without device injection, all workspace software tests and the runnable examples.
The CPU gate verifies CUDA is absent from all eight current feature-bearing
members, then runs Clippy, workspace tests and CPU-only acquisition. The GPU gate
requires real CUDA and runs numerical algorithm/simulator checks plus acquisition.
The legacy ignored simulator performance benchmarks remain excluded. They are
not evidence for production frame-rate qualification.

Fail this procedure for an unexpected exit code, skipped required GPU body,
incorrect backend, incomplete metadata, order/position assertion, missing frame,
timeout, slab leak or pool exhaustion. Record commit, GPU/driver, selected backend,
test totals and gate exit codes. Retain logs under ignored `artifacts/r04/` if
desired. No recordings or persistent services are created; `exit` removes the
interactive container. Keep Cargo/build cache volumes. Physical camera, stage,
USB recovery and sustained hardware capture remain R09.

## Validation evidence

| Check | Result |
|---|---|
| `scripts/dev.sh backend` | Passed formatting, default CUDA features, Clippy, workspace tests/doc tests, Rust docs, layering, seven example smokes and dependency policy without GPU injection |
| HAL and acquisition | All 31 HAL tests and 8 acquisition bus/metadata tests passed; original test contents unchanged |
| Simulator software regression | Camera modes/triggers, pacing, clock, motion, physics, rendering, samples, optics, loaders and device tests passed; original tests unchanged |
| `scripts/dev.sh gpu-tests` | Passed 15 real numerical GPU tests, composed-simulator backend selection and required-GPU acquisition; no skipped-body messages |
| Required CUDA withheld | The CUDA-enabled acquisition checkpoint exited `1` with `renderer: CPU` and `CUDA required but unavailable` |
| Default runtime fallback | The CUDA-enabled checkpoint without GPU injection captured six frames successfully on CPU |
| `scripts/dev.sh cpu-tests` | Passed CUDA-feature absence for all eight members, Clippy, workspace tests and CPU-only acquisition; six frames with complete metadata and all slabs returned |
| Synthetic full-size frame bus | 44 of 44 frames delivered in order at 14.65 fps; preview dropped 28 frames; no pool exhaustion |
| Integrated CUDA acquisition | Six ordered 1368 × 912 frames; zero unplaced/refused; stopped camera, cancelled stage, pool in-use/exhausted both zero |
| Lockfile parity | 165 workspace/dependency packages; every retained registry version, source and checksum matches the pinned 654-package source lock; only `ex-acquisition` is a new package |
| Focused source parity | 85 source files audited; six intentional differences confined to simulator CUDA glue/manifests and camera-example code/docs; HAL, acquisition modules, original tests and simulator algorithms unchanged |
| Documentation and shell checks | Local Markdown links/manual-section requirement, shell syntax, Compose configuration and diff whitespace checks passed |

The GPU run used the RTX 3070 Ti Laptop GPU through the established WSL/container
environment. Persistent evidence is under `artifacts/r04/` in the WSL checkout:
`backend.log`, `backend-summary.json`, `gpu.log`, `missing-gpu.log`, `cpu.log`
and `parity.json`. Default workspace test output includes optional GPU test
bodies that return early without `OSCOPE_CUDA=1`; only the required-device log
provides GPU evidence. The two legacy simulator performance benchmarks stay
ignored, and the registration timing benchmark remains explicitly filtered out.

This phase uses simulated devices. Physical USB/camera/stage qualification,
sustained production capture and instrument acceptance remain pending in R09
and later phases. Hosted CI availability is recorded separately from local
container validation.

## Next phase

After owner acceptance, R05 ports storage, shared memory, jobs, processing,
extension workers and recording with their dependency-closed examples. R06 adds
the API/server runtime. Preview and higher-level microscope workflows follow
their dependencies; they are not placeholders in R04.
