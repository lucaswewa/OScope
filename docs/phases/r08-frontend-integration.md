# R08 Vue integration and independent gates

- Status: Complete; awaiting owner review; hosted CI blocked
- Date: 2026-11-23
- Source: `44bc08a53f98dc29db1fa6b488a4f6269732a71a`
- Prerequisite: R07 approved by owner on 2026-11-23
- Owner approval: Pending

## Port specification

Port the pinned `web/` tree: Vue application, API clients, generated types,
recorded reference data, unit/tool tests, browser/contract tests and visual
baselines. Preserve styling, interactions, algorithms, dependency versions,
scientific assertions and screenshot tolerances. Do not optimize or redesign
the frontend. Regenerate API types from the actual Linux backend; adapt the
power controls/tests to its supported restart action and absent host power.

Replace native-server browser fixtures with per-test Linux runtime containers,
each with its own synthetic configuration/data/scratch, dynamic loopback port and
bounded shutdown. Preserve account/login/control-lease tests, reconnect/restart,
HTTP/WebSocket contracts, preview, scan and stored-image scenarios. Preserve
Chromium's suite and Firefox/WebKit main paths. Existing machine-specific visual
baselines stay unchanged and explicitly opt in; accessibility checks still run.

Keep the frontend tooling in a pinned Node 24 container without Rust or CUDA.
The integration-only runner may use the WSL Docker socket and host networking to
launch its own backend containers; the normal frontend and backend development
containers and deployed runtime do not receive that socket. Integration test
servers bind loopback and use the source fixture's local HTTP mode; production
TLS/account enforcement and real CUDA remain covered by R07 runtime acceptance.
Serve the built frontend through the runtime's existing web option for a runnable
same-origin checkpoint. Customer release bundles/provisioning remain R10-R12.

Add a Python-standard-library gate router, usable without Node or Rust. Include
committed changes since a merge base plus staged, unstaged, untracked, deleted
and both sides of renamed paths. Route docs, environment, backend, frontend and
integration independently; treat shared build/harness inputs conservatively.
Backend-only checks require no Node. Frontend-only quality checks require no
Rust. Integration consumes an explicitly prepared immutable backend image,
checks OpenAPI-generated types, and runs relevant browser specs against it.
Keep scientific examples, Python workers, CPU-only/GPU, timing and release checks
separate. Full regression remains an explicit checkpoint/release operation.

CI reports stable results for each category even when no work applies. Cache
the integration backend image by its backend build inputs, so frontend edits
reuse it; bootstrap a missing image only when backend preparation is selected
or explicitly requested. Test router behavior using real temporary Git repos,
including rename/deletion and concurrent index/worktree changes. Audit source
parity and dependency lock changes before completing the phase.

## Manual test

Prerequisites: the R01/R07-qualified WSL2 Linux checkout, Docker/Compose, Python
3.11+ and enough physical Windows and Linux storage. Frontend quality checks need
no GPU, CUDA or Rust. Required-GPU integration additionally needs the qualified
NVIDIA driver/toolkit. First installs need the configured npm/browser registries.
Use simulated devices and a development PC. Run shell commands from the Linux
repository root. Use `sudo` for Docker unless your account has socket access.

1. Build the separate tools and run frontend quality:

   ```sh
   sudo bash scripts/frontend.sh build
   sudo bash scripts/frontend.sh check
   sudo bash scripts/frontend.sh browser-build
   ```

   Expect all commands to exit zero: locked npm installation, Vue/TypeScript
   checks, ESLint/Prettier, unit/tool tests, built assets and notices, licence and
   advisory policy. The frontend image must contain no `cargo` or `rustc`. No
   backend compilation or GPU probe should run. Browser installation is a separate
   image build. npm lock versions and existing reviewed advisory exceptions stay
   unchanged. Cache/download, type, lint, test or policy failures are failures.

2. Exercise change routing and the independent backend entry point:

   ```sh
   bash scripts/check-gates.sh
   python3 scripts/gates.py --base origin/codex/linux-container-rebuild \
     --output artifacts/r08/manual-gate-plan.json
   sudo python3 scripts/gates.py --base origin/codex/linux-container-rebuild --run frontend
   sudo python3 scripts/gates.py --base origin/codex/linux-container-rebuild --run backend
   sudo python3 scripts/gates.py --base origin/codex/linux-container-rebuild --run cpu
   ```

   Expect router tests to pass, including real temporary Git repos with committed,
   staged, unstaged, untracked, deleted and renamed paths. A clean checkout equal
   to the base reports `not applicable`. In a feature branch with frontend-only
   edits, the plan must select frontend/relevant integration, with backend/CPU
   false. Backend edits select their dependent packages and no frontend units.
   Documentation-only changes select docs. Shared gate changes select broad
   checks. Check the JSON reasons before running a selected category. The tests
   exercise these cases without modifying your application files. Use a meaningful
   development base, not legacy `main`, which intentionally differs by the entire
   migration. Explicit `sudo bash scripts/dev.sh backend oscope-core ex-core-tour
   xtask` also tests the focused Rust command without Node. With no package
   arguments, the existing backend/CPU commands still run their full suites.

3. Check the exact prepared image and run browser integration:

   ```sh
   sudo docker image inspect oscope-backend:r07 --format '{{.Id}}'
   sudo bash scripts/integration.sh types
   sudo bash scripts/integration.sh
   ```

   Expect generated API types to match the image, all applicable Chromium cases
   and Firefox/WebKit main paths to pass, and no uncaught page errors. Account,
   control lease, media, preview, camera/stage, scan, gallery/download, reconnect,
   restart and accessibility assertions remain enabled. Host reboot/shutdown
   must be absent and return HTTP 404. The browser renderer qualification must
   confirm WebGL2; Firefox uses the virtual display's software GL. Keep the
   original colour/mask assertions and tolerances. Machine-specific `@visual`
   cases, the owner-specific stored-view latency case and the GPU-specific case
   explicitly skip in this CPU run. The standalone
   1920-pixel draw budget is excluded into its timing gate. Those skips/exclusions
   do not qualify visual, GPU or timing behavior.

   Every test must use a unique container, loopback port and synthetic folder.
   Inspect failures in `web/test-results/` and preserve the console log. The
   launcher's cleanup must leave no containers with its run label, and must not
   touch deployment volumes. The integration runner has Docker socket access;
   use it for this repository's tests on the development host. Normal frontend
   checks and deployed containers do not receive that socket.

   If no compatible image is installed, prepare it explicitly with
   `sudo bash scripts/integration-backend.sh`, then run with
   `sudo env OSCOPE_BACKEND_IMAGE=oscope-backend:integration bash scripts/integration.sh`.
   This explicit preparation may compile Rust. Ordinary frontend and browser
   commands must not silently do so. After an intentional API change, regenerate
   types using `sudo bash scripts/integration.sh types-write`, review the diff,
   then rerun `types` and frontend quality.

4. Require actual CUDA preview delivery:

   ```sh
   sudo env OSCOPE_E2E_GPU=1 bash scripts/integration.sh \
     --project=chromium tests/e2e/preview-contract.spec.ts
   ```

   Expect required-device kernels to execute and the GPU-specific case to pass
   with an actual rendered session's `gpu` and `sent` evidence. Missing devices,
   CUDA failure, CPU fallback in that case, a skipped required case or bad media
   fails qualification. This tests transport/render selection, not new production
   frame-rate limits. R07 retains the separate numeric CPU/GPU reference tests.

   On an idle host, also run `sudo bash scripts/integration.sh render-perf`.
   Expect its single Chromium case to pass the unchanged zero server-drop,
   draw-count and maximum-two replacement/in-flight limits at 1920 pixels.
   It runs with one worker; a routine functional-suite exclusion is not a pass.

   Check the intentional container power-page adaptation in the qualified Linux
   browser image, in both themes:

   ```sh
   sudo env OSCOPE_VISUAL=1 bash scripts/integration.sh --project=chromium \
     tests/e2e/power.spec.ts --grep 'container restart tab matches'
   ```

   Expect one case to pass against its light/dark Linux baselines with the
   existing `0.01` screenshot threshold. Do not update snapshots to hide a
   failure. Other legacy screenshots retain their original machine-specific
   baselines and require separate qualification on that rendering environment.

   The inherited 150 ms stored-view latency budget remains an owner-machine
   qualification, with its original image size, sampling and limit. It is not
   qualified by the routine browser run or the live-draw timing gate. On that
   qualified, idle machine it is explicitly runnable as:

   ```sh
   sudo env OSCOPE_PERF=1 bash scripts/integration.sh --project=chromium \
     tests/e2e/viewer.spec.ts --grep '@perf$'
   ```

   Expect the single case to execute and pass; a skip is pending evidence.

5. Open the persistent same-origin application in a Windows browser:

   ```sh
   sudo bash scripts/frontend.sh dist
   r08_demo=(sudo env COMPOSE_PROJECT_NAME=oscope-r08-review OSCOPE_PORT=8218)
   "${r08_demo[@]}" bash scripts/runtime.sh account list
   # First use: enter a unique development password at the hidden prompt.
   "${r08_demo[@]}" bash scripts/runtime.sh account add operator
   "${r08_demo[@]}" bash scripts/app.sh up
   "${r08_demo[@]}" bash scripts/runtime.sh health
   "${r08_demo[@]}" docker compose -f compose.runtime.yaml -f compose.web.yaml cp \
     backend:/var/lib/oscope/data/tls/certificate.pem artifacts/r08/demo-ca.pem
   sudo chmod 644 artifacts/r08/demo-ca.pem
   ```

   The dedicated project keeps this review separate from other deployments.
   If `operator` already exists, use `account passwd operator` instead. Default
   `up` reserves the GPU; `up-cpu` explicitly tests fallback. Expect a healthy
   runtime serving the existing built app over HTTPS at `https://127.0.0.1:8218`.
   Copy only the public certificate. Trust that development certificate using
   your Windows/browser certificate procedure; never bypass TLS verification.
   One explicit PowerShell 7.4+ procedure for the validated checkout is:

   ```powershell
   $r08Pem = wsl -d Ubuntu-24.04 -u root --cd /home/lucas/src/OScope-container --exec cat artifacts/r08/demo-ca.pem
   $r08Path = 'C:\repos\OScope\artifacts\r08\demo-ca.pem'
   [IO.File]::WriteAllText($r08Path, ($r08Pem -join "`n"))
   $r08Cert = [Security.Cryptography.X509Certificates.X509Certificate2]::CreateFromPemFile($r08Path)
   $r08Thumbprint = $r08Cert.Thumbprint
   $r08AlreadyTrusted = Test-Path -LiteralPath "Cert:\CurrentUser\Root\$r08Thumbprint"
   if (!$r08AlreadyTrusted) {
     Import-Certificate -FilePath $r08Path -CertStoreLocation Cert:\CurrentUser\Root
   }
   ```

   Adapt the distro/checkout path if yours differs. This adds only the review's
   public certificate to the current user's trust store; retain the variables
   for cleanup. In the browser, verify certificate trust, reject a wrong password,
   log in, take control, home the simulated stage and start Live View. Try preview
   pan/zoom, histogram/saturation, capture into a new project, a small scan and
   reopening/downloading a stored record. On Power, only service restart should
   be offered. Restart, log in again if the session expires, and verify the
   project persists. Check About and both themes. No Linux shared-memory mapping
   or native Windows backend is used. The configuration is simulated; these
   operations are not physical microscope acceptance.

6. Clean up the review without deleting its persistent data:

   ```sh
   "${r08_demo[@]}" bash scripts/app.sh down
   ```

   Close the review browser tab. In the same PowerShell session, remove the trust
   entry only if this procedure added it:

   ```powershell
   if (!$r08AlreadyTrusted) {
     Remove-Item -LiteralPath "Cert:\CurrentUser\Root\$r08Thumbprint"
   }
   ```

   Keep named volumes, caches, reports and failure traces. Do not use `down -v`,
   delete instrument data or replace an existing instrument's release. Record
   commands, image ID/backend revision, frontend revision, test results and any
   missing evidence. Owner approval remains separate from automated validation.

## Validation evidence

The port retains all 583 files from the pinned `web/` tree. The source audit
finds 569 byte-identical files and 14 deliberate adaptations: `web/README.md`,
Playwright configuration, schema export/reader/types, the restart-only power
controls and their tests/words, container fixtures, startup-log filtering and
the opt-in GPU preview/timing tests. The new rendering qualification spec checks
WebGL2 explicitly. The npm lock and all legacy PNG baselines are byte-identical;
Rust/Python implementation and Cargo lock files are unchanged from R07. The
source audit is retained as `artifacts/r08/source-audit.json`.

Six router regressions pass, including real Git index/worktree/rename cases and
backend image identity/cache isolation. An SDK packaging README change selects
backend checks and changes the cache key; ordinary documentation/frontend changes
do not. A Docker-context smoke confirms that required README survives the new
context exclusions. Actionlint 1.7.12 validates the workflows, and shell syntax
checks pass. The focused `oscope-core ex-core-tour xtask` default and explicit
CPU-only gates each pass 41 tests; the default also passes strict Rustdoc,
layering, core-tour smoke and dependency policy. Neither command invokes Node.
The unchanged full R07 Rust/scientific suites are not repeated for this port.

The persistent `oscope-r08-checkpoint` Compose review passes trusted HTTPS,
byte-exact built JS delivery, 23 public dependency notices, wrong-password
rejection, authenticated readiness and control-lease acquisition. Inspection
confirms the frontend mount is read-only and the runtime reserves a GPU; actual
CUDA diagnostic kernels pass. The checkpoint is stopped with its named state
and scratch volumes retained. The report is `artifacts/r08/checkpoint.json`.
The compatible backend image remains the approved R07 image:
`sha256:6faf43fc6c6c72b4abbb0cfa579410a16f20e9d490c9e8ce4c6ba7796751d667`,
OCI revision `ea3481a4e887bb06293f7a6c0cd49b0585fbae46`. Frontend and backend
component identities are separate; integration freezes the image ID before
checking its schema and starting any fixture.

Initial integration attempts exposed synthetic-folder ownership, restart wording
and logging-filter assumptions in the ported harness. Those are corrected.
Firefox without a display lacked WebGL2; the browser image now uses Xvfb and an
explicit renderer qualification, preserving the original saturation/false-colour
assertions. One parallel software-rendering run missed the existing 1920-pixel
frame budget. That case now has its own one-worker idle-host timing gate, with
the original draw/drop/replacement limits unchanged. Failed attempts and traces
are retained separately from successful qualification evidence.

Frontend quality passes all 84 files / 742 unit and tool tests, typechecking,
ESLint/Prettier, build, 318 package licences (23 bundled, zero refused) and
advisories (zero shipped, zero new, two inherited reviewed dev exceptions).
Generated API compatibility passes against the frozen R07 image. Required CUDA
integration executes diagnostic kernels and passes all six preview contracts,
including actual GPU session evidence with no GPU skip. The idle live-draw timing
gate passes its single original 1920-pixel case. The targeted power visual case
passes a fresh no-update comparison against both new Linux baselines; both
screenshots were inspected. These additions leave all legacy baselines unchanged.
Logs are `frontend.log`, `gpu.log`, `render-perf.log` and `power-visual.log` under
`artifacts/r08/`. The legacy owner-machine stored-view latency budget and other
machine-specific visual baselines remain pending qualification.

The final complete functional browser run passes: **285 passed, 35 skipped** in
8.9 minutes across Chromium and the Firefox/WebKit main paths. The 35 explicit
skips are 33 machine-specific visual cases, the separate required-GPU case and
the owner-machine stored-view latency case; none is counted as qualification.
The single live-draw timing case is separately selected and passes as documented
above. The successful log is `artifacts/r08/browser.log`. A preceding run
passed 284 cases but encountered Chromium `ERR_NETWORK_CHANGED` while the
separate persistent checkpoint was being started on the same Docker host. Its
trace/log are retained; the successful complete rerun used a quiet host after
that checkpoint was stopped. No browser retry or assertion relaxation was added.

Windows certificate-store/browser review remains for the owner using
the manual procedure above; the automated TLS client does not claim that review.
Hosted CI is unverified: the R07 documentation run
[45134985069](https://github.com/lucaswewa/OScope/actions/runs/45134985069)
was rejected before execution because account payments failed or the spending
limit was reached. No hosted pass or image-cache seed is claimed. After Actions
is available, dispatch `Independent quality gates` with `full=true` once to
prepare its backend image cache and qualify the full hosted checkpoint.

Physical devices/WSL USB feasibility, production provisioning and instrument
acceptance remain R09-R12. Owner approval of R08 is pending.
