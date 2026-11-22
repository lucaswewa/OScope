# Working on the Linux container rebuild

## Scope and authorization

- The owner approved a clean branch, the Linux container architecture, and the
  factory workflow on 2026-11-22. Build the development environment, then port
  implementation phase by phase from the pinned source in README.md.
- Keep `main` and the legacy source commit intact. Do not merge, push, tag, or
  deploy to a customer instrument without the owner's instruction.
- The supported backend target is Linux x86_64, initially Ubuntu 24.04 in Docker.
  The Windows host uses WSL2. Do not add a native Windows backend gate.
- Prefer CUDA by default and preserve CPU fallback. A GPU qualification or factory
  test must require actual kernel execution and fail when a required test skips.
- Do not optimize or redesign the Vue frontend. Port it only when its phase is
  reached. The future Windows Tauri app uses HTTP/WebSocket transport.

## Phases and gates

- Follow [the migration plan](docs/migration-plan.md). Keep phase boundaries and
  source provenance explicit. Do not bulk-restore the old workspace.
- Before a port, record the source files, dependencies, preserved behavior and
  intended tests. Preserve numerical tolerances and meaningful regression tests.
- Give each phase a focused commit and notes with commands, outcomes and gaps.
  Mark a phase complete only when its exit criteria pass. Hardware access or a
  missing external tool is recorded as pending evidence, never as a pass.
- Every phase note must include a `## Manual test` procedure with prerequisites,
  commands, expected results, failure conditions and cleanup where needed. Keep
  commands usable in the WSL/container environment and distinguish GPU checks
  from CPU fallback. Record owner approval separately from automated validation.
- Backend and frontend checks are independent. Integration checks cover the API
  boundary. Long GPU, hardware, endurance and release checks are separate gates.
- Keep at least one runnable checkpoint at every application port phase.
- Early tooling phases run only their own checks. Do not resurrect the legacy
  Windows/WSL full replay or repeat the complete application suites for a docs edit.

## Implementation rules

- Preserve layered crates, with `teta-wot` confined to `oscope-api`. Add the
  architecture check when the Rust workspace is ported.
- Start with behavior parity. Resource sharing, GPU memory admission, persistence
  workers and kernel tuning are separate measured changes after the relevant port.
- Unsafe code stays confined to the approved compute, shared-memory, extension
  and device-FFI boundaries, with a `SAFETY:` comment for every unsafe block.
- Use LF line endings. Do not commit generated data, credentials, test recordings,
  build caches, vendor SDKs, or a configured instrument's disk image.
- Preserve the legacy dependency license policy and use versions at least two
  weeks old when adding dependencies. Keep existing lock versions where practical.
- Use simulated devices for software checks. Physical motion tests require the
  documented fixture, operator preparation and instrument-specific limits.
- Stop only processes started for the task. Do not delete volumes or instrument
  data as part of a test or update.

## Documentation

- Architecture decisions go in numbered ADRs; existing decisions remain readable
  at the legacy source commit. The next number after this branch's ADR is 0063.
- Keep development commands separate from factory/operator procedures.
- Distinguish designed capability, implemented behavior and verified evidence.
- Update the migration status and phase notes with each implementation commit.
