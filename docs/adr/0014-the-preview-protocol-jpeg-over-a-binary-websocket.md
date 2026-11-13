# ADR-0014: The preview protocol: JPEG over a binary WebSocket

- Status: Accepted
- Date: 2026-11-13
- Phase: P17

## Context

- **D6** asks for a live preview of JPEG over a binary WebSocket, sized to each client's viewport, or a full-resolution ROI crop when zoomed in. A client that falls behind gets the newest frame, older ones are dropped, and the preview never runs faster than the camera. Raw frames never leave Rust (§5.5).
- **§5.20's budget:** a 20 MP frame to a JPEG sent at 1920 px in 30 ms with CUDA, 60 ms on the CPU.
- **`teta-wot` v0.2.0 has binary streams (U04).** A `#[binary_stream]` method runs once for each connection, with a `Subscriber`:
  - the client's parameters, set by JSON text messages, each of which replaces the last;
  - a one-slot sink, where an unsent message is replaced by the next and counted as dropped;
  - binary messages only, from the server; a text message goes back only to answer parameters it can't read.
- **The sink only learns that a client is slow when the socket stops taking data.** On P17's tests, Windows' loopback buffers took about 800 KB of previews, 26 frames, before a client that read four a second slowed the server at all. That client saw frames seconds old, and still in order.
- **Plan §5.4 placed the stream at `GET /api/v1/media/preview`,** OScope's own media plane, with JSON messages from the server for statistics and dirty mosaic tiles.

## Decision

**1. The stream is the `preview` Thing's binary stream, `/api/v1/preview/stream`,** not a hand-written media-plane route. It inherits `teta-wot`'s authentication, its shutdown and its TD link with the parameters' schema. The `preview` Thing also holds the defaults as saved settings, and lists the sessions with their statistics.

**2. Each binary message is a header, then its payload.** Numbers are little-endian:

| Offset | Bytes | Field |
|---|---|---|
| 0 | 4 | `OSPV` |
| 4 | 1 | the version: 1 |
| 5 | 1 | the kind: 1 for a frame |
| 6 | 1 | the codec: 1 for JPEG |
| 7 | 1 | the JPEG quality |
| 8 | 4 | the header's length, 72: where the payload starts |
| 12 | 4 | the message's number in its session, from 0 |
| 16 | 8 | the frame's ID |
| 24 | 8 | when its exposure started, in ns on the server's clock |
| 32 | 8 | when its last row's exposure ended |
| 40 | 8 | when the message was ready |
| 48 | 16 | the sensor's pixels it shows: x, y, width and height, in full-sensor coordinates |
| 64 | 4 | the image's width in pixels |
| 68 | 4 | its height |

- **Fields are only added at the end,** and a reader takes the payload from the header's length.
- **The version changes only if a field changes its meaning,** and a reader refuses a version it doesn't know.
- **Kinds and codecs** are numbered as they come. P18's saturation mask will be a kind of its own.

**3. The client's JSON text messages** give `width` and `height` (the viewport), `roi` (sensor pixels), `quality`, `max_fps` and `ack`. What a message leaves out takes the `defaults` setting's value. Each message replaces the last, so a client sends them all each time.

**4. Clients may acknowledge.** `ack` is the number of the last message the client has shown. Once a client sends one, the session sends at most two messages beyond it. A frame published meanwhile replaces the one waiting, so the next message is of the newest frame. A client that never acknowledges is served as fast as its socket takes the messages, which is what `teta-wot`'s sink gives.

**5. What a client gets:**
- **The ROI** is clipped to the frame and widened to whole cells of the mosaic. An ROI that misses the frame gets the whole frame. The header says which pixels a message shows.
- **The size** fits the viewport, keeping the ROI's shape, and is never larger than the ROI: the client enlarges it.
- **The rate** is the camera's at most, and at most `max_fps`.
- **Adapting:** while a client keeps missing frames (3 of 10, replaced unsent or skipped while it hadn't acknowledged), its quality comes down 15 a step to 30, then its size a quarter a step to a quarter of its viewport. After 50 messages in a row without missing one, it goes back up a step.

**6. Statistics** are each message's own times, in its header, and each session's last second, in the `preview` Thing's `sessions` property: fps, the mean latency from the end of the exposure to the message being ready, bytes a second, messages sent and missed, quality and size. The server sends no JSON messages: `teta-wot`'s streams carry binary messages only.

## Consequences

- **Gains:**
  - A client that acknowledges sees the newest frame within a frame or two, whatever its link and the operating system's buffers. P17's slow-client test checks it.
  - A client that doesn't acknowledge, such as a quick script, still works.
  - The preview is described in the Thing Description, with the parameters' schema, and uses the server's authentication.
  - The header is enough to draw a frame where it belongs on the sensor, and to measure latency and frame rate.
- **Costs:**
  - A message from the client for each frame shown, of a few dozen bytes.
  - Each acknowledgement resends every parameter, because `teta-wot`'s parameters replace each other.
  - Each client's preview is made for it alone: one encode per client and frame.
  - The server's clock isn't the client's: a client compares header times with each other, and adds its own time from arrival to drawing.
- **To watch:**
  - P19's shared-memory ring for the desktop app reuses the header, with the payload in the ring.
  - P30's dirty mosaic tiles and P18's statistics need either their own kinds of message or their own streams. JSON from the server would need `teta-wot` to send text on a binary stream.

## Alternatives considered

- **`/api/v1/media/preview`, written by hand, as the plan had it:** JSON from the server would be possible, but the stream would leave the Thing Descriptions, and would need its own authentication, shutdown and parameters.
- **Relying on the socket's backpressure alone:** the test showed seconds of stale frames on loopback before anything was dropped, and adapting makes it worse: smaller messages fit more of them in the buffers.
- **Acknowledgement required of every client:** a client that forgot would stall after two frames.
- **Statistics as a kind of binary message:** they would take the one slot from a frame. Per-message times in the header, and per-session numbers as a property, need no slot.
- **WebP or H.264:** later, if remote links need it (D6).
