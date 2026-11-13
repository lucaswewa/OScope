# P19: Sidecar contract, service mode, shared-memory preview

- Status: In review
- Branch / PR: `m1/p19-sidecar-service-shm` / [#32](https://github.com/lucaswewa/OScope/pull/32)
- Size: M · estimated ~2,300 before it started (shared memory, the preview ring and the private token file ~450; the server's managed and service modes ~250; the restart action, the service and the ring's publisher in `oscope-api` ~170; packaging and its test scripts ~185; tests ~500; `ex-sidecar` and `ex-shm-reader` ~330; and about a third on top, from P18's measured density); one pull request, at the owner's standing choice for P12–P23 · measured **2,500** hand-written lines added (code 1,458; tests 405; examples 423; packaging and its scripts 214), and 78 removed. Docs and `Cargo.lock` aren't counted. **That is 1,900 over M's 600, and about 200 over the estimate.**

## Summary

The server runs as a service, is supervised by the desktop app, and feeds it frames without HTTP ([spec](../phases.md#p19), D21, plan §5.18).

- **The sidecar contract** (`oscope-server --managed`):
  - any free port;
  - one JSON ready line, with the port, the process ID, the local token's path and the preview ring's name;
  - closing stdin stops it;
  - exit code 75 asks to be started again, which the `system` Thing's new `restart` action does.
- **The local token:** a new one each start, readable by the server's own account alone (mode 0600; a one-entry access list on Windows).
- **Service mode:**
  - **Windows:** `oscope-server service install`, `uninstall` and `run`, through `teta-wot`'s service support. OScope sets the service manager's recovery, so that exit code 75 restarts it.
  - **Linux:** a systemd unit, `packaging/linux/oscope.service`.
  - Both were installed, restarted through exit code 75, stopped and removed, on this PC and under WSL.
- **The shared-memory preview ring** (`oscope-shm`): a header and slots, each a seqlock over one preview, in a Windows named file mapping or `/dev/shm`. The `preview` Thing publishes ADR-0014's messages to it, at its defaults.
- **`ex-sidecar`,** a small supervisor that restarts the server after exit code 75; and **`ex-shm-reader`,** a second process that reads the ring and reports its rate and torn reads.
- **ADR-0015** records all of it.

## What was built

| Where | What |
|---|---|
| `oscope-shm` `map.rs` | Named shared memory: `CreateFileMappingW` and `MapViewOfFile` in `Local\oscope-{name}` on Windows; a 0600 file in `/dev/shm`, `mmap`ed, removed by its creator, on Linux. Names of 1–64 letters, digits, `-` and `_` |
| `oscope-shm` `ring.rs` | `RingWriter` and `RingReader`: the header (`OSRING01`, version, slots, slot bytes, messages written, the writer's PID) and slots (sequence, number, length, message), every word atomic; the seqlock of Boehm (2012); `Read::{New, Same}`, `torn()`, `ShmError::Overtaken` after 100 tries |
| `oscope-shm` `private.rs` | `write_private`: a file only its owner can read, private from creation: `mode(0o600)`, or `CreateFileW` with a protected DACL granting the process's account (its SID, from its token) full control |
| `oscope-api` | `Restart`, the server's restart request; the `system` Thing's `restart` action; `ApiBuilder::ring`, which hands the ring to the `preview` Thing; `oscope_api::service`, `teta-wot`'s Windows service support re-exported, with its `windows-service` feature on Windows |
| `oscope-acquire` `preview/ring.rs` | `publish`: the bus's newest frames, made into previews as `RingSettings` say, read before each one, written to the ring. `Header::of`, shared with the WebSocket |
| `oscope-server` | `--managed` and its ready line; stopping when stdin closes; `Stopped::{Asked, Restart}` and exit code 75; the token; `[shm]` (`enabled`, `name`, `slots`, `slot_bytes`); `service install`, `uninstall`, `run`, with `sc.exe` setting recovery |
| `packaging/` | `linux/oscope.service` and `oscope.toml`; `linux/test-user-unit.sh`; `windows/service-test.ps1`; a README |
| `examples/common` `sidecar.rs` | `serve_if_asked` (an example runs itself as `oscope-server`) and `Sidecar` (start, ready line, exited, stop by stdin) |
| `examples/ex-sidecar`, `ex-shm-reader` | The two programs the spec asks for |
| `examples/ex-live-overlays` | x and y jogs beside z, for Checkpoint A |

## How to try it

```sh
cargo run -p ex-sidecar        # then, from another terminal, the curl it prints, to restart it
cargo run -p ex-shm-reader     # 10 s of reading the ring: the rate, and any torn reads
```

The [server's README](../../../crates/oscope-server/README.md#the-sidecar-contract) describes the contract and service mode. [`packaging/README.md`](../../../packaging/README.md) says how to install the service on either platform, and how to try it.

### Checkpoint A: the demo

The live microscope, end to end, on the simulator:

1. **Start the server and open the page:** `cargo run -p ex-live-overlays`, then `http://127.0.0.1:8219/`. The example homes the simulated stage, places it over the H&E slide's tissue, lights it and starts streaming.
2. **Jog the stage:** hold x + on the page with the speed at 300 µm/s. The tissue slides left, as the camera's mounting says (P17 measured it). y + slides it up; z ± goes out of focus, and the focus value under the image falls.
3. **Zoom to a full-resolution ROI:** scroll in over a nucleus until the status line says the preview is the size of the region it shows: `120 × 80 of 120 × 80`, sensor pixel for screen pixel.
4. **Draw an ROI and a profile:** drag across the nucleus; shift-drag a line through it. The ROI's means and the profile come from the raw 12-bit frame.
5. **Change the exposure:** move the light to the right, then set the exposure to 25 ms. The histograms pile up at 4095, the saturated shares rise, and pink marks the saturated pixels. Back at 15 ms they clear.
6. **Watch the rates:** the status line gives the frames a second and the server's latency; the simulated camera gives about 7 a second at full resolution in 16 bits.

## Performance

§5.20 has no budget for P19. What was measured:

- **The ring, read by another process** (`ex-shm-reader`): every preview the fake camera's 16 frames a second gave, 0 torn reads.
- **The ring under stress** (`the_ring_stays_consistent_while_it_is_read_and_written_at_once`): a writer with no pause, two slots, two readers, in a debug build, for 2 s. Each reader read about 900 messages whole, and detected and read again about 195,000 torn reads; about 1,700 reads were overtaken 100 times in a row and given up. None was returned torn.
- **Stopping:** closing stdin stops a managed server in about 10 ms.

## Design notes and deviations from the plan

**Worth your review:**

- **Recovery for the Windows service is OScope's, through `sc.exe`** (ADR-0015). `teta-wot`'s `install` registers the service but sets no recovery actions, so a service exiting with 75 would just stay stopped. After installing, OScope sets two things: a restart 2 s after a failure, and the "non-crash failures" flag, so that a non-zero exit code counts. That follows your standing decision: a workaround, recorded, with the upstream change as a follow-up.
- **A service's token and ring are its own account's.** As LocalSystem, the token is readable by LocalSystem alone, and the test script has to take the folder over to delete it. And the ring's `Local\` name is per session. So a desktop app running as a user can't use a service's fast path yet. The installers (W9) will grant a group and use `Global\`. Until then the fast path is for managed mode, where the desktop app is the server's own user.
- **The token isn't checked yet:** P19 makes it and reports its path. Authentication (ADR-0031) checks it.
- **`system` gains a `restart` action.** The plan doesn't list it; the contract's exit code 75 needs something to ask for it. In a console it just stops the server, with exit code 75.

**Other choices:**

- **Each example runs itself as the server,** through `oscope-server`'s library, so CI's smoke runs build nothing more.
- **On Linux there are no service commands:** systemd runs the server as it is, and `service …` says so.
- **The ring's previews are made at the `preview` Thing's defaults,** followed live through `Prop::subscribe`: one size for the desktop app. M2 decides between RGBA and JPEG for it.
- **`libc` and `windows-sys`, already in the lock,** map the memory; no new crate. `windows-service` 0.8.1, MIT or Apache-2.0, comes in through `teta-wot`'s feature, on Windows only.

**Found before CI:**

- **A crash on Linux:** the `/dev/shm` mapping was built with struct-update syntax from a temporary `Mapping`, whose `Drop` then unmapped it. The first Linux run of the ring's tests crashed (SIGSEGV). Fixed by setting the one value's path.
- **The stress test's first design** let a writer with no pause overtake every read: a livelock of the test's making, reported as `Overtaken`. The readers now count those, and the writer yields after each write.
- **A missed mutation:** skipping the reader's "is this slot being written?" check went unnoticed. A reader only meets a half-written slot after the writer laps it, and the writer then always finishes first. A unit test now freezes the writer mid-write, and checks the slot is refused until it's finished.
- **The first Windows service test left the token behind,** unreadable to the administrator running it. The script now takes the folder over before deleting it. Its second run left nothing.
- **The first run of the contract mutations stalled.** With "stdin closing ignored", the test's servers never exited. They held the mutation script's output pipes until its 20-minute timeout, so it recorded nothing. The servers were stopped by PID. The contract tests now kill their server when a test ends early, and the mutations ran again, all caught, with nothing left running.

## Tests

| Crate | Test | What it checks |
|---|---|---|
| `oscope-shm` | `shm` (5) | Messages of 0 to 4096 bytes read back whole; a reader sees only newer ones, and a late or lagging reader gets the newest. A slot takes whole words. Refused: too large, a name in use, names that aren't names, rings too small, a ring nobody made. The ring goes with its writer, and the name can be used again. The ring stays consistent while two readers and an unpaced writer race over two slots. The private file is its owner's alone: mode 0600, or one ACE for this account, nothing inherited |
| `oscope-shm` | `a_slot_being_written_is_never_read` (unit) | A slot frozen mid-write is refused, 100 tries, until its sequence is even again |
| `oscope-server` | `sidecar` (3) | With the real binary, managed, and the fakes. The ready line: version, port, PID, an absolute token path in the data root, 64 hex digits, private, and the ring named for the process and written by it. Closing stdin: exit code 0, and the ring gone. `restart`: exit code 75; started again, a new process with a new token. The ring: previews of the camera's frames, ADR-0014 frames without blocks, numbered in order |
| packaging | `test-user-unit.sh`, under WSL | systemd: started, restarted by exit code 75 (one restart, a new process), stopped with exit code 0 and `Result=success`, removed |
| packaging | `service-test.ps1`, as an administrator | The service `oscope-test`: installed, started, restarted by the service manager after exit code 75, stopped with exit code 0, removed with its folder. Run twice, the second time after the cleanup was fixed |
| `ex-sidecar` | `--smoke` | Ready; restarted with 75 as a new process; stopped by stdin with 0 |
| `ex-shm-reader` | `--smoke` | At least 3 previews in 2 s from its own server's ring, each a JPEG with a good header |

The contract and ring tests ran on Windows and on Linux (WSL, its own target folder).

**Mutation checks** (each broke the code on purpose, then the test named was run):

| Broken on purpose | Caught by |
|---|---|
| The reader doesn't check the sequence again | `the_ring_stays_consistent_while_it_is_read_and_written_at_once` |
| The reader reads a slot being written | **missed at first** by the race test; `a_slot_being_written_is_never_read`, added |
| The writer doesn't mark the slot it writes | `the_ring_stays_consistent_while_it_is_read_and_written_at_once` |
| Stdin closing ignored | `sidecar`'s three tests, each waiting for an exit that never came |
| A restart exits with 0 | `a_restart_exits_with_75_and_the_next_server_has_a_new_token` |
| The token written for anyone | `managed_it_says_where_it_is_and_stops_when_stdin_closes`, with its Windows ACL check, added for parity with Linux's mode check |
| The ready line's PID wrong | `managed_it_says_where_it_is_and_stops_when_stdin_closes` |

## Follow-ups and known gaps

- **`teta-wot`:** recovery actions in `service::install`, upstream.
- **The desktop app and a service** (W9): a group for the token and the ring, and the ring in `Global\`.
- **Authentication** (ADR-0031) checks the token.
- **M2:** RGBA or JPEG through the ring, measured.
- **Packages:** `.deb`, MSI and NSIS, with the desktop app (W9).

## Clean room

Nothing was copied from `microscope` or any other codebase.

- **The seqlock** follows Boehm, H.-J. (2012), "Can seqlocks get along with programming language memory models?", *Proceedings of the 2012 ACM SIGPLAN Workshop on Memory Systems Performance and Correctness*, 12–20: the writer's odd sequence, release fence and release store, and the reader's acquire load, acquire fence and second load.
- **Named shared memory, access lists and services** are the platforms' documented APIs: `CreateFileMappingW`, `ConvertStringSecurityDescriptorToSecurityDescriptorW` with SDDL, `sc.exe failure` and `failureflag`; `mmap`; systemd's `Restart=` and `RestartForceExitStatus=`.
- **Exit code 75** is `EX_TEMPFAIL` from BSD's `sysexits.h`, as plan §5.18 has it.
