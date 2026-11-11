# U04: Binary WebSocket streams

- Status: Done
- Branch / PR: `u/u04-binary-streams` in `teta-wot` / [lucaswewa/teta-wot#4](https://github.com/lucaswewa/teta-wot/pull/4)
- Size: planned L, estimated ~620 · measured **733** hand-written changed lines (+730 −3). Docs and `Cargo.lock` aren't counted.

## Summary

`teta-wot` Things can now stream binary messages over WebSockets, with each client getting its own stream ([spec](../phases.md#u04)). This is the transport OScope's live preview will use (D6, plan §5.5).

- **The producer.** A `#[binary_stream]` method, or `Endpoint::binary_stream`, runs once for each connection with a `Subscriber<P>`.
- **Parameters.** `P` is the client's parameters, sent as JSON and validated by deserialising them.
- **A one-slot sink:** the newest message wins, and a slow client never blocks the producer.
- **Counters** of messages sent and dropped.
- **The end.** Streams end on the client's close or the server's shutdown.
- **The TD** links each stream with its subprotocol and parameter schema.

## What was built

All of it is in `teta-wot`.

| Change | Where |
|---|---|
| **`Subscriber<P>`:**<ul><li>`params()` and `changed()` (`false` once the connection has ended);</li><li>`send()`, which never waits, replaces an unsent message and counts it as dropped;</li><li>`sent()`, `dropped()`, `is_closed()`, `closed()`.</li></ul> | `crates/teta-wot-http/src/binary.rs` |
| **`Endpoint::binary_stream(path, producer)`:** a `GET` endpoint that upgrades to a WebSocket. For each connection it creates a `Subscriber`, spawns the producer, and runs the pump.<ul><li>**Reading and writing are concurrent** (the socket is split). Parameter errors are answered through a small channel to the writer.</li><li>**The pump ends** on the client's close or the `ShutdownToken`, then closes the socket within 1 s.</li><li>**The producer** has 1 s to return before it is aborted.</li></ul> | same |
| **`#[binary_stream]` / `#[binary_stream("path")]`:** a method role. The method takes `&self` and one `Subscriber<P>` and returns `()`; the macro generates the `Endpoint::binary_stream` call | `crates/teta-wot-macros/src/thing_impl.rs`, `lib.rs` |
| **The TD link.** `EndpointSpec::link_member`, plus the new `EndpointLink::members`, adds members to an endpoint's TD link. A binary stream's link has `rel` `alternate`, `type` `application/octet-stream`, `subprotocol` `websocket`, and `parameters`: `P`'s schema, converted as the TD converts actions' inputs | `crates/teta-wot-core/src/endpoint.rs`, `runtime.rs` |
| **Exports:** `teta_wot::http::{Subscriber, BINARY_STREAM_MEDIA_TYPE}` | `crates/teta-wot/src/lib.rs` |
| **The example `binary-stream`.** The benchmark: a simulated camera, four clients at 1 MB × 60 Hz, a slow client, bad parameters | `examples/binary-stream/` |
| **Docs:** a guide section, *Binary streams* (the chapter is now *Blobs, MJPEG and binary streams*), and the CHANGELOG | `docs/guide/src/blobs.md` |

## How to try it

```sh
cargo run --release -p binary-stream                  # 3 s; --seconds N to run longer
```

## Performance (the benchmark)

Four clients at 1 MB per frame and 60 Hz, about 250 MB/s in all, plus one slow client, over loopback:

| Build | Each fast client | Server, per fast client | Slow client | Server, slow client |
|---|---|---|---|---|
| Windows, release | 180 frames in 3 s: 60.0 frames/s, 62.9 MB/s | 181 sent, 0 dropped | 15 frames: 5.0 frames/s | 17 sent, 164 dropped |
| Windows, debug | 60.0 frames/s, 62.9 MB/s | — | 5.0 frames/s | — |
| Ubuntu (WSL), debug, in the CI replay | 60.0 frames/s, 62.9 MB/s | 180 sent, 0 dropped | 15 frames: 5.0 frames/s | 19 sent, 162 dropped |

The target, 1 MB at 60 Hz to four clients, is met with no drops, even in a debug build. Only the slow client loses frames, and it loses all but the newest.

## Design notes and deviations from the plan

- **A binary stream is a kind of custom endpoint, not a new kind of affordance.** That gives it, for free:
  - routing and path parameters (U02);
  - credentials, CORS and the shutdown token (U03);
  - a place in OpenAPI.

  The cost is that the TD describes it as a link, as it does MJPEG streams, rather than as an affordance with forms.
- **The TD link carries `subprotocol` and `parameters`,** which aren't standard link terms. The TD still validates against the W3C TD 1.1 schema; a test checks that. If `teta-wot` later gets a vocabulary of its own, the terms could move into it.
- **Validation is deserialisation:** a message must deserialise into `P`. Each message replaces the parameters; `#[serde(default)]` lets clients send only what they change.
- **Reading and writing were split after a hang.** While building the benchmark, the example hung twice:
  - **The cause was the example's slow client.** `tokio::time::timeout_at` returns a message that is already waiting even after the deadline, and the slow client always had one waiting, so its loop never ended. The loop now checks the time too.
  - **The pump was split anyway.** Investigating it showed a real weakness: the first pump didn't read while a send was blocked, so a client that stopped reading and then closed wasn't heard until its socket buffers drained, which might be never.
  - **Not tested.** This case isn't covered by a test, because it depends on socket buffer sizes.
- **The connection's buffers come before the sink.** A slow client's messages are held in the TCP and WebSocket buffers first, and only when those are full does the sink start replacing messages. That is the right behaviour, and the guide says so, but it made the first slow-client test flaky:
  - **The first version** read 256 KB messages every 30 ms and expected gaps.
  - **It failed on Linux** in the CI replay, and in 1 of 5 runs in WSL. Linux autotunes loopback buffers to several megabytes, so all 11 reads came from the buffers, consecutively.
  - **The test now** asks for 1 MB messages, stops reading for 300 ms so the buffers fill, then reads until it finds a gap (at most 200 messages).
  - **It passed** 10 of 10 runs in WSL and 5 of 5 on Windows before the final replays.
- **The producer has a second to return** after its connection ends, so it can log its counters (the example does), before it is aborted.
- **One API break.** `EndpointLink` gained a public field. Code that builds one with a struct literal must add `members`; the CHANGELOG says how.

## Tests

- **Unit** (`binary.rs`):
  - 1,000 sends into a full slot keep the newest and count 999 dropped, and `sent` stays 0;
  - nothing is accepted after the connection ends.
- **Integration** (`crates/teta-wot/tests/binary_stream.rs`, with a `#[binary_stream]` Thing, a real server and a WebSocket client):
  - bad parameters are answered with an error, and good ones change the message size;
  - a client that stops reading 1 MB messages for 300 ms then finds, after what the buffers kept, a gap in the counts: the producer kept going, and messages were dropped;
  - the stream closes on shutdown well within a 60-second grace period;
  - the TD links the stream with `subprotocol`, `type` and the `parameters` schema, and passes W3C validation.
- **Mutation checks:**
  - **Keeping the oldest message rather than the newest** fails the slot test.
  - **Removing the shutdown branch of the pump's main loop wasn't caught,** because a second check, around each send, still ended the stream.
  - **Removing both shutdown checks** fails the shutdown test.
  - All files were restored.
- **CI replayed locally:**

  | Job | Result |
  |---|---|
  | Ubuntu, a fresh LF clone of `7f09e76` in WSL | All 45 steps passed (fmt, clippy, tests, docs, docs pages, 38 examples, the guide); 78 test suites ok |
  | Windows | All 45 steps passed, the guide included; 78 test suites ok |

## Follow-ups and known gaps

- **OpenAPI** lists a binary stream as a `GET` with its description, but not its parameter schema. OScope's generated TypeScript types (M2) will want it, so OpenAPI could carry `P`'s schema, as the TD does.
- **No TestClient helper.** `TestClient`'s `TestWebSocket` can't receive binary messages, so the tests use `tokio-tungstenite` directly. A binary receive would help.
- **Per-subscriber counters aren't readable from outside the producer,** for example for OScope's status page. The producer can expose them itself.

Next phase: U05 (pluggable authentication).

## Clean room

All of it was written for this phase, against the axum and tokio documentation. Nothing was copied.
