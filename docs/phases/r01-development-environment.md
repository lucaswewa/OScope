# R01 Docker development environment

- Status: Complete
- Date: 2026-11-22
- Scope: environment tooling only; no legacy application port

## Implementation

Added a pinned Linux amd64 CUDA 13.4.1 development image, Rust 1.98.1, Python and
uv 0.13.6. Compose has an explicit GPU overlay and a launch mode without GPU
reservation. Development uses an unprivileged UID 1000 and persistent build caches.

Added Ubuntu/WSL Docker bootstrap, CLI/Dev Containers entry points, tool and real
GPU probes, and independent documentation/environment workflows. No microscope
runtime, factory installer, host supervisor or production configuration is claimed.

## Validation

| Check | Result |
|---|---|
| `python3 scripts/check-docs.py` | Passed local links in 9 Markdown files |
| Individual `bash -n` checks | Passed all 3 shell scripts |
| `bash scripts/setup-wsl.sh` as root | Reran successfully against the installed host; Docker remained available |
| `bash scripts/dev.sh config` | Both base and GPU Compose configurations passed |
| `bash scripts/dev.sh build` | Built the pinned Linux amd64 development image |
| `bash scripts/dev.sh tools` | Rust 1.98.1, rustfmt, Clippy, Python, uv 0.13.6 and NVCC 13.4.59 passed; NVRTC/cuFFT loaded; Rust compiled and ran |
| `bash scripts/dev.sh gpu` | Kernel ran on the real GPU and all 4,096 integer results matched |
| Base launch with `NVIDIA_VISIBLE_DEVICES=void`, then `oscope-check gpu` | Failed at device initialization as required, with `find CUDA devices failed`; no false pass or skip |
| Image user | `oscope`, not root |

The successful GPU probe reported driver interface 13050 and CUDA runtime 13040.
The local image identity is
`sha256:d57457b1e85d3a1004cc45e2c9a2059e3f612725d1b3ecc40558f54c2ba4922c`.
This is a development image, not a published or qualified production release.

The intentionally withheld GPU produced the CUDA message `CUDA driver version is
insufficient for CUDA runtime version`. The same image passed with GPU injection,
so this records rejection of missing device access, not a failed host driver.

## Host observations

Ubuntu 24.04 is installed as WSL2 with systemd. The Windows GPU is reported as an
NVIDIA GeForce RTX 3070 Ti Laptop GPU, driver 621.24. Docker was absent and was
installed for this phase: Engine 29.1.3, Compose 2.40.3 and NVIDIA Container Toolkit
1.19.0. The Windows checkout is not accessible through its `/mnt/c` path in this
session; validation uses `/home/lucas/src/oscope-linux-container-rebuild` on the
Linux filesystem. Build/tools/GPU/negative logs are in `/tmp/oscope-r01-*.log` in
that distribution. These observations are development evidence, not a qualified
production baseline.

## Next phase

R02 ports the minimal workspace, core types and architecture checks from the
pinned source. It must establish a locked graph and meaningful core tests before
compute and device services are brought over. No application code is included in
R01.
