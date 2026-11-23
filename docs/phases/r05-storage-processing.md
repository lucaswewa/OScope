# R05 Storage, processing and recording

- Status: Approved
- Date: 2026-11-22
- Source: `44bc08a53f98dc29db1fa6b488a4f6269732a71a`
- Owner approval: Accepted on 2026-11-22
- Scope: durable data, jobs, recipes, extensions and bounded recording

## Port specification

Port `oscope-store`, `oscope-jobs`, `oscope-pipeline`, `oscope-shm`,
`oscope-ext-sdk`, `oscope-ext`, `oscope-report` and `python/oscope-ext`, with
their regression tests, NGFF schemas, report templates/fonts and license notices.
Preserve algorithms, persistence formats, job transitions, numerical tolerances,
extension ABI and worker protocol. Reuse pinned Cargo/uv dependency versions,
except the security correction documented below.
Retain the source optimization setting for CRC checksums used by recording.
Retain the source's reviewed Typst advisory/license exceptions with their reasons;
do not silently widen the dependency policy for new code.

Port acquisition's `recorder.rs`, `ingest.rs` and recorder tests, with only their
actual dependencies. Preview, statistics, capture and mosaic remain R07. Port
the standalone pipeline and recording examples, the native extension example's
library/main and sample Rust node, and Python node example/sample packages.
Defer its API-server integration test to R06/R07. Python cell/report/workflow
packages are protocol fixtures, not a port of scan orchestration.

Normal pipeline/extension/example builds enable CUDA with explicit forwarding
and CPU fallback. Keep CPU-only compilation meaningful. Add required pipeline
GPU equivalence checks; do not rerun unchanged R03/R04 GPU checks as substitute
evidence. Test Linux shared memory and owner-only files, removing Windows mapping
and ACL implementations. Only Linux x86_64 may select a native extension host;
cross-platform package metadata can remain readable for artifact compatibility.

Python workers use the container's Python 3.12 interpreter rather than downloading
a managed Python. Preserve package age limits and optional offline wheelhouses.
Add a required extension gate for SDK unit tests, real Rust/Python shared-memory
IPC, worker crash/timeout recovery and actual native library loading. Missing uv
or skipped required extension tests must fail. Keep this separate from the quick
backend loop. No API, frontend or production deployment is included.

## Implementation

The seven planned service crates and Python SDK are ported. Storage retains its
project/index, annotations/measurements, image/pyramid formats, recording log and
recovery behavior. Jobs retain their durable SQLite history and worker admission;
the engine retains recipe validation, whole/tiled/live/batch execution, memoization
and stock nodes. Reports retain their embedded templates/fonts and sandbox tests.
Acquisition adds only recording and job-based ingest to its R04 bus/metadata.

CUDA is enabled by default through sixteen feature-bearing crates/examples;
dependency defaults remain disabled in workspace references so the CPU-only gate
really removes CUDA. The pipeline example prints its backend and adds
`--require-cuda`, checking both availability and an actual CUDA step. Its GPU
gate runs three unchanged equivalence tests independently of earlier kernels.
The recording example uses the shared simulator preference/fallback method.
The unchanged full-sensor recorder pacing test is filtered from routine default
and CPU-only runs and exposed as `scripts/dev.sh recording-perf`. It writes about
1.2 GB in two seconds and needs an idle host. Data, recovery and queue/error tests
stay in the normal gate; the timing test's checks and tolerances are unchanged.

Shared memory now uses only Linux mappings and owner-only files. The Windows
mapping/ACL implementations are removed, and unsupported SHM host targets fail
explicitly. Windows library keys remain readable as legacy package metadata;
only Linux x86_64 selects a native backend host. The frontend uses network
transport rather than these process IPC files.

Python environments use the image interpreter with `--no-managed-python`; the SDK
development version is 3.12. Its lockfile is unchanged. The required extension
gate runs the original worker tests with `OSCOPE_PYTHON=1`, SDK tests and actual
native/Python examples. Missing uv no longer turns the Python example into a
successful skip. A separate path-triggered extension workflow avoids adding
worker installation to the routine local Rust gate. Compose bounds cold Cargo
build concurrency to two jobs by default for the current 8 GB WSL instance.
SDK unit tests use a temporary environment outside the checkout, so a hosted
runner's source-owner UID does not need to match the container's UID. The gate
is additionally checked with the source bind mount read-only.

Original Rust/Python tests, algorithms, schemas, templates, fonts and ABI/protocol
contents remain unchanged. Ported guides explain current defaults; references to
unported guides/ADRs link to the pinned legacy tree. The native example's server
test and API-dependent examples remain with the later API/workflow ports.

### Necessary dependency correction

The first complete backend run passed tests, documentation, layering and smokes,
then failed dependency policy on the newly published
[RUSTSEC-2026-0592 advisory](https://raw.githubusercontent.com/RustSec/advisory-db/main/crates/url/RUSTSEC-2026-0592.md).
The retained `url 2.5.8` can abort a process when computing certain nested URL
origins. Update only `url` to `2.5.9` and its required `form_urlencoded` to `1.2.3`.
Their registry publication timestamps are both 2026-11-20
([url metadata](https://crates.io/api/v1/crates/url/2.5.9),
[form_urlencoded metadata](https://crates.io/api/v1/crates/form_urlencoded/1.2.3)); this is a documented
exception to the normal two-week age rule for an existing dependency security fix.
Do not ignore this advisory. All other retained Cargo versions/checksums and the
SDK lockfile must match the source. The updated graph requires renewed backend,
report/extension and dependency-policy evidence before R05 is complete.

## Manual test

### Prepare the environment

From PowerShell:

```powershell
wsl -d Ubuntu-24.04 --cd /home/lucas/src/OScope-container
```

In WSL, verify the branch is `codex/linux-container-rebuild`. Build the development
image if absent with `sudo bash scripts/dev.sh build`. Keep the checkout on the
Linux filesystem. Allow about 8 GB for WSL and temporary recording data on disk;
cold builds include Typst and take longer than warm runs. Compose defaults to
two concurrent Cargo jobs (`OSCOPE_DEV_BUILD_JOBS` overrides this on larger hosts).
GPU checks need the NVIDIA Windows driver and WSL/container access. Extension
checks need package-index access on the first run to populate uv's cache. The
SDK's own dependencies use its existing frozen lockfile. No physical microscope,
browser, API server or Node installation is required.

### Record, ingest and reopen

Enter the container without GPU injection:

```sh
sudo bash scripts/dev.sh shell-cpu
```

Inside it:

```sh
cargo run -p ex-record --locked -- --smoke
echo $?
cargo test -p oscope-store --test recordings --test ingest --locked -- --test-threads=1
echo $?
cargo test -p oscope-acquire --test recorder --locked -- --test-threads=1 --skip the_recorder_keeps_up_with_the_test_sensor_at_the_design_rate
echo $?
```

All exit codes must be `0`. The smoke records quarter-size synthetic stripes for
1.5 seconds in a temporary directory. Expect at least 14 frames, zero drops,
successful job-based OME-Zarr ingest, one plane per recorded frame and
`ex-record --smoke: ok`. It reopens the first plane, checks every pixel against
the stripes and verifies ingest removed the committed temporary scratch recording.
The tests cover reopen, cut-short recovery, rolling bounds, corruption rejection,
queue draining, bounded bursts and write failure without a stuck producer. Only
test-owned temporary data is removed; this is not physical-camera throughput.

Run the separate synthetic throughput budget on an idle host from WSL:

```sh
exit
sudo bash scripts/dev.sh recording-perf
echo $?
sudo bash scripts/dev.sh shell-cpu
```

Expect the single exact test to pass with exit `0`: every full-size frame written, correct pixels, no
drops, bounded queue and the original pacing bound. It temporarily writes about
1.2 GB. A timing failure means this host/run did not meet the synthetic budget;
do not relabel it as a correctness pass or production qualification. The final
command re-enters the container for the following steps.

### Inspect recipes, durable jobs and shared memory

Still inside the container:

```sh
cargo run -p ex-pipeline --locked -- --smoke
echo $?
cargo test -p oscope-jobs --test jobs --locked -- --test-threads=1
echo $?
cargo test -p oscope-shm --test shm --locked -- --test-threads=1
echo $?
```

Expect `backend: CPU`, `smoke: ok` and exit `0`. The default recipe must reuse
its memoized second run, invalidate the expected steps after an edit, give the
same tiled result, write outputs and complete its batch job. Jobs tests verify
ordering, worker/GPU budget admission, cancellation, pause/resume, progress and
failure. Their restart test reopens history and marks unfinished work interrupted.
Shared-memory tests verify consistent frames, ownership/unlink cleanup, segment
round trips and mode `0600` for private files.

Require CUDA while still withholding the GPU:

```sh
cargo run -p ex-pipeline --locked -- --recipe examples/ex-pipeline/recipes/bin-and-measure.json --require-cuda --smoke
echo $?
```

This must fail with `CUDA required but unavailable` and a nonzero exit code.
The earlier successful CPU run retains CUDA in the build: it tests runtime fallback.

### Require pipeline CUDA execution

Exit to WSL:

```sh
exit
sudo bash scripts/dev.sh pipeline-gpu
echo $?
```

Expect exit `0`, three numerical GPU tests, no `skipped:` messages, and the
bin-and-measure recipe to print `backend: CUDA`, execute at least one CUDA step
and end with `smoke: ok`. Tests compare graph transfers, stock image nodes and
filters with CPU references, retaining original tolerances. Missing CUDA must
fail. `sudo bash scripts/dev.sh gpu-tests` remains the full qualification command;
the focused gate is sufficient after a pipeline-only change with earlier kernels
unchanged.

### Require Python and native extension execution

From WSL:

```sh
sudo bash scripts/dev.sh extensions
echo $?
```

Expect exit `0` and no `skipped:` messages. This runs frozen SDK unit tests and
sets `OSCOPE_PYTHON=1` for all eight Rust/Python integration tests. They install
real environments and exercise shared-memory nodes, import/export, worker
failure/crash/restart, timeout, idle shutdown, driver conformance and the cells
stub. The native example must load its built Linux `.so`, validate the ABI and
compare whole/tiled output with the extension's original code. The Python
denoising example must improve the seeded image by more than 3 dB. Both examples
end with `smoke: ok`. Missing uv, a dependency, worker or library is a failure.

### Complete backend and CPU-only checks

From WSL:

```sh
sudo bash scripts/dev.sh backend
echo $?
sudo bash scripts/dev.sh cpu-tests
echo $?
```

Both must exit `0`. The default gate runs without GPU injection and includes
record/ingest and pipeline smokes, Rust docs/layering and dependency policy. Its
optional GPU/Python test bodies are not device/worker evidence; use the required
gates above. The CPU gate verifies CUDA is absent from all feature-bearing
workspace members, then runs Clippy/tests and CPU-only acquisition/pipeline checkpoints.

Fail for incorrect data, unrecovered recording/job state, a stuck producer,
unexpected exit code, skipped required body, missing CUDA step, leaked worker or
shared-memory name, or failure to load either extension. Record commit, GPU/driver,
gate exit codes and test totals; keep logs under ignored `artifacts/r05/`.
Smokes/tests clean their temporary data and workers. Retain the SDK's ignored
`.venv` and cache volumes; exit interactive shells to remove their containers.
Do not remove existing instrument datasets. Production capture, factory acceptance,
release installation and the API server remain later phases.

## Validation evidence

| Check | Result |
|---|---|
| `scripts/dev.sh backend` | Passed formatting, Clippy, software tests/doc tests, Rust docs, layering, nine runnable smokes and dependency policy; GPU withheld |
| `scripts/dev.sh cpu-tests` | Passed feature absence for all 16 members, Clippy, software tests and CPU-only acquisition/pipeline smokes |
| `scripts/dev.sh pipeline-gpu` | Three actual CUDA equivalence tests passed; required recipe executed CUDA steps; no skipped bodies |
| Required GPU withheld | CUDA-enabled pipeline exited `1` with `backend: CPU` and `CUDA required but unavailable` |
| `scripts/dev.sh extensions` | 13 SDK units, eight real Rust/Python IPC/recovery tests, native `.so` and Python worker smokes passed; no skipped bodies |
| Read-only checkout | Revised SDK harness passed the same extension gate with a read-only source bind mount and a separate writable target volume |
| `scripts/dev.sh recording-perf` | Original full-sensor pacing/data test passed in isolation; one exact test, unchanged tolerance |
| Recording/ingest checkpoint | 22 synthetic frames, zero drops; 22 reopened 1368 × 912 OME-Zarr planes in three levels; stripe pixels and scratch cleanup verified |
| Native extension | ABI-checked `.so` loaded; whole/tiled outputs matched the source implementation to the bit |
| Python denoising | Real worker improved the seeded sample from 22.0 dB to 35.8 dB, exceeding the original 3 dB requirement |
| Source/test parity | 152 source files audited; original Rust/Python tests, algorithms, schemas, templates, fonts and ABI/protocol unchanged; documented glue/manifests/guides differ |
| Dependency review | 497 retained Cargo packages; only the two documented security-fix versions differ from the source; checksums verified against registry metadata; SDK uv lock identical |
| Environment | Compose configuration, tool probe and locked Python 3.12.3 SDK environment passed; development image unchanged |
| Final documentation | Final formatting and SHM Rust docs passed after the wording edit; Markdown links/manual sections, shell syntax, Compose and diff checks passed |

The synthetic recorder timing assertion missed its bound in one run after a cold
build (71.54 ms mean interval). It is now isolated from routine correctness;
its standalone gate passed without changing test content or tolerance. The
original simulator ignored performance benchmarks remain ignored. Default runs
also contain optional GPU/Python bodies that return early; only the explicit
required gates above qualify actual device/worker execution.

The GPU run used the RTX 3070 Ti Laptop GPU through WSL/container access. Logs and
audit JSON are retained under ignored `artifacts/r05/` in both checkouts, including
`backend.log`, `cpu.log`, `pipeline-gpu.log`, `extensions.log`,
`extensions-readonly.log`, `recording-perf.log`, `missing-gpu.log`, `parity.json`
and `dependency-audit.json`. Hosted job results are separate from this local
container evidence. Physical camera/stage capture, production throughput, factory
acceptance and release deployment remain later qualification work.

## Next phase

After owner acceptance, R06 ports the dependency-closed API/server surface and
runtime image: health/readiness, authenticated control/media, durable paths,
shutdown/restart and container configuration. Workflow-dependent endpoints follow
their R07 prerequisites; the Vue frontend remains R08.
