# OScope Linux container rebuild

This branch rebuilds OScope's development and deployment foundation for a Linux
x86_64 backend in Docker. Production Windows control PCs run the backend through
WSL2. The Vue 3 frontend and future Windows Tauri 2 app connect over the API.

The owner approved this direction on 2026-11-22. This is a clean migration branch:
the existing microscope implementation is available in Git and will be ported in
small, independently verified phases. R02-R08 are approved. The existing Vue app
uses the Linux backend with separate frontend, backend and integration gates.
R09 hardware preparation is approved, with physical testing deferred. R10 adds
engineering host-provisioning tools; clean-PC/cold-boot qualification is deferred.
CUDA remains preferred with CPU fallback.
Every phase includes a manual test procedure in its notes.

## Start here

- [Approved architecture](docs/architecture.md): platform, CUDA, process and data boundaries.
- [Migration plan](docs/migration-plan.md): ordered phases, source mapping, gates and acceptance evidence.
- [Development environment](docs/development.md): container tools and validation.
- [Independent gates](docs/gates.md): select checks for the code you changed.
- [Hardware bring-up](docs/hardware-bringup.md): R09 inventory and deferred physical feasibility.
- [Host provisioning](docs/host-provisioning.md): R10 candidate installer and separate PC qualification.
- [Factory and field workflow](docs/factory-flow.md): installation, calibration, acceptance, updates and recovery.
- [Architecture decision](docs/adr/0062-linux-container-backend.md): the accepted platform decision.
- [Working rules](AGENTS.md): how work on this branch proceeds.

## Legacy source

| Item | Pinned reference |
|---|---|
| Source branch | `main` |
| Source commit | `44bc08a53f98dc29db1fa6b488a4f6269732a71a` |
| Last phase | W6.4, the deep-zoom viewer |
| New integration branch | `codex/linux-container-rebuild` |

Use the pinned commit when porting; `main` may subsequently advance. This branch
retains Git ancestry so individual files and their history remain recoverable.
For example:

```sh
git show 44bc08a53f98dc29db1fa6b488a4f6269732a71a:crates/oscope-core/src/lib.rs
```

The fresh tree deliberately starts with plans and development tooling. Hardware
qualification and unattended startup must pass before this architecture is used
to ship microscopes. See the phase status in the migration plan.

## Current runnable checkpoint

From the WSL Linux checkout:

```sh
sudo bash scripts/frontend.sh build
sudo bash scripts/frontend.sh check
sudo bash scripts/frontend.sh browser-build
sudo bash scripts/integration.sh
```

Integration consumes the approved `oscope-backend:r07` image without compiling
Rust. If it is absent, explicitly prepare a compatible image:

```sh
sudo bash scripts/integration-backend.sh
sudo env OSCOPE_BACKEND_IMAGE=oscope-backend:integration bash scripts/integration.sh
```

Follow the [R08 manual procedure](docs/phases/r08-frontend-integration.md#manual-test)
to open the persistent HTTPS app in Windows, test login/control, preview, scans,
stored images and restart. The app uses simulated devices; hardware and factory
qualification remain later phases. The [gate guide](docs/gates.md) explains the
short daily development loop and separate CUDA, timing and visual checks.

For backend calibration, focus, capture, scans and scientific reports, run
`sudo bash scripts/dev.sh workflows`. Follow the
[R07 manual procedure](docs/phases/r07-microscope-workflows.md#manual-test) to
qualify CPU/GPU previews, build and test the runtime image, and launch a persistent
simulated deployment. The [R05 procedure](docs/phases/r05-storage-processing.md#manual-test)
covers the processing libraries and real extension execution. See also the
[development instructions](docs/development.md). Backend and container runtime
gates run independently of frontend tooling. Use `scripts/dev.sh shell-cpu` for
interactive Rust examples and `scripts/dev.sh backend` for the full Rust gate.

## Factory acceptance preparation

Factory preparation now includes an [acceptance and rework runbook](docs/factory-acceptance.md)
and [R11 manual tests](docs/phases/r11-factory-acceptance.md#manual-test).
The runner exercises an isolated simulated CUDA backend; physical factory
acceptance remains pending hardware, host and release qualification.

## License

MIT. See [LICENSE](LICENSE).
