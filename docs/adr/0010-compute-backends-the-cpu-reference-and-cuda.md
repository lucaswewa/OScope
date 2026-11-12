# ADR-0010: Compute backends: the CPU reference and CUDA through `cudarc`

- Status: Accepted
- Date: 2026-11-12
- Phase: P06

## Context

Every image algorithm in OScope, from the preview to stitching, runs on the compute layer, `oscope-compute`, at layer L1.

- **D7:** CUDA, through `cudarc`, where an NVIDIA GPU is present. A CPU path is the reference that every CUDA kernel is tested against, and the fallback when there is no GPU.
- **CI has no GPU.** CUDA tests run on the owner's machine only (`OSCOPE_CUDA=1`).
- **The preview must stay live** while jobs process data on the same processor (§5.20).
- **A 20 MP frame** is 40 MB raw and 80 MB as floats, so kernels should be measured at that size.

P06 builds the layer with the CPU backend. P07 adds CUDA, and ADR-0011 records how its kernels are built, loaded and redistributed.

## Decision

1. **Algorithms call `Compute`, never a backend directly.** Each kernel is a method of `Compute`, such as `to_f32`, `bin2x2` and `downsample`. It runs on the backend in use, in the lane it is given.
2. **The backends** are `Backend::Cpu` and `Backend::Cuda`. `ComputeInfo` lists both, the one in use first, with whether each is available and why not.
   - **The CPU** is always available.
   - **CUDA** is listed as unavailable until P07 builds it.
   - **`ComputeInfo` also records** the processor's threads, its widest SIMD level (by run-time detection) and the pools' sizes.
3. **There are two rayon pools, one for each `Lane`.**
   - **Live:** the preview and other work that can't wait. It has a quarter of the threads, and at least 2.
   - **Batch:** jobs, with the rest.
   - **Kept apart:** each pool has threads and a queue of its own, so live work never waits behind batch work. A kernel's parallel loops run in the pool that called it.
   - **Threads are named** `oscope-live-N` and `oscope-batch-N`, and `Lane::current()` tells which pool a thread belongs to.
4. **The CPU kernels (`cpu::…`) are the reference.** They are written plainly, with rayon over rows, and every CUDA kernel has one as its twin.
5. **The working format is `Image`:** 32-bit floats, interleaved by channel, row by row, with a `Shape` (width, height, channels). These conventions hold:
   - **`to_f32` scale:** a 16-bit sample is divided by 65,535, an 8-bit one by 255. A mosaic keeps one channel.
   - **Binning** sums 2 × 2 blocks and drops an odd last row or column, as a sensor does.
   - **Downsampling** averages blocks, including the smaller ones at the edges, so the output covers the image.
6. **Equivalence is checked with `equivalence::compare`.**
   - **The tolerance:** a sample passes when `|actual − expected| ≤ abs + rel × |expected|`. NaN matches only NaN.
   - **The report** gives the count outside the tolerance, the largest error, its position (x, y, channel), and the two values there.
   - **The tolerance is chosen per kernel** in its test, and stated.
7. **Benchmarks** use criterion, in `benches/`, one file per area.
   - **Inputs** are a full 20 MP frame.
   - **Throughput** is in input bytes.
   - **Kernels run** in the live lane, through `Compute`.
   - **CI** compiles the benchmarks but doesn't run them.
8. **CUDA (P07)** uses `cudarc`, with the driver, cuFFT and NVRTC loaded at run time. Without a GPU or a driver, `Compute` warns once and uses the CPU. ADR-0011 records the rest.

## Consequences

- **Callers don't change when CUDA arrives:** `Compute` chooses the backend.
- **A `Compute` method for each kernel** keeps the API explicit. It grows with the kernels, and P21's pipeline nodes wrap them.
- **Data moves between host and device** in P07. The pipeline engine (§5.6) keeps data on the GPU between CUDA nodes. `Image` is the host form.
- **The live pool takes a quarter of the threads** even when nothing is live. The batch pool has fewer threads than the processor, which costs batch jobs about a quarter of their speed at most. Both sizes can be set (`ComputeConfig`).
- **The pools don't set OS thread priorities.** Under heavy load, the operating system still shares the cores between them. Live work is kept from waiting in a queue, not given more of the processor.
- **Measured** on the owner's PC, with release builds: on a full 20 MP frame in the live lane (6 threads), `to_f32` takes 6.6 ms, `bin2x2` 2.8 ms, and downsampling by 4 2.8 ms.

## Alternatives considered

- **A `Backend` trait with a method for every kernel,** implemented once for the CPU and once for CUDA. That is the same work, with a trait object between the caller and the kernel. A `match` in `Compute` is simpler while there are two backends.
- **One rayon pool,** the global one. Batch jobs would fill its queue, and the preview would wait behind them.
- **Accumulating in f64 in the CPU kernels.** That is more exact, but the CUDA twins will accumulate in f32, so the reference would differ from them by more. The tolerances cover f32 summation, and the plain f64 reference in the tests checks the kernels.
- **A home-made benchmark harness.** Criterion gives warm-up, statistics and comparison with earlier runs. It is a development dependency only.
