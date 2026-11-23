# R09 hardware bring-up runbook

This is an engineering feasibility procedure for the Windows → WSL2 → Linux
container path. The owner deferred physical testing on 2026-11-23. The implemented
tools collect inventory only; real device drivers and physical acceptance remain
pending. The current OScope application still uses simulated devices.

## Preparation available now

Use PowerShell 7.4+ on Windows and Python 3.11+ in the Linux checkout. No new
Python packages, Node, Rust compilation, GPU or connected device is needed.
Inventory neither loads a vendor library nor opens a camera/serial port. Reports
always contain `scope = inventory_only` and `qualification.status = not_run`.

Windows, from the repository:

```powershell
$r09Run = (Get-Date -Format 'yyyyMMdd-HHmmss') + '-' + [guid]::NewGuid().ToString('N')
$r09HostReport = "artifacts/r09/$r09Run/windows.json"
./scripts/hardware-host.ps1 -Report $r09HostReport
Get-Content $r09HostReport -Raw | ConvertFrom-Json
```

This records OS/GPU/PnP identity, WSL version/distributions and usbipd version/list.
Missing tools are findings, not a physical-test pass. `-RequireUsbipd` additionally
exits 2 if USB forwarding tooling is unavailable. Existing report paths are
refused; generate a new run ID for every attempt. Keep actual component serials
and reports in ignored `artifacts/` or the instrument's controlled evidence store.

In the WSL Linux checkout, as its ordinary non-root owner:

```sh
r09_run="$(date -u +%Y%m%dT%H%M%SZ)-$$"
sudo install -d -o "$(id -u)" -g "$(id -g)" artifacts/r09
mkdir -p "artifacts/r09/$r09_run"
cp configs/hardware.example.toml "artifacts/r09/$r09_run/profile.toml"
python3 scripts/hardware-linux.py \
  --profile "artifacts/r09/$r09_run/profile.toml" \
  --report "artifacts/r09/$r09_run/wsl.json"
```

The unconfigured template intentionally records incomplete prerequisites. Fill
VID/PID and a stable serial for each actual camera/controller from observed
identity. The match is exact and must be unique. Bus numbers and `/dev/ttyUSB0`
are current addresses, not identity. A missing serial requires an explicitly
reviewed alternate identity design; this tool does not silently match by model
or pick the first device. The present profile describes a USB camera and USB
stage controller; revise the design explicitly if the actual controller uses
another transport.

Set `sdk.library` to the absolute path of the verified Linux x86_64 vendor shared
library. A first inventory can record its SHA-256; review it against the supplied
kit, then pin that value in `sdk.sha256`. A Windows DLL, wrong architecture,
changed digest, missing file or unconfigured digest fails strict preflight. An
ELF header/fingerprint match establishes neither SDK loading nor camera support.

```sh
python3 scripts/hardware-linux.py \
  --profile "artifacts/r09/$r09_run/profile.toml" \
  --report "artifacts/r09/$r09_run/wsl-strict.json" --require-ready
```

Expect exit 2 while prerequisites are incomplete; inspect `preflight.issues`.
Exit 0 with `--require-ready` means enumerated prerequisites only. Apparent
read/write permissions do not prove that the kernel/device cgroup will allow
SDK I/O. The collector never opens the node to find out. Run as the intended
service/developer identity; root is rejected for this permission check.

To check the same collector in the existing runtime image, without USB devices
or a vendor kit, use the template negative check:

```sh
r09_image=$(sudo docker image inspect oscope-backend:r07 --format '{{.Id}}')
sudo docker run --rm --network none --ipc private --read-only --cap-drop ALL \
  --security-opt no-new-privileges --tmpfs /tmp:rw,nosuid,nodev,size=64m \
  -e NVIDIA_VISIBLE_DEVICES=void -e OSCOPE_HARDWARE_IMAGE_ID="$r09_image" \
  --mount "type=bind,src=$PWD/scripts/hardware-linux.py,dst=/opt/oscope/hardware-linux.py,readonly" \
  --mount "type=bind,src=$PWD/configs/hardware.example.toml,dst=/opt/oscope/hardware.toml,readonly" \
  --mount "type=bind,src=$PWD/artifacts/r09/$r09_run,dst=/evidence" \
  --entrypoint python3 "$r09_image" /opt/oscope/hardware-linux.py \
    --profile /opt/oscope/hardware.toml --report /evidence/container.json --require-ready
```

Expect exit 2, an incomplete report with that frozen image ID, and
`qualification.status = not_run`. No OScope server, simulator or SDK is started.
The report directory must belong to the runtime's UID 1000. No deployment volume
is attached. This negative check is useful on a development laptop without the
microscope, and is not hardware qualification.

## Physical feasibility sequence — deferred

Before beginning, obtain the exact camera/controller models and serials, the
Linux x86_64 SDK with matching headers/examples, firmware, approved usage and
redistribution terms, and an operator with the instrument's fixture and limits.
Record SDK kit version/hash, dependency requirements and any kernel-driver need.
Tucsen advertises Linux SDK development generally; this does not establish
MIChrome 20, Ubuntu 24.04, WSL or this kit's compatibility.
[Tucsen SDK support](https://www.tucsen.com/sdk-software-support-product/).

1. **Establish a paired reference.** Measure the actual camera with the approved
   Windows vendor/owner tool before forwarding it away from Windows. Record mode,
   output format/bit depth, exposure, all processing/auto settings, read-back ROI,
   USB topology, frame interval distribution, copied bytes and corruption checks.
   Preserve raw diagnostic summaries and selected reference images. No new probe
   executable is provided by this preparation checkpoint.

2. **Prepare USB forwarding.** On the dedicated test PC, install the approved
   usbipd-win package interactively and record its version/installer hash. The
   commands below use the documented 5.x-or-later syntax. Do not change the running
   development host or update/reboot it as part of inventory. R10 will qualify and
   pin the actual production host baseline.

   Keep WSL open, close Windows programs using the selected device, and inspect
   `usbipd list`. Match the physical device, VID/PID, serial and topology before
   choosing its current bus ID. Share only that identified device from an
   administrator PowerShell:

   ```powershell
   usbipd list
   # Set to the current, verified camera bus ID; never a hub or input device.
   $r09CameraBus = Read-Host 'Verified camera bus ID'
   usbipd bind --busid $r09CameraBus
   usbipd list
   ```

   In a normal PowerShell, retain/re-enter that verified value and attach:

   ```powershell
   usbipd attach --wsl --busid $r09CameraBus
   usbipd list
   ```

   Repeat separately for the verified controller when its fixture is prepared.
   A forwarded device is unavailable to Windows and is visible across WSL2
   distributions. Sharing persists; attachment is lost on unplug/WSL restart.
   Install reviewed vendor/identity-specific udev rules before attachment when
   needed, so the intended non-root service identity receives access.
   [Microsoft USB procedure](https://learn.microsoft.com/en-us/windows/wsl/connect-usb),
   [usbipd lifecycle and udev](https://github.com/dorssel/usbipd-win/wiki/WSL-support).

3. **Prove native WSL access first.** Recollect Windows and WSL inventories using
   new report names. Confirm unique profile matches, SuperSpeed topology where
   required, actual SDK identity and non-root access. Run the supplied Linux SDK
   console example according to that verified kit's instructions. Record exact
   commands and outputs, dependencies, return codes and camera identity. A Windows
   DLL, successful `lsusb`, or an ELF header is insufficient. Resolve any Linux
   driver/kernel incompatibility before continuing; never guess the FFI layout
   from the installed Windows headers.

4. **Repeat in a restricted container.** Freeze the candidate image ID. Use a
   clean synthetic evidence/scratch directory, read-only root, private IPC and
   explicit device nodes. Start with the inventory command above, replacing its
   profile with the filled one, and mount the approved SDK tree read-only at the
   same absolute path used by `sdk.library`. Add only verified current USB nodes
   with `--device "$camera_node:$camera_node:rw"` and
   `--device "$controller_node:$controller_node:rw"`; add the exact serial node
   separately when needed by the stage probe. Match supplementary group IDs to
   the reviewed device permissions. Recheck serial identity after hotplug before
   recreating those mappings. Do not use a whole-device-tree mount, privileged
   container, world-writable nodes or host IPC as a substitute for diagnosis.
   Device exposure follows [Docker's device mapping interface](https://docs.docker.com/reference/cli/docker/container/run/#device).

   Inventory still does not load the SDK. A vendor-example/probe image with its
   verified runtime dependencies and permitted distribution must be prepared
   after the actual Linux kit is available; the current R07 application image
   does not contain a real camera/stage driver. Repeat the identical acquisition
   modes/settings and compare against the paired WSL/Windows reference.

5. **Qualify images and sustained operation.** Start with bounded 30-second mode
   measurements and a ten-minute full-resolution 16-bit raw capture copying every
   frame out of the SDK buffer. Record actual duration, delivered frames/bytes,
   median/p99/max host intervals, timing gaps, timeouts, malformed frames,
   channel/mosaic corruption and CPU/GPU/memory use. Record any unavailable
   sensor sequence/timestamps; host arrival gaps are a heuristic, not proof of
   zero sensor-side losses. Do not record an unbounded full stream to disk.

   The historical Windows reference was 5472×3648 raw16 at about 7.24 fps and
   289 MB/s, with a ten-minute copy run. These are comparison data, not new Linux
   acceptance limits. Mode 1 raw8 had known green-channel corruption. Verify
   Bayer phase for odd-Y ROIs, image orientation, processing/white-balance/gamma
   state and raw16 scaling; do not assume Linux defaults equal Windows. Define
   and approve the actual performance/correctness profile using these paired
   measurements before calling feasibility qualified.

6. **Bring up the stage with an operator.** Opening its serial port can reset
   some boards. First document board/firmware, wiring, port-open behavior,
   travel, units, homing direction, hard limits, safe speeds and the physical
   stop. With the prescribed fixture/clearance, use the approved terminal to
   record read-only `$I`, `$I+`, `$$` and `?` responses. Do not change settings or
   issue homing/jog/motion commands during this preparation checkpoint. A later
   real driver must pass HAL conformance plus the fixture's homing, limit,
   direction, position/encoder, repeatability and stop tests. Those procedures
   require actual controller details and instrument-specific limits first.

7. **Prove recovery and combined load.** With the fixture prepared, exercise
   camera/controller unplug/replug, container restart, delayed arrival and PC/WSL
   lifecycle according to the approved recovery procedure. Record identity and
   node changes, cancellation, bounded recovery, interrupted-job reporting and
   explicit re-arm. Never automatically resume motion/acquisition. Repeat capture
   with recording/preview/processing load and actual required CUDA kernels. No
   missing device, simulated replacement, skipped check or required-GPU fallback
   may count as a physical pass.

8. **Restore the test station.** Stop only probes/containers started for the run.
   Detach only the devices this run attached, rechecking changed bus IDs first:

   ```powershell
   usbipd detach --busid $r09CameraBus
   # Only if this procedure newly shared it and sharing is no longer wanted:
   usbipd unbind --busid $r09CameraBus
   ```

   Unbind requires the administrator context. Preserve prior sharing choices,
   report history, calibration and data. Restore deliberately changed device
   settings using the recorded baseline. Do not delete volumes or restart other
   WSL/Docker workloads as cleanup.

## Evidence and decision

| Evidence | Current state |
|---|---|
| Windows/WSL/container inventory and tool tests | Implemented; see R09 phase evidence |
| Exact physical models, firmware, serials and safe fixture | Pending |
| Linux SDK/model support, ABI/dependencies and redistribution review | Pending |
| WSL and restricted-container vendor acquisition | Pending |
| Required real HAL drivers and scientific image checks | Pending |
| Sustained capture, stage/stop/limits, disconnect recovery and combined GPU load | Pending |

Keep immutable run IDs and record operator, instrument/PC/component identities,
host/kernel/driver/toolkit/SDK versions, image and collector IDs, profile hash,
exact commands/settings, measured results, limits, logs, selected images and
every failed/skipped step. Retesting creates a new record. The historical source
is `docs/hardware.md` and `docs/milestone-1/notes/P02b-hardware-survey.md` at
`44bc08a53f98dc29db1fa6b488a4f6269732a71a`.

R09 remains open until physical exit evidence exists. If WSL/USB/SDK cannot meet
the measured requirements, bring native Linux deployment or a Windows acquisition
helper to the owner as a new architecture decision. Preparing R10 tooling may be
authorized separately; it cannot qualify or ship an untested hardware path.
