# ADR-0060: Restarts in a dry run

- Status: Accepted
- Date: 2026-11-20
- Phase: W4.3

## Context

- **W4.3's Power tab restarts the service,** and the app then reconnects by itself, as its spec says. Only a real restart shows that whole: the server stops and exits with code 75, its supervisor starts it again (ADR-0015), and the app finds it.
- **ADR-0032's dry run covers all three actions at once,** and the tests always use it. A test that rebooted or shut down for real would do it to the computer the tests run on.
- **A restart harms nothing a test needs:** only the test's own server stops, and the test can start it again, on the same port, as a supervisor does.

## Decision

- **The configuration's `[power] dry_run_restart`** says whether restarting the service is a dry run, apart from rebooting and shutting down, which `dry_run` says alone. Not given, it is `dry_run`'s: every configuration written before means what it did.
- **The `power` Thing says both,** as its properties `dry_run` and `dry_run_restart`. Each action's answer says whether it was a dry run, as before.
- **`ApiBuilder::power_with(dry_run, dry_run_restart)`** sets them apart. `power(dry_run)` sets both, as before.
- **The app's end-to-end tests** run their servers with `dry_run = true`. Only the restart's tests also set `dry_run_restart = false`. Their harness starts the server again when it exits with code 75, on the same port, with the same folder.

## Consequences

- **Gains:**
  - The Power tab's restart is tested whole: the confirmation, the server gone, the app reconnecting, and back on its tab.
  - Rebooting and shutting down stay dry runs in every test, whatever a restart does.
- **Costs:** one more key in the configuration, and one more property.
- **To watch:** a server with `dry_run = true` and `dry_run_restart = false` really restarts. Run by hand, without a supervisor, it just stops (ADR-0015).

## Alternatives considered

- **A server with `dry_run = false` for the restart's test alone:** a mistake in the app or in the test could then reboot the computer the tests run on.
- **The `system` Thing's `restart`,** which ADR-0015's supervisors use and no dry run covers. The test would then restart the server with another action than the one the Power tab offers, without the `power` Thing's safeguards: the confirmation, and no hardware job running.
- **A restart that is never a dry run:** a server run by hand from a checkout would stop for good the first time its restart was tried.
