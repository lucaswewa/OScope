# P51: Power actions

- Status: In review
- Branch / PR: `m1/p51-power` / [#67](https://github.com/lucaswewa/OScope/pull/67)
- Size: S · not estimated in lines before it started, an oversight; one pull request, at the owner's standing choice · measured **738** hand-written lines added (code 249: `oscope-api` 235, `oscope-server` 14; tests 228; examples 239; glue 22), and 3 removed. **That is 438 over S's 300.** The tests and the example are nearly two thirds of it: each safeguard has its test, and the example walks them all

## Summary

Restarting the service, and rebooting or shutting down the host, safely ([spec](../phases.md#p51); plan §5.19; [ADR-0032](../../adr/0032-power-actions-and-their-privileges.md)).

- **The `power` Thing:** `restart`, `reboot` and `shutdown`, and the event `powering`.
- **The commands:**
  - `restart` exits with code 75 for the supervisor (ADR-0015);
  - `reboot` and `shutdown` run `shutdown.exe /r` or `/s` on Windows, and `systemctl reboot` or `poweroff` on Linux, which the packaging's polkit rule allows (P54).
- **The safeguards:**
  - the control lease, when the server has accounts (P49);
  - `confirm: true`, or a 400;
  - no hardware job running, or a 409 naming it, unless `force`.
- **A dry run:** `ApiBuilder::power(true)`, or the server's `[power] dry_run`. It says what each action would run, and does nothing.
- **`ex-power`:** the safeguards and the commands, always in a dry run.

## What was built

- **`oscope-api::power`:**
  - `Action` and `Platform`;
  - `command(action, platform)`, a pure function, so each platform's commands are tested on either;
  - `Powered`;
  - the `PowerThing`, with `dry_run`, `restart`, `reboot` and `shutdown`, and the event `powering`;
  - `ApiBuilder::power(dry_run)`, and `ApiBuilder::restart`, the server's restart request, as `Api::restart` gives it.
- **`oscope-server`:** `[power] dry_run`, false by default. Its served API and its printed documents have the Thing; the printed ones in a dry run.
- **`ex-power`** (port 8214, shared with `ex-demosaic` and `ex-filters`), with a job of its own that holds the instrument until cancelled, as a scan would; its page and README.
- **Tests:** `oscope-api`'s `power` (new) and `auth`; `oscope-server`'s `config`, `cli` and `sidecar`.

## How to try it

```sh
cargo run -p ex-power
```

Then open http://127.0.0.1:8214/. On Windows it prints:

```text
ex-power: a dry run (true); rebooting unconfirmed: refused (400): confirm it, with confirm: true, to reboot it
  shutting down while a job holds the instrument: conflict (409): example.hold is running on the instrument: let it end, cancel it, or force it
  forced: shutdown.exe /s /t 0
  restart: (the server itself)
  reboot: shutdown.exe /r /t 0
  shutdown: shutdown.exe /s /t 0
  open http://127.0.0.1:8214/ to try them
```

## Performance

None is relevant: a power action is one request, and the hardware jobs it checks are one query.

## Design notes and deviations from the plan

- **Never real here:**
  - Every test and the example run in a dry run. A power action was never really run on the owner's PC, nor a server's restart through the `power` Thing.
  - **Guards:**
    - The test's first confirmed action is the restart, and it checks at once that nothing asked the server to restart. Had the dry run been broken, the test stops there, before it asks for a reboot.
    - `ex-power` does the same, through `ApiBuilder::restart`, and checks `dry_run` before any action. Every replay runs it, beside the tests.
  - **The one test whose server acts,** `sidecar`, with the defaults, only reads its `dry_run`, and invokes nothing.
- **`system/restart` stays,** as ADR-0015's sidecar contract has it, for the desktop app. The `power` Thing's restart adds the safeguards.
- **The `powering` event** comes before the action, so clients can warn their users.
- **Compute jobs don't stop a power action,** only hardware jobs: a reboot loses a compute job's work, but no sample is left mid-scan.

### What the mutants' reading found

Eighteen mutants, listed in advance from the code:

- **the safeguards:** unconfirmed going; only compute jobs, or any job, refusing; an ended job refusing; the force ignored, or always on; the 409 naming no job; power needing no lease;
- **what is said:** no event; each platform's commands; the platform; the restart's action; the dry run's property;
- **the configuration:** its dry run ignored by the server, or always on;
- **two synthetic ones, for the guards:** a dry run that asks the server to restart, in the test and in `ex-power`.

Reading the tests against the list first found gaps, now covered:

- a running compute job not refusing, and a hardware job that has ended no longer refusing;
- the lease: `auth`'s sweep of every Thing Description now serves the `power` Thing;
- the configuration's `dry_run` reaching the served API, either way;
- the guards.

It also found a race in `ex-power`. A cancelled job ends at its next checkpoint, after `jobs/cancel` answers, so the script's next action could find it still running. The script and the test now wait for the job to end.

**All 18 are caught.** The guards' mutants stop with "a dry run acted", before a reboot or a shutdown is asked for.

**Read, not run:** the mutants that take the dry run away. Each could run `shutdown.exe` on the owner's PC:

- the early return dropped or inverted: what the guards stop;
- `connect` reading the dry run as false, or a test's `power(true)` made false: the test and the example check `dry_run` first, before any action.

### The page

The page wasn't seen in a browser here. It asks the browser's `confirm` before each action, sends `confirm: true` and the box's `force`, and shows the command it would run or why it was refused. The smoke run checks that the page and its script are served.

### Two cores

`scripts/two-core.sh`, five times each, side by side, on two CPUs each:

| Target | Passed | Time |
|---|---|---|
| `oscope-api:power` | 5 of 5 | 2.0–2.2 s |
| `oscope-api:lib` | 5 of 5 | 0.2–0.9 s |
| `oscope-api:auth` | 5 of 5 | 19.0–20.2 s |
| `oscope-server:cli` | 5 of 5 | 0.9–2.3 s |
| `oscope-server:config` | 5 of 5 | 0.2–1.1 s |
| `oscope-server:sidecar` | 5 of 5 | 0.6–1.2 s |
| `ex-power --smoke` | 5 of 5 | 0.3–6.1 s |

## Tests

| Suite | Tests | What they show |
|---|---|---|
| `oscope-api` lib `power` (new) | 1 | Each platform's commands, and this platform's |
| `oscope-api` `power` (new) | 1 | A dry run: refused without a confirmation; each action's command on this platform, and its event; nothing done: no restart asked for, checked before any reboot, and the server still serves. A compute job not refusing; a hardware job refusing a shutdown, naming it, forced past, and once ended, no longer refusing |
| `oscope-api` `auth` | 5 | Its sweep of every Thing Description now has the `power` Thing: each action refuses a caller without the lease |
| `oscope-server` `config` | 6 | `[power] dry_run`, false by default, and set by a file |
| `oscope-server` `cli` | 10 | A server whose file says `dry_run = true` serves a dry run |
| `oscope-server` `sidecar` | 3 | A server with the defaults acts: only read |
| `ex-power --smoke` | — | The dry run, checked first; the 400; the restart, done first, asking for nothing; the 409 naming the job, and forced; each action a dry run; the page |

**The local replays,** the merge gate (ADR-0038), each run alone on `4428d1f`. The commit since adds only these lines and the measurements above.

- **Windows:** `scripts/ci-local.sh`, in 7 minutes: 68 of 68 steps, 609 tests in 231 suites, the Python tests real, and every example's `--smoke`. No CUDA path changed, so the CUDA tests didn't run.
  - **Its first run failed, outside the code.** The build output had just been cleaned, and the cold examples build ran this PC out of memory: eleven `rustc` processes aborted with "memory allocation … failed". The rerun passed.
- **Linux:** `scripts/ci-clone.sh` on a fresh WSL clone, in 13 minutes: 68 of 68 steps, 609 tests in 231 suites; 862 files, none with a CR.

## Follow-ups and known gaps

- **The polkit rule** comes with the packaging (P54), and its test with it.
- **A cold Windows replay can run out of memory** in its examples build, which links some 70 debug binaries at once on this PC's 24 threads: `scripts/ci-local.sh` could cap that build's jobs.
- **What acts is untested here:** the `power` Thing's restart of a server that acts, and the commands really run. They go in P44–P54's manual checklist, for a machine that may be rebooted.

## Clean room

Nothing was copied from `microscope` or any other codebase. The commands are Windows' and systemd's documented ones.
