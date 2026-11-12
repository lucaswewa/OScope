# ADR-0008: The frame model, buffer pool and frame bus

- Status: Accepted
- Date: 2026-11-12
- Phase: P03

## Context

- **Frames are big and fast.** A 20 MP RGGB mosaic in 16-bit samples is 40 MB. At 14.5 fps that is 580 MB/s, and the faster modes reach 67 fps (`docs/hardware.md`).
- **Many consumers read each frame.**
  - The recorder must get every frame.
  - The preview, the live statistics and the live mosaic only want the newest one.
  - Workflows want "the first frame exposed after t" (§5.5).

  Copying a frame for each consumer would multiply the memory traffic.
- **A slow consumer must not stall the camera,** except the recorder, whose losses are worse than a short wait.
- **The memory must be reusable and pluggable.** Allocating 40 MB per frame churns the allocator. P07 needs page-locked (pinned) memory, so that frames reach the GPU without a staging copy.
- **Producers sit low in the layers.** The simulated camera (`oscope-sim`, L2) and later drivers fill frames, so the pool can't live in `oscope-acquire` (L4) without breaking ADR-0005.
- **The camera shapes the metadata.** The MIChrome 20 has a rolling shutter and no timestamps of its own (P02b).

## Decision

**1. Where things live.**

- **In `oscope-core`:** the frame types, the pool and views. They have no I/O and no async, so every layer can use them.
- **In `oscope-acquire`:** the frame bus.

**2. Formats.**

- `PixelFormat` covers:
  - `Bayer16 { pattern, significant_bits }` and `Bayer8 { pattern }`;
  - `Mono8`, `Mono16`, `Rgb8`, `Rgb16` and `Rgba8`;
  - `PlanarF32 { planes }`.
- Samples are little-endian. Frames are tightly packed, row by row; a planar format's planes follow one another.

**3. Metadata.** `FrameMeta` holds §5.5's fields:

- the frame's ID, format and size;
- its ROI in full-sensor coordinates (ADR-0007), its binning, and its mode;
- its host timestamp, and an optional sensor timestamp;
- its first row's exposure start, its exposure, and an optional line time for a rolling shutter;
- its gain and black level;
- the objective and the lights;
- the stage position at the exposure's midpoint;
- the frames the camera dropped before it.

With a rolling shutter, row *r*'s exposure starts *r* line times after the first row's. The exposure's midpoint is the middle row's.

**4. The pool.**

- **`FramePool`** holds a fixed number of equal slabs, all allocated when it is made.
- **Taking a slab:** a producer takes one as a `FrameBuffer`, fills it, and seals it with its metadata into a `Frame`.
- **When every slab is in use,** taking one gives nothing: the frame is lost and counted, and the producer goes on. Nothing panics or waits.
- **Slabs come from a `SlabAllocator`:** the heap now, pinned memory from P07, without changing the pool.

**5. Frames and views.**

- **A `Frame` is shared by reference count.** Cloning one shares its pixels, and its slab returns to the pool when the last clone is dropped.
- **A `FrameView`** borrows one plane of pixels, or a rectangle of it, with a stride. It copies nothing.
- **A rectangle of a mosaic** carries the Bayer pattern of its own first cell, so that an ROI at an odd offset still demosaics correctly.

**6. The bus.** One producer publishes, and every subscriber gets a clone of the same `Frame`.

- **Lossless subscribers** get every frame, in order, through a bounded queue. When it is full, the producer either waits (`WhenFull::Wait`, for the recorder) or drops the frame for that subscriber alone and counts it (`WhenFull::Drop`).
- **Latest-wins subscribers** have one slot. A new frame replaces one not yet taken, and the replaced frame counts as dropped.
- **`next_frame_after(t)`** gives the first frame whose exposure starts at or after `t`. It starts waiting when it is called, not when its future is first polled.
- **Counts:** each subscriber's frames delivered and dropped, and the frames published.
- **Ending:** a subscription ends when its receiver is dropped. Every subscription ends when the bus's last clone is dropped.
- **Threads:** `publish` is called from the producer's own thread, not from async code, because a waiting subscriber can block it. Subscribers receive with `async` methods, and a lossless one can also block on a thread of its own.

## Consequences

- **Gains:**
  - A frame's pixels are written once and never copied, however many subscribers read them.
  - Memory use is fixed, and set when the pool is made.
  - A slow preview or statistics task can't slow the camera. Only the recorder can, and only when asked to.
  - P07's pinned memory and P12's simulated camera fit without changes here.
- **Costs:**
  - A consumer that keeps frames holds slabs. Holding too many empties the pool, and frames are lost, counted.
  - A frame can't be changed once sealed. Processing writes its result into a new frame from a pool.
  - `publish` must not run on an async executor's thread.
- **To watch:**
  - The pool's size: 24 × 40 MB, about 1 GB by default (§5.5). The modes beyond 14.5 fps have smaller frames.
  - Whether the recorder keeps up at 580 MB/s (P29).

## Alternatives considered

- **Copying frames to each subscriber:** simple, but 580 MB/s for each one.
- **A broadcast channel for everything:** its slow receivers lag, then lose frames in bulk. It can't wait for one subscriber, or give another only the newest frame.
- **Allocating each frame:** churns the allocator at 40 MB a time, and can't use pinned memory.
- **The pool in `oscope-acquire`:** the simulated camera and the drivers below L4 couldn't use it (ADR-0005).
- **Frames written into by consumers:** unsafe to share. Results go into new frames instead.
