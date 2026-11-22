# ex-compute-info

R03 includes CUDA by default. Run this example in the Linux development container
with `cargo run -p ex-compute-info --locked -- --smoke`. With GPU injection the
first available backend is CUDA; in `shell-cpu` it reports runtime CPU fallback.
The host-to-host microbenchmarks remain CPU operations by design. See the
[R03 manual procedure](../../docs/phases/r03-compute-algorithms.md#manual-test).

The compute layer, `oscope-compute` (P06):

- its backends: the CPU, and CUDA, which is listed as not in this build until P07;
- the processor's threads and widest SIMD instructions;
- how the threads are divided between the live and batch pools;
- microbenchmarks of the first kernels on a full 20 MP frame, in each pool;
- an equivalence check between two kernels, with its report.

## How to try it

```sh
cargo run --release -p ex-compute-info
cargo run --release -p ex-compute-info -- --runs 30
```

Use `--release`, since a debug build is ten times slower. It prints something like this, on a 24-thread i9-12900:

```text
backends:
  Cpu   available      rayon: 6 live and 18 batch threads, Avx2
  Cuda  not available  not in this build: CUDA arrives with P07
processor: 24 threads, Avx2; pools: 6 live, 18 batch

the median of 10 runs on a 5472 × 3648 RGGB16 frame:
                                   live                  batch
  to_f32              6.16 ms   6.48 GB/s      6.15 ms   6.49 GB/s
  bin2x2              3.04 ms  26.29 GB/s      3.16 ms  25.28 GB/s
  downsample ×4       3.25 ms  24.58 GB/s      2.45 ms  32.58 GB/s

bin2x2 against 4 × downsample ×2: 0 of 4990464 samples outside 1e-6 + 0e0 × |expected|; the largest error is 2.38e-7, at x 160, y 3, channel 0: …
```

**The kernels:**

- **`to_f32`** turns the 40 MB raw frame into 80 MB of floats from 0 to 1.
- **`bin2x2`** sums each 2 × 2 block.
- **`downsample ×4`** averages each 4 × 4 block, as a preview would.

**The rate** counts input bytes. `to_f32` is limited by memory, so the pool's size matters little.

**The check** compares binning with four times downsampling by 2: both are sums of the same blocks, added in a different order. The report gives the largest difference and where it is, as the CUDA twins' tests will (P07).

**`--smoke`** runs each kernel three times, in a debug build too:

```sh
cargo run -p ex-compute-info -- --smoke
```

It checks four things:

- the CPU backend is available, and each pool has threads;
- the last sample's value;
- the outputs' shapes;
- the equivalence check passes.

It ends with `smoke: ok`, or says what failed and exits with code 1. It doesn't check speeds, which vary from one machine to another.

For criterion's statistics, run the benchmarks: `cargo bench -p oscope-compute`.
