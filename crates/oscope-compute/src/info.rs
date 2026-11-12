//! What the computer offers: the backends, the processor's threads and its
//! widest SIMD instructions.

use serde::{Deserialize, Serialize};

/// Where kernels run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum Backend {
    /// The processor, with rayon: always there, and the reference.
    Cpu,
    /// An NVIDIA GPU, with CUDA (P07).
    Cuda,
}

/// The widest SIMD instructions the processor has, of those the compiler
/// can use for x86_64.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SimdLevel {
    /// None that OScope recognises.
    Scalar,
    /// SSE2, which every x86_64 processor has.
    Sse2,
    /// SSE4.1.
    Sse41,
    /// AVX2 with FMA.
    Avx2,
    /// AVX-512 Foundation.
    Avx512,
}

impl SimdLevel {
    /// This processor's.
    pub fn detect() -> Self {
        #[cfg(target_arch = "x86_64")]
        {
            use std::arch::is_x86_feature_detected as has;
            if has!("avx512f") {
                Self::Avx512
            } else if has!("avx2") && has!("fma") {
                Self::Avx2
            } else if has!("sse4.1") {
                Self::Sse41
            } else {
                Self::Sse2
            }
        }
        #[cfg(not(target_arch = "x86_64"))]
        {
            Self::Scalar
        }
    }
}

/// A backend, and whether it can run here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackendInfo {
    /// Which.
    pub backend: Backend,
    /// Whether kernels can run on it here.
    pub available: bool,
    /// What it is, or why it isn't available.
    pub detail: String,
}

/// A GPU that CUDA can use.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GpuInfo {
    /// The driver's number for it.
    pub ordinal: usize,
    /// Its name, such as `NVIDIA RTX A2000 12GB`.
    pub name: String,
    /// Its compute capability: (8, 6) for Ampere.
    pub compute_capability: (i32, i32),
    /// Its memory.
    pub memory_bytes: u64,
    /// The newest CUDA version the driver supports, such as `13.2`.
    pub driver_cuda: String,
    /// Whether freed memory is kept in the device's pool for reuse.
    pub memory_pool: bool,
    /// Whether NVRTC, CUDA's run-time compiler, can be loaded.
    pub nvrtc: bool,
    /// Whether cuFFT can be loaded.
    pub cufft: bool,
}

/// What the compute layer found, and how it divided the processor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComputeInfo {
    /// Every backend, the one in use first.
    pub backends: Vec<BackendInfo>,
    /// The processor's logical threads.
    pub cpu_threads: usize,
    /// The widest SIMD instructions it has.
    pub simd: SimdLevel,
    /// The live pool's threads, for the preview and other work that can't
    /// wait.
    pub live_threads: usize,
    /// The batch pool's threads, for jobs.
    pub batch_threads: usize,
    /// The GPU, if CUDA can use one.
    pub gpu: Option<GpuInfo>,
}

impl ComputeInfo {
    /// The backend in use: the first available one.
    pub fn backend(&self) -> Backend {
        self.backends
            .iter()
            .find(|b| b.available)
            .map_or(Backend::Cpu, |b| b.backend)
    }
}
