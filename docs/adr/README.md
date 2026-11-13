# Architecture decision records

One record per significant decision, so that later phases, and later readers, can see what was decided and why.

## Index

| ADR | Title | Status | Phase |
|---|---|---|---|
| [0001](0001-record-architecture-decisions-and-the-delivery-process.md) | Record architecture decisions, and the delivery process | Accepted | P00 |
| [0002](0002-clean-room-and-dependency-licences.md) | Clean room: what OScope may take from `microscope`, and the dependency licence policy | Accepted | P00 |
| [0003](0003-platforms-windows-and-ubuntu-x86-64.md) | Platforms: Windows 11 and Ubuntu LTS on x86_64 | Accepted | P00 |
| [0004](0004-teta-wot-v0-2-0-extended-upstream-first.md) | Depend on `teta-wot` v0.2.0, extended upstream first | Accepted | P00 |
| [0005](0005-layered-crates-and-their-dependency-rules.md) | Layered crates and their dependency rules | Accepted | P01 |
| [0006](0006-control-plane-and-media-plane.md) | Control plane and media plane | Accepted | P04 |
| [0007](0007-units-coordinates-and-the-clock.md) | Units, coordinates and the clock | Accepted | P02 |
| [0008](0008-the-frame-model-buffer-pool-and-frame-bus.md) | The frame model, buffer pool and frame bus | Accepted | P03 |
| [0009](0009-logging.md) | Logging | Accepted | P05 |
| [0010](0010-compute-backends-the-cpu-reference-and-cuda.md) | Compute backends: the CPU reference and CUDA through `cudarc` | Accepted | P06 |
| [0011](0011-building-loading-and-redistributing-cuda-kernels.md) | Building, loading and redistributing CUDA kernels | Accepted | P07 |
| [0012](0012-the-hardware-abstraction-layer.md) | The hardware abstraction layer | Accepted | P09 |
| [0013](0013-the-simulator-clock-camera-mounting-and-3d-samples.md) | The simulator: its clock, camera mounting and 3D samples | Accepted | P10 |
| [0014](0014-the-preview-protocol-jpeg-over-a-binary-websocket.md) | The preview protocol: JPEG over a binary WebSocket | Accepted | P17 |
| [0034](0034-driver-errors-over-teta-wot.md) | Driver errors' HTTP statuses, and other names, over teta-wot | Accepted | P16 |

The ADRs still to come, and the phase that writes each one, are listed in [plan §10](../implementation-plan.md#10-planned-adrs).

## How ADRs work

- **When.** Write an ADR when a phase makes a decision that later phases will build on or that would be costly to reverse. Examples: a data format, an interface, a protocol, a dependency, a policy.
- **Where.** In the phase that makes the decision, in the same pull request. Approving the pull request accepts the ADR, so it is merged with the status *Accepted*.
- **Numbers** run across the whole project (`NNNN`) and are never reused. The plan's §10 reserves numbers for the planned ADRs. An ADR that wasn't planned takes the next free number above the reserved ones, and the plan's change log records it.
- **Files** are named `NNNN-short-title.md`, from [the template](template.md).
- **After acceptance,** an ADR is never edited except for its status. A decision that changes gets a new ADR, and the old one is marked "Superseded by ADR-NNNN".
- **Upstream `teta-wot` phases** record their design decisions in their notes (`docs/milestone-1/notes/UNN-…`), not here, unless the decision is OScope's own.
