# Production pilot and backend measurement

R13 implements pilot preparation and simulated engineering observations. It does
not accept a production pilot or promote an optimization. The owner approved R12's
engineering checkpoint; hardware, clean-PC, physical acceptance and full-product
release qualification remain open. See [phase evidence/manual tests](phases/r13-production-pilot.md).

## Select and qualify the pilot

Operations and engineering first select actual instrument/PC serials and hardware
variants: camera/controller revisions and firmware, optics/objectives, GPU/VRAM,
Windows/WSL/driver/SDK baseline and supported installation/update paths. Cover
every intended supported combination and rework scenario. The owner chooses the
cohort size and endurance duration from product risk and measured variation.
The annual target of 300 units does not determine a statistically adequate sample.

Copy [the pilot profile](../configs/pilot.example.json) into a controlled revision.
Its empty cohort, null requirements/budgets and missing qualification references
are intentional. Do not turn the example into an approved profile by filling
unverified labels. The current preflight cannot authenticate physical evidence or
run a physical pilot adapter, so it stays blocked even if all claims are filled.

Before a production trial, close and link:

1. [R09 hardware/USB/SDK qualification](hardware-bringup.md) for the exact devices.
2. [R10 clean-PC and before-login lifecycle](host-provisioning.md), including GPU
   failure, USB reconnect and version policy.
3. [R11 physical calibration/acceptance](factory-acceptance.md), using the approved
   fixture, targets, operator preparation and instrument-specific motion limits.
4. [R12 complete authenticated product release and upgrade path](release-lifecycle.md),
   including the Windows startup owner, frontend/host/profile assets and trust.

Then have operations perform installation, per-objective calibration, acceptance,
maintenance, compatible rollback and service export without developer intervention.
Use approved fixtures for motion/stop tests; do not derive travel limits from
simulator coordinates. Use a dedicated qualified instrument for controlled
power-loss, cold-boot, reconnect and recovery tests. A simulator or CPU fallback
cannot satisfy a CUDA-specified physical pilot.

## Measure factory work

For each attempt retain instrument/PC/variant IDs, exact release/image and baseline,
profile/calibration versions, operator/station, start/end time, outcome, failure
reason and the preceding attempt ID. Link R11/R12 receipts and physical evidence;
retain failed attempts rather than replacing them after rework.

Record operator active time, unattended machine time, waiting for fixtures/parts
and rework time separately. Machine-run duration is not hands-on labor. Capture
first-pass yield, failure causes, training needs, station occupancy and interruptions
by variant before choosing staffing or a takt-time limit. Around six microscopes
per week is a planning volume, not a measured station capacity or acceptance limit.
Central authenticated manufacturing inventory and this manual timing capture still
need an operational implementation; the current runner does not measure them.

## Run the tooling and readiness check

From the repository root in WSL, with Python 3.12+:

```bash
bash scripts/check-pilot.sh
bash scripts/check-gates.sh
python3 scripts/pilot.py plan --profile configs/pilot.example.json
python3 scripts/pilot.py preflight --profile configs/pilot.example.json \
  --output artifacts/r13/preflight
```

The portable tests need no Docker, GPU, Rust or Node. They also run on Windows.
Preflight exits 2 with a new UUID directory and blocked report; it makes no Docker,
device or motion calls. Every current report keeps production_qualified,
optimization_approved and shipment_authorized false.

## Collect engineering observations

Require Linux x86_64, root Docker access, NVIDIA Container Toolkit, real CUDA and
the existing local R07 runtime image. No image build/export or vendor SDK is
needed. Keep space for small bounded synthetic projects and retained reports.

```bash
sudo python3 scripts/pilot.py run-engineering \
  --profile configs/pilot.example.json --image oscope-backend:r07 \
  --seconds 45 --interval 2 --output artifacts/r13/observations
sudo python3 scripts/pilot.py run-engineering \
  --profile configs/pilot.example.json --image oscope-backend:r07 \
  --seconds 600 --interval 5 --output artifacts/r13/observations
```

Every run creates its own simulated configuration/account/state, resolves an
immutable image ID, verifies actual CUDA kernels, trusted TLS and a rendered GPU
preview session, then samples only its backend process. It cannot target an
existing server or mount physical hardware. Other running container names are
recorded as possible contention; they are never stopped. Unknown GPU identity
prevents comparisons even when kernel execution itself succeeds.

The workload is explicitly small: quarter sensor (1368×912), a central 256×192
ROI, simulated time rate 10, and a 160×120 JPEG preview capped at 5 fps. It reuses
the R07 regression's blank-glass scan coordinate. Each configured burst performs
dark calibration, capture, a four-tile scan, stitching and a scientific report,
with the unchanged 2.5 µm simulated scan-position bound. Camera acquisition stops
at each workflow boundary and preview resumes afterwards. This tests lifecycle
and bounded concurrent work, not uninterrupted recording or physical image quality.

The configured 1–10 bursts are spread through the observation window. The runner
refuses another burst once retained private state reaches 256 MiB. It creates no
unbounded recording ring and deletes no acquired data. After measurement it
stops its clients, restarts its own backend and verifies every burst's saved jobs,
scan and report byte hashes plus the final active calibration/TLS identity.
Only the owned labelled container is removed; private synthetic state is retained.

## Interpret the measurements

| Field | Meaning and limit |
|---|---|
| preview_window_mean_p95_ms | Nearest-rank p95 of sampled server-reported one-second mean exposure-end-to-message-ready latency; not per-frame p95 or GPU render time |
| workflow_median_seconds | Median wall time of the configured complete R07 workflow bursts; not a confidence interval or operator labor |
| rss_peak_bytes | Maximum observed RSS of the owned Linux backend process; excludes device VRAM and other processes |
| fds_peak | Maximum observed open Linux file descriptors for that backend |
| preview_drop_per_sent | Last sampled cumulative session dropped/sent, preserving P53's denominator |
| cpu_seconds | Raw process CPU time sampled from /proc; no GPU utilization attribution |

samples.jsonl retains actual monotonic sample times and counters. Sampling/API
work adds some overhead; keep the same harness/interval for comparisons. Reported
latency includes the simulator, scheduling and queueing; do not compare it to the
legacy 30 ms/20 MP GPU-render benchmark. Full-resolution recorder, demosaic,
registration/stitch/zoom budgets, GPU VRAM attribution, continuous combined-load
endurance, physical camera/stage metrics and operator time are not measured here.

The legacy P53 leak reference keeps its exact steady-state rule: discard at least
300 seconds and the first quarter of the measured span, then require 10 samples.
The Theil–Sen growth estimate must exceed **both** 32 MiB and 5% of median RSS to
flag memory growth; FD growth must exceed both 32 and 10%. These are software
regression references, not approved physical product limits. The source preview
reference requires dropped <= 0.01 × sent. A missed reference fails the run.
A short/sparse series reports insufficient_evidence, not a leak-free pass.
Ten minutes exercises this detector; it does not qualify a four-hour or product-
specified endurance requirement. Numerical limits and duration for production
must be explicitly approved in the pilot profile after physical measurements.

## Verify and compare retained evidence

Replace RUN-ID with the printed UUID:

```bash
python3 scripts/pilot.py verify artifacts/r13/observations/RUN-ID
```

Verification checks file hashes and recomputes summaries from raw measurements.
HTML is a standalone escaped engineering report, not a Vue change. Checksums
detect accidental corruption; they are not signed physical qualification evidence.
Export only manifest-listed public files plus checksums.json. Never copy private/
(accounts, TLS keys, application state/logs). A hard kill may leave incomplete
raw evidence; without a verified final report it cannot count as a completed run.

For a comparison, collect at least the configured number of **independent runs**
on each side (three by default), alternating baseline/candidate order on the same
prepared idle host. Use identical workload/profile, simulator config, duration,
interval, GPU/driver, kernel/Docker, host identity and harness sources. Do not
count multiple workflow bursts in one process as independent repetitions. Keep
one immutable image per side and retain exact source/image identities.

```bash
python3 scripts/pilot.py compare --profile configs/pilot.example.json \
  --baseline artifacts/r13/observations/BASE-1 \
  --baseline artifacts/r13/observations/BASE-2 \
  --baseline artifacts/r13/observations/BASE-3 \
  --candidate artifacts/r13/observations/CANDIDATE-1 \
  --candidate artifacts/r13/observations/CANDIDATE-2 \
  --candidate artifacts/r13/observations/CANDIDATE-3 \
  --output artifacts/r13/comparison.json
```

Different context, another container, unknown GPU identity, a failed/incomplete
run, missing metrics or insufficient steady-state evidence
returns not_comparable and exits 2. Duplicate run IDs are invalid input and exit 2
without creating a comparison. Missing configured budgets/regression allowance
returns budgets_missing and exits 2. A configured regression or absolute budget
miss returns regression_detected and exits 2. An available comparison exits 0
with medians/deltas and any same-image control explicitly labelled; it still
never approves an optimization or shipment. Run medians alone establish neither
statistical confidence nor causality. Independently review output correctness,
representativeness, variance, cold/warm cache behavior and tradeoffs before a
separate measured implementation and owner decision.

Different image IDs alone do not establish different application code; R12's
metadata-only candidate is an example. Review source/build/configuration identity
alongside image identity before describing a result as a code improvement.

## Backend optimization decisions

The preparation checkpoint changes no application hot path. The
[backend optimization review](backend-optimization-review.md) maps concrete
source locations to hypotheses, missing measurements and regression checks.
Use it to choose one change after comparable evidence exists. Preserve numerical
tolerances, CUDA-default/CPU fallback, live/batch fairness, cancellation and data
durability; do not redesign or optimize the frontend. R13 remains open until a
representative physical pilot, approved budgets and any justified backend changes
have accepted evidence.
