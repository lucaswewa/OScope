# Independent development gates

Run from the Linux checkout in WSL. The router uses Python 3.11+ and Git only;
Node and Cargo are not needed to decide what changed. Choose the branch your work
started from, normally `origin/codex/linux-container-rebuild` during migration:

```sh
python3 scripts/gates.py --base origin/codex/linux-container-rebuild
sudo python3 scripts/gates.py --base origin/codex/linux-container-rebuild --run frontend
sudo python3 scripts/gates.py --base origin/codex/linux-container-rebuild --run backend
sudo python3 scripts/gates.py --base origin/codex/linux-container-rebuild --run cpu
sudo python3 scripts/gates.py --base origin/codex/linux-container-rebuild --run integration
```

Each command runs its category when selected, or reports `not applicable` and
exits successfully. The plan includes committed changes since the merge base,
staged/unstaged edits, untracked files, deletions and both rename paths. A clean
checkout equal to its base selects nothing. Use `--committed-only` for a fixed
comparison, `--output artifacts/gate-plan.json` to save it, and `--all` for an
explicit complete checkpoint. CI uses the PR base or the previous pushed commit.

| Gate | Work | Tools required |
|---|---|---|
| docs | Local links, phase procedures and whitespace | Python/Git |
| environment | Development image/tool probe | Docker and backend tools image |
| backend | Formatting, strict Clippy/Rustdoc, affected package tests, layering, affected short examples, dependency policy | Backend container; no Node |
| cpu | Explicit CPU-only feature resolution and affected tests | Backend container; no Node |
| frontend | Locked install, types, lint, all unit/tool tests, build, licences and advisories | Node tools container; no Rust/CUDA |
| integration | Generated API compatibility and relevant browser/contract specs | Prepared backend image and browser tools |
| extensions | Python/native extension IPC and SDK | Backend tools container |
| workflows | Longer scientific examples and Python API planning/reporting | Backend tools container |
| runtime | Packaged TLS, authentication, lifecycle and persistence | Prepared backend image/Python |
| hardware | Inventory/profile/SDK-metadata fixture tests and Windows inventory smoke | Python 3.11+; PowerShell 7.4+ on Windows; no device/Rust/Node |
| provisioning | Baseline/template/runtime fixtures and Windows host-planner/task contracts | Python/PowerShell; no application builds or PC changes |
| acceptance | Acceptance profile/report/rework fixtures | Python only; CUDA/physical labs separate |
| release_tools | Real signature and release-state fixtures | Python/OpenSSL 3; no image builds |
| pilot_tools | Leak detector, observation/report and comparison fixtures | Python only; no automatic soak |

Frontend implementation changes select frontend and relevant integration specs.
Unit-test-only edits select frontend. Browser-harness/spec edits select integration.
Hardware inventory scripts, their example profile and tests select only the
hardware-tools category. Its passing CI result qualifies tooling, not physical
hardware; inventory reports always label functional qualification `not_run`.
Provisioning tools have their own category too. Its simulated required-CUDA lab
and clean-PC/cold-boot qualification are separate explicit checks, not hosted
fixture evidence. See the [R10 procedure](phases/r10-host-provisioning.md#manual-test).
Backend changes select backend/CPU checks and relevant boundary checks; they do not
select frontend units. Root routing/shared harness changes receive broader checks.
The package graph comes from Cargo manifests without invoking Cargo. Backend test
selection includes reverse dependencies. No package list means the full workspace.
Direct `scripts/dev.sh backend` and `cpu-tests` retain their full-check behavior;
optional package arguments provide an explicit focused run.

## Frontend and browser environments

```sh
sudo bash scripts/frontend.sh build
sudo bash scripts/frontend.sh check
sudo bash scripts/frontend.sh browser-build
sudo bash scripts/integration.sh
```

The frontend image pins Node 24.21.0, with the original npm lock. It has no Rust or
CUDA toolchain. Its npm cache and Linux `node_modules` live in dedicated volumes.
Frontend checks and integration serialize dependency installation within one
checkout, so one run cannot remove packages while another run uses them.
The browser image additionally has the lock-matched Playwright browsers, Docker
CLI and a virtual display. The display enables Firefox's software WebGL2; the
original colour/saturation assertions remain enabled. Browser dependencies are
installed from the lock before integration even when the frontend gate was not
selected. The existing `web/scripts/changed.mjs` helper and tests are retained
from the source; `scripts/gates.py` is the current gate entry point.

Integration requires an existing image, by default the approved R07 backend:

```sh
sudo docker image inspect oscope-backend:r07
sudo bash scripts/integration.sh --project=chromium tests/e2e/contract.spec.ts
sudo bash scripts/integration.sh --project=firefox tests/e2e/values.spec.ts
```

The launcher freezes that image's ID before exporting OpenAPI and starting tests.
It never compiles Rust. If preparation is needed, request it explicitly:

```sh
sudo bash scripts/integration-backend.sh
sudo env OSCOPE_BACKEND_IMAGE=oscope-backend:integration bash scripts/integration.sh
```

Each test gets a unique container, loopback listener and synthetic state folder.
The integration runner alone has Docker socket access and Linux host networking;
its child backend containers retain private IPC, a read-only root and dropped
capabilities. Its account tests use the source fixture's loopback HTTP mode.
R07's separate runtime acceptance checks trusted TLS and deployed network/account
enforcement. This is not a factory or customer deployment profile.

The launcher removes only containers labelled with its unique run ID; fixtures
remove only their own temporary folders. Deployment volumes are never attached.
Keep failure traces in `web/test-results/` and logs under ignored `artifacts/`.

Required GPU integration is separate:

```sh
sudo env OSCOPE_E2E_GPU=1 bash scripts/integration.sh --project=chromium tests/e2e/preview-contract.spec.ts
```

It requires the actual CUDA diagnostic, then confirms a rendered preview session
reports GPU execution. Missing CUDA fails. The GPU-specific case is explicitly
skipped in CPU browser runs and is not counted as GPU evidence there.

The original 1920-pixel live draw budget runs independently on an idle host:

```sh
sudo bash scripts/integration.sh render-perf
```

It retains the source's zero server drops, at most two in-flight/replaced frames
and draw-count assertions, with one browser worker. Routine integration excludes
that timing case while retaining functional preview/decoder/viewport tests. A
timing skip in routine output does not qualify this budget.

The inherited stored-view 150 ms latency qualification remains opt-in on the
owner's qualified machine:

```sh
sudo env OSCOPE_PERF=1 bash scripts/integration.sh --project=chromium \
  tests/e2e/viewer.spec.ts --grep '@perf$'
```

Its original limits remain unchanged; the routine suite explicitly skips it.

## CI and backend image reuse

`Independent quality gates` reports stable results for every category and an
aggregate `Quality gates` check. Unselected categories succeed with `not
applicable`. The older focused workflows remain available by manual dispatch;
they no longer duplicate automatic checks.

The prepared backend image cache key hashes backend build inputs, including SDK,
local edits and executable bits. Frontend files and ordinary Markdown do not
invalidate it; `python/oscope-ext/README.md` is a required SDK packaging input
and does invalidate it. Backend and frontend Docker build contexts exclude each other's inputs.
Image preparation runs in its own job; frontend quality does not wait for it.
Integration restores the exact-key saved image. A missing image on a frontend-only
change fails with a preparation instruction, rather than compiling Rust as a
hidden frontend dependency. Dispatch the workflow with `full=true` to bootstrap
or restore an evicted image cache. Backend changes may prepare the new image.
Image identity and frontend source revision remain separate component identities.

GPU, recorder timing, machine-specific visual baselines, hardware endurance and
release qualification remain explicit gates. See the [R08 manual procedure](phases/r08-frontend-integration.md#manual-test)
for the complete checkpoint and a browser-accessible simulated application.
The [R09 hardware runbook](hardware-bringup.md) separates read-only preparation
from the deferred physical camera/controller/WSL/container feasibility work.

R11 acceptance tooling has its own portable acceptance gate:

```sh
bash scripts/check-acceptance.sh
python3 scripts/gates.py --base HEAD --run acceptance
```

Only acceptance scripts, fixtures and its profile select this category. Routine
checks run Python fixtures on Linux and Windows without Docker, Node, Rust or a
GPU. They are tool tests, not a native Windows backend gate. The required-CUDA
engineering exercise is a separate explicit lab in the
[R11 manual procedure](phases/r11-factory-acceptance.md#manual-test). Factory
preflight remains blocked until physical, host and release qualification; neither
fixtures nor simulator results constitute instrument acceptance.

R12's release_tools gate runs real OpenSSL signature and portable transaction
fixtures independently of application builds:

```sh
bash scripts/check-release.sh
python3 scripts/gates.py --base HEAD --run release_tools
```

Release scripts and tests select only this category. Missing OpenSSL fails; no
signature or required-GPU check is counted as passing after a skip. The separate
[offline/CUDA update lab](phases/r12-release-lifecycle.md#manual-test) runs only
when explicitly requested, without a native Windows backend gate.

R13 pilot scripts, profile and tests select only pilot_tools:

```sh
bash scripts/check-pilot.sh
python3 scripts/gates.py --base HEAD --run pilot_tools
```

The portable tests need no Docker, Rust, Node or GPU. Observation runs, legacy
full-resolution budgets, long soak and physical pilot qualification remain
separate explicit checks. See [the pilot procedure](phases/r13-production-pilot.md#manual-test).
