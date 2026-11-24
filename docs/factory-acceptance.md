# Factory acceptance and rework

R11 provides a guided checklist, a versioned profile, retained attempt reports and
an isolated engineering adapter. It does **not** implement physical acceptance.
The exact camera/stage drivers, fixture and operating limits still need R09
qualification; the Windows host needs R10 qualification; approved release
verification belongs to R12. No current runner outcome authorizes shipment.
See [the production flow](factory-flow.md) and
[R11 evidence and manual tests](phases/r11-factory-acceptance.md).

## Operator preparation

Operations should have a station with a barcode scanner, qualified host baseline,
approved offline release, certified calibration targets and the exact fixture.
Engineering must first approve the hardware BOM, per-objective calibration
procedure, camera mode matrix, motion bounds, stop procedure, endurance duration
and measured acceptance limits. These are product qualification inputs, not
values inferred from a simulator or the production rate of 300 microscopes/year.
Assign an operator badge ID and station ID. Never copy another microscope's
credentials, identity or calibration to save installation time.

The tracked [profile template](../configs/acceptance.example.json) deliberately
contains null qualifications, serials, firmware and physical limits. Its 10x
objective is an example, not an approved production objective list. Copy it to a
controlled profile revision only after the qualification evidence exists. Record
the exact camera and controller identities; the initial survey's MIChrome 20 and
grblHAL model names alone do not prove a matching device. Profile references are
labels at this checkpoint, not verified signatures or attached qualification
evidence. Filling every field cannot unlock an unimplemented acceptance adapter.

## Read-only preparation today

From the Linux checkout, print the guided sequence and check the supplied profile:

```bash
python3 scripts/acceptance.py plan --profile configs/acceptance.example.json
python3 scripts/acceptance.py factory-preflight \
  --profile configs/acceptance.example.json \
  --instrument ENG-001 --pc ENG-PC-001 --operator engineer --station development \
  --output artifacts/r11/preflight
```

Use scanned IDs with letters, digits, dots, underscores, colons or hyphens, up to
96 characters. The preflight exits 2 and writes a new blocked attempt. It makes
no Docker, USB, camera or stage calls, even when qualifications are claimed in
the profile. Read the listed gaps and send them through the qualification process.
There is no force, skip or factory-pass option.

## Engineering exercise today

Use [the R11 manual test](phases/r11-factory-acceptance.md#manual-test) for the
complete command. The explicit run-engineering command needs root on Linux
x86_64, Docker/NVIDIA Container Toolkit and a local R07 runtime image. It resolves
the image tag once to an immutable image ID, records its OCI revision, creates a
new simulated configuration and random bootstrap account, and exposes a fresh
loopback-only TLS port. It never pulls an image or accepts an existing server
address. No physical device mounts, USB attach, Windows startup changes or
existing deployment changes are performed.

The runner executes actual CUDA smoke kernels, verifies TLS/login/control and
live simulated configuration, then runs autofocus, exposure, white balance and
all six calibration wizard steps. It preserves the legacy numerical tolerances
against injected simulator truth. The R07 workflow checks GPU preview delivery,
dark calibration, capture, scan positions, stitching and scientific report output.
A clean restart then checks saved jobs, acquisitions, all active calibrations,
TLS identity and unchanged output byte hashes. These are software regression
checks; they do not measure physical frame rate, motion accuracy, reference-image
quality or endurance. Frontend behavior and performance are outside this runner.

Success exits 0 with outcome engineering_passed and shipment_authorized false.
Failure exits 1, blocked/invalid input exits 2 and Ctrl+C exits 130 when the
runner can finish its cleanup. Required CUDA cannot skip or fall back to CPU.
Every attempt removes only its own labelled container and retains its private
synthetic data. A failure stops subsequent steps; cleanup failure also fails the
attempt. Exception types and source locations are reported without exception text
or subprocess stderr, which could contain credentials. Inspect retained private
application state locally when deeper diagnosis is needed.

## Attempt evidence and rework

Every run creates a new UUID directory with report.json, report.html,
events.jsonl, profile.json, checksums.json and any step-specific JSON evidence.
The report records operator/station/instrument/PC IDs, times, runner version,
exact profile hash, source hashes, image identity, host/kernel/Docker information,
calibration measurements and workflow references. Unknown Windows/physical/SDK
qualification is explicit. Frontend is recorded as not tested. Each step has a
status; failed and unrun work cannot count as passing evidence.

Open report.html locally to review it. Verify the exported evidence using the
printed UUID directory (replace RUN-ID):

```bash
python3 scripts/acceptance.py verify artifacts/r11/lab/RUN-ID
```

Export only files listed in checksums.json plus checksums.json itself. Do not
archive the entire run directory: private/ contains accounts, TLS keys, local
tokens, logs and application data. Checksums detect accidental corruption; they
are **not** signatures, an authenticated release or tamper-proof factory records.
R12 must define authenticated release/inventory storage and its trust boundary.
If power loss or a hard kill prevents report finalization, preserve the event
journal as an incomplete attempt; it never counts as a pass.

For rework, fix the cause and invoke a new full run with --previous pointing to
the old report.json. The runner verifies its checksums and requires the same
instrument, PC and exact profile hash. It records the preceding run ID and report
hash, creates a new directory and executes every step again. A changed profile or
PC starts a new series without --previous; retain the old reports in instrument
history. The previous run is never overwritten or used to reuse passed steps.
Do not resume an interrupted physical motion sequence automatically.

## Deferred physical station acceptance

After qualification, the physical adapter must guide the operator through the
printed checklist with explicit fixture readiness before any motion. Each
objective needs its own target/calibration and measurement records. Record camera
mode/frame/timestamp/corruption/reconnect results, actual device serials and
firmware, stage home/limits/repeatability/stop results, focus error, reference scan
comparison, combined-load duration and recovery/persistence checks against the
approved limits. Retain selected image evidence with hashes and units, and keep
all failed/reworked attempts.

Only a verified release, qualified host, correct physical devices and complete
passing measurements may produce a future accepted result. Have an operator run
one reference microscope without developer intervention before closing R11.
Measure operator time, rework causes and throughput during R13's pilot before
setting station capacity. Until these exit criteria pass, use this checkpoint
for engineering preparation only.
