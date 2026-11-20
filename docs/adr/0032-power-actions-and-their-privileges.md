# ADR-0032: Power actions and their privileges

- Status: Accepted; extended by [ADR-0060](0060-restarts-in-a-dry-run.md)
- Date: 2026-11-16
- Phase: P51

## Context

- **Plan §5.19 wants power actions:** restarting the service, and rebooting or shutting down the host, from the app.
- **They are the most disruptive actions OScope offers.** A reboot mid-scan loses the scan, and a shutdown leaves the instrument unreachable until someone walks to it.
- **ADR-0015's exit code 75** already asks a supervisor to start the server again. The desktop app, systemd and Windows' service manager all honour it.
- **The service's account** must be allowed to reboot:
  - On Windows, LocalSystem, which the service runs as, has the shutdown privilege, and so does a user at a workstation.
  - On Linux, a system service needs a polkit rule for logind's reboot and power-off.
- **The tests run on the owner's PC,** which must never actually reboot.

## Decision

**1. The `power` Thing** has three actions: `restart`, `reboot` and `shutdown`. Each answers what it did, or in a dry run would do: the action, its command, and whether it was a dry run.

**2. The commands:**

| Action | Windows | Linux |
|---|---|---|
| `restart` | the server stops, and exits with code 75 (ADR-0015) | the same |
| `reboot` | `shutdown.exe /r /t 0` | `systemctl reboot` |
| `shutdown` | `shutdown.exe /s /t 0` | `systemctl poweroff` |

They are built by `power::command(action, platform)`, so the tests check both platforms' on either.

**3. The safeguards,** checked in this order:
- **The control lease,** when the server has accounts (ADR-0031): every action needs it.
- **A confirmation:** `confirm: true`, or a 400.
- **No hardware job running,** or a 409 naming it, unless `force: true`. Compute jobs don't stop it.

**4. The event `powering`** says what is about to be done, before it is, so clients can warn their users. It is logged at warning level too.

**5. A dry run,** `ApiBuilder::power(true)` or the server's `[power] dry_run = true`, does everything but act. The examples and tests always use it. The server acts by default.

**6. Linux's privilege:** the packaging (P54, ADR-0033) installs a polkit rule that lets the service's account use `org.freedesktop.login1.reboot` and `power-off`, and nothing else.

## Consequences

- **Gains:**
  - The app can restart the service after a settings change or an update, without anyone at the console.
  - One request can't reboot the host by accident: it takes the lease, a confirmation, and no hardware job, or a deliberate force.
- **Costs:**
  - **A forced power action mid-job** loses that job. The force is the user's choice, logged.
  - **The commands are platform-specific,** in a function of their own, with both tested on either platform.
- **To watch:** `systemctl` without the polkit rule fails with an authentication error, which the action reports. The packaging's tests (P54) check the rule is there.

## Alternatives considered

- **logind's D-Bus interface directly** on Linux: the same polkit check, with D-Bus code to keep. `systemctl` is what the plan names.
- **Windows' `ExitWindowsEx`** through FFI: unsafe code in `oscope-api`, which ADR-0005 forbids, to save starting `shutdown.exe`.
- **No dry run:** the tests couldn't check the commands without rebooting the PC they run on.
