# ADR-0011: Building, loading and redistributing CUDA kernels

- Status: Accepted
- Date: 2026-11-12
- Phase: P07

## Context

P07 adds CUDA to the compute layer (ADR-0010, D7).

**The machine:**
- **The owner's PC** has an NVIDIA RTX A2000 12 GB, with compute capability 8.6 and driver 596.51, which supports CUDA 13.2. The target host is similar.
- **CI's runners** have no GPU and no CUDA toolkit.

**The tools:**
- **`cudarc` 0.19.10** supports CUDA up to 13.4, and can load the CUDA libraries at run time instead of linking them.
- **The toolkit:** CUDA 13.4 was installed with `winget`; its `nvcc` is V13.4.59.

**What building P07 found:**
- **PTX from CUDA 13.4 is refused by a CUDA 13.2 driver,** with `CUDA_ERROR_UNSUPPORTED_PTX_VERSION`. The driver can compile PTX only from a toolkit no newer than itself. Machine code (SASS) has no such limit within a major version.
- **On Windows, `nvcc` needs MSVC's `cl.exe`,** even to compile device code. It isn't on `PATH` outside a developer prompt.
- **`cudarc` doesn't wait after a copy from a borrowed slice.** From pinned memory, that copy is truly asynchronous: the frame pool could hand the slab to the next frame while the GPU still reads it.
- **For P06's memory-bound kernels,** a call from host memory to host memory costs more on the GPU than on the CPU, once the copies are counted:

  | 20 MP frame, release build | CPU (6 threads) | GPU, data already there | Host → GPU → host |
  |---|---|---|---|
  | `to_f32` | 6.2 ms | 2.5 ms, with the upload | 17.6 ms |
  | `bin2x2` | 3.3 ms | 0.47 ms | 10.4 ms |
  | `downsample` ×4 | 3.0 ms | 0.36 ms | 7.4 ms |

## Decision

1. **CUDA is the `oscope-compute` feature `cuda`,** off by default.
   - **Loading:** `cudarc` loads the driver, NVRTC and cuFFT when the program runs, and nothing is linked. A build with the feature runs on machines without a GPU.
2. **The toolkit is CUDA 13.4** (`cudarc`'s `cuda-13040`). It is needed to build with the feature.
   - **The driver** must support CUDA 13, which means driver 580 or newer. Any 13.x driver runs the kernels.
3. **The kernels are CUDA C in `kernels/*.cu`.** `build.rs` compiles them into a fat binary that the crate embeds:
   - **The command** is `nvcc --fatbin -arch=all-major -O3`, giving SASS for each major architecture CUDA 13 supports (sm_75, 80, 90, 100, 110 and 120) and PTX for the newest.
   - **Why a fat binary, not the plan's PTX:** SASS runs on any CUDA 13 driver, and the PTX lets a GPU newer than the toolkit compile the kernels.
   - **Finding `nvcc`:** from `NVCC`, then `CUDA_PATH`, then `PATH`. Without it, the build fails, saying to install CUDA 13.4 or build without the feature.
   - **On Windows,** `build.rs` finds `cl.exe` with `find-msvc-tools`, as the `cc` crate does, and passes it to `nvcc` with the environment it needs.
4. **`Compute::new` opens the GPU `cuda_device` (0 by default).**
   - **If it can't,** for want of a driver, a GPU or the kernels, it uses the CPU, and `ComputeInfo` says why.
   - **The warning** is logged once in the process's life.
5. **Each lane has a CUDA stream.** The live stream has the GPU's highest priority, and the batch stream its lowest.
6. **Memory:**
   - **Allocations** come from the device's default memory pool, in stream order. Freed memory stays in the pool for the next allocation, since the release threshold is set to the maximum.
   - **Every upload from host memory waits until its copy is done,** whether from pinned or ordinary memory, before it returns.
   - **Downloads** copy into ordinary vectors.
7. **Pinned memory for frames:** `Cuda::pinned_allocator()` is P03's `SlabAllocator`.
   - **The flags:** page-locked, portable, and not write-combined, since the CPU reads frames too.
   - **If pinning fails,** it warns and gives ordinary memory.
   - **The gain:** a 40 MB upload takes 1.64 ms from pinned memory (24.4 GB/s, PCIe 4 × 16), against 2.97 ms from ordinary memory.
8. **Where kernels run.** This refines ADR-0010's first decision with the measurements above:
   - **`Compute`'s host methods** (`Image` in, `Image` out) run on the CPU.
   - **Work that stays on the GPU** goes through `Compute::cuda()`. Its kernels take and give `DeviceImage`s, and `upload` and `download` cross between host and device.
   - **The pipeline engine (P21)** keeps data on the GPU between CUDA nodes this way.
9. **The CUDA twins match their CPU references to the bit.** They add in the same order, and CUDA's division is IEEE-rounded unless fast maths is asked for, which it isn't. So their tests use `Tolerance::EXACT`, and a twin that rounds differently fails.
   - **The GPU tests** run with `OSCOPE_CUDA=1`.
   - **The fallback test** runs wherever the feature is on.
10. **CI:** a workflow, *CUDA build*, runs weekly, by hand, and on pull requests that change the compute crate. It installs `cuda-nvcc-13-4` on Ubuntu, then runs, with the feature, clippy, the tests (the GPU ones skip themselves) and the example. The main CI doesn't build the feature.
11. **Redistribution.**
    - **What OScope's binaries contain:** only OScope's code, the kernels included, compiled by `nvcc`. At run time they load NVIDIA's libraries from the system: the driver's (`nvcuda.dll`, `libcuda.so`), and later NVRTC (`nvrtc64_130_0.dll`) and cuFFT (`cufft64_12.dll`), from the toolkit.
    - **What the EULA allows:** CUDA 13.4.1's EULA (`EULA.txt`, Attachment A) lists the CUDA runtime, cuFFT and NVRTC libraries as distributable. They may be shipped only inside an application, under its §1.1.2 distribution requirements.
    - **When that is decided:** whether OScope's installers bundle them is decided when packaging, against ADR-0002. Until then, they come with the NVIDIA driver and toolkit.

## Consequences

- **One build serves both kinds of machine.** Without a GPU, it says why, and computes on the CPU.
- **Building with the feature needs the CUDA toolkit**, and on Windows, MSVC. The main CI and anyone without the toolkit build without it.
- **The fat binary** makes the build slower, by a few seconds, and the library larger: 173 KB for the four kernels.
- **Upgrading CUDA** means a new `cudarc` feature (`cuda-13050` and so on), and a check that drivers of the same major version still run the SASS.
- **Waiting for every upload** costs overlap between copying and computing. If the preview (P17) needs that overlap, it can keep frames alive until their copy's event, using `cudarc`'s pinned slices directly.
- **Host calls never use the GPU,** even where it would be faster, so a caller that wants the GPU must use the device API.

## Alternatives considered

- **PTX only, as the plan said.** A driver older than the toolkit refuses it, which already happened here.
- **NVRTC at run time,** compiling the CUDA C source when the program starts. That needs NVRTC on every machine, and puts compile errors at run time. NVRTC stays available for kernels that must be generated, such as P21's fused nodes.
- **Linking the CUDA libraries at build time.** The program wouldn't start without them, and the main CI couldn't build it.
- **`Compute`'s host methods on the GPU whenever there is one.** The round trips above make that slower than the CPU for these kernels.
- **Write-combined pinned memory** (`cudarc`'s default). The GPU reads it as fast, but the CPU reads it many times slower, and the recorder and the statistics read frames on the CPU.
