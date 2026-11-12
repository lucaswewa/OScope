//! Compute backends: the CPU, and CUDA when a GPU is present (P07). Device
//! buffers, pinned host memory, streams, and the kernel build.
//!
//! Layer L1 of ADR-0005. Unsafe code is allowed here, for CUDA (plan
//! §4.3). ADR-0010 records the design:
//!
//! - **[`Compute`]** is what algorithms call. It runs each kernel on the
//!   backend in use, in the [`Lane`] it is asked for.
//! - **The CPU backend** ([`cpu`]) is rayon, and the reference that every
//!   CUDA kernel is compared with ([`equivalence`]).
//! - **Two pools:** live work, such as the preview, has threads of its own,
//!   so that batch jobs can't hold it up ([`Pools`]).
//!
//! # Benchmarks
//!
//! Benchmarks use criterion, in `benches/`, one file for each area, such as
//! `benches/kernels.rs`:
//!
//! - **Inputs are the real size:** a full 20 MP MIChrome 20 frame, 5472 ×
//!   3648.
//! - **Throughput is in input bytes,** so that criterion reports GB/s.
//! - **Kernels run in the live lane,** through [`Compute`], as the preview
//!   calls them.
//! - **Run them** with `cargo bench -p oscope-compute`, or `-- bin2x2` for
//!   one. CI compiles them but doesn't run them.

pub mod cpu;
#[cfg(feature = "cuda")]
pub mod cuda;
pub mod equivalence;
mod image;
mod info;
mod pools;

pub use image::{Image, Shape};
pub use info::{Backend, BackendInfo, ComputeInfo, GpuInfo, SimdLevel};
pub use pools::{Lane, Pools};

use oscope_core::frame::{FrameView, PixelFormat};

/// What the compute layer can't do.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum ComputeError {
    /// The kernel doesn't take frames of this format.
    #[error("the kernel doesn't take {0:?} frames")]
    Unsupported(PixelFormat),
    /// A buffer has the wrong number of samples.
    #[error("{got} samples where the shape has {expected}")]
    Size {
        /// The shape's.
        expected: usize,
        /// The buffer's.
        got: usize,
    },
    /// Two images that should match have different shapes.
    #[error("an image of {got:?} where {expected:?} was expected")]
    Shape {
        /// The first image's.
        expected: Shape,
        /// The second's.
        got: Shape,
    },
    /// A downsampling factor of 0.
    #[error("the factor must be at least 1")]
    Factor,
    /// A thread pool couldn't start.
    #[error("a thread pool couldn't start: {0}")]
    Pool(String),
    /// CUDA failed, or isn't available.
    #[error("CUDA: {0}")]
    Cuda(String),
}

/// How to divide the processor between the pools.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ComputeConfig {
    /// The live pool's threads: by default a quarter of the processor's,
    /// and at least 2.
    pub live_threads: Option<usize>,
    /// The batch pool's threads: by default the rest, and at least 1.
    pub batch_threads: Option<usize>,
    /// The GPU for CUDA, by the driver's number: 0 by default.
    pub cuda_device: Option<usize>,
}

/// The compute layer: its backends, its pools, and the kernels.
#[derive(Debug)]
pub struct Compute {
    info: ComputeInfo,
    pools: Pools,
    #[cfg(feature = "cuda")]
    cuda: Option<cuda::Cuda>,
}

/// Opens the GPU if the build has CUDA; otherwise, or if that fails, why
/// not. A failure is logged once in the process's life.
fn open_gpu(config: &ComputeConfig) -> (Option<GpuInfo>, String, Option<Gpu>) {
    #[cfg(feature = "cuda")]
    {
        match cuda::Cuda::open(config.cuda_device.unwrap_or(0)) {
            Ok(gpu) => {
                let info = gpu.info().clone();
                let detail = format!(
                    "{}, compute {}.{}, {} MB; the driver supports CUDA {}",
                    info.name,
                    info.compute_capability.0,
                    info.compute_capability.1,
                    info.memory_bytes >> 20,
                    info.driver_cuda
                );
                (Some(info), detail, Some(gpu))
            }
            Err(error) => {
                static WARNED: std::sync::Once = std::sync::Once::new();
                WARNED.call_once(|| {
                    tracing::warn!("CUDA isn't available, so OScope computes on the CPU: {error}");
                });
                (None, error.to_string(), None)
            }
        }
    }
    #[cfg(not(feature = "cuda"))]
    {
        let _ = config;
        let detail = "not in this build: build with the `cuda` feature".to_owned();
        (None, detail, None)
    }
}

/// The GPU, if the build has CUDA.
#[cfg(feature = "cuda")]
type Gpu = cuda::Cuda;
/// No GPU, in a build without CUDA.
#[cfg(not(feature = "cuda"))]
type Gpu = std::convert::Infallible;

impl Compute {
    /// Finds the backends and starts the pools.
    pub fn new(config: &ComputeConfig) -> Result<Self, ComputeError> {
        let cpu_threads = std::thread::available_parallelism().map_or(1, |n| n.get());
        let live = config
            .live_threads
            .unwrap_or((cpu_threads / 4).max(2))
            .max(1);
        let batch = config
            .batch_threads
            .unwrap_or(cpu_threads.saturating_sub(live))
            .max(1);
        let simd = SimdLevel::detect();
        let (gpu, detail, _opened) = open_gpu(config);
        let mut backends = vec![
            BackendInfo {
                backend: Backend::Cpu,
                available: true,
                detail: format!("rayon: {live} live and {batch} batch threads, {simd:?}"),
            },
            BackendInfo {
                backend: Backend::Cuda,
                available: gpu.is_some(),
                detail,
            },
        ];
        // The one in use first.
        backends.sort_by_key(|backend| !backend.available || backend.backend == Backend::Cpu);
        Ok(Self {
            info: ComputeInfo {
                backends,
                cpu_threads,
                simd,
                live_threads: live,
                batch_threads: batch,
                gpu,
            },
            pools: Pools::new(live, batch)?,
            #[cfg(feature = "cuda")]
            cuda: _opened,
        })
    }

    /// What it found.
    pub fn info(&self) -> &ComputeInfo {
        &self.info
    }

    /// The pools, for work of other kinds.
    pub fn pools(&self) -> &Pools {
        &self.pools
    }

    /// The GPU, for work that stays on it: its kernels take and give
    /// [`cuda::DeviceImage`]s (ADR-0011).
    #[cfg(feature = "cuda")]
    pub fn cuda(&self) -> Option<&cuda::Cuda> {
        self.cuda.as_ref()
    }

    /// [`cpu::to_f32`], in `lane`.
    pub fn to_f32(&self, lane: Lane, view: &FrameView<'_>) -> Result<Image, ComputeError> {
        self.pools.install(lane, || cpu::to_f32(view))
    }

    /// [`cpu::bin2x2`], in `lane`.
    pub fn bin2x2(&self, lane: Lane, image: &Image) -> Image {
        self.pools.install(lane, || cpu::bin2x2(image))
    }

    /// [`cpu::downsample`], in `lane`.
    pub fn downsample(
        &self,
        lane: Lane,
        image: &Image,
        factor: u32,
    ) -> Result<Image, ComputeError> {
        self.pools.install(lane, || cpu::downsample(image, factor))
    }
}
