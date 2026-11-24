# R10 Host baseline and provisioning

- Status: Tooling complete; clean-PC/cold-boot qualification deferred
- Date: 2026-11-23
- Prerequisite: R09 preparation approved by owner on 2026-11-23
- Physical hardware qualification: Still pending under the R09 deferral
- Owner approval: Pending
- Owner direction: Build/test tools here; qualify a clean PC later

## Implementation specification

Implement the host provisioning boundary described by the approved factory flow.
The R01 developer bootstrap intentionally follows live package repositories and
is not a production installer. R10 introduces an explicit candidate baseline,
local artifact/version checks, per-instrument identity and resumable installation
state, a Linux runtime initializer and a Windows startup task/supervisor definition.
Use the existing Linux image by immutable ID; never compile Rust or frontend code
on an operations PC. Do not copy a configured instrument as a template.

Windows owns its dedicated account, registered WSL distribution and startup task.
The startup task must use the same Windows identity that owns the distribution;
an interactive-login trigger or SYSTEM's unrelated WSL registration is not a
before-login boot solution. Keep a foreground WSL process alive. Docker owns
backend process restarts; the host supervisor owns WSL/Docker readiness and must
not race it or resume physical motion. Bound failures and report actionable state.

Keep host baseline, application image and instrument configuration identities
separate. Pin local template/image artifacts by SHA-256 and exact installed tool
versions. An unconfigured/candidate baseline is not a production qualification.
Protect initial credentials, create unique installation identity, and preserve
existing accounts, TLS, calibration and data on repeat/recovery. Refuse foreign
installations or changed identity rather than overwriting them.

Validate planners/state transitions with fixtures, and exercise the Linux
bootstrap against a dedicated simulated runtime with real TLS and CUDA on this
development host. Do not change this PC's Windows startup tasks, accounts, WSL
distribution registrations, global WSL settings or drivers for the lab check.
Fresh-PC installation, task execution before login, reboot/resume and hardware
readiness require a dedicated qualification PC and are recorded separately.
Release signatures/update/rollback belong to R12; physical factory acceptance
belongs to R11 and depends on the missing R09 evidence. No frontend optimization.

## Manual test

Use the [host provisioning runbook](../host-provisioning.md). Prerequisites for
local tooling checks: PowerShell 7.4+ on Windows, Python 3.11+ in WSL, the Linux
checkout and the existing R07 image. The separate runtime lab needs Docker root
access and a working CUDA GPU. It uses simulated devices only and does not
change Windows accounts/tasks, WSL registrations, drivers or global settings.

1. Run the independent tooling and router fixtures in WSL:

   ```sh
   bash scripts/check-provisioning.sh
   bash scripts/check-gates.sh
   python3 scripts/gates.py --all --run provisioning
   ```

   Expect all provisioning and router tests to pass, with no Rust/Node build,
   GPU probe or connected hardware. They cover immutable image/identity fields,
   foreign-state refusal, preservation, Linux version drift, maintenance,
   container bindings and clean-template rejection. A hardware/provisioning-only
   edit must select its own category rather than application/image gates.

2. In PowerShell from the Windows repository, run:

   ```powershell
   pwsh -NoProfile -File tests/provisioning/host.Tests.ps1
   ```

   Expect the host contracts to pass: boot trigger, correct Windows owner,
   password logon, bounded retries, foreground lifetime, interrupted-step resume,
   no repeated completed mutations, distinct encrypted bootstrap credentials,
   artifact/version rejection and foreign-root refusal. The task definition is
   accepted by Windows' in-memory Task Scheduler API; task registration remains
   unchanged. No WSL import or actual startup-task registration is performed.

3. Inspect an unconfigured candidate without installing on this PC:

   ```powershell
   $r10Target = 'C:\ProgramData\OScope\review-' + [guid]::NewGuid().ToString('N')
   $r10Plan = ./scripts/provision-host.ps1 -Mode Plan -BaselinePath configs/host-baseline.example.json -BundleDirectory artifacts/r10 -InstallRoot $r10Target -InstrumentSerial R10-SIM -PcSerial ENGINEERING-PC | ConvertFrom-Json
   $r10Plan.ready
   $r10Plan.issues
   Test-Path -LiteralPath $r10Target
   ```

   Expect `ready=false`, explicit missing pins/artifacts and host drift findings,
   and `Test-Path=false`. The example is deliberately unusable for Apply. Physical
   free space is checked on the local install drive, not inferred from the WSL
   virtual disk's larger guest free-space value. Reports/plans retain
   qualification `pending`. Invalid identity, another owner, changed artifacts
   or an unowned install root must fail rather than replace it.

4. From WSL, run the explicit simulated CUDA/bootstrap lab:

   ```sh
   sudo python3 tests/provisioning/lab.py --image oscope-backend:r07 \
     --report "artifacts/r10/lab-$(date -u +%Y%m%dT%H%M%SZ).json"
   ```

   Expect zero and `status=passed` for scope `simulated_linux_bootstrap_lab`.
   Required CUDA diagnostics must execute (no skip or fallback), trusted HTTPS
   and login must work, and two fresh installations must have distinct UUIDs and
   certificates. Reinitialization preserves TLS, synthetic data and credentials;
   a wrong credential is refused without rotation. Foreign identity is refused.
   The foreground supervisor remains alive during maintenance, the container
   stays stopped, and only explicit resume restarts it. A bounded supervisor
   invocation must finish successfully. The report must still mark physical
   hardware and Windows import/task/cold boot `not_run` and production
   qualification `pending`. A normal inventory/TLS pass cannot erase those gaps.

   The lab chooses unique ports/names/paths and removes only its owned containers,
   preserving all synthetic state. Do not point it at an instrument dataset or
   manually stop another review deployment. An image, CUDA, permission or TLS
   failure is a failed lab check, not successful negative evidence. Keep the log.

5. On the future disposable build station, prepare a clean candidate exactly as
   the runbook describes and audit it:

   ```sh
   python3 scripts/provision-template.py --archive /staging/host-rootfs.tar \
     --baseline-id YOUR-CANDIDATE-ID --report /staging/rootfs.audit.json
   ```

   The guard must reject carried machine IDs, unlocked/password-configured
   accounts, keys/history, journal/instrument/Docker state, unsafe archive paths,
   missing systemd/isolated PATH settings or another baseline ID. It never
   extracts or sanitizes an installed instrument. A guard pass establishes only
   the checked template metadata, not provenance, completeness or bootability.
   Pin the reviewed candidate artifacts and all host/Linux versions before Apply.

6. **Deferred clean-PC qualification:** follow the runbook's separate dedicated
   Windows account/PC procedure. Record a fresh Apply, same-install repeat,
   interrupted install/resume and a cold boot before interactive login. Prove
   readiness timestamps/task events predate that login; a task XML or start after
   login is not sufficient. Verify exact baseline pins, CUDA, TLS, Windows-local
   client access, distinct unit identities/credentials and bounded fault recovery.
   Actual R09 camera/stage qualification remains required for the combined platform.
   Do not execute those host changes or reboot this development PC for this review.

7. Preserve receipts, reports, synthetic data and controlled credential handoff.
   Do not publish credential files. The local lab already removes its own
   containers; no data/volumes need deletion. On the qualification PC, stop only
   the task/distribution/probes that belong to that installation when required,
   and use its explicit maintenance operation. Do not use global WSL shutdown,
   unregister another distribution or reset an instrument to a template.

## Validation evidence

Evidence on 2026-11-23 is retained under ignored `artifacts/r10/`:

| Check | Result |
|---|---|
| Runtime/template fixtures | 12 passed on WSL Python 3.12.3 and Windows Python 3.12.14 |
| Gate router regressions | 8 passed on both platforms; provisioning-only changes select no Rust/Node/image work |
| Windows host contracts | 30 assertions passed on PowerShell 7.6.9 |
| Native task-definition API | Accepted boot/password/owner definition in memory; registered OScope task list unchanged |
| Real-host Plan | Missing candidate pins/artifacts/tooling and insufficient physical free space reported; no install directory created |
| Required-CUDA Linux lab | Passed two separate simulated installations using the existing immutable R07 image |
| Repeat/recovery checks | Trusted TLS/login; distinct UUIDs/certificates; identity/TLS/data/credential preservation; wrong credentials and foreign identity refused |
| Supervisor/maintenance | Foreground lifetime, maintenance stop, explicit resume and bounded cycles passed |
| Syntax/docs/workflow | Shell/PowerShell syntax, local links and Actionlint 1.7.12 passed |

The final simulated lab report is `lab-final-current.json`, with matching runtime
tool SHA-256 `ec83e3465c46cb6c6dd51f9c8c6e41f4d9c41b42a02b9e8d799078db35a9c0bd`.
It records `windows_import_task_cold_boot=not_run`, `physical_hardware=not_run`
and `production_qualification=pending`. The exact runtime image is
`sha256:6faf43fc6c6c72b4abbb0cfa579410a16f20e9d490c9e8ce4c6ba7796751d667`.
All lab-owned containers were removed without deleting synthetic state; the
already-running R08 review deployment was left running. No Windows account,
startup task, WSL registration, global WSL setting, driver or existing instrument
state was changed.

The Windows contracts use fixture action drivers for interruption/read-back tests
and actual Windows DPAPI for distinct protected credential handoff. They do not
prove the complete Windows Apply path. The template guard is tested with small
synthetic archives, not a new bootable qualified rootfs. This development PC has
about 6 GiB free on the install drive and no usbipd; the guest's much larger free
space is not substituted for that physical check. No large rootfs/image archive
or new distribution was created here.

Backend image inputs, application Rust/Python implementation, Cargo locks and
frontend files are unchanged. There is no new application dependency or hidden
frontend/backend compilation. The full R08 application suites are not repeated
for host tooling. Linux version drift is checked before startup; Windows update
policy remains part of the managed baseline qualification.

Hosted CI remains unverified because the account billing/spending limit blocked
R09 before execution. R10 adds Linux/Windows provisioning-tool jobs, not a native
Windows backend gate; inspect the fresh run after push. Release authentication,
updates/rollback, real device qualification and factory acceptance remain their
separate phases.

The owner explicitly deferred clean-PC qualification. A complete candidate
baseline, full Windows import/apply, actual task registration, before-login cold
boot, interrupted import/reboot recovery, Windows client access and the combined
hardware platform remain **pending**. This is the completed tooling checkpoint;
R10 host/platform qualification remains open.
