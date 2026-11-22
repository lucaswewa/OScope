# R02 Minimal workspace and core

- Status: Complete locally; hosted CI blocked
- Owner approval: Accepted on 2026-11-22
- Date: 2026-11-22
- Source: `44bc08a53f98dc29db1fa6b488a4f6269732a71a`
- Scope: Linux Rust workspace, core vocabulary, architecture rules and core tour

## Port specification

Port `crates/oscope-core/` and `examples/ex-core-tour/` from the pinned source,
including their existing tests. Preserve units, affine geometry, timestamps,
identifiers, document versions, pooled frames and metadata without algorithm
changes. No compute, simulator, API, server or frontend crate is included.

Port the dependency-layer tables and checker from `xtask/src/lib.rs` and
`xtask/tests/layering.rs`. Exclude the legacy performance/soak commands because
their backend dependencies have not been ported. Adapt the actual-workspace test
to the current members, retain the synthetic rule-violation tests, and provide a
small `cargo xtask check-layering` command.

The root manifest lists only the three ported members and their dependencies.
Start from the legacy lockfile and prune it without upgrading retained packages.
Set the default build target to `x86_64-unknown-linux-gnu`. Core types remain
portable; application support is Linux-only and no Windows gate is introduced.

## Intended checks

- Rust formatting and Clippy with warnings denied.
- Existing core tests, documentation tests and generated API documentation.
- Layering checker on the actual workspace and synthetic broken dependency graphs.
- Core-tour example with `--smoke`.
- Linux dependency licenses/advisories with the retained policy and applicable
  legacy exceptions only.
- Exact retained-package versions compared with the source lockfile.
- Intentional layering violation detected by the actual manifest-based checker.
- Local document links, shell syntax and clean diff.

Run the backend gate in the Linux development container without Node or browser
installation. Record results and unresolved checks before marking R02 complete.

## Implementation

The workspace contains `oscope-core`, `ex-core-tour` and `xtask`. Core and tour
contents match the pinned source exactly, including the existing tests. The
architecture checker retains its layer tables and negative graph tests; only its
performance-module reference, current-workspace presence assertion, metadata lock
flag and CLI were adapted.

The Cargo lockfile contains 68 packages, down from 654 in the source workspace.
Every retained package version, source and checksum matches the source lockfile.
Target-specific transitive lock entries remain because Cargo resolves them even
when only Linux is supported. No new application dependency was introduced.

Added a Linux-only backend gate and path-filtered workflow. It uses the development
container and does not install Node or browsers. The image now includes pinned
cargo-deny 0.20.2 and is tagged `oscope-development:r02`. Environment/backend image
builds share a GitHub Actions layer cache without publishing application images.

## Manual test

The R02 checkpoint exercises units, geometry, clocks, frame types and architecture
rules through the command line. Use the existing WSL checkout and development
image. For a fresh machine, first follow [development setup](../development.md).

### Open the development container

From Windows PowerShell:

```powershell
wsl -d Ubuntu-24.04 --cd /home/lucas/src/OScope-container
```

In the WSL shell:

```sh
git branch --show-current
sudo bash scripts/dev.sh shell-cpu
```

The branch should be `codex/linux-container-rebuild`. If the image has not been
built, run `sudo bash scripts/dev.sh build` from WSL before opening the container.
The container's working directory is `/workspace`.

Core functions do not require a GPU. `shell-cpu` launches without GPU injection,
so the CUDA base image's driver-not-detected warning is expected in this mode.

### Inspect the core tour

Inside the container:

```sh
cargo run -p ex-core-tour --locked -- --smoke
```

Check the printed results:

| Item | Expected at R02 |
|---|---|
| Length conversion | 25 mm becomes 25,000 micrometres |
| Exposure | 2.5 ms |
| Fitted pixel size | Approximately 0.24000 micrometres per pixel |
| Fitted rotation | Approximately 0.502 degrees |
| Image flip | `true` |
| Fit residual | Approximately 0.037 micrometres RMS |
| Far-corner error | Approximately 0.032 micrometres |
| Fake-clock exposure midpoint | `0.070250000 s` |
| Final result | `smoke: ok` |

Run the command again. The geometry results should be identical because the noise
uses a fixed seed. The system-clock start and current timestamps change on each
run. The example returns a nonzero exit code if its built-in checks fail.

### Check architecture and core behavior

Still inside the container:

```sh
cargo xtask check-layering
cargo test --workspace --locked
```

Expect `Layering passed for 3 workspace packages` and 41 passing tests in total,
with no failures or ignored tests. The total is spread across multiple test
binaries. Coverage includes frame ownership and pool reuse, strides, Bayer
layout, geometry, clocks, identifiers, document versions and dependency rules.
These package/test counts describe R02; later ports add more.

### Run the full R02 backend gate

Exit the container to return to WSL, then run:

```sh
exit
sudo bash scripts/dev.sh backend
echo $?
```

The exit code printed immediately after the gate must be `0`. The gate checks
formatting, Clippy, tests, Rust documentation, layering, the tour and dependency
policy. The duplicate `getrandom` warning is permitted by the retained policy;
failed checks stop the gate. Dependency/advisory checks may need network access.

### Optionally check the GPU environment

From WSL:

```sh
sudo bash scripts/dev.sh gpu
```

Expect `CUDA kernel passed: 4096 values` followed by the actual GPU and driver
information. This checks the development environment's real CUDA execution; the
R02 core tour itself runs on the CPU. A missing GPU or an incorrect result must
fail this optional environment check rather than skip it.

Record the commit tested, tour result, architecture result, test counts and gate
exit code. Record the GPU result separately if that check was run.

## Validation evidence

| Check | Result |
|---|---|
| Development image build | Passed, including cargo-deny 0.20.2 |
| `bash scripts/dev.sh backend` | Passed formatting, Clippy with warnings denied, tests, docs, layering, smoke and dependency policy |
| Core and architecture tests | 41 passed, none failed or ignored |
| `cargo xtask check-layering` | Passed for all 3 workspace packages |
| `cargo run -p ex-core-tour --locked -- --smoke` | Passed units, seeded affine fit and clock checks |
| Dependency checks | Advisories, bans, licenses and sources passed; retained duplicate `getrandom` versions are warnings under the original policy |
| Source parity | Core/tour match the source tree exactly; all 68 retained lock entries match versions/sources/checksums |
| Environment tools and CUDA probe | Passed; real GPU verified 4,096 kernel results on the RTX 3070 Ti Laptop GPU |

## Hosted CI

The initial branch push triggered the documentation and environment workflows for
`bcd66ab`. GitHub rejected both jobs before any steps ran. Its job annotation
reports failed account payments or an insufficient spending limit and points the
account owner to Billing and plans. These failures do not provide code-validation
results. The R02 local checks above passed; hosted execution remains pending the
account setting being resolved.

- [Documentation run](https://github.com/lucaswewa/OScope/actions/runs/45061301496)
- [Environment run](https://github.com/lucaswewa/OScope/actions/runs/45061301518)

For the actual-manifest negative check, temporarily removed `[lints] workspace =
true` from the core manifest in the isolated WSL checkout and ran the architecture
CLI. It exited unsuccessfully with `oscope-core doesn't take the workspace lints,
which forbid unsafe code`. Restored the original manifest immediately. This
checks Cargo metadata/manifest inspection as well as the synthetic rule tests.

Logs are `/tmp/oscope-r02-backend.log`, `/tmp/oscope-r02-build.log`,
`/tmp/oscope-r02-tools.log`, `/tmp/oscope-r02-gpu.log` and
`/tmp/oscope-r02-layering-negative.log` in Ubuntu-24.04. The host is a development
workstation, not a qualified instrument. Hosted CI results are tracked separately
from this local validation evidence.

## Next phase

R03 ports compute and algorithm libraries with CPU references and CUDA equivalence.
It must define CUDA feature propagation and application fallback checks without
adding unported device/API dependencies. Resource consolidation and kernel tuning
remain separate measured changes.
