# R07 Microscope workflows

- Status: Approved
- Date: 2026-11-22
- Source: `44bc08a53f98dc29db1fa6b488a4f6269732a71a`
- Owner approval: Accepted on 2026-11-23
- Prerequisite: R06 approved by owner on 2026-11-22

## Port specification

Port `crates/oscope-workflows` from the pinned source, including autofocus and
exposure, calibration fitting/procedures/photometry/storage/wizard, scan planning
and execution, histology scanning, stitching, depth, deconvolution, stains, cells,
workspace and report runners. Preserve the implementations, regression tests,
reference data and numerical tolerances. CUDA is the normal feature selection;
explicit CPU-only builds and runtime fallback remain supported.

Complete `oscope-acquire` with capture, preview/rendering/recipe/statistics,
live mosaics and the internal shared-memory publisher, with their source tests.
Restore the deferred API modules, Things, workflow job registrations, workspace
and deep-zoom routes/tests/snapshots. Restore source capability expectations where
R06 deliberately exposed a smaller API. Retain authenticated readiness, origin
checks, control leases, Linux signals, restart exit 75 and absent host power
actions. The deployed server uses HTTP/WebSocket previews; it does not restore
the Windows managed sidecar or a host-accessible shared-memory transport.

Port the Rust smoke examples `ex-calibrate`, `ex-autofocus`, `ex-capture`,
`ex-scan`, `ex-histo-scan` and `ex-checkpoint-f`, with the shared HTTP helper.
Copy existing static page fixtures unchanged where those smoke checks serve
them. Omit the helper's obsolete managed-sidecar module and unused vendor route. Do not port or
optimize `web/`, install frontend dependencies or add native Windows backend gates.

Keep the dependency graph locked to retained source versions/checksums and R05's
reviewed URL security fix. Keep `teta-wot` confined to the API crate. Audit the
focused file set and feature forwarding before recording completion.

Validate workflow scientific reference tests, acquisition/preview and full API
regressions in default and explicit CPU modes. Keep the longer simulated
calibration/scan/analysis examples and required GPU checks independently runnable.
Run an authenticated simulated workflow through the actual Linux runtime image,
including binary preview media and durable scientific outputs, and preserve
R06's deployment acceptance checks. Record image identity and source revision.
Physical microscope motion, hardware feasibility and manufacturing acceptance
remain R09-R11; measured engine optimizations remain R13.

## Manual test

Prerequisites: a Linux checkout of this branch in Ubuntu 24.04 WSL2, Docker
Engine/Compose and the R01-qualified NVIDIA Windows driver/container toolkit.
Use a development PC and simulated devices. No microscope connection, physical
motion, Node installation or Vue build is needed. Run the shell commands below
from the Linux repository root. The first builds and Python environment creation
need the configured registries/package index; offline bundles remain R12 work.
Use `sudo` for Docker unless your account already has socket access.

1. Check both the physical Windows disk and the Linux filesystem before a cold
   build. In Windows PowerShell, run `Get-PSDrive C`; in WSL, run `df -h /` and
   `sudo docker system df`. Ensure the host has room for the build, images and
   growing WSL virtual disk. Guest free space alone is insufficient evidence.
   Do not proceed through read-only filesystem or I/O errors. Do not delete
   deployment volumes to make room.

   ```sh
   git branch --show-current
   sudo bash scripts/dev.sh config
   sudo bash scripts/dev.sh build
   sudo bash scripts/runtime.sh config
   ```

   Expect this migration branch, valid CPU/GPU Compose profiles and a successful
   development image build. Keep logs under ignored `artifacts/r07/`.

2. Validate the port and scientific reference tests in both feature modes:

   ```sh
   sudo bash scripts/dev.sh backend
   sudo bash scripts/dev.sh cpu-tests
   ```

   Both commands must exit zero. Expect formatting, strict Clippy, feature
   forwarding, reference/unit/API tests and their existing numerical tolerances
   to pass. The backend command also checks documentation, architecture, examples
   and dependency policy. It uses a CUDA-enabled build without GPU injection;
   `cpu-tests` compiles explicitly without CUDA. Neither counts as GPU evidence.
   Existing ignored throughput benchmarks and opt-in worker/GPU bodies do not
   count as executed qualifications. The full-sensor recorder pacing test keeps
   its independent R05 command.

3. Run the longer simulated microscope checkpoints and real Python integrations:

   ```sh
   sudo bash scripts/dev.sh workflows
   sudo bash scripts/dev.sh workflows-cpu
   sudo bash scripts/dev.sh workflow-extensions
   ```

   Expect all six examples (`ex-calibrate`, `ex-autofocus`, `ex-capture`,
   `ex-scan`, `ex-histo-scan`, `ex-checkpoint-f`) to complete and exit zero in
   both feature modes. They check recovered simulator geometry/backlash,
   focus/exposure, capture/export/import, scan patterns and positions, tissue
   selection, stitching, stain/cell analysis and generated reports. Checkpoint F
   retains its source requirements, including more than 50 nuclei, a positive
   fraction between 0.2 and 0.6 and valid PDF/HTML sections. A timed-out example
   fails the command. The extension command must actually execute the Python
   spiral planner and report generator; a missing uv/worker or skipped execution
   is a failure. These are separate from the routine Rust gate.

4. Require CUDA and compare GPU preview output to the CPU reference:

   ```sh
   sudo bash scripts/dev.sh workflows-gpu
   ```

   Expect the required-device diagnostic to execute real CUDA kernels, the exact
   preview test `the_gpu_s_previews_are_the_cpu_s` to execute successfully and all
   six examples to finish. The preview test compares JPEG output byte for byte
   across four viewports/ROIs. No-device success, CPU fallback in that required
   test, a skipped required test or changed reference tolerance is a failure.
   Other existing numerical GPU checks remain available as `gpu-tests` and
   `pipeline-gpu`.

5. Build and qualify the actual deployment image, including both preview paths:

   ```sh
   sudo bash scripts/runtime.sh build
   sudo docker image inspect oscope-backend:r07 \
     --format '{{.Id}} {{index .Config.Labels "org.opencontainers.image.revision"}}'
   sudo python3 scripts/check-runtime.py --image oscope-backend:r07 \
     --report artifacts/r07/manual-runtime-cpu.json
   sudo python3 scripts/check-runtime.py --image oscope-backend:r07 --gpu \
     --report artifacts/r07/manual-runtime-gpu.json
   ```

   Both reports must say `status: passed`, with the same immutable image ID and
   full source revision. A clean checkout embeds its commit; a dirty development
   build reports `unknown` and is not a release identity. The CPU run checks
   fallback and rejection by the required-device diagnostic. The GPU run requires
   CUDA kernels and an actual GPU-rendered preview session, rather than merely
   detecting a device. Both verify authenticated TLS/HTTP/WebSocket, control and
   origins, OSPV binary framing and JPEG media, a dark calibration, capture,
   positioned scan, stitched deep zoom and generated PDF/HTML/model files.
   Calibration, completed jobs, scan frame counts and report hashes must survive
   both SIGTERM/start and Docker-supervised API restart. R06 isolation, account,
   certificate, configuration, session/token rotation, SDK and worker checks stay
   enabled. Host reboot/shutdown stay unavailable.

   The preview stream follows the pinned framework's browser flow: HTTP upgrade
   101, then `{"authorization":"Bearer …"}` as the first WebSocket message.
   Missing or invalid first-message credentials must close with code 1008 before
   media is sent. A hostile origin must receive HTTP 403. The runner checks both
   refusals and the valid flow. The separate raw media ping retains its R06
   HTTP-header authentication checks.

   This runner owns a unique temporary container, random loopback port and
   temporary synthetic data. Its `finally` cleanup removes only those resources.
   It does not attach the persistent deployment volumes or print credentials.

6. Optionally inspect the persistent simulated deployment:

   ```sh
   sudo bash scripts/runtime.sh account list
   # First use only: sudo bash scripts/runtime.sh account add operator
   sudo bash scripts/runtime.sh up
   sudo bash scripts/runtime.sh status
   sudo bash scripts/runtime.sh health
   sudo bash scripts/runtime.sh logs
   sudo bash scripts/runtime.sh down
   ```

   Create the development account at its hidden password prompt if none exists.
   Expect the service healthy and the authenticated internal health probe to
   succeed. Default `up` injects GPU; use `up-cpu` to exercise fallback explicitly.
   `down` removes the deployment container/network and preserves state/scratch
   volumes. Never add `--volumes`, delete instrument data or replace an existing
   configured instrument's image as part of this phase test. Windows clients use
   HTTP/WebSocket; frontend/browser acceptance is R08.

Record commands, exit codes, image/revision, GPU/driver and reports for review.
Missing hardware or unavailable external services are pending evidence, not
passes. Owner approval is separate from successful software validation. These
simulator tests do not establish real microscope calibration or factory limits.

## Build resources

Cold validation exposed a full physical Windows disk even though WSL reported
ample guest capacity. The interrupted run is not counted as a test result. Only
generated Cargo artifacts were cleared, preserving source, registries, logs and
deployment volumes. Check physical host space as well as guest space; Microsoft's
[WSL disk-space documentation](https://learn.microsoft.com/en-us/windows/wsl/disk-space)
explains the distinction. Host storage monitoring belongs in the R10 baseline.

Tests now use Cargo's `line-tables-only` debug information, preserving filename
and line-number backtraces while avoiding full type/variable information in every
large test binary. Numerical code, assertions, tolerances and existing optimization
levels are unchanged; development/release profiles keep their existing settings.
For a focused debugger session inside the development container, set
`CARGO_PROFILE_TEST_DEBUG=2`. See
[Cargo profiles](https://doc.rust-lang.org/cargo/reference/profiles.html#debug).
The scientific smoke runner selects this same test profile so it can reuse
compiled test dependencies rather than generating a second set of full debug
libraries. The six example implementations and their checks are unchanged.

## Validation evidence

- Source/dependency audit: 102 source/static fixture files and eight API snapshots
  match the pinned source byte for byte after line-ending normalization. No
  retained registry package version or checksum changed from R06. The deliberate
  composition/helper/feature changes are described above.
- Default backend gate: passed, including strict Clippy/Rustdoc, layering for 38
  packages, nine existing runnable examples and dependency policy. Workspace
  tests recorded 664 passes; five existing benchmarks/doctests ignored
  and the R05 recorder timing test filtered into its independent gate. Opt-in
  GPU/Python bodies in this count do not establish required execution.
- Shell/Python syntax, workflow YAML parsing, documentation links and staged diff
  whitespace checks: passed.
- Explicit CPU gate: passed, including CUDA exclusion for all 26 feature-bearing
  members, strict Clippy, 643 workspace tests and diagnostic/acquisition/pipeline
  smokes. Four existing benchmarks/doctests remain ignored and the recorder
  timing test stays in its separate gate.
- Scientific smokes: all six examples passed in the default CUDA-enabled build
  without GPU injection and in the explicit CPU-only build. Both H-DAB checkpoint
  runs counted 2,277 nuclei, 39.4% positive, with positive tissue DAB/H-score and
  five-section PDF/HTML reports. The default scan's 30 frames were each within
  1.41 µm of their planned positions, inside the unchanged 2.5 µm source tolerance.
  These are simulator observations, not new production limits.
- Required Python API integrations: both executed and passed with
  `OSCOPE_PYTHON=1`: the spiral planner/scan and generated report, including its
  expected refusal case. No skipped worker body is counted as evidence.
- Required GPU gate: passed on the NVIDIA GeForce RTX 3070 Ti Laptop GPU
  (compute 8.6, 8191 MiB; driver supports CUDA 13.5). The diagnostic executes
  `to_f32`, `bin2x2` and downsample-by-four kernels against CPU references.
  The required preview test confirms actual GPU rendering and byte-identical
  CPU/GPU JPEGs for four viewports/ROIs. All six GPU-injected examples pass;
  H-DAB counted 2,275 nuclei, 39.3% positive, with five report sections. These
  whole-workflow checks use the original scientific bounds.
- Runtime CPU/GPU qualification: both passed against the same Linux amd64 image.
  Each ran authenticated browser-style preview media, dark calibration, single
  capture, a four-frame positioned scan, stitching/deep zoom and PDF/HTML/model
  generation. Completed jobs, active dark calibration, scan frames and report
  hashes survived SIGTERM/start and Docker-supervised restart. The GPU run
  required actual diagnostic kernels and a GPU-rendered preview session. Both
  retained the R06 account/TLS/origin/lease/isolation/rotation checks, 13 locked
  Python SDK tests, a real worker ping and linked-library/toolchain checks.
- Image identity: rebuild from the clean phase commit before pushing, then run
  both runtime acceptance modes again. Reports record the immutable image ID and
  full OCI revision; the check requires the matching API commit. Local logs,
  source audit and JSON reports live under ignored `artifacts/r07/`.
- Gates: the longer scientific/Python API checkpoints have an independent
  path-triggered workflow; GPU qualification stays an explicit workstation gate.
  The backend CI job allows 60 minutes for its expanded default/CPU checks.
  Runtime CI exercises the actual packaged image. No frontend tooling is used.
  Hosted execution is reported separately from these local passes.

The first runtime harness attempt assumed every unauthenticated WebSocket must
fail at HTTP upgrade. Inspection of the pinned framework confirmed its documented
first-message authentication for binary streams. The corrected harness verifies
policy-close 1008 before media for missing/invalid credentials, valid first-message
authentication and hostile-origin rejection; application authentication code was
preserved. The failed harness attempt is not counted as a pass.

The owner approved R07 on 2026-11-23 and authorized R08. Real camera/USB operation, physical
calibration, unattended host startup, factory acceptance and release/rollback
qualification remain R09-R12.
