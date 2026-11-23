# OScope Vue application

This is the Vue 3 application ported from the pinned source in the
[repository README](../README.md). It uses HTTP/WebSocket APIs and is served by
the Linux backend. Styling, rendering algorithms and dependency lock versions
are preserved. The supported power action is service restart; Windows host
reboot/shutdown are outside the container's API. Tauri integration is future work.

## Develop and check

Run these commands from the repository root in the WSL Linux checkout:

```sh
sudo bash scripts/frontend.sh build
sudo bash scripts/frontend.sh check
sudo bash scripts/frontend.sh browser-build
sudo bash scripts/integration.sh
```

Frontend quality runs in a Node-only image: locked install, types, lint, all
unit/tool tests, production build, licences and advisories. Browser integration
uses a separate tools image and an existing immutable backend image. It checks
OpenAPI compatibility, then starts an isolated backend container per test. It
never compiles Rust or launches a native Windows backend.

The default image is `oscope-backend:r07`. If it is absent, prepare one explicitly:

```sh
sudo bash scripts/integration-backend.sh
sudo env OSCOPE_BACKEND_IMAGE=oscope-backend:integration bash scripts/integration.sh
```

After an intentional API change, run `scripts/integration.sh types-write`, review
`src/api/generated/openapi.ts`, then run `types` and frontend quality. API scripts
require `OSCOPE_OPENAPI` to name an exported JSON schema; they have no implicit
native-server build or launch fallback.

For targeted browser work:

```sh
sudo bash scripts/integration.sh --project=chromium tests/e2e/contract.spec.ts
sudo env OSCOPE_E2E_GPU=1 bash scripts/integration.sh --project=chromium tests/e2e/preview-contract.spec.ts
sudo bash scripts/integration.sh render-perf
```

The routine suite runs Chromium cases plus Firefox/WebKit `@main` paths. A
virtual display enables Firefox software WebGL2 without weakening colour/mask
assertions. Required CUDA integration executes kernels and checks GPU-rendered
session evidence. The original 1920-pixel frame budget runs separately with one
worker on an idle host. The inherited stored-view latency check remains an
explicit `OSCOPE_PERF=1` qualification on the owner's machine. CPU runs do not
qualify GPU behavior.

See [the gate guide](../docs/gates.md) for change selection, package dependencies,
image preparation/cache behavior and CI. Traces remain in `web/test-results/`;
console logs/reports belong under ignored `artifacts/`. Dependency installation
and integration serialize within one checkout. Keep development caches and
persistent deployment volumes.

## Run the application

Follow the complete [R08 manual procedure](../docs/phases/r08-frontend-integration.md#manual-test)
for a separate persistent HTTPS review project, account setup, Windows
certificate trust, login/control, preview, scans, stored records and cleanup.
`scripts/app.sh up` reserves the GPU; `up-cpu` explicitly exercises fallback.
The backend serves `web/dist` read-only from the same origin.

For Vite editing, use `scripts/frontend.sh install` then `scripts/frontend.sh dev`.
`OSCOPE_SERVER` names the qualified backend HTTPS address; configure Node's
`NODE_EXTRA_CA_CERTS` with its public CA when using a private development CA.
Do not disable TLS verification. Set these inside `scripts/frontend.sh shell`
when needed. Vite's `/api` and `/health` proxy preserves browser Host/origin.

## Frontend structure and preserved checks

- `src/ui/`: shared components, tokens, icons, charts and schema forms; the only
  layer that imports reka-ui. Colours use token roles. Contrast tests cover the
  documented text/background pairs in both themes.
- `src/api/`: typed requests, actions/jobs, property/event subscriptions, accounts,
  control leases, media and preview workers; the only layer using network APIs.
- `src/host/`: browser storage, files, clipboard, notifications and window access
  behind an interface suitable for the future desktop adapter.
- `src/features/`: individual tabs composed from UI/API/host layers.
- `src/app/`: shell, navigation, routes, shortcuts and locale words.

Lint enforces imports between layers. Unit tests live beside their components;
tool tests are under `tests/tools`. Browser tests retain accessibility, live
properties, authentication, lease exclusion, reconnect/restart, decoded preview,
scientific workflows and stored-image contracts. There is no frontend tuning or
redesign in this port.

Existing machine-specific screenshot baselines remain unchanged. Visual checks
are explicit `OSCOPE_VISUAL=1` opt-ins on their qualified platform; a default
skip is not visual evidence. R08 adds Linux light/dark baselines only for the
intentional container power-control change. Do not bulk-regenerate legacy
baselines. The separate timing gate retains its original assertions and limits.

Dependency additions must follow the repository's minimum-age and licence
policy. Keep reviewed dev advisory exceptions in `advisories.json`; shipped
advisories cannot be exempted. Historical design ADRs remain available at the
pinned legacy commit rather than being silently copied into this rebuild.
