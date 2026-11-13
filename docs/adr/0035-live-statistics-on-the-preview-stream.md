# ADR-0035: Live statistics on the preview stream

- Status: Accepted
- Date: 2026-11-13
- Phase: P18

## Context

- **§5.5 and P18's spec:** live statistics are computed from raw frames, not the 8-bit preview, at up to 10 Hz. They are histograms, a saturation mask at preview resolution packed into bits, line profiles, ROI statistics and a focus value, and they all go out on the preview WebSocket.
- **ADR-0014** made the preview a `teta-wot` binary stream, with a header and then a JPEG. It said that statistics and P18's mask would need kinds of message of their own, or streams of their own, because:
  - **the stream's sink has one slot:** a message sent while another waits replaces it;
  - **the server can't send text** on a binary stream.
- **A statistics message sent beside a frame would compete for that slot.** Sent just after a frame, it would replace the frame if the socket hadn't taken it yet, and the reverse.
- **Accepted ADRs aren't edited** (the ADR index), so this extends ADR-0014 in a record of its own.

## Decision

**1. Statistics travel inside a frame's message,** as blocks after its JPEG. A client that asks for statistics gets them with the preview of the same frame, in the one slot, and they can't displace each other.

**2. The header grows by one field,** under ADR-0014's rule that fields are only added at the end:

| Offset | Bytes | Field |
|---|---|---|
| 72 | 4 | the JPEG's length |

- The header's length is now 76, and the version stays 1.
- A message's blocks follow its JPEG. Each is its kind (1 byte), 3 zero bytes, its length (4, little-endian), then its bytes.
- A reader of ADR-0014's 72-byte header, which takes the rest as the JPEG, still reads a message without blocks correctly. Only a client that asks for statistics gets blocks, and it reads the JPEG's length.

**3. Two kinds of block,** numbered with ADR-0014's kinds of message (1 is a frame):

- **2, statistics:** UTF-8 JSON (`oscope_acquire::stats::LiveStats`).
  - Each channel's 12-bit histogram, from a subsample of about a million samples by whole cells, in the bins asked for. With it, the samples counted, the 0.1, 1, 50, 99 and 99.9 % percentiles, and the share at full scale.
  - The ROI's mean, standard deviation, minimum and maximum, channel by channel, when an ROI is asked for.
  - A profile, cell by cell along a line, when one is asked for.
  - The focus value: the normalised variance of the green cells over the ROI, or over the region the preview shows.
  - Values are in the sensor's own units, 0 to 4095 for a 12-bit sensor. A mosaic's channels are its colours.
- **3, the saturation mask:** the preview's width and height (little-endian `u32`s), then rows of ⌈width / 8⌉ bytes. Each pixel's bit runs from a byte's highest bit down, and is set where a raw sample under that pixel is at full scale. It is sent only when one is, and only if asked for; a message with statistics but no mask means none.

**4. A client asks with `stats`** in its parameters: `roi`, `profile` (`from` and `to`, in sensor coordinates), `bins` (256 by default, up to 4096) and `mask` (true by default).
- Its session measures the frame it previews, in the live pool.
- **Messages with statistics are sent at least 100 ms apart:** the next is measured no sooner than 100 ms after the last was sent. Timed from the measuring instead, a preview that took longer to make than the next would put them closer.
- A message without statistics is a frame that came within 100 ms of the last statistics.

## Consequences

- **Gains:**
  - Statistics and the frame they describe arrive together, so overlays are never drawn on the wrong frame.
  - No message competes with a frame for the slot, and acknowledgements (ADR-0014) cover both.
  - Clients that don't ask pay nothing, and P17's readers still work.
- **Costs:**
  - Each session measures for itself, so two clients asking for statistics measure the same frame twice. The histograms are subsampled to keep that small.
  - A mask is a bit a preview pixel: about 300 KB at 1920 × 1280, up to 10 times a second, but only while something is saturated.
  - Statistics come no faster than frames: a camera slower than 10 Hz gives them with every frame.
- **To watch:**
  - P30's dirty mosaic tiles can be another kind of block.
  - If several clients measuring the same frames costs too much, the frame-wide parts (histograms, percentiles) can move to one shared subscriber, and the per-client parts (ROI, profile, mask) stay in each session.

## Alternatives considered

- **Messages of their own on the same stream:** they would compete with frames for the one slot.
- **A second binary stream for statistics:** a slot of its own, but the spec asks for the preview's WebSocket. Statistics could also arrive for a frame the client never sees.
- **JSON text from the server:** `teta-wot`'s binary streams can't send it.
- **The mask as a JSON array:** several times larger than bits.
