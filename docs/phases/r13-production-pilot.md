# R13 — pilot preparation and measured optimization

Status: preparation tooling validated; production pilot and optimization approval pending. The owner approved R12's engineering
checkpoint and authorized R13 on 2026-11-23. Physical hardware, clean-PC,
acceptance and full-product release qualification remain open. Approval does not
substitute for those measurements or authorize customer deployment.

## Scope and source record before implementation

Pinned behavioral source: 44bc08a53f98dc29db1fa6b488a4f6269732a71a. Inspect its
xtask/src/perf.rs, crates/oscope-server/tests/soak.rs and P53 phase notes. Preserve
the leak detector's Theil–Sen slope, warm-up max(300 seconds, first quarter),
minimum 10 steady samples, 32 MiB/5% memory and 32/10% handle growth references,
and preview dropped <= 0.01 * sent. Short runs must say insufficient evidence,
never count the legacy detector's false result as a measured pass.

Use the current R07 TLS/workflow helpers and R11 isolated simulated runtime
bootstrap. New Python standard-library tools add a versioned pilot profile,
readiness report, bounded observation runner, process RSS/FD/CPU sampling, actual
CUDA/preview checks, workflow timings, output/restart verification and guarded
baseline/candidate comparisons. Keep raw observations and provenance. No new
application dependency or Rust/Vue change is planned in the preparation step.

Existing full-resolution legacy benchmarks remain separate explicit gates.
Quarter-sensor simulator observations do not measure their 20 MP budgets or set
physical production limits. Resource sharing, GPU admission, persistence workers
and kernel changes require comparable measurements plus their own regression
evidence before implementation or promotion. Do not invent an improvement when
no comparable baseline exists.

## Intended validation

Portable Linux/Windows fixtures cover the original leak/noise/spike/sawtooth/
fraction/floor behavior, inadequate warm-up/samples, invalid/non-finite profiles,
tampered/incomplete reports, missing GPU evidence, incompatible comparison
contexts, duplicate runs, insufficient independent repetitions and budget gaps.
The pilot-tools gate runs without Docker, Rust, Node or GPU. A separate explicit
Linux required-CUDA lab samples only its own backend and retains bounded
synthetic data. An existing review deployment must not be stopped; report it as
possible contention and do not claim an isolated timing qualification.

## Manual test

Prerequisites: Python 3.12+, this checkout and a writable evidence directory.
Run the portable tooling gate from the repository root:

```bash
bash scripts/check-pilot.sh
bash scripts/check-gates.sh
python3 scripts/pilot.py plan --profile configs/pilot.example.json
python3 scripts/pilot.py preflight --profile configs/pilot.example.json \
  --output artifacts/r13/preflight
```

Expected: tests pass; plan explains measurements, cohort and qualification gaps.
Preflight writes a fresh blocked report and exits 2 without device/Docker calls.
Filling profile claims cannot unlock missing physical qualification tooling.

For engineering observation, additionally require Linux x86_64, root Docker
access, NVIDIA Container Toolkit, a working NVIDIA GPU, the local R07 runtime
image and space for bounded synthetic projects (no image build/archive needed):

```bash
sudo python3 scripts/pilot.py run-engineering \
  --profile configs/pilot.example.json --image oscope-backend:r07 \
  --seconds 45 --interval 2 --output artifacts/r13/observations
```

Expected: actual CUDA kernels and GPU preview delivery, timed workflow bursts,
RSS/FD/CPU samples for the owned backend, retained measurements/provenance and
restart/reopen hash verification. This short run must report insufficient leak
evidence. Missing/skipped CUDA, failed workflow, bad preview framing, lost output,
worker error or interruption fails the run and retains evidence. CPU fallback
remains supported by the application but cannot qualify a required-GPU pilot.

For a longer software reference soak, use --seconds 600 --interval 5. At least
10 samples must remain after the original warm-up. This is still simulated
preview with bounded workflow bursts, not physical camera throughput or a full
combined-load/endurance qualification. Inspect other workloads and all not-measured
fields. Never claim an idle-host comparison while the review deployment runs.

The pilot runbook will document verify/compare commands, expected refusals,
representative unit selection, operator timing and optimization decision review.
The runner removes only its labelled container and keeps private synthetic state.
Export only public report/measurement files, never private accounts, TLS keys or
logs. Keep failed attempts and do not delete instrument data or volumes. Physical
pilot acceptance and any runtime optimization remain pending evidence/approval.

## Evidence

Validated on 2026-11-23:

- 19 pilot-tool fixtures passed on Windows Python 3.12.14 and WSL Python. They
  preserve P53's flat/warming/sawtooth/spike/slow-growth/floor/fraction behavior;
  short runs now report insufficient evidence. Profile, GPU/counter, retained
  summary, comparison and report-integrity guards passed. No GPU/body skip is
  counted as evidence by these portable tests.
  The final fault-injection case verifies that worker shutdown failure still
  invokes cleanup of the owned runtime.
- 11 gate-router tests passed on Windows and Linux. Selected pilot_tools commands,
  shell syntax, actionlint and links in 31 Markdown files passed locally. Pilot
  script/profile/test changes select only this gate. Shared router/CI changes
  select broader categories; the application suites were not repeated for this
  tooling change. No new library or application dependency was added.
- Real Windows and Linux preflight commands exited 2 with blocked reports. The
  fixture proves no subprocess/device calls occur, and filled qualification labels
  cannot unlock the missing physical pilot adapter.
- Short run 7e3dbb98-e792-44ae-ae82-1d7297168f82 completed 45 seconds, 22 samples,
  three workflow bursts, actual required CUDA/preview and restart/output checks.
  Memory/FD references correctly remained insufficient_evidence after no steady
  samples. It was not treated as an endurance pass.
- Long run c9ae5aef-4f3b-4485-9ee5-a0777428b4c8 completed the 600-second observation
  window with 119 samples, 59 after the original 300-second warm-up, 2,959 decoded
  GPU preview frames and three complete workflow bursts. CUDA, TLS/session,
  measurement, persistence and owned-container cleanup checks all passed. Every
  burst's project/jobs/scans/report hashes survived restart; the final active
  calibration and TLS identity were retained.
- Its last sampled session counters were 2,938 sent and zero dropped; the reader
  continued until the observation window ended. Peak sampled RSS was 379,809,792
  bytes and FD count 38. Estimated steady RSS growth was 22,495,733 bytes (about
  21.45 MiB), below the 32 MiB floor even though it exceeded the fractional bound;
  the preserved AND rule therefore passed. FD growth estimate was zero. These
  references do not prove zero growth or four-hour/physical endurance.
- Sampled preview-window-mean p95 was 235.81 ms and median workflow burst time
  3.56 s. Neither is an approved product budget, a 20 MP GPU-render measurement
  or operator active time. No runtime optimization was implemented or approved.
- The actual comparison of the short/long evidence exited 2 with not_comparable:
  other containers, different seconds/interval, inadequate independent runs and
  insufficient short-run soak evidence were all reported. It produced no
  improvement/promotion claim.

Both observations used immutable image
sha256:6faf43fc6c6c72b4abbb0cfa579410a16f20e9d490c9e8ce4c6ba7796751d667,
source revision ea3481a4e887bb06293f7a6c0cd49b0585fbae46 and an RTX 3070 Ti Laptop
GPU with driver 621.24. The existing oscope-r08-review-backend-1 deployment was
recorded as possible contention and remained running. The long report retains
its original runner source hashes. A subsequent cleanup-only try/finally safeguard
was checked by the added fault-injection test and final 45-second run
0751d743-d7fc-4146-8304-12039120a72c; its source hashes match this checkout. The
measurement loop and leak detector were unchanged, and the distinct harness
versions are not pooled as comparable repetitions. Public reports, raw
measurement JSONL, profiles and checksums verified on Linux and again after
export to Windows. Private accounts/TLS/application data were not exported.

Earlier failed run 663eda06-b26d-40bc-84ce-bcc9b3a40113 is retained. It exposed the
R07 helper's camera-start precondition when the observer already had streaming
active. The observer now stops acquisition at each workflow boundary and resumes
afterwards; the application/API and original regression helper were not changed.
The workload is documented as bounded preview/workflow intervals, not uninterrupted
recording. Evidence remains under ignored artifacts/r13/ in both checkouts.

All run-owned containers were removed. No customer instrument, Windows startup,
account, WSL setting, vendor SDK, dataset/volume, Rust/application Python, Vue or
dependency lock was changed. Backend build-input revision remains
ccf323596f80704c42ebb25e7fecc168e8d353c0. No new runtime image was built or exported.

Hosted execution remains unverified: R12's run was blocked before startup by
GitHub account billing/spending limits. Inspect the fresh R13 run after push.

The owner approved R12's engineering checkpoint separately from automated
validation and product qualification. R09 hardware/SDK/drivers, R10 clean-PC,
R11 physical acceptance, complete R12 release/startup integration, representative
pilot units, operator time/yield, approved budgets and a qualified isolated
comparison host remain open. Longer endurance and GPU-memory attribution also
remain unmeasured. R13's full exit criterion is still an accepted representative
production pilot and validated measured backend changes where justified.
