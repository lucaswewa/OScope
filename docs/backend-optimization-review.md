# Backend optimization review for the pilot

The Linux port preserves layered crates, CUDA-default computation with CPU
fallback, scientific tolerances and existing application behavior. Backend,
frontend and boundary checks are independent. Measure a qualified workload before
changing runtime resource ownership or algorithms. The current
[pilot observer](production-pilot.md) provides limited engineering observations;
it does not provide physical or isolated comparison evidence.

## Architecture relevant to measurement

The API is the transport/composition boundary; teta-wot remains confined there.
HAL/simulator/acquisition provide frames and device control. Compute/imaging/
stitch/volume implement numerical work; pipelines compose nodes; jobs schedule
durable work; storage persists projects and records. Workflows combine these for
calibration, capture, scan, analysis and reports. The future Windows Tauri client
uses HTTP/WebSocket. No frontend optimization is included.

[Preview connection](../crates/oscope-api/src/preview.rs) and
[workspace connection](../crates/oscope-api/src/workspace.rs) each construct a
Compute instance. [Compute](../crates/oscope-compute/src/lib.rs) owns
[live/batch CPU pools](../crates/oscope-compute/src/pools.rs) and optional
CUDA resources. [CUDA initialization](../crates/oscope-compute/src/cuda/mod.rs)
creates priority streams and loads kernels; its allocation pool may retain freed
device memory. [Pipeline engines](../crates/oscope-pipeline/src/run.rs) already
accept Arc<Compute>, so resource injection has an existing boundary. This does
not prove a costly duplicate driver context: measure resource ownership, thread
counts and device memory before drawing that conclusion.

[Project document writes](../crates/oscope-store/src/store.rs) serialize to a
temporary file and rename it. [Job persistence](../crates/oscope-jobs/src/store.rs)
serializes and performs SQLite work through a mutex-protected connection. These
are boundaries to profile; their existence alone does not show a latency problem.

## Candidate changes and required evidence

| Candidate | Required baseline and success evidence | Regression requirements |
|---|---|---|
| Share a compute service across API consumers | Per-consumer pool/stream/module counts; startup time; attributed host/device memory; preview latency under representative batch work; isolated before/after comparison | Preserve live/batch scheduling, concurrent clients, cancellation, CPU fallback, actual CUDA execution and numerical/API tests |
| Add GPU memory admission | Peak VRAM by workload/GPU, allocation failures and concurrent worst-case jobs; bounded admission and recovery | Release capacity on cancellation/failure; no deadlocks/starvation or silent fallback in required-GPU qualification; preserve numerical results |
| Move blocking persistence to bounded workers | Trace SQLite/document serialization, filesystem latency and executor stalls on WSL; lower contention without unbounded backlog | Preserve ordering, transaction boundaries, atomic documents, job recovery and old/new acquisitions through update/rollback |
| Tune a hot kernel or transfer path | Profile the specific algorithm at representative full dimensions; distinguish transfer/JIT/cold-cache from steady compute | Keep reference images/tolerances, required GPU numerical tests and CPU parity; preserve rendering/calibration semantics |

These are hypotheses, not implemented optimizations or ranked wins. Process RSS
excludes VRAM, sampled preview-window means are not kernel timings, and the
quarter-sensor workload is not a full-resolution throughput budget. The observer
cannot establish the admission, kernel or physical persistence claims above.

## Decision procedure

The preparation run's ten-minute shared-host observation had 119 resource samples
and 59 steady samples. Its RSS growth estimate was about 21.45 MiB: above 5% of
the steady median, but below P53's 32 MiB floor, so the legacy reference passed.
FD slope was zero and the last sampled preview counter reported no drops. This
is not a finding of zero growth or proof of long-term stability. Collect a longer
qualified baseline to distinguish cache lifetime from sustained growth before
choosing a memory change. The actual comparison was refused for contention,
different duration/interval and insufficient independent repetitions. See
[the R13 evidence](phases/r13-production-pilot.md#evidence).

Approve the hardware/release/profile and collect representative isolated baselines
with independent repetitions. Use original full-resolution backend benchmarks
where the workload matches; keep recording/GPU/endurance separate from the daily
tools gate. Retain raw data, cache state, exact image/host versions, the hypothesis
and the output that must remain equivalent.

Implement one measured backend change in a focused follow-up. Compare on the same
host/harness/workload with alternating baseline/candidate order and unchanged
thresholds. Evaluate scientific output, memory, latency, fairness, cancellation,
CPU fallback and persistence together. A faster median is insufficient when
correctness, tail latency or recovery regresses. Defer a candidate without
convincing evidence; configuration differences and same-image controls are not
code improvements.

Record owner acceptance separately from measurement. R13 preparation changes no
application hot path and promotes no candidate. Physical pilot results, approved
budgets, GPU-memory attribution and a clean comparison host remain necessary to
choose and validate the next runtime change.
