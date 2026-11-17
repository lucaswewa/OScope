# P51: Power actions

- Status: In review
- Branch / PR: `m1/p51-power` / [#67](https://github.com/lucaswewa/OScope/pull/67)
- Size: S · not estimated in lines before it started, an oversight; one pull request, at the owner's standing choice · measured SIZE

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
  - **The server reads only `dry_run`** in the one test that serves one that acts (`sidecar`), and invokes nothing.
- **`system/restart` stays,** as ADR-0015's sidecar contract has it, for the desktop app. The `power` Thing's restart adds the safeguards.
- **The `powering` event** comes before the action, so clients can warn their users.
- **Compute jobs don't stop a power action,** only hardware jobs: a reboot loses a compute job's work, but no sample is left mid-scan.

### What the mutants' reading found

MUTANTS

### The page

The page wasn't seen in a browser here. It asks the browser's `confirm` before each action, sends `confirm: true` and the box's `force`, and shows the command it would run or why it was refused. The smoke run checks that the page and its script are served.

### Two cores

TWOCORE

## Tests

| Suite | Tests | What they show |
|---|---|---|
| `oscope-api` lib `power` (new) | 1 | Each platform's commands, and this platform's |
| `oscope-api` `power` (new) | 1 | A dry run: refused without a confirmation; each action's command on this platform, and its event; nothing done: no restart asked for, checked before any reboot, and the server still serves. A compute job not refusing; a hardware job refusing a shutdown, naming it, forced past, and once ended, no longer refusing |
| `oscope-api` `auth` | 5 | Its sweep of every Thing Description now has the `power` Thing: each action refuses a caller without the lease |
| `oscope-server` `config` | 6 | `[power] dry_run`, false by default, and set by a file |
| `oscope-server` `cli` | 10 | A server whose file says `dry_run = true` serves a dry run |
| `oscope-server` `sidecar` | 3 | A server with the defaults acts: only read |
| `ex-power --smoke` | — | The dry run; the 400; the 409 naming the job, and forced; each action a dry run; the page |

REPLAYS

## Follow-ups and known gaps

- **The polkit rule** comes with the packaging (P54), and its test with it.
- **What acts is untested here:** the `power` Thing's restart of a server that acts, and the commands really run. They go in P44–P54's manual checklist, for a machine that may be rebooted.

## Clean room

Nothing was copied from `microscope` or any other codebase. The commands are Windows' and systemd's documented ones.
