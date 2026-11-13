# ADR-0015: The sidecar contract, service mode and the desktop fast path

- Status: Accepted
- Date: 2026-11-13
- Phase: P19

## Context

- **D21:** the backend stays a separate process. The desktop app starts and supervises it (a sidecar), or the server runs as a service. The desktop app gets a local fast path: preview frames through shared memory, with no HTTP and no re-encoding.
- **Plan §5.18** sketches the contract:
  - a `--managed` flag;
  - a ready line on stdout, with the port and the local token's path;
  - stopping when stdin closes;
  - exit code 75 to be restarted;
  - a per-boot token in a file only the service's group can read.
- **`teta-wot` v0.2.0 runs a server as a Windows service** (feature `windows-service`): `run_as_service`, `install` and `uninstall`. It reports the server's exit code to the service manager. But its `install` sets no recovery actions, so a service that exits with 75 isn't started again.
- **Only `oscope-api` may depend on `teta-wot`** (ADR-0005).
- **The owner's standing decision for P12–P23:** a change `teta-wot` would need is worked around in OScope and recorded in an ADR, with the upstream change as a follow-up.

## Decision

**1. The sidecar contract,** `oscope-server --managed`:

- **The port** is any free one, unless `--port` gives one.
- **Stdout carries one JSON line when the server is ready,** and nothing else. Logs go to stderr.

  ```json
  {"ready": true, "version": "0.1.0", "port": 53647, "pid": 14316,
   "token": "C:\\…\\data\\run\\local.token", "shm": "preview-14316"}
  ```

  `token` is the local token's absolute path. `shm` is the preview ring's name, or `null` without one.
- **Stdin stays open while the supervisor wants the server.** When it closes, the server stops as it does on Ctrl-C. A supervisor that dies closes it too.
- **Exit codes:**
  - 0: stopped as asked;
  - **75** (`EX_TEMPFAIL`): start me again. The `system` Thing's `restart` action asks for it;
  - 2: a wrong configuration or `[devices]`, which restarting won't fix;
  - 1: any other failure.

**2. The local token.** Each start writes a new one: 32 random bytes from an OS-seeded generator, in hex, in `{data_root}/run/local.token`. It is readable only by the account the server runs as, from the moment it is created:
- **Linux:** mode 0600.
- **Windows:** a protected access list with one entry, full control for that account's SID.

The ready line gives its path. P19 makes the token; the authentication phase (ADR-0031) checks it.

**3. Service mode:**

- **Windows:** `oscope-server service install`, `uninstall` and `run`, through `teta-wot`'s service support, which `oscope-api` re-exports.
  - `install` records `--config`, an absolute `--data-root` and `--port`, because a service starts in `C:\Windows\System32`. The service runs as LocalSystem.
  - **Then OScope sets recovery with `sc.exe`** (the `teta-wot` workaround): `failure … actions= restart/2000`, and `failureflag … 1`, so that a non-zero exit, 75 included, counts. The service manager then starts it again 2 s after it exits with 75.
- **Linux:** systemd runs `oscope-server` as it is, with `packaging/linux/oscope.service`:
  - `Type=exec`;
  - SIGTERM for a graceful stop;
  - `Restart=on-failure` and `RestartForceExitStatus=75`;
  - a state folder;
  - the usual hardening.

  The `service` commands say they are Windows'.

**4. The preview ring,** in `oscope-shm`: one writer, any number of readers, in any processes, without locks.

- **Named shared memory:**
  - **Windows:** a file mapping `Local\oscope-{name}`, in the session's namespace, with the default security;
  - **Linux:** a file `/dev/shm/oscope-{name}`, mode 0600, removed by the writer when it is dropped.
- **The layout,** in little-endian 64-bit words, each read and written atomically:
  - **the header:** `OSRING01`, the version (1), the slots, each slot's bytes, the messages written, and the writer's process ID;
  - **each slot:** its sequence, the number of the message it holds, the message's length, then the message.
- **A seqlock on each slot** (Boehm 2012):
  - **The writer:** the sequence goes odd, a release fence, the message, the sequence goes even with release ordering, then the header's count.
  - **A reader:** it reads the newest slot's sequence (acquire), the message, an acquire fence, then the sequence again. A read whose sequence was odd, or changed, is retried, up to 100 times. A torn message is never returned.
- **The messages** are ADR-0014's frame messages, without blocks.
  - The `preview` Thing writes them, made at its `defaults` (viewport, quality, rate), `look` and `cpu_only`, read before each one.
  - They come latest-wins, never faster than the camera.
- **The configuration's `[shm]`:**
  - it is on by default only when managed;
  - it is named `preview-{pid}` when managed, so two servers don't meet, and `preview` otherwise;
  - it has 3 slots of 8 MiB by default.
- **RGBA or JPEG** for the fast path is M2's to measure (§5.18). P19 sends JPEG, as the WebSocket does.

## Consequences

- **Gains:**
  - One binary runs in a console, under the desktop app, and as a service on either platform.
  - A supervisor learns where the server is without asking it over HTTP. It can stop the server even if the supervisor itself dies, and restart it on request, with no service manager.
  - The desktop app reads the newest preview without HTTP or socket buffers.
  - The contract is tested on both platforms. The Windows service and the systemd unit were each installed, restarted through exit code 75, stopped and removed.
- **Costs:**
  - **A service's token and ring are its own account's.** LocalSystem's or `oscope`'s can't be read by a desktop app running as another user. And `Local\` is per session, so a service's mapping in session 0 is invisible from a desktop session. The installer (W9) will grant a group, and name the mapping in `Global\`, which services may create. Until then the fast path is for managed mode.
  - **An administrator has to take ownership** to delete a LocalSystem service's token. `packaging/windows/service-test.ps1` does.
  - **One preview size for the ring:** the `preview` Thing's defaults.
  - **`install` runs `sc.exe` twice** after `teta-wot`'s own install.
- **To watch:**
  - recovery actions in `teta-wot`'s `install`, upstream;
  - the token's use by authentication (ADR-0031);
  - M2's choice of RGBA or JPEG for the ring.

## Alternatives considered

- **A plain-text ready line:** JSON can gain fields without breaking supervisors.
- **Stopping by a signal:** Windows has no signal a supervisor can send a child process portably. Stdin closing works everywhere, and also when the supervisor dies.
- **The token on the command line or in an environment variable:** other processes can see those.
- **`memmap2`, or the `shared_memory` crate, for the ring:** `libc` and `windows-sys`, already in the lock, do it in a few calls, and `shared_memory` is unmaintained.
- **A pipe or socket for the fast path:** a protocol, a copy, and a buffer that can hold stale frames (ADR-0014's lesson). Shared memory is what D21 chose.
