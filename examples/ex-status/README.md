# ex-status

P50's instrument status: each subsystem's health, rolled up for the status indicator, with faults injected into the simulated microscope.

1. **The subsystems:**
   - **The devices:** the camera, the stage and the lights, each asked each second.
   - **The GPU,** through NVML, loaded at run time: its name, temperature and memory. Without an NVIDIA driver it is offline, which counts for nothing, as the CPU does the work.
   - **The scratch disk:** its free space, and how fast it fills.
   - **The jobs** (degraded if one failed in the last ten minutes), and the extensions (degraded if one has a problem).
2. **The roll-up,** the instrument's state, is the worst any subsystem counts as:
   - the camera or the stage offline or at fault, or the scratch disk full, is a **fault**;
   - anything else not well is **degraded**.

   The `status` Thing gives it as `GET /api/v1/status/instrument`, and tells each change in the event `changed`.
3. **The faults,** the `simulation` Thing's `inject_fault`, with `active` true or false:
   - `camera_unplugged`: the camera answers nothing, and stops streaming;
   - `stage_alarm`: the stage stops in alarm, as a limit switch leaves it;
   - `scratch_full`: the scratch disk is said to be full.
4. **The script:**
   - At the start, the stage wants homing: a fault. Homed, all is well.
   - Then each fault, in turn, is injected and taken away again.
5. **The page** shows the indicator and each subsystem, kept up by the event. Its boxes inject the faults.

## How to try it

```sh
cargo run --release -p ex-status
```

It serves at http://127.0.0.1:8215/. On the owner's PC, at real time, it took 23 s and printed:

```text
ex-status: at the start, "fault": "stage: in alarm: homing required"; homed, "all well"
  camera_unplugged: "fault", "camera: disconnected"; taken away, "all well"
  stage_alarm: "fault", "stage: in alarm: a limit switch (a fault injected)"; taken away, "all well"
  scratch_full: "fault", "disks: the scratch disk is full (a fault injected)"; taken away, "all well"
  the GPU: "ok", "NVIDIA RTX A2000 12GB, 41 °C, 439 of 12282 MiB"
  open http://127.0.0.1:8215/ to see the indicator, and inject faults
```

## Its checks

`cargo run -p ex-status -- --smoke` runs the script at ten times real time, then checks that:

- the instrument starts at fault, the stage wanting homing, and is well once homed;
- each fault is the fault the status reports, saying which and why, and the status is well again without it;
- the page and its script are served.
