# R12 — release lifecycle tooling

Status: engineering tooling validated; full release qualification pending. The owner authorized this phase on
2026-11-23. Hardware, clean-PC and physical acceptance evidence remain pending.
Do not tag, publish a product release or deploy to a customer instrument.

## Scope and provenance before implementation

Pinned source: 44bc08a53f98dc29db1fa6b488a4f6269732a71a. Inspect its ADR 0033,
packaging/README.md and docs/releasing.md: preserve license policy, immutable
release identity, owner-only promotion and retention of instrument data. Replace
native Windows/deb packaging and the combined legacy replay with Linux container
delivery. Inspect the current scripts/provision-runtime.py, scripts/acceptance.py,
scripts/check-runtime.py, scripts/runtime_workflows.py, store schema version 1,
configuration version 2, control lease, job queue and camera/stage idle APIs.

R12a builds/verifies signed offline backend engineering bundles with an externally
pinned Ed25519 public key, exact image/source identity, hashes, sizes and schema/
baseline compatibility. R12b installs and changes isolated simulated runtimes,
preserving identity, credentials, calibration and acquisitions; compatible rollback
changes code without restoring an old data snapshot. Retain update receipts,
private consistent metadata backups and a traceable local inventory.

Use Python's standard library, installed OpenSSL 3 and Docker. No new application
dependency, Rust/Vue changes, frontend optimization or vendor SDK. Test keys stay
in ignored lab artifacts and never become production trust. New release-state
directories are distinct from R10 roots: existing fixed-image Windows startup
receipts must not be silently rewritten. Full product bundles, production key
custody/revocation distribution, R10 Windows lifecycle integration and physical
upgrade qualification remain separate pending evidence.

## Intended validation

Portable Linux/Windows fixtures cover real signing/verification, wrong/revoked
keys, tampering, malformed metadata, unsafe payload paths, schema/baseline mismatch,
sequence replay, data-preserving rollback, interruption and retained maintenance,
history integrity and an independent release-tools gate. Missing OpenSSL is a
failure, not a skipped signature check. A separate Linux required-CUDA lab uses
two distinct immutable images with identical application code (one metadata-only
candidate); it exercises actual offline import, idle refusal, update, compatible
rollback, new acquisitions after update, interrupted recovery and inventory.
This does not prove behavior across a real application/schema migration.

## Manual test

Prerequisites: Python 3.12+, OpenSSL 3 (on PATH or OSCOPE_OPENSSL), this checkout
and a writable evidence directory. Run portable tests from the repository root:

```bash
bash scripts/check-release.sh
bash scripts/check-gates.sh
```

Expected: all fixtures pass without Docker, Rust, Node, physical devices or GPU.
Signature failures, replayed updates and incompatible rollback must fail closed.

For the separate engineering lab, require Linux x86_64, Docker with NVIDIA
Container Toolkit and working CUDA, a local R07 runtime image, and space for two
compressed bundles plus two private cached copies (about 8 GiB for the reference
images) and metadata. Check the drive holding the WSL VHD as well as the guest.
No existing server
or physical instrument is used:

```bash
sudo python3 tests/release/lab.py --image oscope-backend:r07 \
  --output artifacts/r12
```

Expected: a new retained run directory, signed backend bundles, actual required
CUDA kernel execution, verified TLS, update and rollback with unchanged identity,
calibration/credentials and existing/new output hashes. Missing CUDA must fail;
CPU fallback cannot qualify this lab. The metadata-only second image must be
reported honestly. Failed/interrupted changes remain stopped in maintenance;
recovery and compatible rollback require an explicit command. No automatic
restore may erase newer acquisitions. Review the lab report and local inventory.

The release runbook will document individual packaging, signature verification,
install/update/recovery and inventory commands with expected results. Test keys
must never authenticate a product release. Preserve failed receipts and private
backups; export only public reports, never keys, credentials or whole state roots.
Remove only lab-owned containers as documented; do not prune images/volumes or
delete instrument data. Qualification on a clean PC and physical instrument,
release promotion and full product installation remain pending.

## Evidence

Validated on 2026-11-23:

- 20 portable release fixtures passed on Windows Python 3.12.14/OpenSSL 3.5.9
  and WSL Python/OpenSSL 3.0.13. They execute real Ed25519 signatures and cover
  wrong/revoked keys, altered manifests/artifacts, both image-ID forms, unsafe
  archives, sequence floors/replay, compatible rollback preserving newer data,
  interruption/recovery, immutable identity, history/receipt integrity and safe
  stationary versus moving/faulted stage states. Missing OpenSSL fails.
- 10 gate-router tests passed on Windows and Linux. The selected release_tools
  gate, shell syntax, actionlint and links in 28 Markdown files passed locally.
  Release-tool paths select only that gate. Shared router/CI edits deliberately
  select all categories; the full application suites were not repeated here.
- The real offline/CUDA lab passed in run
  0c447ffc-b4e4-4899-861c-1964585a6db0. It reused and reverified the signed
  Docker-save bundles built in 0876cfdf-6f9c-43b3-8798-bc899e3a273a, then
  created a fresh isolated installation. Final runner source hashes in the report
  match the Windows checkout. Disposable signing keys stayed in private ignored
  WSL artifacts and were not exported to Windows or installed as product trust.
- The two exact images were
  sha256:6faf43fc6c6c72b4abbb0cfa579410a16f20e9d490c9e8ce4c6ba7796751d667 and
  sha256:80f9e6933ea51d49d1b16271546394cca62ce9f6ad26455f30ce36f921e5d625.
  Both use source revision ea3481a4e887bb06293f7a6c0cd49b0585fbae46; the second
  differs only in image metadata. The original bundle-building lab removed its
  unused second image before import. Docker load consumed authenticated archives;
  no registry/package access was used by install/update/rollback.
- Actual required CUDA kernels and trusted authenticated TLS passed after install,
  recovery rollback, update and final rollback. Streaming caused refusal before
  shutdown. A child process exited with code 91 immediately after clean shutdown,
  leaving a durable pending transaction. Explicit recover/rollback retained data
  and returned to the verified first image; no acquisition or homing was resumed.
- Original scan/stitch/report outputs, jobs, calibration, TLS identity and login
  survived. A new acquisition made after update remained after rollback; all
  seven selected PNG/PDF payload hashes were unchanged. Inventory recorded 11
  history events, active release r12-lab-1 and sequence high-water 2. Normal replay
  of sequence 2 was then refused. Physical acceptance remained not_run.

Earlier failed attempts remain available. One exposed Docker 29's containerd
index ID versus configuration digest distinction; verification now checks the
signed identities and OCI chain separately, with a portable regression fixture.
Another rejected the simulator's expected homing-required startup alarm. The
adapter now accepts that stationary state, explicitly rejects movement/other
faults and never homes automatically. Metadata staging was changed to authenticate
the source manifest, then fully verify one private copy; it avoids three complete
archive reads without loading unverified bytes. Old failed reports were retained.

Public evidence is under ignored artifacts/r12/ in both checkouts. Only lab reports,
inventory and receipts were copied to Windows. Images/bundles, private backups,
keys and synthetic installation state remain in WSL. Lab-owned containers were
removed; the existing oscope-r08-review-backend-1 deployment remained running.
No Windows startup/account/WSL configuration, physical device, vendor SDK,
instrument data, dependency lock, Rust/application Python or Vue source changed.
The backend build-input revision remains ccf323596f80704c42ebb25e7fecc168e8d353c0.

Hosted execution remains unverified: R11's run was blocked before startup by
GitHub billing/spending limits. Inspect the fresh run after pushing R12.

Owner continuation is separate from product promotion. Full frontend/host/profile
assets, third-party shipping notices/SBOM, protected production keys and trust
updates, centralized authenticated inventory, R10 Windows startup integration,
clean-PC/offline installation, real application/schema upgrade and physical
microscope update/rollback remain pending. R12's full product exit criteria are
not marked complete, and no report from this checkpoint authorizes shipment.
