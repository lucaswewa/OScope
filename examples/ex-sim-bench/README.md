# ex-sim-bench

How fast the simulated camera renders at full resolution, against §5.20's budgets: at least 14.5 frames a second with CUDA, and 5 on the CPU (P15).

**What makes it fast:** the camera keeps a cache of rendered tiles of the world, for each objective, light and depth.

- **The first frame** renders its tiles: 345 to 495 of them for a 20 MP frame at 10×, which takes seconds.
- **After that,** a frame only samples the cached tiles through the camera's mounting, over the stage's motion, and adds the sensor's noise.
- **On the GPU,** that sampling is a CUDA kernel, with the tiles held in GPU memory.
- **When the stage moves,** only the tiles coming into view are rendered.

**The example** puts the simulated MIChrome 20 over the H&E tissue at 10×, in mode `design` (20 MP, 16 bits, 14.5 frames a second), on a stepped clock. For each backend it times:

- the first, cold frame;
- warm frames with the stage still, and jogging at 0.2 and 1 mm/s.

Each is the median of eight frames, timed from the clock reaching the frame's end to the frame arriving.

## How to try it

```sh
cargo run --release -p ex-sim-bench
cargo run --release -p ex-sim-bench --features cuda    # with the CUDA toolkit
```

On the owner's PC (i9-12900, 24 threads; RTX A2000 12 GB) it prints:

```text
the simulated MIChrome 20, 5472 × 3648, mode `design`, over H&E tissue at 10×; median of 8 frames:
               cold          still   jog 0.2 mm/s     jog 1 mm/s   tiles
  CPU       3099 ms    99 ms 10.1/s   119 ms  8.4/s   323 ms  3.1/s     495
  GPU       3105 ms    16 ms 62.1/s    18 ms 54.7/s   162 ms  6.2/s     495
CPU: 10.1 frames a second, still: meets §5.20's 5
GPU: 62.1 frames a second, still: meets §5.20's 14.5
```

**What the numbers show:**

- **Still, both backends meet the budget:** the CPU at twice it, the GPU at four times.
- **Jogging slowly costs little:** at 0.2 mm/s, a frame renders a column of new tiles every few frames.
- **Jogging at 1 mm/s, neither keeps up with 14.5 frames a second.** Each frame then brings about 70 µm of new world into view at its leading edge. The tissue renders on the CPU at about 6.5 million pixels a second, whichever backend samples it. Rendering the samples themselves on the GPU would remove that limit; P15's notes record it as a follow-up.
- **The cold frame** is the old cost of a frame: the whole field rendered once.

**`--smoke`** runs the scaled-down sensor (1368 × 912), on the CPU only, three frames each, and checks:

- that a warm frame is faster than the cold one;
- that the cache holds tiles.

```sh
cargo run -p ex-sim-bench -- --smoke
```

It ends with `smoke: ok`, or says what failed and exits with code 1.

**The same budgets are ignored tests** in `oscope-sim`:

```sh
cargo test --release -p oscope-sim --test bench -- --ignored
OSCOPE_CUDA=1 cargo test --release -p oscope-sim --features cuda --test bench -- --ignored
```
