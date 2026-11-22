# Container development environment

R01 establishes a Linux x86_64 development container before any microscope code is
ported. It will contain the pinned Rust toolchain, CUDA compilation tools, Python
and uv. Tool checks and a required-device CUDA probe validate the environment;
they do not establish camera support or production readiness.

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

`tools` launches without a GPU reservation and verifies Linux x86_64, the pinned
Rust compiler, Cargo, rustfmt, Clippy, Python, uv, NVCC, NVRTC/cuFFT library loading,
and compilation/execution of a small Rust program. `gpu` additionally requires
GPU access, allocates pinned/device memory, launches a CUDA kernel, downloads and
verifies all 4,096 results. Missing hardware or an incorrect result fails.

The CUDA development base is pinned to its Linux amd64 manifest digest. Rust is
1.98.1 and uv is 0.13.6, matching the source baseline. Ubuntu package versions in
the development image are resolved at build time; this is not a bit-for-bit
reproducible or production-qualified release. Source ports add locked application
dependencies in R02.

## Interactive development

```sh
sudo bash scripts/dev.sh shell       # GPU access required
sudo bash scripts/dev.sh shell-cpu   # no GPU reservation
```

The same image is used in both modes. It contains CUDA tools in either mode;
`shell-cpu` verifies launch without GPU injection, not the backend's future CPU
fallback. There is no microscope server to run until R06.

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
probe locally for environment/CUDA changes. Backend, frontend and application
integration gates arrive with their ports; the old full replay is not restored.

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
