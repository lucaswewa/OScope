# P52: Settings and configuration migration

- Status: In review
- Branch / PR: `m1/p52-settings` / [#68](https://github.com/lucaswewa/OScope/pull/68)
- Size: M · estimated ~1,550 before it started (the `settings` Thing ~350; the lease's exemption and the builder ~20; the simulator's checks ~30; the file's version, migration, writing and `config migrate` ~260; tests ~460; `ex-settings` ~280; glue ~30; a tenth on top; plan's change log); one pull request, at the owner's standing choice · measured **1,627** hand-written lines added (code 772: `oscope-api` 419, `oscope-server` 301, `oscope-sim` 52; tests 597; examples 230; glue 28), and 58 removed. **That is 1,027 over M's 600, and 5 % over the estimate**

## Summary

One place for the settings, and old configurations upgraded safely ([spec](../phases.md#p52); plan §5.4).

- **The configuration file's version 2:**
  - `[network]` for version 1's `[server]`, and `[simulator]` for its `[devices.simulator]`;
  - a file without `version` is version 1's, and is still read, migrated as it is read;
  - `oscope-server --config FILE config migrate` rewrites it, keeping its comments, and the old file as `FILE.v1.bak`.
- **The `settings` Thing,** in every server:
  - **the system's settings** in the file: the data and scratch roots, the address and the port;
  - **the simulator's,** in the file too;
  - **display preferences:** defaults, and each account's own;
  - **`document`,** all of them at once, which **`import`** takes.
- **Safely:**
  - each change is checked as the next start would check it;
  - only what changed is written, the comments kept;
  - the new file is read back before it replaces the old one, which is kept as `FILE.bak`.
- **`ex-settings`:** a version-1 file migrated, then read, changed, refused, exported and imported.

## What was built

- **`oscope-api::settings`:**
  - `Display`, with `Theme`, `Units` and `Colormap`;
  - `System`, `Saved`, and the `ConfigFile` trait, which the server implements;
  - the `SettingsThing`: `system`, `simulator`, `display_defaults`, `display` (with its reset), `accounts`, `restart_needed`, `document`, and `import`;
  - `ApiBuilder::config_file`; `devices::property_refused`, a property's refusal with its status (ADR-0034's mark);
  - the control lease's one exemption: writing one's own `display`.
- **`oscope-sim`:** `SimulatorConfig::check`, taken out of `Simulator::new`, with the sensor's, the objective's and the sample's names added; `Serialize` on the simulator's settings.
- **`oscope-server`:**
  - `CONFIG_VERSION`, `migrate`, `migrate_file`, and `Config::{devices_config, saved}`;
  - `ConfigError::Version`;
  - `TomlFile`, the `ConfigFile` it serves with;
  - `config migrate`;
  - `config.server` renamed `config.network`.
- **The sample and packaged configurations** are version 2's; the server's README says how versions work.
- **`ex-settings`** (port 8219, shared with `ex-live-overlays`), its page and README.
- **A dependency:** `toml_edit` 0.22.27 (MIT OR Apache-2.0, released 2025-06-06), already in the tree through another crate. It keeps a file's comments and order as it is changed.

## How to try it

```sh
cargo run -p ex-settings
```

Then open http://127.0.0.1:8219/. It prints:

```text
ex-settings: migrated: [server] is now [network]; [devices.simulator] is now [simulator]
ex-settings: open http://127.0.0.1:8219/
  the file's: port 8300, the sample "H&E tissue"
  your display: {"theme":"dark","units":"mm","scale_bar":true,"colormap":"viridis"}
  the next start's: the sample H-DAB tissue, port 8301; a restart needed: true
  a clock rate of 0: 400, the simulator's settings: a clock rate of 0
  imported: ["system","simulator","display_defaults","accounts"]
  the file now:
    version = 2
    # ex-settings' configuration, as P01–P51 wrote them: version 1's.
    …
```

Migrating a file by hand:

```sh
cargo run -p oscope-server -- --config my.toml config migrate
```

## Performance

None is relevant: the file is read when the settings are, and written when they change.

## Design notes and deviations from the plan

- **Version 2's changes:**
  - `[network]` and `[simulator]` match the `settings` Thing's names;
  - the simulator isn't a device, so it has a section of its own.
- **Migrated as it is read:**
  - A version-1 file is migrated in memory; only `config migrate`, or a change through the `settings` Thing, rewrites it.
  - The migration keeps each line where it was, so an error still names its line. The tests that wrote `[server]` before still pass unchanged.
- **Written safely:**
  - Only the keys that changed are written, and the rest of the file is kept, comments and all.
  - The new file is written beside the old one, then read back and compared with what was asked; only then does it replace the old one.
  - The old file is kept as `FILE.bak`, and a version-1 one also as `FILE.v1.bak`, once.
- **The file, not the running server:**
  - `system` and `simulator` are what the next start reads; `restart_needed` says a change is waiting.
  - P51's `power/restart` takes it up.
  - The command line's `--port` and `--data-root` aren't in the file, so they aren't shown.
- **"Defaults"** in the spec's system settings are read as the display defaults.
- **Display preferences:**
  - They are teta-wot settings, kept in the data root's settings folder across restarts.
  - Writing one's own needs no control lease: plan §5.18 keeps a client's own view free of it. The defaults need it.
  - Without accounts, the caller is the local account.
- **`document` is a property, not an `export` action,** so that reading it needs no lease. `import` needs the lease.
- **Refusals:**
  - 400 for what the next start would refuse;
  - 422 from teta-wot for a value that doesn't fit the schema, such as a unit of inches;
  - 409 when the server has no configuration file.
- **An address on the network needs accounts,** as the server has them now: a server without accounts refuses to start there (P49).
- **The simulator's checks** were in `Simulator::new` alone, without a test. `SimulatorConfig::check` now has them, with the sensor's, the objective's and the sample's names, and each has a test.

### What the mutants' reading found

Forty-two mutants, listed in advance from the code:

- **the file's versions:** no version read as 2's; a newer one read; an old file not migrated; version 1's section taken in version 2; `[server]` not renamed; the simulator not moved; version 1's steps skipped;
- **writing the file:** not stamped; the new file unchecked; version 1's file, or the last one, not kept; a file migrated twice; every key written; the port or the address never written; the data root always written; relative roots not from the file's folder; whole numbers written as decimals; the simulator's section not merged; the server's `settings` without its file;
- **the `settings` Thing:** each refusal of the system's settings; the simulator unchecked; no restart needed; nothing written; any format, or a newer document, imported; the display imported before the checks; accounts replaced, not joined; never one's own display; one's own kept as the local account's; a reset that keeps them; the document without accounts; anonymous callers nameless; no system to read;
- **the lease:** one's own display needing it, and all of the settings free of it;
- **the simulator's checks:** `Simulator::new` unchecked; any sensor, objective or sample.

Reading the tests against the list first found gaps, now covered:

- the server handing the `settings` Thing its file;
- a second migration keeping nothing;
- a write touching only the keys that changed, the address among them: the file's lines are counted;
- whole numbers kept whole;
- a write that wouldn't read back refused, and the file kept;
- missing roots;
- an imported part alone, and its accounts joining those there;
- the simulator's checks, which had no test: each refusal, by `check` and by `Simulator::new`.

**All 42 are caught.**

### Two cores

`scripts/two-core.sh`, five times each, side by side, on two CPUs each:

| Target | Passed | Time |
|---|---|---|
| `oscope-api:settings` | 5 of 5 | 2.3–2.4 s |
| `oscope-api:auth` | 5 of 5 | 16.2–17.3 s |
| `oscope-server:config` | 5 of 5 | 0.3–1.1 s |
| `oscope-server:cli` | 5 of 5 | 0.9–2.3 s |
| `oscope-server:sidecar` | 5 of 5 | 0.6–1.1 s |
| `oscope-sim:devices` | 5 of 5 | 0.7–0.9 s |
| `ex-settings --smoke` | 5 of 5 | 0.3–1.5 s |

## Tests

| Suite | Tests | What they show |
|---|---|---|
| `oscope-api` `settings` (new) | 4 | **Display:** each account's own, without the lease; the defaults with it; a value not in the schema refused; kept across a restart; reset. **The file's:** each refusal (roots missing or one folder, the network without accounts, the simulator's sample, clock rate, unknown key), nothing written; good ones written, the rest default, a restart needed; without a file, null, and a 409. **The document:** exported, imported into another server whole; a part alone, its accounts joining those there; a wrong part, nothing changed; another format or a newer version refused. Its description |
| `oscope-api` `auth` | 5 | The sweep of every description lets one's own `display` through without the lease |
| `oscope-server` `config` | 10 | Version-1 fixtures read as version 2's; migrating keeps the comments, the meaning and the old file, once; versions it doesn't read refused; the settings file writes only what changed, keeps the last file, keeps whole numbers whole, and refuses what wouldn't read back. The tests that wrote `[server]` now read version-1 files |
| `oscope-server` `cli` | 11 | `config migrate`, once; the served `settings` has the file's port |
| `oscope-sim` `devices` | 2 | Each of the simulator's refusals, by `check` and by `Simulator::new` |
| `ex-settings --smoke` | — | The migration; display; the next start's changes; the 400; the import; the file; the page |

**The local replays,** the merge gate (ADR-0038), each run alone on `826991c`. The commit since adds only these lines and the measurements above.

- **Windows:** `scripts/ci-local.sh`, in 14 minutes: 69 of 69 steps, 619 tests in 233 suites, the Python tests real, and every example's `--smoke`. No CUDA path changed, so the CUDA tests didn't run.
- **Linux:** `scripts/ci-clone.sh` on a fresh WSL clone, in 9 minutes: 69 of 69 steps, 619 tests in 233 suites; 873 files, none with a CR.

## Follow-ups and known gaps

- **Where the service's file lives (P54):** the packaged service reads `/etc/oscope/oscope.toml`, which it can't write. Until the packaging puts it where the service can, the `settings` Thing's changes fail there, saying why.
- **The other sections,** `[logging]`, `[jobs]`, `[python]`, `[security]` and `[power]`, aren't in the `settings` Thing: the spec names the roots, the network, the display and the simulator.
- **No client shows the display preferences** before M2.

## Clean room

Nothing was copied from `microscope` or any other codebase. The migration is written from TOML's and `toml_edit`'s documented behaviour.
