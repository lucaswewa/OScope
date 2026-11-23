# R09 Real devices and WSL feasibility

- Status: Preparation complete; physical testing deferred; hardware qualification pending
- Date: 2026-11-23
- Source: `44bc08a53f98dc29db1fa6b488a4f6269732a71a`
- Prerequisite: R08 approved by owner on 2026-11-23
- Owner direction: Prepare tools/runbook here; test physical hardware later
- Hardware qualification: Not run

## Implementation specification

The pinned `docs/hardware.md`, P02b survey and `oscope-hal` describe a Tucsen
MIChrome 20, a future trigger-capable replacement, and a grblHAL controller whose
board, settings, encoders and wiring remain unknown. There are no real drivers
to restore. Preserve these historical facts as references, not current device
identity or Linux/container performance evidence. The installed Windows SDK is
not a Linux SDK and must not be loaded in the Linux backend.

R09 preparation adds read-only Windows/WSL/container inventory tools, an explicit
per-instrument USB/SDK profile, focused dependency-free tool tests and a physical
bring-up runbook. Reports identify the collection environment, exact USB identity,
SDK binary fingerprint and missing prerequisites. They always label functional
qualification `not_run`; successful enumeration never qualifies acquisition or
motion. Strict preflight exits nonzero on missing/ambiguous identity, missing
access or an unconfigured/wrong SDK. Reports do not overwrite earlier evidence.
Do not open cameras/serial ports, load vendor libraries, attach USB devices,
install drivers, send controller commands or execute motion from these tools.

Use Python 3.11+ standard library and PowerShell 7.4+; add no application packages
or Rust FFI bindings. Inspect Linux SDK ELF metadata without executing it. Keep
SDKs, identities and reports outside tracked source; use ignored `artifacts/`
for local evidence. No frontend changes or full application replay is required.
Add a separate hardware-tools gate that runs without Rust, Node, GPU or devices.

Later physical work must establish Linux SDK/model support and redistribution
terms, run vendor examples in WSL and a restricted container, implement real HAL
drivers from the verified Linux headers/protocol, then qualify sustained capture,
scientific image correctness, stage limits/stop behavior and disconnect recovery.
Actual CUDA execution remains required where the hardware profile specifies it.
No simulator result, inventory report or historical Windows measurement closes
those exit criteria. R10 production provisioning remains a separate phase.

## Manual test

Prerequisites: the existing Windows/WSL development checkout, PowerShell 7.4+,
Python 3.11+, Docker and the approved R07 runtime image. Preparation needs no
camera, stage, Linux SDK, GPU, Rust build or Node. Run only the inventory/tools
steps now. Physical access, vendor examples, controller queries and motion are
deferred; follow the [hardware runbook](../hardware-bringup.md) when the actual
equipment, Linux kit, operator and fixture are available.

1. In the WSL checkout, run the independent fixture tests:

   ```sh
   bash scripts/check-hardware.sh
   bash scripts/check-gates.sh
   python3 scripts/gates.py --all --run hardware
   ```

   Expect nine hardware-tool tests and seven router tests to pass. The explicit
   last command runs only the hardware-tool category. Fixtures cover stable
   identity after bus changes, missing/ambiguous/inaccessible devices, wrong
   platform/root permissions, malformed profiles, wrong SDK architecture and
   fingerprints, disconnect during sysfs collection and immutable reports. They
   are synthetic; none is evidence of a working camera or stage. Hardware-tool
   paths alone select `hardware`, without backend/frontend/image builds.

2. On Windows, from the repository root in PowerShell 7.4+:

   ```powershell
   $r09Run = (Get-Date -Format 'yyyyMMdd-HHmmss') + '-' + [guid]::NewGuid().ToString('N')
   $r09Report = "artifacts/r09/$r09Run/windows.json"
   ./scripts/hardware-host.ps1 -Report $r09Report
   $r09Inventory = Get-Content $r09Report -Raw | ConvertFrom-Json
   $r09Inventory.qualification
   $r09Inventory.prerequisite_issues
   ```

   Expect valid JSON, `scope = inventory_only`, qualification `not_run`, readable
   WSL version/distribution text and an explicit finding if usbipd is missing.
   Ordinary inventory exits zero after collecting findings; this does not imply
   readiness. No library, camera or port should be opened; no USB device should
   be shared/attached. The host collector itself requires Windows.

   On a host without working usbipd, the following must create a new report and
   exit 2:

   ```powershell
   pwsh -NoProfile -File scripts/hardware-host.ps1 -Report "artifacts/r09/$r09Run/windows-strict.json" -RequireUsbipd
   $LASTEXITCODE
   ```

   If usbipd is available, zero means that tool's version/list queries worked,
   not microscope qualification.
   Rerunning with an existing report path must fail without changing that file;
   compare `Get-FileHash` before and after. Always use new paths for normal runs.

3. In WSL, create a separate run directory and collect with the blank profile:

   ```sh
   r09_run="$(date -u +%Y%m%dT%H%M%SZ)-$$"
   sudo install -d -o "$(id -u)" -g "$(id -g)" artifacts/r09
   mkdir -p "artifacts/r09/$r09_run"
   python3 scripts/hardware-linux.py --profile configs/hardware.example.toml \
     --report "artifacts/r09/$r09_run/wsl.json" --require-ready
   echo "$?"
   ```

   Expect exit **2** and a written JSON report with `preflight.ready = false`,
   issues for unconfigured camera/stage identity and SDK, `sdk.loaded = false`
   and qualification `not_run`. A zero exit for this blank strict profile is a
   failure. Keep the report and use a new file for any rerun. Running without
   `--require-ready` records the same findings but exits zero for collection.

4. Execute the runbook's device-free container inventory command using this
   same run directory. Expect exit **2** and `container.json`, with the frozen
   R07 image ID, UID 1000, `environment.container = true`, incomplete prerequisites
   and qualification `not_run`. The root filesystem is read-only, capabilities
   are dropped, networking/GPU/host IPC are unavailable, and only script/profile
   and the new evidence directory are mounted. There is no SDK/device mount and
   the OScope entrypoint is bypassed. An image/permission/JSON failure is a tool
   failure, not successful negative hardware evidence.

5. Review the runbook's physical sequence and pending evidence table. Actual
   Windows↔WSL↔container capture comparison, Linux SDK loading, HAL drivers,
   sustained capture/image correctness, stage/stop/limits and disconnect/restart
   recovery remain **not run** until the equipment session. The inherited Windows
   measurements do not pass them. Do not move the stage or install/attach/update
   host components while performing this preparation review.

6. Keep all reports, logs, profiles and run IDs. These inventory commands start
   no persistent application; the negative container is `--rm`. Do not delete
   state/scratch volumes or instrument data. Later USB cleanup must undo only
   attachments/new sharing introduced by that physical test, as the runbook
   specifies. Leave R09 hardware qualification open and record owner approval of
   the preparation checkpoint separately.

## Validation evidence

Preparation validation on 2026-11-23:

| Check | Result |
|---|---|
| Inventory/profile/SDK metadata fixtures | 9 passed on WSL Python 3.12 and Windows Python 3.12.14 |
| Independent router regressions | 7 passed on both platforms; hardware-tool paths select no Rust/Node/image work |
| Windows PowerShell collector | Ran on PowerShell 7.6.9; readable WSL/device JSON; qualification `not_run` |
| Missing usbipd strict check | Expected exit 2 with a written report; no attachment/installation attempted |
| Report collision protection | Windows existing-report retry failed with byte-identical hash; Python fixture also verifies preservation |
| Native WSL strict inventory | Expected exit 2; unconfigured identities/SDK and unavailable USB sysfs recorded; UID 1000 |
| Runtime container strict inventory | Expected exit 2 with valid JSON and frozen image ID; no physical qualification |
| Container isolation | Inspected non-root UID 1000, read-only root, private IPC, no network, dropped capabilities and no USB/GPU reservations |
| Workflow and documentation | Actionlint 1.7.12, shell/PowerShell syntax and local Markdown links passed |

The container uses the existing approved R07 image
`sha256:6faf43fc6c6c72b4abbb0cfa579410a16f20e9d490c9e8ce4c6ba7796751d667`;
its entrypoint is overridden with Python to run inventory only. No backend
rebuild, dependency/lock change, frontend change, device access or GPU benchmark
is introduced. The full R08 application suites are not repeated for these tools.

Windows has WSL 3.0.5.0 / kernel 6.18.58.1 and the existing Windows TUCam kit at
`C:\Program Files (x86)\TUCam_SDK`. No Linux SDK was supplied. usbipd is absent,
USB sysfs is unavailable in the current WSL session, and the PnP inventory shows
no identified MIChrome 20/grblHAL controller. This development laptop is not a
qualified microscope/control-PC baseline.

Windows testing initially exposed locale-dependent reads in the shared gate
router. Repository metadata now explicitly uses UTF-8; all seven router tests
then pass on both platforms. The failed initial log is preserved alongside the
successful runs. Evidence is under ignored `artifacts/r09/`, including the
Windows/WSL/container reports, fixture/router logs and container inspection.

Hosted CI remains unverified because the account's Actions billing/spending
limit blocked R08 before execution. R09 adds Linux/Windows hardware-tool jobs,
not a native Windows backend gate. Inspect the fresh run after pushing; do not
interpret this local preparation as a hosted or physical-device pass.

Physical camera, stage, SDK loading/ABI, real HAL drivers, sustained acquisition,
image correctness, stop/limits and recovery are explicitly deferred by the owner
and remain **not run**. R09 as a whole remains open. The runbook records the
required equipment session and the decision path if WSL cannot meet the measured
requirements; production provisioning and shipment are not qualified here.
