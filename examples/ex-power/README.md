# ex-power

P51's power actions ([ADR-0032](../../docs/adr/0032-power-actions-and-their-privileges.md)), always in a dry run: each says what it would run, and does nothing.

1. **The `power` Thing:**
   - `restart` stops the server, which exits with code 75 for its supervisor to start it again;
   - `reboot` and `shutdown` run `shutdown.exe /r` or `/s` on Windows, and `systemctl reboot` or `poweroff` on Linux.
2. **The safeguards,** on each action:
   - `confirm: true`, or a 400;
   - no hardware job running, or a 409, unless `force: true`;
   - the control lease, when the server has accounts (P49).
3. **The script:**
   - Rebooting unconfirmed is refused.
   - A job holding the instrument refuses shutting down, until forced.
   - Then each action says what it would run here.
4. **The page** asks you to confirm each action, and says what it would run. Its box forces past a job.

## How to try it

```sh
cargo run -p ex-power
```

It serves at http://127.0.0.1:8214/, and on Windows prints:

```text
ex-power: a dry run (true); rebooting unconfirmed: refused (400): confirm it, with confirm: true, to reboot it
  shutting down while a job holds the instrument: conflict (409): example.hold is running on the instrument: let it end, cancel it, or force it
  forced: shutdown.exe /s /t 0
  restart: (the server itself)
  reboot: shutdown.exe /r /t 0
  shutdown: shutdown.exe /s /t 0
  open http://127.0.0.1:8214/ to try them
```

On Linux, the reboot and shutdown lines say `systemctl reboot` and `systemctl poweroff`.

**With the server,** power is real unless the configuration says otherwise:

```toml
[power]
dry_run = true
```

On Linux the packaging's polkit rule lets the service's account reboot and power off (P54).

## Its checks

The script goes on only in a dry run. It checks that before any action, and again after the restart, which it does first: had the dry run acted, the server would have been asked to restart, and the script stops there, before a shutdown.

`cargo run -p ex-power -- --smoke` runs the script, then checks that:
- rebooting unconfirmed is refused with a 400;
- shutting down while a job holds the instrument is refused, naming the job, and forced it goes;
- each action, confirmed, was a dry run;
- the page and its script are served.
