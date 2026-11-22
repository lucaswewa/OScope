# OScope Linux container rebuild

This branch rebuilds OScope's development and deployment foundation for a Linux
x86_64 backend in Docker. Production Windows control PCs run the backend through
WSL2. The Vue 3 frontend and future Windows Tauri 2 app connect over the API.

The owner approved this direction on 2026-11-22. This is a clean migration branch:
the existing microscope implementation is available in Git and will be ported in
small, independently verified phases. No microscope backend has been ported yet.

## Start here

- [Approved architecture](docs/architecture.md): platform, CUDA, process and data boundaries.
- [Migration plan](docs/migration-plan.md): ordered phases, source mapping, gates and acceptance evidence.
- [Development environment](docs/development.md): container tools and validation.
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

## License

MIT. See [LICENSE](LICENSE).
