# R11 — factory acceptance tooling

Status: tooling checkpoint complete; physical acceptance pending. The owner authorized continuation on
2026-11-23. R09 physical hardware and R10 clean-PC qualification remain deferred;
this does not constitute physical acceptance or shipment approval.

## Scope and source record

The behavioral reference remains `44bc08a53f98dc29db1fa6b488a4f6269732a71a`.
Inspect the already ported `crates/oscope-api/tests/calibration.rs`,
`crates/oscope-api/tests/focus.rs`, `examples/ex-calibrate/src/main.rs`,
`scripts/check-runtime.py` and `scripts/runtime_workflows.py`. Reuse the R07 TLS
client and scientific workflow checks. Do not change Rust, Vue, runtime images or
dependency locks. New tooling uses Python's standard library and the installed
Docker CLI; portable fixtures do not require Docker, Rust, Node or a GPU.

Preserve the source's autofocus error bound of one quarter of the reported depth
of field, scan position error below 2.5 µm, persisted output byte hashes, and
required CUDA execution. Simulator tolerances are regression references, never
physical production limits. Factory limits, exact device identities, fixture and
approved qualification references must come from physical qualification.

## Intended behavior and validation

- A versioned profile and guided operator checklist identify prerequisites and
  outstanding physical measurements. Unqualified example values stay empty.
- Factory preflight retains a blocked report and makes no device calls. Real
  device adapters, hardware qualification, host qualification and authenticated
  release promotion are still outstanding; setting profile text cannot bypass
  these implementation gaps.
- The explicit engineering runner creates its own isolated simulated backend,
  verifies immutable image identity and actual CUDA kernels, then exercises TLS,
  calibration, focus, preview, capture, scan, stitching, reports and restart
  persistence. It cannot target an existing server or physical devices.
- Every attempt gets a new run ID and retained event journal, JSON/HTML report
  and checksums. Rework links to a previous attempt without reusing passed steps.
  Failures/interruption retain evidence and stop subsequent steps. Engineering
  results never authorize shipment. Credentials stay outside exported evidence.
- Portable fixtures cover invalid profiles/limits, failure/interruption, report
  integrity, escaping, rework identity, blocked preflight without side effects and
  gate independence. A separate explicit Linux/GPU lab exercises real workflows.

## Manual test

Prerequisites: Python 3.12+, this checkout, and a writable evidence directory.
Portable checks also run on Windows. Run from the repository root in WSL:

```bash
bash scripts/check-acceptance.sh
python3 scripts/acceptance.py plan --profile configs/acceptance.example.json
python3 scripts/acceptance.py factory-preflight \
  --profile configs/acceptance.example.json --instrument ENG-001 --pc ENG-PC-001 \
  --operator engineer --station development --output artifacts/r11/preflight
```

Expected: fixtures pass; the plan lists operator actions and missing
qualifications. Preflight exits 2, writes a new blocked report with shipment
authorization false, and never starts Docker or moves a stage. The example
profile must never produce a factory pass.

For the engineering lab, additionally require Linux x86_64, Docker with NVIDIA
Container Toolkit, a working NVIDIA GPU and a locally built R07 runtime image.
Use a fresh evidence root on the WSL Linux filesystem. No physical devices or
existing deployment are needed:

```bash
sudo python3 scripts/acceptance.py run-engineering \
  --profile configs/acceptance.example.json --image oscope-backend:r07 \
  --instrument ENG-001 --pc ENG-PC-001 --operator engineer --station development \
  --output artifacts/r11/lab
```

Expected: exit 0 only after actual CUDA kernels, trusted TLS/authentication,
scientific workflows and restart/reopen checks pass. Read the printed run
directory's report.json and report.html. Outcome must be engineering_passed,
shipment_authorized false; physical qualification remains not_run. CPU fallback
is supported by the application, but cannot satisfy this required-GPU check.
An unavailable GPU, failed/skipped workflow, checksum change, wrong identity or
timeout fails the attempt. Ctrl+C must retain an interrupted report. A hard kill
may leave only a journal and must never be interpreted as a pass.

Rework uses the same command plus --previous pointing to the old report.json;
it creates a new full attempt and preserves the old evidence. Changed instrument,
PC or profile is refused. Archive only the report/evidence files, not private/
(accounts, certificates and application state). The runner removes only its own
labelled container; it retains private application data for diagnosis. After
review, remove only the explicitly identified engineering run directory if no
longer needed. Never remove instrument volumes or an unrelated container.

Physical testing stays pending. Follow the fixture and operator procedure in the
factory runbook once hardware, host and release qualification are approved. Do
not substitute simulated results, CPU fallback or invented physical limits.

## Evidence

Validated on 2026-11-23:

- 18 portable acceptance tests passed on Windows Python 3.12.14 and WSL Python;
  9 gate-router tests passed on both. The selected acceptance gate passed on
  Windows. Linux shell syntax, actionlint and local links in 25 Markdown files
  passed. Acceptance-tool paths select only their own gate; shared router/CI
  edits intentionally select all categories. Application suites were not repeated
  for this tooling change.
- Real factory-preflight commands on Windows and Linux exited 2 with retained
  blocked reports. The fixture also proves no subprocess/device calls happen.
  Populating profile claims cannot unlock the missing physical adapter.
- Final required-CUDA lab run: 59630ae7-5aa8-45bc-bded-b3db023eb5d1, linked to
  the preceding complete attempt 5b8a4bee-42e5-4bbf-ba02-9604a37fce46. All seven
  steps passed, including cleanup. Report checksums verified on Linux and again
  after exporting public evidence to Windows. Recorded source hashes match the
  final runner/client/workflow files; private application state was not exported.
- The immutable runtime was
  sha256:6faf43fc6c6c72b4abbb0cfa579410a16f20e9d490c9e8ce4c6ba7796751d667,
  OCI revision ea3481a4e887bb06293f7a6c0cd49b0585fbae46. Actual CUDA kernels
  executed; the preview session reported GPU execution. Four scan frames,
  stitching, scientific report generation and restart/reopen byte-hash checks
  passed. All active calibration sets survived restart unchanged.
- Autofocus error was 0.09293 µm against the original depth-of-field/4 bound of
  2.61097 µm. Geometry pixel error fraction was 0.00000526 (bound 0.002), rotation
  -19.99974° against -20° (absolute error bound 0.1°), backlash x/y 2.99794/3.00037
  µm against 3 µm (bound 0.2 µm), and mean colour delta-E 0.01380 (bound 0.5).
  These are injected simulator truth checks, not physical product limits.
- Earlier failed attempts were retained, including an empty-field autofocus
  rejection caused by using the capture smoke test's blank-glass coordinate, and
  an HTTP rejection when clearing ROI with null. The final runner uses the focus
  regression's seeded first tissue island and an explicit full sensor rectangle.
  No tolerance was loosened. New attempts link the old reports and rerun all
  steps; failure/interruption and cleanup-failure behavior also passed fixtures.

Evidence is under ignored artifacts/r11/ in the Windows checkout and WSL lab;
only public manifest-listed files were copied to Windows. Each attempt's private/
state remains in WSL for diagnosis. Every lab-owned container was removed. The
existing oscope-r08-review-backend-1 deployment remained running. No Windows
startup/account/WSL setting, physical device, vendor SDK or existing instrument
data was changed. Backend build-input revision remains
ccf323596f80704c42ebb25e7fecc168e8d353c0; Rust, application Python, Vue and
dependency locks are unchanged.

Hosted CI remains unverified: the preceding R10 run was blocked before execution
by account billing/spending limits. Inspect the fresh run after push; the new
Linux/Windows acceptance jobs exercise portable tools, not a Windows backend.

Owner authorization to continue and automated software evidence are separate from
physical acceptance. Exact serials/firmware, Linux camera SDK and real drivers,
fixture/limits, clean-PC/cold boot, authenticated release, physical per-objective
calibration, reference-image comparison and combined-load/recovery evidence remain
pending. R11's full exit criterion is still one reference microscope accepted
without developer intervention; no report from this checkpoint authorizes shipment.
