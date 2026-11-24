# Linux container migration plan

The owner approved this plan on 2026-11-22. The integration branch is
`codex/linux-container-rebuild`. It begins with a clean tracked tree and preserves
ancestry from `44bc08a53f98dc29db1fa6b488a4f6269732a71a` on `main`. Port from that
commit so each phase has a stable behavioral reference.

## Delivery and status

Each phase has a focused commit, a runnable check appropriate to its scope and
notes under `docs/phases/`. Record source paths, preserved behavior, commands,
results and unresolved evidence. Every phase includes a manual test procedure with
prerequisites, commands, expected results and any cleanup. Record owner approval
separately from automated evidence. Long phases may be split into named subphases.
Do not mark a phase complete because its files exist or because a hardware test
skipped. Keep the owner informed as implementation progresses.

| Phase | Deliverable | Status | Exit evidence |
|---|---|---|---|
| R00 | Clean branch, approved architecture and factory flow | Complete | Pinned source; plans and working rules saved |
| R01 | Docker development environment and tooling checks | Complete | Image built; tools and real CUDA kernel passed; missing-GPU check failed as required; see phase notes |
| R02 | Minimal Linux workspace, core and architecture checks | Approved by owner, 2026-11-22; hosted CI blocked | 41 tests; core-tour smoke; actual-manifest rejection; locked dependency parity; Linux container gate |
| R03 | Compute and algorithm libraries | Approved by owner, 2026-11-22 | CUDA-default and CPU-only gates; 13 GPU numerical tests; required-device rejection; source/lock parity; manual procedure |
| R04 | HAL, simulator, frame bus and live acquisition | Approved by owner, 2026-11-22 | Default/CPU-only gates; 15 numerical GPU tests; simulated acquisition metadata, fan-out and cancellation; manual procedure |
| R05 | Storage, recording, jobs and processing engine | Approved by owner, 2026-11-22 | Record/ingest/reopen; durable jobs; default/CPU gates; 3 pipeline GPU tests; real Python/native extensions; manual procedure |
| R06 | API and container runtime | Approved by owner, 2026-11-22 | Default/CPU gates; real TLS/API/WebSocket runtime checks; persistent settings/data; signals/restart; required GPU kernels; manual procedure |
| R07 | Calibration, scans and analysis workflows | Approved by owner, 2026-11-23 | Default/CPU gates; six scientific examples in three modes; required CUDA preview parity; real Python API tests; runtime workflows and restart persistence; manual procedure |
| R08 | Vue integration and separate final gates | Approved by owner, 2026-11-23; hosted CI blocked | 742 frontend tests; 285 browser passes; required CUDA delivery; separate timing/visual checks; independent router; HTTPS checkpoint; manual procedure |
| R09 | Real device and WSL hardware feasibility | Preparation approved by owner, 2026-11-23; physical qualification pending | Inventory tools/runbook; 9 fixture and 7 router tests on Linux/Windows; expected missing-hardware failures; physical SDK/acquisition/stage evidence pending |
| R10 | Qualified host baseline and provisioning | Tooling complete; clean-PC/cold-boot qualification deferred | Pinned candidate planner, resumable receipts, owner boot task and simulated CUDA/TLS lab; physical PC qualification pending |
| R11 | Factory calibration and acceptance runner | Planned | One reference microscope completes guided installation and physical acceptance |
| R12 | Release bundles, updates, rollback and inventory | Planned | Offline installation; verified image identity; compatible rollback; traceable reports |
| R13 | Production pilot and measured optimization | Planned | Representative pilot; performance budgets; accepted runtime optimizations |

R09 begins with a feasibility spike after R01 if the real camera and operator are
available. A sustained USB/SDK failure blocks commitment to the WSL production
target. It need not delay simulator and algorithm ports, but no hardware shipment
or platform qualification can pass without it. If WSL access cannot meet the
requirements, bring native Linux deployment or a Windows acquisition helper to the
owner as an explicit architecture decision.

## Port sequence and source mapping

| Phase | Legacy source to inspect and port |
|---|---|
| R02 | Root Cargo manifests/lock/toolchain, `crates/oscope-core`, `xtask` layering tables and tests, core tour |
| R03 | `oscope-compute`, `oscope-imaging`, `oscope-stitch`, `oscope-volume`, CUDA kernels and relevant examples |
| R04 | `oscope-hal`, `oscope-sim`, acquisition bus/device services, simulator and frame-bus examples |
| R05 | `oscope-store`, `oscope-jobs`, `oscope-pipeline`, `oscope-shm`, `oscope-ext-sdk`, `oscope-ext`, `oscope-report`, `python/oscope-ext` |
| R06 | `oscope-api`, `oscope-server`, configuration/authentication/media tests; replace service packaging with runtime packaging |
| R07 | `oscope-workflows`, remaining acquisition/preview services and scan/calibration/analysis examples |
| R08 | `web/` at the pinned commit; preserve behavior and existing tests; adapt only transport/test/deployment integration |
| R09 onward | Hardware survey, M3 specifications, packaging and release tests; real drivers were planned in the legacy source |

The legacy graph is not identical to the port order. For example, acquisition
depends on storage, pipelines and jobs, and API modules use workflows. Bring only
the dependency-closed subset needed for a runnable checkpoint; defer dependent
modules to their later phase. Split a phase or advance a prerequisite explicitly
rather than adding empty placeholder crates or restoring the full workspace.

Reuse the algorithm implementations and their references. Rebuild platform glue
around Linux container deployment. Preserve the original lock versions where
possible; review any necessary graph or license change. The frontend is ported
in R08 with its existing behavior; it is not rewritten or optimized.

## Independent gates

| Gate | Trigger | Required work |
|---|---|---|
| Documentation | Plans/runbooks change | Local links, source references, status consistency and diff checks |
| Environment | Container/tooling inputs change | Shell/config checks, image build, tools probe; real GPU probe on the workstation |
| Backend | Rust/Python sources or backend build inputs change | Formatting, Clippy, layering, affected tests/doc tests, affected example checks, dependency checks |
| Frontend | Frontend sources or tooling change | Types, lint, units, build and npm dependency checks; no Rust suite |
| Integration | API/media/authentication, UI or integration harness changes | API types/schema compatibility and relevant browser specs against isolated containers |
| GPU/hardware | Relevant kernels, drivers, scheduling or host inputs change | Explicit required-device tests with evidence; no silent skipping |
| Release | Candidate promotion | Full regression, reference microscope qualification, endurance, upgrade and rollback |

Early phases activate only the gates that have real implementation. At R08 the
path router must include committed, staged, unstaged, untracked, deleted and
renamed files locally, as well as shared configuration changes. Use stable gate
results when a category is not applicable. Never require Node for backend-only
quality checks or Rust compilation for frontend-only quality checks. Full
regression is a release/checkpoint gate, not the per-edit development loop.

## Production readiness checkpoints

1. R01: reproducible tools and a real GPU probe; no microscope application yet.
2. R06: runnable Linux server in the runtime image with durable state.
3. R08: existing application behavior through HTTP/WebSocket.
4. R09: exact real hardware works through Windows, USB forwarding, WSL and Docker.
5. R11: factory operator installs, calibrates and accepts one instrument without
   developer intervention.
6. R12: upgrade and rollback preserve instrument identity, calibration and data.
7. R13: qualified pilot is accepted for manufacturing use.

## Evidence and release constraints

- No production credential or configured instrument disk is used as a template.
- Test the same immutable runtime image that operations installs.
- Factory acceptance rejects simulated substitutions and CPU fallback for a
  microscope specified to include CUDA.
- Establish frame rates, corruption, stage accuracy, optics and latency limits
  from pilot measurements. Legacy measurements are references, not new limits.
- Bound temporary recordings. Preserve reports, selected evidence images and
  instrument history. Never erase customer datasets to recover from an update.
- The factory profile and test-runner version are recorded with each result.
- An image rollback is offered only with a compatible data/configuration schema
  or a validated restoration procedure that protects newer customer data.
