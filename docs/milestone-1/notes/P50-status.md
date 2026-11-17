# P50: Instrument status and health

- Status: Done
- Branch / PR: `m1/p50-status` / [#66](https://github.com/lucaswewa/OScope/pull/66)
- Size: M · estimated ~1,450 before it started (in `oscope-sim`, the faults ~80; the GPU through NVML ~80; the disk monitor ~60; the `status` Thing ~300; the fault actions ~50; tests ~400; `ex-status` ~300; glue ~30; a tenth on top); one pull request, at the owner's standing choice · measured **1,035** hand-written lines added (code 503: `oscope-api` 443, `oscope-sim` 60; tests 297; examples 212; glue 23), and 3 removed. **That is 435 over M's 600, and 29 % under the estimate**

## Summary

The data behind the instrument status indicator ([spec](../phases.md#p50); plan §5.19).

- **Each subsystem's health,** `ok`, `degraded`, `fault` or `offline`, with a message:
  - the camera, the stage and the lights, each asked each second;
  - the GPU, through NVML, loaded at run time: its temperature and memory;
  - the scratch disk: its free space, and how fast it fills;
  - the jobs, and the extensions.
- **The roll-up:** the worst any subsystem counts as.
  - The camera or the stage lost, or the scratch disk full, is a fault.
  - An absent GPU counts for nothing.
  - Anything else not well degrades it.
- **The `status` Thing:** `instrument`, and the event `changed`.
- **Faults to inject in the simulator:** the camera unplugged, the stage in alarm, the scratch disk full, through the `simulation` Thing's `inject_fault`.
- **`ex-status`:** the indicator, and the faults in turn.

## What was built

- **`oscope-sim`:**
  - `SimCamera::unplug`: unplugged, it stops streaming, and its settings, configuring and starting answer `Disconnected`;
  - `SimStage::inject_alarm`, which stops the stage where it is and ends a move in progress with the alarm, as a hard limit does;
  - `Simulator::faults`, `SimFaults`, holding `scratch_full`.
- **`oscope-api`:**
  - `status`: `State`, `Health`, `Instrument`, `roll_up`, and the `StatusThing`, served by every server. It samples every second, and emits `changed` when the state or message changes.
  - The `simulation` Thing's `inject_fault(fault, active)`.
- **The dependency:** `nvml-wrapper` 0.13.0, which loads NVML at run time and keeps its unsafe code to itself (MIT or Apache-2.0; `libloading`'s ISC).
- **`ex-status`** (port 8215, shared with `ex-sim-render`), its page and README.

## How to try it

```sh
cargo run --release -p ex-status
```

Then open http://127.0.0.1:8215/. At real time it took 23 s, and printed:

```text
ex-status: at the start, "fault": "stage: in alarm: homing required"; homed, "all well"
  camera_unplugged: "fault", "camera: disconnected"; taken away, "all well"
  stage_alarm: "fault", "stage: in alarm: a limit switch (a fault injected)"; taken away, "all well"
  scratch_full: "fault", "disks: the scratch disk is full (a fault injected)"; taken away, "all well"
  the GPU: "ok", "NVIDIA RTX A2000 12GB, 41 °C, 439 of 12282 MiB"
  open http://127.0.0.1:8215/ to see the indicator, and inject faults
```

## Performance

A sample asks each device once, reads NVML and the disk's statistics, and lists the jobs. It takes a few milliseconds each second.

## Design notes and deviations from the plan

- **The simulated stage starts in alarm,** wanting homing, as grblHAL does. So a new instrument's status is a fault until it is homed, which is true.
- **A full scratch disk is a fault injected,** not a disk filled: the status reads the simulator's flag beside the disk's own state, which a test reaches with a reserve larger than any disk.
- **"Recent write rate" is how fast the scratch disk fills:** its free space falling each second. Deletions offset it, and other programs' writes count; it is what a recording needs.
- **The GPU's thresholds:** degraded at 87 °C, or with its memory 95 % full. NVML missing is offline, not a fault.
- **Jobs:** degraded for ten minutes after a job fails. Running and queued counts are in the message.
- **Only the status Thing's event is new.** Each subsystem's change is in the instrument's, as its `subsystems`.

### What the mutants' reading found

Nineteen mutants, listed in advance from the code:

- **the roll-up:** each weight, the worst, and the first of the worst;
- **each probe:** disconnected, the stage's alarm, the injected disk, the disk's own full and low states, failed jobs, the filling rate;
- **the simulator's faults:** an unplugged camera streaming, answering or starting; an alarm leaving its move waiting, or not stopping the stage; an alarm not cleared; a disk never emptied.

Reading the tests against the list first found gaps, now covered:

- the scratch disk's own full and low states, from a reserve or a low mark larger than any disk;
- an unplugged camera's stream stopped, and its start refused;
- a move in progress ended by the alarm;
- a failed job degrading the jobs.

The run then missed three:

- **Fixed in the test:** the stage not stopping for the alarm. The test now waits, within 5 s, for its reported place to settle. That place lags by the reports in flight, as a controller's do, so a first version that read it twice, 300 ms apart, failed on the right code in 3 of 8 runs on two cores; now 8 of 8 pass.
- **Equivalent through the API:** an unplugged camera's start. The camera Thing asks for its settings before starting it, which refuses first.
- **Not tested, by choice:** the filling rate's sign. Other programs on the disk sway its value, so only its never being negative is checked.

So **17 of the 19 are caught,** one is equivalent, and one is untested by choice.

### The page

The page wasn't seen in a browser here. It reads `instrument`, then keeps up through `EventSource` on `changed`. The script's run, which injects the same faults through the same action, showed each reported.

### Two cores

`scripts/two-core.sh`, five times each, side by side, on two CPUs each:

| Target | Passed | Time |
|---|---|---|
| `oscope-api:lib` | 5 of 5 | 0.8 s |
| `oscope-api:status` | 5 of 5 | 23.7–24.5 s |
| `oscope-api:devices` | 5 of 5 | 18.5–20.9 s |
| `oscope-api:auth` | 5 of 5 | 17.4–20.1 s |
| `oscope-sim:camera` | 5 of 5 | 0.4–0.6 s |
| `oscope-sim:stage` | 5 of 5 | 0.3–0.5 s |
| `ex-status --smoke` | 5 of 5 | 7.4–8.9 s |
| `ex-sim-stage --smoke` | 5 of 5 | 1.5–2.5 s |

Before the settle wait, `oscope-api:status` passed 5 of 8: see the mutants. After it, 8 of 8 passed, and then the 5 above.

## Tests

| Suite | Tests | What they show |
|---|---|---|
| `oscope-api` lib `status` (new) | 1 | The roll-up's rules, each subsystem's weight, and the first of the worst saying why |
| `oscope-api` `status` (new) | 2 | **At the start,** the stage's alarm, a fault, homed to all well; the subsystems in order; the GPU's measures if NVML is there; the disk's. **Unplugged,** the camera stops streaming and won't start: offline, a fault, the event; back, well. **The stage's alarm** ends a move, a fault; cleared, well. **The disk full**, a fault; back, well. **A failed job** degrades the jobs. **The scratch disk's own** full and low states, from its policy |
| `oscope-api` `devices` | 9 | The `simulation` Thing's description, with `inject_fault` |
| `ex-status --smoke` | — | The start's fault, homed to well, each fault reported as it is and taken away, the page |

**The local replays,** the merge gate (ADR-0038), each run alone on `55c8f24`. The commit since adds only these lines and the measurements above.

- **Windows:** `scripts/ci-local.sh`, in 14 minutes: 67 of 67 steps, 607 tests in 229 suites, the Python tests real, and every example's `--smoke`. No CUDA path changed, so the CUDA tests didn't run.
- **Linux:** `scripts/ci-clone.sh` on a fresh WSL clone, in 10 minutes: 67 of 67 steps, 607 tests in 229 suites; 853 files, none with a CR.

## Follow-ups and known gaps

- **A hot GPU** isn't tried: it would need one.
- **An extension's problem** isn't tried here: the extensions' own tests make broken packages.
- **The filling rate** isn't checked for its value, only that it is never negative, as other programs on the disk sway it.

## Clean room

Nothing was copied from `microscope` or any other codebase. NVML is NVIDIA's documented library, through `nvml-wrapper`.
