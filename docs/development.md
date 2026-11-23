# Container development environment

The Linux x86_64 development container contains the pinned Rust toolchain, CUDA
compilation tools, Python, uv and cargo-deny. Tool checks and a required-device
CUDA probe validate the environment. The current application checkpoint uses
simulated devices, recording/ingest, durable jobs, recipe processing and the core
API/runtime container. Physical drivers and production readiness remain later
phases. See the [R06 manual procedure](phases/r06-api-runtime.md#manual-test) for
the separate runtime image, account setup and deployment acceptance check.

## WSL prerequisites

Use Windows 11, a current WSL2 kernel, an NVIDIA Windows driver compatible with
CUDA 13, and a dedicated Ubuntu 24.04 distribution. WSL must have systemd enabled.
The WSL distribution owns Docker Engine and NVIDIA Container Toolkit. No Docker
Desktop installation is required for this workflow. Do not install an NVIDIA
Linux display driver inside WSL.

Clone this branch to a Linux path such as `~/src/OScope`, using the repository's
configured Git remote. Keep builds and active datasets off `/mnt/c`. The Windows
checkout's ignored legacy recordings/build caches may remain after the clean
branch switch; they are not ported implementation or Docker build inputs.

From the Linux checkout, bootstrap the development host once:

```sh
sudo bash scripts/setup-wsl.sh
```

This installs Ubuntu's Docker/Compose/Buildx packages, configures NVIDIA's runtime
and restarts Docker. Run it before other container workloads. It does not change
Windows drivers, USB attachment, the WSL kernel, user group membership or GPU
firmware. If the distribution already uses Docker Desktop integration, use a
separate distribution for this Docker Engine workflow.

The bootstrap resolves host packages from configured repositories. A production
baseline will lock and qualify their versions in R10; this script is not a factory
installer. Inspect `docker version`, `docker compose version` and `nvidia-ctk
--version` when recording environment evidence.

## Build and verify

```sh
sudo bash scripts/dev.sh config
sudo bash scripts/dev.sh build
sudo bash scripts/dev.sh tools
sudo bash scripts/dev.sh gpu
```

Use `sudo` for the Docker socket unless your development account already has
access. Containers run as `oscope`, UID 1000, matching the usual WSL account. For
another source-owner UID use Dev Containers' UID adjustment or adapt the developer
identity explicitly; do not make the source world-writable.

Compose defaults to two concurrent Cargo jobs to fit cold Typst/storage builds
in the current 8 GB WSL instance. Set `OSCOPE_DEV_BUILD_JOBS` for a larger host.
Python extensions use the image's Python 3.12; uv does not download interpreters.

`tools` launches without a GPU reservation and verifies Linux x86_64, the pinned
Rust compiler, Cargo, rustfmt, Clippy, Python, uv, NVCC, NVRTC/cuFFT library loading,
and compilation/execution of a small Rust program. `gpu` additionally requires
GPU access, allocates pinned/device memory, launches a CUDA kernel, downloads and
verifies all 4,096 results. Missing hardware or an incorrect result fails.

The CUDA development base is pinned to its Linux amd64 manifest digest. Rust is
1.98.1, uv is 0.13.6 and cargo-deny is 0.20.2, matching the source baseline.
Ubuntu package versions in the development image are resolved at build time;
this is not a bit-for-bit reproducible or production-qualified release.
Application dependencies are locked in the ported workspace. Its default Cargo target is
`x86_64-unknown-linux-gnu`.

## Interactive development

```sh
sudo bash scripts/dev.sh shell       # GPU access required
sudo bash scripts/dev.sh shell-cpu   # no GPU reservation
```

The same image is used in both modes. It contains CUDA tools in either mode;
`shell-cpu` launches without GPU injection. The CUDA-enabled acquisition checkpoint
exercises runtime CPU fallback there; there is no microscope server until R06.

## Backend gate and simulated acquisition checkpoint

From the Linux checkout:

```sh
sudo bash scripts/dev.sh build
sudo bash scripts/dev.sh backend
```

The backend gate runs Rust formatting, Clippy with warnings denied, all current
workspace tests and doc tests, Rust documentation, the architecture checker,
standalone example smoke checks, and dependency licenses/advisories. It runs
without a GPU reservation, Node or browser installation. The current Rust ports
include core, compute, imaging, stitching, volume, architecture tooling,
HAL, simulator, acquisition/recording, storage, jobs, pipelines, reports and
extensions, plus runnable examples. Physical
device drivers and API services remain later phases.

R03 adds `sudo bash scripts/dev.sh gpu-tests` for required-device numerical
equivalence and `sudo bash scripts/dev.sh cpu-tests` for an explicit CPU-only
build. Normal builds contain CUDA and permit runtime CPU fallback. See the
[R03 manual procedure](phases/r03-compute-algorithms.md#manual-test) for algorithms
and the [R04 manual procedure](phases/r04-devices-acquisition.md#manual-test) for
devices, backpressure and live acquisition. See the
[R05 manual procedure](phases/r05-storage-processing.md#manual-test) for recording,
job recovery, processing and extensions. Each phase includes its own procedure.

R05 adds independent commands for changed backend subsystems:

```sh
sudo bash scripts/dev.sh pipeline-gpu
sudo bash scripts/dev.sh extensions
sudo bash scripts/dev.sh recording-perf
```

The first requires real CUDA recipe steps and runs pipeline equivalence tests.
The second runs locked Python SDK units, real worker IPC/recovery and native/Python
extension smokes. It fails when uv or required execution is unavailable. Extension
checks have their own path-triggered workflow; they are separate from the routine
Rust gate. A cold extension run downloads package wheels into the retained uv cache.
The third command isolates the original full-sensor recorder timing budget on an
idle host. Routine backend/CPU checks filter that single timing test while retaining
correctness/recovery/error tests. No original tolerance is relaxed; synthetic
throughput is separate from production hardware qualification.

Inside `scripts/dev.sh shell` or `shell-cpu`, try the checkpoint directly:

```sh
cargo run --package ex-acquisition --locked -- --smoke
cargo xtask check-layering
```

Acquisition prefers CUDA, with CPU fallback, and checks metadata and slab cleanup
for six frames. Add `--require-cuda` inside the GPU shell to reject fallback.
The core tour also remains available for units and a seeded affine fit; see
[R02 evidence](phases/r02-core-workspace.md).

The Compose service mounts source at `/workspace` and keeps Cargo registry/Git,
build output and uv caches in named volumes. No service port, USB device, Docker
socket or host IPC namespace is exposed. Development shared memory defaults to
1 GiB and can be set with `OSCOPE_DEV_SHM_SIZE`; production capacity is measured
later. Keep the volumes between development runs.

VS Code users may reopen the WSL checkout using
[the Dev Containers configuration](../.devcontainer/devcontainer.json). Its default
uses the GPU overlay. CPU-only development uses the explicit shell command above.

## Independent initial checks

```sh
python3 scripts/check-docs.py
for script in scripts/*.sh containers/*.sh; do bash -n "$script"; done
git diff --check
```

The documentation workflow does not build the image. The environment workflow
runs only for environment inputs and checks the image without a GPU. Run the GPU
probe locally for environment/CUDA changes. The backend workflow is limited to
backend/tooling paths and uses the same development image. Both container workflows
reuse a GitHub Actions image-layer cache; they load images locally rather than
publishing a release. Frontend and application integration gates arrive with
their ports. The old combined full replay is not restored.

## Troubleshooting

- `docker` missing: run the WSL bootstrap in the dedicated Ubuntu distribution.
- Socket permission denied: use the documented `sudo` commands or your team's
  existing Docker access configuration.
- NVCC succeeds but the GPU probe fails: check the Windows NVIDIA driver, WSL
  `/dev/dxg`, NVIDIA Container Toolkit configuration and the GPU overlay.
- A runtime reports an insufficient driver: qualify a compatible Windows driver;
  do not install a Linux display driver into WSL to resolve it.
- Slow builds or filesystem errors from `/mnt/c`: clone the branch into the WSL
  Linux filesystem and build there.

See [the phase plan](migration-plan.md) for source ports and
[the factory workflow](factory-flow.md) for production installation. R01 evidence
is recorded in [the phase notes](phases/r01-development-environment.md).
