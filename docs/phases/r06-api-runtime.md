# R06 API and Linux container runtime

- Status: Approved
- Date: 2026-11-22
- Source: `44bc08a53f98dc29db1fa6b488a4f6269732a71a`
- Owner approval: Accepted on 2026-11-22
- Scope: core HTTP/WebSocket API, authentication and deployable runtime checkpoint

## Port specification

Port the dependency-closed `oscope-api` modules: accounts/sessions/leases,
origins, logs, system/status/settings, basic devices, jobs/extensions, project
CRUD/import/export/ingest/templates, uploads and stored data/volume media. Preserve
their behavior and meaningful regression tests. Adapt only composition and
capability assertions for the available modules. Keep `teta-wot` confined to the
API crate, at its source-pinned revision. Retain R05's reviewed URL security fix.

Defer preview, capture, autofocus/exposure, calibration, scans, live mosaics,
workspace analysis, workflow job registration and deep-zoom orchestration to
R07. Do not publish their Things/routes or add placeholders. Keep source tests
and snapshots for those services with that later port.

Port server configuration/migration, CLI, TLS/accounts, logs and composition.
Remove native Windows service commands and the managed sidecar/preview-ring
lifecycle. Keep restart exit code 75, graceful Linux signals and durable state.
Host reboot/shutdown must be unavailable and must never spawn `systemctl` or
Windows commands. Reject requests for the not-yet-ported preview ring explicitly.

Add authenticated readiness beside the existing health route, including draining
state. Package a non-root Linux runtime image with CUDA libraries, Python 3.12,
uv and the SDK; no Rust compiler or Vue build. Use a read-only root filesystem,
writable state/scratch volumes and temporary runtime storage. Publish only
localhost port 8210, bind within the container, and require accounts/TLS for that
non-loopback listener. A local health probe reads the private local token and
trusts the persisted certificate. Default launch injects GPU; explicit CPU
launch verifies fallback. Include the existing required CUDA diagnostic binary.

Seed the active configuration once into `data/config/oscope.toml` from the image's
`/etc/oscope/oscope.toml`. Settings and their backups are writable in the state
volume and survive image replacement. Do not overwrite this file at each start;
configuration migration and release compatibility are explicit R12 work.

Validate real HTTP/TLS/WebSocket authentication, leases/origins, readiness,
SIGTERM, restart, persistence and absent deferred/host-power capabilities.
Qualify the actual runtime image and its CUDA diagnostic. Keep runtime checks
separate from routine library checks and record image identity. Production host,
USB, factory provisioning and release promotion remain later phases.

## Manual test

This historical procedure targets the R06 checkpoint at
`6796e641b738b1c82594916214f36e4c0f0976a9`, including its scripts and Compose files.
Run it from a separate checkout of that commit. The current branch uses the R07
image and expanded acceptance runner; use the [R07 procedure](r07-microscope-workflows.md#manual-test)
for the current checkpoint.

Prerequisites: use the Linux checkout in Ubuntu 24.04 WSL2, with Docker, Compose,
Python 3.12 and the R01 NVIDIA driver/container-toolkit qualification complete.
The first build and locked SDK environment installation require network access
to the configured registries/package index. Offline wheel bundles are R12 work.
Use simulated devices and a development control PC. These commands do not move
physical hardware. Run from the repository root; `sudo` is unnecessary if your
user already has Docker access. No Node, Vue build or native Windows Rust is
required. The first image build compiles release dependencies and can take much
longer than subsequent builds. Allow enough disk space for both images/caches.

1. Confirm the branch, inspect the profiles and build the image:

   ```sh
   git branch --show-current
   sudo bash scripts/runtime.sh config
   sudo bash scripts/runtime.sh build
   sudo docker image inspect oscope-backend:r06 \
     --format '{{.Id}} {{index .Config.Labels "org.opencontainers.image.revision"}}'
   ```

   Expect `codex/linux-container-rebuild`, both Compose configurations valid and
   a successfully built Linux runtime image. A clean checkout embeds the full
   Git revision in its OCI label and the first 12 digits in the API; a dirty
   development build reports `unknown`. Record the image ID and revision.

2. Run the isolated deployment acceptance check against that image:

   ```sh
   sudo python3 scripts/check-runtime.py --image oscope-backend:r06 \
     --report artifacts/r06/manual-runtime.json
   ```

   Expect `status: passed` and every listed check. This creates a unique temporary
   container and synthetic account/data, with a random loopback port and no GPU
   injection. It validates missing-account refusal, trusted TLS, login, control
   leases, rejected origins, authenticated HTTP/WebSocket media, PNG upload/import
   through a real job, byte-range downloads and traversal rejection. It also
   verifies non-root/read-only isolation, SIGTERM exit 0, WebSocket close 1001,
   Docker-supervised API restart, persistent account/certificate/project/job
   history, expired sessions and local-token rotation. Deferred workflows and
   host reboot/shutdown must be absent. It runs all 13 locked SDK tests and a real
   Python worker protocol ping in the runtime image. It checks Python/uv and
   linked libraries, and absence of Rust/Node toolchains. Its cleanup removes
   only its own container and temporary bind directories; it never uses the
   deployment's named volumes. No password/token appears in its report.

3. Qualify actual CUDA execution in the same image:

   ```sh
   sudo bash scripts/runtime.sh gpu-check
   ```

   Expect an NVIDIA device, the CUDA backend and a successful smoke kernel.
   `--require-cuda` must fail when a working GPU cannot be used. CPU fallback in
   step 2 cannot substitute for this qualification. Record the GPU/driver and
   diagnostic output. An image compiled with CUDA still runs without GPU
   injection, as step 2 proves.

4. Try the deployment profile with a persistent development account:

   ```sh
   sudo bash scripts/runtime.sh stop
   sudo bash scripts/runtime.sh account add operator
   sudo bash scripts/runtime.sh up
   sudo bash scripts/runtime.sh status
   sudo bash scripts/runtime.sh health
   sudo bash scripts/runtime.sh account list
   ```

   Enter a unique development password twice at the hidden prompt. Expect
   `operator` listed and the service healthy. The default `up` reserves the GPU;
   `up-cpu` is the explicit alternative when testing a host without one. If the
   account already exists, use `account passwd operator` instead of adding it.
   Do not put passwords in shell arguments or tracked configuration. The image
   ships without accounts. TLS and a first account are required before the
   network listener starts.

5. Check the trusted external health endpoint and authentication boundary:

   ```sh
   mkdir -p artifacts/r06
   sudo docker compose -f compose.runtime.yaml cp \
     backend:/var/lib/oscope/data/tls/certificate.pem artifacts/r06/runtime-ca.pem
   curl --fail --cacert artifacts/r06/runtime-ca.pem https://127.0.0.1:8210/health
   curl --cacert artifacts/r06/runtime-ca.pem -o /dev/null -w '%{http_code}\n' \
     https://127.0.0.1:8210/ready
   sudo docker compose -f compose.runtime.yaml exec -T backend \
     /usr/local/bin/oscope-healthcheck
   ```

   Expect `{"status":"ok"}`, HTTP 401 without a token, and a successful internal
   authenticated probe. Copy only the public certificate, never `key.pem`,
   `accounts.json` or `local.token`. Do not bypass certificate verification.
   Step 2 exercises authenticated user login/control/media without exposing
   secrets in shell history. Windows clients connect to the published localhost
   API; they do not map Linux shared memory. Browser integration is R08.

6. Stop and start the same development deployment:

   ```sh
   sudo bash scripts/runtime.sh stop
   sudo bash scripts/runtime.sh up
   sudo bash scripts/runtime.sh health
   sudo bash scripts/runtime.sh account list
   sudo bash scripts/runtime.sh logs
   sudo bash scripts/runtime.sh down
   ```

   Expect a graceful stop, then healthy service with the same account,
   configuration and certificate. Inspect logs for startup/stop errors. `down` removes containers
   and networks while preserving state/scratch volumes. Never add `--volumes`
   or delete instrument datasets to reset a test. The isolated check in step 2
   supplies the stronger project/job/credential lifecycle assertions.

Any failed assertion, unhealthy container, missing actual GPU kernel, incorrect
exit/close behavior, absent persisted data, exposed host power action or secret
in a report is a failure. A Docker `unhealthy` status does not itself restart a
container; the process-exit restart policy is tested separately. Collect the
report and logs, fix the cause, and rerun the affected check. Do not approve R06
based on a skipped GPU test. Cold boot, real camera/USB, physical calibration,
factory acceptance and customer upgrade/rollback remain R09-R12 work.

## Validation evidence

- Source audit: 65 API/server files, of which 44 match the pinned source exactly
  after newline normalization, 19 are adapted and two are new lifecycle suites.
  Adaptations cover manifests/build identity, composition, readiness/auth routing,
  container process lifecycle, Linux configuration documentation and capability
  assertions. Preserve the source numerical comparisons/tolerances. The status
  test uses a controlled failed job in place of the deferred auto-exposure job;
  the volume test explicitly expects the deferred deep-zoom route to be absent.
  Restart tests retain confirmation, compute-versus-hardware locking, force,
  dry-run and event assertions, with host command cases removed.
- Dependency audit: 602 locked packages. Every retained registry version/checksum
  and Git source matches the baseline except R05's reviewed `url 2.5.9` and
  `form_urlencoded 1.2.3` security update. `teta-wot v0.2.0` remains pinned to
  `338336fb3f138c1ec139d8fccfca9338f3826ddd`; only the API crate depends on it.
  Preserve the existing reviewed advisory/license exceptions; add only that
  source Git allowlist entry. Dependency policy checks pass.
- Default backend gate: passed in the Linux development container. Formatting,
  strict Clippy, tests/doc tests, strict Rustdoc, layering for 30 packages, nine
  runnable examples and cargo-deny all pass. Cargo reports 537 passes, including
  70 API and 28 server unit/integration tests; two existing simulator throughput
  benchmarks and one SDK macro doctest remain ignored. The full-sensor pacing test remains in its separate R05 performance
  gate. Opt-in GPU/worker test bodies in the routine suite do not count as GPU
  qualification or fresh extension-worker evidence.
- Shell syntax and documentation links/manual-test presence: passed.
- Explicit CPU-only gate: passed, including feature resolution for all 18
  feature-bearing members, strict Clippy, workspace tests and CUDA/acquisition/
  pipeline CPU examples. This is distinct from running the CUDA-enabled image
  without GPU injection, which also passes and selects the CPU backend.
- Runtime image: built for Linux amd64 from the pinned CUDA 13.4.1 runtime
  manifest. The isolated acceptance check passes every assertion described in the
  manual procedure, including persisted settings, the 13 locked SDK tests and a
  real Python worker ping. The no-GPU required-device diagnostic fails as intended.
  The test's temporary container/data are cleaned up; deployment volumes are
  preserved. No microscope hardware is accessed.
- Required GPU execution in that image: passed on the NVIDIA GeForce RTX 3070 Ti
  Laptop GPU (compute 8.6, 8191 MiB, driver supports CUDA 13.5). NVRTC, cuFFT and
  pinned memory are available. `to_f32`, `bin2x2` and downsample-by-four match
  their CPU references exactly on a 5472 × 3648 RGGB16 frame. These diagnostics
  are correctness evidence, not new manufacturing throughput limits.
- Build identity: the final image is rebuilt from the clean phase commit before
  pushing, then the runtime acceptance and required GPU checks run against it.
  The runtime report records its immutable image ID and full OCI source revision;
  acceptance checks that the API reports the matching 12-digit commit.
  Reproduce with the manual commands above. Local logs/reports are under
  `artifacts/r06/`, including `backend.log`, `cpu.log`, `runtime.json`, `gpu.log`
  and `source-audit.json`; generated evidence and credentials are not committed.
- CI: the container-runtime workflow is separate from backend checks, scoped to
  API/server/storage and runtime build inputs, and requires no frontend tooling.
  Hosted workflow execution is reported separately from these local passes.

This is a simulated server/container checkpoint. `/ready` means the API can serve
requests; it does not qualify physical microscope health or calibration. Real
camera/USB feasibility, unattended Windows/WSL boot, factory acceptance, offline
bundles and compatible customer updates/rollback remain R09-R12. Runtime Ubuntu
packages are resolved during the build; bit-for-bit reproducible releases and
third-party distribution notices remain release packaging work. The owner approved
R06 on 2026-11-22 and authorized R07.
