# ADR-0003: Platforms: Windows 11 and Ubuntu LTS on x86_64

- Status: Accepted; its CI section superseded by [ADR-0038](0038-local-ci-replays-side-by-side.md)
- Date: 2026-11-11
- Phase: P00

## Context

goal.md asks for Linux and Windows.

- **The workload needs a workstation.**
  - The camera produces 20 MP frames at up to 14.5 fps: about 580 MB/s of 16-bit raw data.
  - Processing uses CUDA when an NVIDIA GPU is present.
- **The owner's development machine** is a Windows 11 laptop: an Intel i7-12700H and an RTX 3070 Ti laptop GPU. It runs Ubuntu 24.04 under WSL 2.
- **CI runners are standard GitHub-hosted runners,** with no GPU and no instrument.
- **Linux is a first-class target,** so supporting it from the start costs less than porting late.

## Decision

**Supported platforms**

| Operating system | Architecture |
|---|---|
| Windows 11 | x86_64 |
| Ubuntu 22.04 LTS | x86_64 |
| Ubuntu 24.04 LTS | x86_64 |

**Not supported:**

- macOS;
- Linux on ARM64, such as a Raspberry Pi or NVIDIA Jetson (D26);
- other Linux distributions, which may work but are not tested.

**CI**

- GitHub Actions runs on `windows-latest` and `ubuntu-24.04` for **every** pull request (D35).
- Release builds for Ubuntu are made on 22.04, so that one `.deb` runs on both LTS versions.

**Code**

- Platform-specific code lives in small `cfg` blocks, each with an equivalent for the other platform or a clear error.
- Line endings are LF everywhere, enforced by `.gitattributes`.

**Local Linux checks**

- On the owner's Windows machine, Linux is checked under WSL (`Ubuntu-24.04`).
- The Linux build uses its own `CARGO_TARGET_DIR` inside the Linux file system, so the two builds never share a target folder.

**Deployment**

- Windows: a Windows service; in M1, a zip with install scripts; from M2, the Tauri installer.
- Ubuntu: a `.deb` with a systemd unit.

**The reference workstation** for performance budgets:

- 8 or more cores;
- 32–64 GB of RAM;
- an NVIDIA GPU with 8 GB or more;
- an NVMe scratch disk of 1 TB or more.

## Consequences

- **Gains:**
  - Neither platform falls behind, and platform bugs show up in the pull request that causes them.
- **Costs:**
  - CI time doubles.
  - GPU paths are tested only on the owner's machine (`OSCOPE_CUDA=1`), never in CI.
- **USB under WSL.**
  - WSL doesn't see USB devices without extra tooling (usbipd-win).
  - In M3, hardware tests on Linux need either that tooling or a native Linux machine.

## Alternatives considered

- **Windows first, Linux in a later milestone:** less CI cost early, but platform assumptions spread through the code and the port becomes expensive.
- **Linux CI only when a pull request asks for it:** cheaper, but Linux is a primary target here, so problems would surface late.
- **Also NVIDIA Jetson (ARM64 with CUDA):** a third CI target and its own CUDA versions to track. Declined while planning (D26).
