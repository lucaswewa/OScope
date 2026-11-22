# ADR 0062 Linux container backend

- Status: Accepted by the owner on 2026-11-22
- Phase: R00
- Source baseline: `44bc08a53f98dc29db1fa6b488a4f6269732a71a`

## Context

The original backend targets Windows and Linux, while combined backend/frontend
gates slow development. The owner selected a clean migration branch with a Linux
container backend, Windows WSL2 deployment, preferred CUDA with CPU fallback and
independent gates. Production is about 300 microscopes per year and needs
repeatable installation, physical acceptance and field updates.

## Decision

1. Build the backend for Linux x86_64, initially Ubuntu 24.04 and CUDA 13.4.1.
2. Use Docker Engine in a dedicated WSL2 distribution on Windows control PCs.
   Qualify actual camera/SDK/USB/GPU behavior and unattended startup before shipment.
3. Preserve the layered Rust design and port from the pinned source in ordered
   phases. Start with plans and development tooling, not the old implementation.
4. Include CUDA in normal builds, prefer GPU processing and preserve CPU fallback.
   GPU qualification and factory acceptance require verified device execution.
5. Use HTTP/WebSocket across Windows/Linux. Keep shared memory internal to Linux
   workers. Do not assume Windows Tauri can map Linux shared memory.
6. Separate backend, frontend, integration, hardware and release gates. Preserve
   the Vue frontend's behavior and exclude frontend optimization.
7. Version host baseline, application and instrument configuration independently.
   Install immutable verified artifacts and preserve instrument state on updates.
8. Provide factory provisioning, calibration/acceptance evidence, offline delivery,
   inventory and qualified update/rollback procedures.

## Consequences

The backend has one supported binary environment and consistent container tools.
The product still owns the Windows/WSL lifecycle, USB forwarding, GPU compatibility
and storage performance. CUDA compilation is required in the development image;
CPU fallback does not remove build-tool requirements.

The existing implementation remains on `main`. Previous ADRs are available at the
source commit. This decision replaces their conflicting Windows backend support,
native sidecar/shared-memory and combined-gate requirements for this branch.
Production hardware support remains unqualified until its phase provides evidence.

If USB/SDK acquisition cannot satisfy the instrument's requirements, an explicit
follow-up decision chooses native Linux or a Windows acquisition component. Do
not claim WSL production readiness from simulator or generic GPU tests alone.
