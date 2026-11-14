# ADR-0022: Scratch recording and ingest

- Status: Accepted
- Date: 2026-11-14
- Phase: P29

## Context

- **The raw stream** (plan §5.5): during acquisition, a recorder appends every frame to a frame log on the scratch NVMe: preallocated, sequential, uncompressed, with an index. An ingest job then writes the frames into the project's OME-Zarr store, compressed with zstd (ADR-0021). Logging never compresses on the hot path.
- **The design rate** is 580 MB/s: 16-bit full frames of the MIChrome 20, 5,472 × 3,648, at 14.5 fps (§5.20). The MIChrome 20 itself delivers 289 MB/s. §5.20's budget: 580 MB/s for 10 minutes with no dropped frames.
- **The frame bus** (ADR-0008) has lossless subscribers, which apply backpressure: while one's queue is full, the camera's thread waits, and a camera that can't deliver counts the frames it loses in the next one's `dropped_before`.
- **The scratch root** (plan §5.12) holds frame logs, caches and temporary files, and its free space is watched.
- **A crash or a power cut** can leave a log's last frame half written, or, through the page cache, its header written and some of its pixels not.

## Decision

**1. Recordings** live in the scratch space, a folder each:

```text
{scratch root}/recordings/<recording>/recording.json, .lock, 000000.frames, 000001.frames, …
{scratch root}/tmp/
```

- **IDs** are version 7 UUIDs, `RecordingId` in `oscope-core`.
- **`recording.json`** is a versioned document, `oscope.recording` version 1: the name, when it started and ended, what it recorded, how many frames and bytes it holds, the frames the camera dropped between them, those discarded when rolling, why it ended early if it did, and whether it was recovered.
- **`.lock`** is held, with the standard library's file locks, while the recording records: nothing recovers, ingests or deletes it meanwhile, in this process or another.

**2. A frame log is a run of segment files,** each little-endian:

- **a header** of 4,096 bytes: `OSCOPE.FRAMES`, the format's version (1), the segment's number and the recording's ID;
- **records,** each at a multiple of 4,096 bytes: a header of 64 bytes (`FRAM`, the lengths of the frame's metadata and pixels, its number in the recording, and a CRC-32 of the metadata and pixels), then the metadata, `FrameMeta` as JSON, then the pixels as the frame's buffer holds them;
- **an index footer,** written when the segment closes: `OSCOPE.INDEX`, how many records, their offsets, and a CRC-32 of the footer; its own offset is the file's last 8 bytes.

Records aligned to 4,096 bytes let a later writer bypass the page cache without changing the format. A CRC-32 costs about 2 ms of a 40 MB frame (`crc32fast`, 24 GB/s), and finds what a power cut tore.

**3. Writing:**

- **Segments** are 4 GiB by default, about 7 s at the design rate. Each is preallocated (`fs4`: `fallocate` on Linux, the allocation size on Windows), written in order through the page cache, and closed when the next record wouldn't fit: its footer is written and the preallocation trimmed. The last is synchronised to the disk when the recording ends.
- **Before a segment,** the writer checks the disk's free space: a segment that would leave less than the policy's reserve is refused, and the recording ends there.
- **Rolling,** if asked: the oldest closed segments are deleted to stay within a number of bytes, at least two segments. The 10-minute run of §5.20 records this way: 348 GB in all, at most 100 GB at once.

**4. The recorder** (`oscope-acquire`) is a lossless subscriber with a queue of 16 frames, about a second at the design rate and 640 MB of the frame pool, writing from a thread of its own:

- **Stopping** writes the frames already queued, then ends the recording.
- **A write that fails,** on a full disk say, ends the recording there, its frames kept, and drops the subscription, so the camera waits no more.
- **Drops** are the sum of the recorded frames' `dropped_before`, the first's apart: the frames the camera lost between those recorded.

**5. Recovery.** When the scratch space opens, each recording that hasn't ended and isn't locked is recovered. Each segment without a footer is scanned record by record, each CRC-32 checked, as far as the first record that doesn't hold together. It is trimmed after its last whole record and given its footer. A last segment whose header was never written is deleted. The document then counts what remains, and says it was recovered.

**6. Ingest** is a compute job, `projects.ingest`, with the parameters `recording`, `project`, and optionally `name` and `keep`:

- **One acquisition** per recording, a plane for each frame. The frames must share their size and format. Bayer and grey frames give one channel, RGB three, RGBA four; planar floats are refused.
- **Eight frames at a time** are read, checked against their CRC-32 and written, which bounds the memory to about 640 MB; the pyramid's planes are made in parallel too.
- **The acquisition's record** holds each frame's position and the midpoint of its exposure, the camera's settings as at the first frame, the objective, and the recording's ID and source.
- **When it is committed,** the recording is deleted, unless `keep`.
- **Cancelled,** it stops between frames: the draft goes, and the recording stays.

**7. The scratch space's health and clean-up:**

- **The policy:** a reserve of 4 GiB that recordings leave free; low under 35 GB, about a minute at the design rate. The state is `ok`, `low`, or `full` under the reserve and a segment.
- **The `system` Thing's `scratch` property** gives the state, the disk's space, and the recordings waiting with their bytes.
- **Clean-up:**
  - `tmp/` is emptied when the space opens;
  - a recording a crash cut short is recovered then, and kept;
  - ingest deletes the recording it took in;
  - a rolling recording deletes its own oldest segments.

  Nothing else deletes a recording: it is raw data. A full disk refuses new segments.

**8. `crc32fast` is optimised in dev builds** too, so that tests can record at the design rate in debug builds.

## Consequences

- **Measured on the owner's PC:** see P29's notes for the rates and the 10-minute run.
- **A recording's frames are an acquisition's planes,** on NGFF's `z` axis. Time-lapses and scans, whose frames are times or tiles, may want axes of their own (P36, P39).
- **The page cache** holds what is written until the disk takes it: over minutes, the disk's sustained rate decides, not the cache. A writer that bypasses the cache can come later without changing the format.
- **A segment reserves 4 GiB** as it opens.
- **One OScope per scratch root.** The locks keep recordings in progress safe, but two servers would recover each other's crashed recordings.

## Alternatives considered

- **A file for each frame.** Simpler to read, but 14.5 files a second, each with its metadata, and no preallocation.
- **Compressing as it records.** zstd's fastest levels manage a few hundred MB/s a core; 580 MB/s would take several cores beside the preview's.
- **Writing OME-Zarr straight away.** The same compression, and many chunk files a frame, on the hot path.
- **Bypassing the page cache now** (`O_DIRECT`, `FILE_FLAG_NO_BUFFERING`). It needs aligned buffers and platform-specific opening; buffered writes are measured first.
- **A marker after each record, instead of a checksum.** It finds a record cut short by a crash, but not one torn by a power cut, whose later pages reached the disk before earlier ones.
- **One file per recording.** Rolling and recovery are harder, and a crash's preallocated tail could be hundreds of gigabytes.
