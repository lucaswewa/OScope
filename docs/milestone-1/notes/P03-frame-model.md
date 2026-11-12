# P03: Frame model, buffer pool, frame bus

- Status: In review
- Branch / PR: `m1/p03-frame-model` / [#11](https://github.com/lucaswewa/OScope/pull/11)
- Size: L · estimated ~1,275 before it started (code ~755, tests ~340, examples ~165, glue ~15), over L's 900; the owner chose one pull request · measured **1,602** hand-written lines (code 973, tests 433, examples 186, glue 10). Docs and `Cargo.lock` aren't counted. **That is 702 over L's 900, and 327 over the estimate.**

## Summary

Raw frames now move through the backend without copies ([spec](../phases.md#p03)). The new [ADR-0008](../../adr/0008-the-frame-model-buffer-pool-and-frame-bus.md) records the design.

- **The frame model** is in `oscope-core`, where every layer can use it:
  - pixel formats, with the Bayer pattern;
  - frame metadata, with P02b's line time and optional sensor timestamp;
  - a pool of fixed slabs, with a pluggable allocator for P07's pinned memory;
  - reference-counted frames;
  - views of a rectangle of a frame, which copy nothing.
- **The frame bus** is in `oscope-acquire`:
  - lossless subscribers, which either make the producer wait or drop and count;
  - latest-wins subscribers;
  - `next_frame_after(t)`;
  - counts for each subscriber.
- **The example, `ex-frame-bus`,** pushes 20 MP mosaics at 14.5 fps through the pool and the bus. On the owner's PC it held 14.5 fps (580 MB/s) for 60 s, and the lossless recorder lost no frame: the spec's *Done when*.

## What was built

| Where | What |
|---|---|
| `oscope-core::frame::format` | **`PixelFormat`:** `Bayer16 { pattern, significant_bits }`, `Bayer8`, `Mono8/16`, `Rgb8/16`, `Rgba8`, `PlanarF32 { planes }`, little-endian, with channels, sample and pixel sizes, planes, and a frame's bytes (saturating). **`BayerPattern`:** the colour at a pixel, and the pattern shifted by an offset |
| `oscope-core::frame::meta` | **`FrameMeta`:** §5.5's fields, as ADR-0008 lists them. `FrameMeta::new` fills in a whole-sensor frame; `row_exposure` and `exposure_midpoint` follow a rolling shutter. **`LightState`** |
| `oscope-core::frame::pool` | **`FramePool`:** fixed slabs from a `SlabAllocator` (`HeapAllocator` now). `take()` gives a `FrameBuffer`, or `None` and a count when all are in use. `PoolStats` |
| `oscope-core::frame` | **`Frame`:** a sealed buffer and its metadata, cloned by reference count. Its slab goes back to the pool with the last clone. `bytes`, `view`, `plane`, `same_pixels` |
| `oscope-core::frame::view` | **`FrameView`:** one plane, or a rectangle of it, with a stride. `row`, `sample`, and `roi`, which shifts a mosaic's pattern |
| `oscope-core` | `PixelRect`, and two errors: `BufferTooSmall` and `OutsideFrame` |
| `oscope-acquire::bus` | **`FrameBus`:**<ul><li>`publish`;</li><li>`subscribe_lossless(name, capacity, WhenFull::{Wait, Drop})`, which returns a tokio `mpsc::Receiver`;</li><li>`subscribe_latest`, which returns a `LatestReceiver`;</li><li>`next_frame_after`, `published` and `subscribers`.</li></ul> |
| `examples/ex-frame-bus` | The example, with `--seconds` and `--smoke` |
| `docs/adr/0008-…` | ADR-0008, and its index row |
| Docs | The examples README lists the examples; plan §7, the change log and the notes index |

**Dependencies:** `oscope-acquire` takes `tokio` (`sync`; `macros`, `rt` and `time` for tests) and `chrono` for its tests. Both are already in the lock at `teta-wot` v0.2.0's versions, and **the lock gained no packages.**

## How to try it

```sh
cargo run --release -p ex-frame-bus -- --seconds 60   # see its README for the output
cargo run -p ex-frame-bus -- --smoke
cargo test -p oscope-core --test frame                # 9 tests
cargo test -p oscope-acquire                          # 6 tests
```

## Performance

**The spec's *Done when*** was measured on the owner's PC (`docs/hardware.md`'s host: i9-12900, 64 GB) with `cargo run --release -p ex-frame-bus -- --seconds 60`.

| | |
|---|---|
| Frames | 870 of 5472 × 3648 RGGB16, 39.9 MB each |
| Rate during the run | 14.51–14.53 fps, 579–580 MB/s |
| The lossless recorder | 870 received, in order, 0 dropped |
| The preview (200 ms per frame) | 296 taken, 574 dropped |
| The statistics (a 512 × 512 ROI, ≤ 10 Hz) | 550 taken, 320 dropped |
| The pool | at most 2 of 24 slabs in use; never exhausted |

The run's last line reports 14.34 fps, because it divides by the time to the end of the reporting loop, a second after the last frame. The smoke check, in a debug build, also held 14.46 fps over 3 s.

## Design notes and deviations from the plan

- **The size, again over.**
  - **The estimate already counted documentation** at P02's measured density, and came to about 1,275. The owner chose one PR over a split into about 785 and 580.
  - **It measured 1,602.** The code came to 973 against 755, mostly in the bus, whose waiting, closing and counting each needed more than planned.
  - **Estimates from here** add a quarter on top of the density-corrected figure.
- **The pool is in `oscope-core`, not `oscope-acquire`,** though the spec names both crates. The simulated camera (L2) and the drivers fill frames, and ADR-0005 forbids them reaching `oscope-acquire` (L4). The bus is in `oscope-acquire`.
- **When the pool runs out, the newest frame is lost,** as the spec says. `take()` returns `None` and counts it, and the producer goes on.
- **`FrameMeta` has only the metadata the plan lists,** with two additions:
  - P02b's line time and black level;
  - the frame's format and size, so that metadata alone describes the pixels, as when it is saved or sent.
- **`exposure_midpoint` is the middle row's** for a rolling shutter. That is where the stage's position is interpolated.
- **The line time is per frame row.** The camera driver works it out for its binning.
- **Lossless subscribers receive through tokio's `mpsc::Receiver`,** rather than a type of our own. It offers `recv().await`, `blocking_recv()` for a thread such as the recorder's, and `try_recv()`.
- **`publish` runs on the producer's thread.** A waiting subscriber can block it, which an async executor must not allow. The camera drivers and the simulator have their own threads.
- **Latest-wins counts:** `delivered` counts frames taken, and `dropped` counts frames replaced before being taken. Lossless `delivered` counts frames queued.
- **`next_frame_after` starts waiting when it is called,** not when its future is first polled. Writing the tests showed the difference: otherwise a frame published between the call and the first poll is missed.
- **Planar frames:** `Frame::plane(i)` gives each plane's view, and `view()` gives the first.
- **Views read samples as `u16`,** from bytes, so `oscope-core` keeps forbidding unsafe code. Fast paths that reinterpret whole rows belong in `oscope-compute` (P06).
- **tokio moved from 1.53.1 to 1.53.3, beyond the spec.**
  - **What failed.** The CI replay's `cargo deny` failed on a new advisory, RUSTSEC-2026-0386: tokio's `AsyncFd` constructors are unsound. That is Unix only, and nothing in OScope or `teta-wot` uses `AsyncFd`.
  - **Where tokio came from.** It has been in the lock since P01, through `teta-wot`, so `main` fails the same check now.
  - **The fix.** 1.53.3 is fixed. It was released on 2026-10-13, so it meets the two-week rule. The workspace now requires it, and the lock changed only tokio's version.
  - **It is a separate commit** in this pull request.

## Tests

- **`oscope-core/tests/frame.rs`,** 9 tests:
  - the pool counts a lost frame, takes back dropped buffers, and is held by clones;
  - clones share one slab (pointer equality), and bytes are exactly the frame's;
  - a buffer too small for its frame is refused;
  - formats' sizes: a 20 MP mosaic is 39,923,712 bytes, and sizes saturate;
  - a planar frame's planes;
  - **a property test over 300 seeded cases.** It takes six formats and random sizes, and checks that a rectangle of a rectangle reads the same samples as its parent, and as the direct rectangle. A mosaic's colours follow the offset, and reading outside fails;
  - empty and outside rectangles are refused;
  - a rolling shutter's row times and midpoint;
  - the JSON of the metadata and the formats.
- **`oscope-acquire/tests/bus.rs`,** 6 tests:
  - every subscriber gets the same pixels, and the pool holds one slab;
  - a waiting lossless subscriber gets all 200 frames in order, from a producer on its own thread that must wait for a slow consumer;
  - a dropping lossless subscriber keeps the oldest, counts the rest, and the dropped frames' slabs return;
  - a latest-wins subscriber gets the newest, counts what it missed, and is woken by a new frame;
  - `next_frame_after`, using the fake clock, between two frames and exactly at one;
  - subscriptions end with their receiver, and with the bus, for lossless and latest-wins subscribers and for waiters. A full waiting queue whose receiver has gone doesn't block the producer.
- **Mutation checks,** each restored afterwards:

  | Broken on purpose | Failed |
  |---|---|
  | The pool's lost frames not counted | `an_exhausted_pool_counts_the_lost_frame_and_reuses_returned_slabs` |
  | A waiting subscriber's full queue dropping instead | `a_waiting_lossless_subscriber_gets_every_frame_in_order` (see below) |
  | A replaced latest frame not counted | `a_latest_wins_subscriber_gets_the_newest_frame` |
  | A subscriber that has gone kept | `subscriptions_end_with_their_receiver_or_with_the_bus` |
  | `next_frame_after` taking `<` for `<=` | `the_first_frame_after_a_moment_is_found_by_its_exposure` (see below) |
  | A mosaic's pattern not shifted by an ROI | `a_view_of_a_rectangle_reads_the_same_pixels_as_its_parent` |

  **Two of them found weak tests,** which were then fixed:
  - **Turning waiting into dropping made the test hang,** not fail. It waited for frames that were gone, because the test still held the bus. Now the producer owns the bus, so the queue ends and the test fails.
  - **`<` for `<=` passed.** No frame was exposed exactly at the moment waited for. The test now also waits for a frame's own moment.
- **Windows** (the working tree): all 8 CI steps passed on `4278bab`, with 45 suites and 48 tests. Both examples' `--smoke` passed. The first replay, on `5da8241`, failed only `cargo deny`'s advisories (see *Design notes*).
- **Ubuntu 24.04.5** (WSL, a fresh clone of `4278bab` in the Linux file system, 102 files, no CR in any): all 8 steps passed, with the same 45 suites and 48 tests.
- **GitHub CI:** CI_RESULT

## Follow-ups and known gaps

- **`teta-wot`'s own lock** still has tokio 1.53.1. That matters only for the binaries `teta-wot` builds itself, such as v0.2.0's Windows package, where the unsound Unix API can't be reached. A later `teta-wot` release can take 1.53.3.
- **Pinned memory** (P07) plugs in as a `SlabAllocator`.
- **The pool's size** is fixed at creation; the server's configuration will choose it (§5.5's default is 24).
- **Recording at 580 MB/s to disk** is P29's. The host's sustained write rate is P02b's last measurement, running after U06's soak.
- **U06's 24-hour soak** ends about 2026-11-12 16:45 PST. Its result goes into the U06 notes, and P02b's disk-write result into its notes and `hardware.md`, in this pull request.

## Clean room

Written for this phase. The design follows plan §5.5. Nothing was copied from `microscope` or any other codebase, and no algorithm needs a citation.
