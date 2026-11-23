# ADR-0062: The simulated camera's image on the GPU

- Status: Accepted
- Date: 2026-11-20
- Phase: the GPU simulator (`docs/performance/gpu-simulator.md`)

## Context

- **The simulated camera is the live view's main cost** ([the live view's baseline](../performance/live-view-baseline.md)).
  - On the CPU it takes ~160 ms a still frame at full resolution, and over 2 s a frame while the stage jogs.
  - The first frame's tiles take 4.5 s.
  - Its 20 threads slow the preview beside it 7–9 times.
- **P15's GPU path covers only the exposure.** The tiles of the slide are still rendered on the CPU, and the server never turns the path on.
- **ADR-0013** makes the simulator deterministic, with a field rendered twice the same to the bit, and the CPU's twin the reference for every kernel (ADR-0010).
- **The slide is too large to hold.** At 10× its coverslip is ~318,000 tiles, about 250 GB as floats, and the owner's laptop GPU has 8 GB. Tiles must still be made on demand.

## Decision

1. **The H&E section's tiles are made on the GPU,** in two cached stages:
   - **the layers,** before their blur, independent of the focus;
   - **the tiles,** blurred, multiplied and with the glass and colour response, keyed as the CPU's.
   - Other samples, other light, and blurs beyond 64 pixels render on the CPU into the same cache.
2. **Tiles on the GPU are half precision** (IEEE 754 binary16).
3. **GPU tiles agree with the CPU's within a tolerance, not to the bit:** every sample within 2⁻⁸, and the mean within 2⁻¹². Frames without noise agree within 2 counts. The GPU is still deterministic: the same GPU makes the same tile to the bit, and the nuclei, from integer hashes, are the same on both.
4. **The camera makes tiles ahead of the stage between frames,** and **a frame makes at most a budget of new tiles.** One that needs more is exposed from a level four times coarser, and the rest are made between frames. In stepped time, every waiting tile is made before the camera waits, so the frames don't depend on the host's speed.
5. **The server's simulated camera renders on the GPU** when the build has CUDA, there is a GPU, and `[simulator] gpu` is true, the default.
   - Its frames are packed on the GPU and copied by DMA into pinned slabs.
   - Frames still reach the frame bus in host memory, as the real camera's do.
6. **The CPU path stays the reference and the fallback.** It keeps rendering bit for bit as before.
7. **The camera renders on threads of its own:** one pool in the process, of the processor's threads less the quarter that `oscope-compute`'s live pool takes. On rayon's global pool, with all the threads, it left the preview beside it waiting for cores.

## Consequences

- **Gains:**
  - the camera's still frames should take tens of milliseconds, not ~160;
  - jogging and focusing no longer stall on the CPU's tiles;
  - the CPU is free for the preview.
- **Costs:**
  - H&E's model exists twice, in Rust and in CUDA, and a change to one must be made to the other; the twin tests catch a drift;
  - ADR-0013's "the same to the bit" now holds for each backend separately, not between them;
  - up to 0.9 GB of the server's VRAM goes to the caches: at most a quarter of what is free. With less than a frame's worth free, the camera stays on the CPU.
- **To watch:**
  - the twin tests' bounds, if the model gains finer detail than half precision keeps;
  - how often frames fall back to the coarse level;
  - other samples, which stay on the CPU until they get kernels of their own.

## Alternatives considered

- **Everything in VRAM ahead of time:** the slide doesn't fit. A tile cache made on demand is the same idea at the size that fits.
- **Rendering frames continuously ahead:** a frame can't be rendered before its exposure ends, since the stage's path during it isn't known until then. Tiles can be made ahead, and are.
- **Single-precision tiles:** twice the memory, for precision the sensor's noise hides. The owner chose half.
- **Frames left in VRAM for the preview:** faster for the preview, but the frame bus, the recorder and everything downstream would have to take device frames, which the real camera doesn't make.
