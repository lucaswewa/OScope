# P29: Scratch recorder and ingest

- Status: In review
- Branch / PR: `m1/p29-scratch-recorder` / [#43](https://github.com/lucaswewa/OScope/pull/43)
- Size: M · estimated ~2,900 before it started (the frame log ~520; scratch space ~170; ingest ~210; the recorder ~260; the ingest job and the scratch's health over HTTP ~130; the server's wiring ~25; tests ~790; `ex-record` ~300; and a fifth on top); one pull request, at the owner's standing choice for P24–P34 · measured **2,920** hand-written lines added (Rust code 1,455, of which `oscope-store` 1,089 and `oscope-acquire` 290; tests 1,036; examples 384, `ex-record`'s 378 among them; glue 45), and 47 removed. **That is 2,320 over M's 600, and on the estimate.**

## Summary

The raw half of the dual stream ([spec](../phases.md#p29); plan §5.5; [ADR-0022](../../adr/0022-scratch-recording-and-ingest.md)): every frame, as the camera gives it, to the scratch disk, then into the project.

- **`oscope-store`:**
  - **the frame log:** a recording's segment files, 4 GiB each, preallocated, holding each frame's metadata and pixels in a record with a CRC-32, and an index footer; rolling, for long runs; recovery by scanning after a crash;
  - **the scratch space:** its health, and its clean-up;
  - **ingest:** a recording into an acquisition, eight frames at a time, and the pyramid's planes in parallel.
- **`oscope-acquire`:** the recorder, a lossless subscriber writing from a thread of its own; the `projects.ingest` job.
- **`oscope-api`:** the `system` Thing's `scratch` property; the `projects` Thing registers the ingest job. The server opens the scratch space at start-up, recovering what a crash cut short.
- **`oscope-core`:** `RecordingId`. **`oscope-hal`:** the fake camera free-runs on a fixed schedule, counting the frames lost while its sink holds it, and fills its rows at memset speed.
- **`examples/ex-record`.**
- **P28 is marked Done.**

## What was built

| Where | What |
|---|---|
| `oscope-store`, `recording` | `Recording`, the document `oscope.recording` version 1; `RecordingOptions`; `RecordingWriter`, which appends frames and finishes; `RecordingReader`, which lists frames and reads their pixels, checked; `FrameEntry`; recovery |
| `oscope-store`, `scratch` | `Scratch`: opened, it empties `tmp/` and recovers crashed recordings; its health, recordings, folders, new recordings and deletions. `ScratchPolicy`, `ScratchHealth`, `ScratchState` |
| `oscope-store`, `ingest` | `ingest`: a recording into a new acquisition, with each frame's position and time, its progress told, cancellable; the recording deleted unless kept |
| `oscope-store`, `image` | The pyramid's planes made in parallel |
| `oscope-acquire` | `Recorder`, `RecorderStats` and `QUEUE`; `ingest_jobs` and `INGEST`, the job's kind |
| `oscope-api` | `ApiBuilder::scratch`; the `system` Thing's `scratch` property; the `projects` Thing registers `projects.ingest` |
| `oscope-server` | Opens the scratch space at start-up |
| `oscope-core` | `RecordingId` |
| `oscope-hal` | The fake camera's schedule and fill |
| `examples/ex-record` | Recording at the design rate, then ingest |
| `examples/ex-server-hello` | Opens a scratch space, as the server does: its smoke reads every property of the `system` Thing, now nine |
| Workspace | `crc32fast`, already in the lock, taken directly and optimised in dev builds; `oscope-acquire` takes `oscope-store`, `oscope-jobs` and `tokio`'s `macros` |

## How to try it

[`ex-record`](../../../examples/ex-record/README.md):

```sh
cargo run --release -p ex-record --features cuda                      # 10 s of the simulated camera, then ingested
cargo run --release -p ex-record -- --test-sensor --seconds 30        # the recorder alone, at the design rate
cargo run --release -p ex-record --features cuda -- --seconds 600 --keep-gb 100   # the 10-minute run
```

Over HTTP, a recording is ingested by `POST /api/v1/jobs/submit` with `{"kind": "projects.ingest", "params": {"recording": …, "project": …}}`, and `GET /api/v1/system/scratch` gives the scratch space's health. P30's `capture` Thing will make recordings over HTTP.

## Performance

**On the owner's PC:** i9-12900, 24 threads; RTX A2000 12 GB; a Micron MTFDKBA1T0TFH NVMe of 1 TB, its only disk, with 216 GiB free. Release builds.

**The 10-minute run,** for §5.20's budget of 580 MB/s for 10 minutes with no dropped frames: **met.** The simulated MIChrome 20, rendering on the GPU, recorded 8,701 frames, 347 GB, in 600 s: 579 MB/s, the full 14.5 fps, with none dropped. Rolling within 100 GB, it kept the newest 2,388 frames, 89 GiB, and deleted the 6,313 oldest as it went. The queue held at most 11 of its 16 frames, about 250 s in: the writes stalled for about three quarters of a second there, and the queue took it, with 5 frames to spare.

**30 s runs,** frames of 39.9 MB at 14.5 fps:

- **the test sensor:** 435 frames, 17.4 GB, at 578 MB/s; none dropped; the queue at most 1 of 16.
- **the simulated MIChrome 20, rendering on the GPU:** 14.5 fps once its first frame had rendered, which took 3.5 s. The 45 frames whose time passed meanwhile counted as dropped, so the recorder now starts once the camera streams, as an acquisition would.

**The recorder's own cost:** a CRC-32 of a 40 MB frame takes about 1.7 ms (`crc32fast`, 24 GB/s; 0.7 GB/s unoptimised, hence the dev profile's exception). The rest is the operating system's: the writes into the page cache, and the disk.

**Ingest,** eight frames at a time:

- **the simulated H&E,** 390 frames, 15.6 GB: 63.7 s, 245 MB/s, to 10.3 GB, 1.5 times smaller;
- **the test sensor's stripes,** 435 frames, 17.4 GB: 25.4 s, 684 MB/s, to 0.07 GB.

On data like a slide's, ingest is slower than recording: it runs afterwards, as plan §5.5 has it.

## Design notes and deviations from the plan

**Worth your review:**

- **One pull request,** as you decided for P24–P34. It came in on the estimate, and 2,320 lines over M's 600. The frame log, with its recovery, took 663 lines, more than its ~520; the tests are more than a third.
- **The 10-minute run met §5.20's budget** on this PC's NVMe: 579 MB/s for 600 s, none dropped. But the queue once held 11 of its 16 frames, so a slower or busier disk could fill it. The instrument's own scratch NVMe (D17) is the one that counts: this was the PC's only disk, shared with everything else.
- **The fake camera changed** (`oscope-hal`). It slept a whole frame interval after each frame, so at the design size it delivered 13.4 fps, not 14.5. It now keeps a fixed schedule, as a free-running sensor does, and the frames whose time passes while its sink holds it are lost and counted. Its 16-bit rows are filled a byte at a time: 257 times a level has the level in both bytes. Every test that uses it passed unchanged.
- **`crc32fast` is optimised in dev builds:** the workspace's first `[profile]` section. Unoptimised, it checks 0.7 GB/s, too slow for the recorder's tests at the design rate on two cores.
- **The `zstd` crates P28 took early:** `zstd` 0.13.4 and `zstd-safe` 7.4.0 turn two weeks old at 14:12 UTC on 2026-11-15, after this pull request. They are verified then: not yanked, the same licence, and a fresh `cargo deny check`. The notes of the phase in progress then record it.
- **A recording's frames are an acquisition's planes,** on NGFF's `z` axis. Time-lapses and scans may want axes of their own (P36, P39).
- **No HTTP for the recorder yet.** P30's `capture` Thing records bursts. P29 gives HTTP the ingest job and the scratch's health.
- **132 GB of incremental build caches** were deleted from `target/` before the 10-minute run, to give the disk room: C: had 113 GiB free.

**Other choices:**

- **Records at multiples of 4 KiB,** so that a writer bypassing the page cache could come later without a new format. The writer goes through the cache now: measured first.
- **A CRC-32 for each record,** checked when ingest reads it and when recovery scans; a marker alone would miss a record a power cut tore.
- **Segments of 4 GiB,** about 7 s at the design rate: a crash's preallocated tail is at most that, and rolling deletes that much at a time. Only the last is synchronised to the disk, when the recording ends.
- **A queue of 16 frames,** about a second at the design rate and 640 MB of the frame pool.
- **Drops** count the frames lost between those recorded, not before the first.
- **The scratch policy:** a reserve of 4 GiB, and low under 35 GB, a minute at the design rate. Settings can change them later.
- **Recordings in progress** hold a file lock, so that no other process recovers, ingests or deletes them.

## Tests

| Crate | Test | What it checks |
|---|---|---|
| `oscope-store` | `recordings` (6) | **Round trips** of frames through segments of 64 KiB, two frames each, with their metadata, pixels, drops and footers; **the MIChrome 20's frames,** three of 40 MB in the default 4 GiB segment, trimmed. **Recovery** after six kinds of crash: the last record whole, cut in its pixels, its metadata or its header, torn by zeros, a new segment never written, and a header claiming a terabyte: each recovered to its whole frames, with a footer on each segment. **Rolling** within three segments. **Refused:** a frame that doesn't match its checksum, another format's segment, segments under 64 KiB, rolling within one segment, an empty name. **The scratch space:** `tmp/` emptied, a recording in progress neither recovered nor deleted, its health ok, low and full, its recordings and bytes, a full disk refusing a segment |
| `oscope-store` | `ingest` (4) | **The data equal to the frames,** six raw frames of 1,301 × 1,029 in three levels, plane by plane, with their positions, times, camera and objective, the progress told, the recording deleted; **the MIChrome 20's frames,** two, in five levels, the recording kept; **each format's channels,** grey, RGB and RGBA. **Refused:** a recording still recording, one with no frames, frames of two sizes, planar floats; **cancelled,** no acquisition left and the recording kept |
| `oscope-acquire` | `recorder` (3) | **The test sensor at the design rate,** 5,472 × 3,648 at 14.5 fps for 2 s: every frame written, none dropped, the queue never full, a frame each 69 ms, the stripes of every tenth frame. **Stopping** writes the frames still queued. **A recording that can't write** ends with its reason and holds the camera up no more |
| `oscope-hal` | `fakes` (1 new) | The fake camera keeps its schedule, and counts the frames lost while its sink held it |
| `oscope-api` | `recordings` (1) | **Over HTTP:** the scratch's health, the `projects.ingest` job submitted and followed to its acquisition, the recording gone; a recording that isn't there refused (400) |
| `oscope-api`, `oscope-server` | `api`, `cli` | The `system` Thing's ninth property; without a scratch space, an error that says so |
| `ex-record` | `--smoke` | 1.5 s of the test sensor at a quarter size, none dropped, ingested, every frame a plane, the first as the sensor drew it, the recording gone |

**Where they ran:** on Windows, and on Linux under WSL, both with uv, in the local replays (`scripts/ci-local.sh` on Windows, and `scripts/ci-clone.sh` on a fresh clone under WSL: 39 of 39 steps each, 383 tests in 153 suites, the Python tests real). The first replay under WSL failed one step: `ex-server-hello --smoke` reads every property of the `system` Thing, and the new `scratch` property answers with an error on a server without a scratch space. The example now opens one, as the server does. A Windows replay run beside the second failed `ex-sim-stage --smoke`: a stage motion came 56 ms late in simulated time at 20×, 2.8 ms on the host, while the two replays shared the processor. That smoke then passed 5 times out of 5 alone and 5 out of 5 pinned to two cores, and the Windows replay run alone passed every step. On Windows, `oscope-store`'s `recordings` and `ingest` suites, `oscope-acquire`'s `recorder`, `oscope-hal`'s `fakes`, `oscope-api`'s `recordings` and `ex-record --smoke` passed 5 times out of 5 each with their binaries pinned to two cores, `recorder` 10 times. The first pinned runs of `recorder` failed 2 times of 5. Its check that the frames came at the design rate wanted a mean interval of 68 to 70 ms, and on two cores the first frame came late, filling a fresh 40 MB buffer, which shortened the mean to 67.9 ms. The check now takes its bound from the sensor's schedule: with none dropped, each frame comes less than a period late, so the mean is within a period, over the intervals, of 69 ms.

**Mutation checks** (each broke the code on purpose, then the tests were run): twenty-four of twenty-four caught. Reading the tests against the list first found five gaps, now covered: no check of a normally closed segment's footer; frame 0 dropping nothing, so counting its drops couldn't show; no header claiming a huge length; frames too small to be still queued at a stop; and nothing holding the fake camera up.

| Broken on purpose | Caught by |
|---|---|
| Records' checksums not checked when scanning | `recordings_cut_short_are_recovered` |
| A frame's pixels not checked when read | `damaged_frames_other_formats_and_bad_options_are_refused` |
| No footer when a segment closes | `frames_round_trip_through_segments_with_their_metadata`, and two more |
| Preallocation never trimmed | the same, and `a_recording_holds_the_michrome_20_s_frames` |
| Rolling never deletes | `a_rolling_recording_keeps_within_its_bytes` |
| Free space not checked | `the_scratch_space_reports_its_health_and_cleans_up` |
| Drops counted before the first frame too | `frames_round_trip_through_segments_with_their_metadata` |
| A last segment without a header kept | `recordings_cut_short_are_recovered` |
| Recovered segments not given a footer | the same |
| A record's length not bounded by its file | the same: its test binary aborts, asked for a terabyte |
| `tmp/` not emptied | `the_scratch_space_reports_its_health_and_cleans_up` |
| A recording in progress recovered | the same |
| Never low | the same |
| Frames written into the wrong planes | `ingested_data_equals_the_recorded_frames`, and two more |
| The recording kept after its ingest | `ingested_data_equals_the_recorded_frames` |
| Cancelling ignored | `ingest_refuses_what_it_cannot_take_and_stops_when_cancelled` |
| Frames of other sizes accepted | the same |
| Positions not kept | `ingested_data_equals_the_recorded_frames` |
| Stopping drops the frames queued | `stopping_writes_every_frame_published_before` |
| A failed write doesn't end the recording | `a_recording_that_cannot_write_ends_without_holding_the_camera_up` |
| The recorder's queue drops frames when full | `stopping_writes_every_frame_published_before` |
| The fake camera counts no frame lost while held | `a_free_running_camera_keeps_its_schedule_losing_frames_while_held` |
| The fake camera sleeps a whole period after each frame | `the_recorder_keeps_up_with_the_test_sensor_at_the_design_rate` |
| The ingest job not registered | `a_recording_is_ingested_into_a_project_by_a_job` |

## Follow-ups and known gaps

- **The recorder over HTTP,** with P30's `capture` Thing.
- **Bypassing the page cache,** if the instrument's NVMe needs it.
- **Faster ingest** of slide-like data: more frames at once where memory allows.
- **Axes for time and tiles** in recordings' acquisitions (P36, P39).
- **The scratch policy in the settings** (the `settings` Thing).
- **Synchronising each segment as it closes,** if a power cut must lose less.

## Clean room

Nothing was copied from `microscope` or any other codebase. The frame log's format is OScope's own. Its checksums are the standard CRC-32 (ISO-HDLC, IEEE 802.3), through `crc32fast`.
