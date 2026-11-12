# ex-cuda-check

The CUDA backend (P07):

- **the GPU** that CUDA uses, or why there is none;
- **the CUDA twins** of the first kernels against their CPU references, on a full 20 MP frame, with their times. The comparison should say `exact`;
- **uploads:** how fast a 40 MB frame goes to the GPU from the frame pool's pinned memory, and from ordinary memory.

## How to try it

The CUDA backend needs the CUDA 13.4 toolkit to build (its `nvcc`), and an NVIDIA GPU with a CUDA 13 driver to run. [ADR-0011](../../docs/adr/0011-building-loading-and-redistributing-cuda-kernels.md) explains.

```sh
cargo run --release -p ex-cuda-check --features cuda
cargo run --release -p ex-cuda-check --features cuda -- --runs 30
```

On the owner's PC it prints:

```text
backends, the one in use first:
  Cuda  available      NVIDIA RTX A2000 12GB, compute 8.6, 12281 MB; the driver supports CUDA 13.2
  Cpu   available      rayon: 6 live and 18 batch threads, Avx2

GPU 0: NVIDIA RTX A2000 12GB, compute 8.6, 12281 MB; the driver supports CUDA 13.2
memory pool kept: true; NVRTC: true; cuFFT: true

the median of 20 runs on a 5472 × 3648 RGGB16 frame, in the live lane:
                        CPU        GPU round trip  speed-up  against the CPU
  to_f32            6.22 ms    2.53 ms   17.64 ms      2.5 ×  exact
  bin2x2            3.25 ms    0.47 ms   10.35 ms      6.9 ×  exact
  downsample ×4     2.98 ms    0.36 ms    7.37 ms      8.3 ×  exact

uploading the 40 MB frame:
  from pinned    memory    1.64 ms   24.4 GB/s
  from ordinary  memory    2.97 ms   13.4 GB/s
```

**The columns:**

- **CPU:** `Compute`'s host method, in the live pool.
- **GPU:** the kernel, with its data already on the GPU. `to_f32`'s time includes uploading the 40 MB frame, since its input is a frame in host memory.
- **round trip:** uploading, the kernel, and downloading the result into ordinary memory.

**What to look for:**

- **The kernels** are several times faster on the GPU.
- **The round trips** are slower than the CPU, which is why `Compute`'s host methods stay on the CPU. Work that stays on the GPU goes through `Compute::cuda()`.
- **Pinned memory** uploads at the bus's full rate, about twice as fast as ordinary memory.

**Without the feature, or without a GPU,** it lists the backends, says why CUDA isn't in use, and stops:

```sh
cargo run -p ex-cuda-check                 # "not in this build"
```

**`--smoke`** runs everything three times, in a debug build too:

```sh
cargo run -p ex-cuda-check --features cuda -- --smoke
```

- **With a GPU,** it checks that every twin matches its CPU reference exactly, and that the pool's memory is pinned.
- **Without one,** there's nothing to check.

It ends with `smoke: ok`, or says what failed and exits with code 1. CI runs it without the feature, and the weekly *CUDA build* runs it with the feature and no GPU.
