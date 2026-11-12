//! The CUDA backend (ADR-0011): the GPU, a stream for each lane, its memory
//! pool, pinned host memory, and the CUDA twins of the CPU kernels.
//!
//! The driver is loaded when the program runs, not linked, so a build with
//! the `cuda` feature also runs where there is no GPU: [`Cuda::open`] then
//! says why, and `Compute` uses the CPU.

use std::sync::Arc;

use cudarc::driver::{
    CudaContext, CudaFunction, CudaSlice, CudaStream, DeviceRepr, DriverError, LaunchConfig,
    PinnedHostSlice, PushKernelArg, result, sys,
};
use cudarc::nvrtc::Ptx;
use oscope_core::{FrameView, HeapAllocator, Slab, SlabAllocator};

use crate::{ComputeError, GpuInfo, Image, Lane, Shape, cpu};

mod color;

pub use color::DemosaicKernel;

/// The image kernels, compiled from `kernels/image.cu` by `build.rs`.
const IMAGE_KERNELS: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/image.fatbin"));

/// The colour kernels, compiled from `kernels/color.cu`.
const COLOR_KERNELS: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/color.fatbin"));

/// Threads in a block.
const BLOCK: u32 = 256;

impl From<DriverError> for ComputeError {
    fn from(error: DriverError) -> Self {
        Self::Cuda(error.to_string())
    }
}

/// An image in GPU memory.
#[derive(Debug)]
pub struct DeviceImage {
    shape: Shape,
    data: CudaSlice<f32>,
}

impl DeviceImage {
    /// Its shape.
    pub fn shape(&self) -> Shape {
        self.shape
    }
}

/// The kernels, loaded on the GPU.
#[derive(Debug)]
struct Kernels {
    to_f32_u16: CudaFunction,
    to_f32_u8: CudaFunction,
    bin2x2: CudaFunction,
    downsample: CudaFunction,
    superpixel: CudaFunction,
    demosaic: CudaFunction,
    subtract_black: CudaFunction,
    correct: CudaFunction,
    display: CudaFunction,
}

/// A GPU, ready for kernels.
#[derive(Debug)]
pub struct Cuda {
    context: Arc<CudaContext>,
    live: Arc<CudaStream>,
    batch: Arc<CudaStream>,
    kernels: Kernels,
    info: GpuInfo,
}

impl Cuda {
    /// Opens GPU `ordinal`, or says why it can't.
    pub fn open(ordinal: usize) -> Result<Self, ComputeError> {
        // SAFETY: this only tries to load the driver's library.
        if !unsafe { sys::is_culib_present() } {
            return Err(ComputeError::Cuda("no CUDA driver was found".to_owned()));
        }
        let count = CudaContext::device_count()?;
        if ordinal >= count as usize {
            let found = format!("the driver sees {count} GPUs");
            return Err(ComputeError::Cuda(format!(
                "there is no GPU {ordinal}: {found}"
            )));
        }
        let context = CudaContext::new(ordinal)?;
        // Live work gets the GPU's highest priority, batch work its lowest.
        let (least, greatest) = result::stream::get_priority_range()?;
        let live = context.new_stream_with_priority(greatest)?;
        let batch = context.new_stream_with_priority(least)?;
        let module = context.load_module(Ptx::from_binary(IMAGE_KERNELS.to_vec()))?;
        let color = context.load_module(Ptx::from_binary(COLOR_KERNELS.to_vec()))?;
        let kernels = Kernels {
            to_f32_u16: module.load_function("to_f32_u16")?,
            to_f32_u8: module.load_function("to_f32_u8")?,
            bin2x2: module.load_function("bin2x2")?,
            downsample: module.load_function("downsample")?,
            superpixel: color.load_function("superpixel")?,
            demosaic: color.load_function("demosaic")?,
            subtract_black: color.load_function("subtract_black")?,
            correct: color.load_function("correct")?,
            display: color.load_function("display")?,
        };
        let memory_pool = keep_freed_memory(&context).is_ok();
        let mut version = 0;
        // SAFETY: the driver is loaded, and writes one int.
        unsafe { sys::cuDriverGetVersion(&mut version) }.result()?;
        let info = GpuInfo {
            ordinal,
            name: context.name()?,
            compute_capability: context.compute_capability()?,
            memory_bytes: context.total_mem()? as u64,
            driver_cuda: format!("{}.{}", version / 1000, version % 1000 / 10),
            memory_pool,
            // SAFETY: these only try to load the libraries.
            nvrtc: unsafe { cudarc::nvrtc::sys::is_culib_present() },
            // SAFETY: as above.
            cufft: unsafe { cudarc::cufft::sys::is_culib_present() },
        };
        Ok(Self {
            context,
            live,
            batch,
            kernels,
            info,
        })
    }

    /// What it is.
    pub fn info(&self) -> &GpuInfo {
        &self.info
    }

    fn stream(&self, lane: Lane) -> &Arc<CudaStream> {
        match lane {
            Lane::Live => &self.live,
            Lane::Batch => &self.batch,
        }
    }

    /// Waits until the lane's work on the GPU is done.
    pub fn synchronize(&self, lane: Lane) -> Result<(), ComputeError> {
        Ok(self.stream(lane).synchronize()?)
    }

    /// Copies `data` to the GPU, and waits until the copy is done. From
    /// pinned memory a copy runs while the program goes on, and `data` is
    /// only borrowed: the frame pool may give its slab to the next frame as
    /// soon as this returns.
    fn copy_up<T: DeviceRepr>(&self, lane: Lane, data: &[T]) -> Result<CudaSlice<T>, ComputeError> {
        let stream = self.stream(lane);
        let copy = stream.clone_htod(data)?;
        stream.synchronize()?;
        Ok(copy)
    }

    /// Copies bytes to the GPU, and waits: the time an upload takes.
    pub fn upload_bytes(&self, lane: Lane, bytes: &[u8]) -> Result<CudaSlice<u8>, ComputeError> {
        self.copy_up(lane, bytes)
    }

    /// Copies bytes back from the GPU, once the lane's work on them is done.
    pub fn download_bytes(
        &self,
        lane: Lane,
        bytes: &CudaSlice<u8>,
    ) -> Result<Vec<u8>, ComputeError> {
        Ok(self.stream(lane).clone_dtoh(bytes)?)
    }

    /// Copies an image to the GPU.
    pub fn upload(&self, lane: Lane, image: &Image) -> Result<DeviceImage, ComputeError> {
        let data = self.copy_up(lane, image.data())?;
        Ok(DeviceImage {
            shape: image.shape(),
            data,
        })
    }

    /// Copies an image back, once the lane's work on it is done.
    pub fn download(&self, lane: Lane, image: &DeviceImage) -> Result<Image, ComputeError> {
        if image.shape.is_empty() {
            return Ok(Image::zeros(image.shape));
        }
        let mut data = self.stream(lane).clone_dtoh(&image.data)?;
        data.truncate(image.shape.len());
        Image::from_vec(image.shape, data)
    }

    /// Memory for an image of `shape`, at least one sample.
    fn alloc(&self, lane: Lane, shape: Shape) -> Result<CudaSlice<f32>, ComputeError> {
        Ok(self.stream(lane).alloc_zeros::<f32>(shape.len().max(1))?)
    }

    /// [`cpu::to_f32`]'s twin: copies the frame to the GPU, and converts it
    /// there.
    pub fn to_f32(&self, lane: Lane, view: &FrameView<'_>) -> Result<DeviceImage, ComputeError> {
        let format = view.format();
        if !cpu::converts(format) {
            return Err(ComputeError::Unsupported(format));
        }
        let shape = Shape::new(view.width(), view.height(), format.channels());
        let mut out = self.alloc(lane, shape)?;
        if !shape.is_empty() {
            let bytes = self.copy_up(lane, view.data())?;
            let stream = self.stream(lane);
            let (function, stride) = match format.sample_bytes() {
                2 => (&self.kernels.to_f32_u16, view.stride() as u64 / 2),
                _ => (&self.kernels.to_f32_u8, view.stride() as u64),
            };
            let (row_len, height) = (shape.row_len() as u32, shape.height);
            let mut launch = stream.launch_builder(function);
            launch
                .arg(&bytes)
                .arg(&stride)
                .arg(&row_len)
                .arg(&height)
                .arg(&mut out);
            // SAFETY: the arguments are the kernel's, in kernels/image.cu:
            // `bytes` holds `height` rows `stride` samples apart, the last at
            // least `row_len` long, and `out` holds `height` rows of `row_len`
            // floats. Each thread writes its own samples.
            unsafe { launch.launch(grid(row_len, height)) }?;
        }
        Ok(DeviceImage { shape, data: out })
    }

    /// [`cpu::bin2x2`]'s twin.
    pub fn bin2x2(&self, lane: Lane, image: &DeviceImage) -> Result<DeviceImage, ComputeError> {
        let input = image.shape;
        let shape = Shape::new(input.width / 2, input.height / 2, input.channels);
        let mut out = self.alloc(lane, shape)?;
        if !shape.is_empty() {
            let (in_width, channels) = (input.width, input.channels as u32);
            let (out_width, out_height) = (shape.width, shape.height);
            let mut launch = self.stream(lane).launch_builder(&self.kernels.bin2x2);
            launch.arg(&image.data).arg(&in_width).arg(&channels);
            launch.arg(&mut out).arg(&out_width).arg(&out_height);
            // SAFETY: the arguments are the kernel's: the input holds
            // `in_width` × 2 `out_height` pixels of `channels`, and `out`
            // holds `out_width` × `out_height`. Each thread writes its own.
            unsafe { launch.launch(grid(shape.row_len() as u32, out_height)) }?;
        }
        Ok(DeviceImage { shape, data: out })
    }

    /// [`cpu::downsample`]'s twin.
    pub fn downsample(
        &self,
        lane: Lane,
        image: &DeviceImage,
        factor: u32,
    ) -> Result<DeviceImage, ComputeError> {
        if factor == 0 {
            return Err(ComputeError::Factor);
        }
        let input = image.shape;
        let (out_width, out_height) = (input.width.div_ceil(factor), input.height.div_ceil(factor));
        let shape = Shape::new(out_width, out_height, input.channels);
        let mut out = self.alloc(lane, shape)?;
        if !shape.is_empty() {
            let (width, height, channels) = (input.width, input.height, input.channels as u32);
            let mut launch = self.stream(lane).launch_builder(&self.kernels.downsample);
            launch
                .arg(&image.data)
                .arg(&width)
                .arg(&height)
                .arg(&channels)
                .arg(&factor);
            launch.arg(&mut out).arg(&out_width).arg(&out_height);
            // SAFETY: the arguments are the kernel's: the input holds `width`
            // × `height` pixels of `channels`, and `out` holds `out_width` ×
            // `out_height`. Each thread writes its own samples.
            unsafe { launch.launch(grid(shape.row_len() as u32, out_height)) }?;
        }
        Ok(DeviceImage { shape, data: out })
    }

    /// An allocator of pinned host memory, for the frame pool: the GPU
    /// copies from it at the bus's full rate, without staging.
    pub fn pinned_allocator(&self) -> PinnedAllocator {
        PinnedAllocator {
            context: self.context.clone(),
        }
    }

    /// Whether `bytes` are pinned host memory that the GPU can copy from
    /// directly.
    pub fn is_pinned(&self, bytes: &[u8]) -> bool {
        let mut kind = 0_u32;
        let attribute = sys::CUpointer_attribute::CU_POINTER_ATTRIBUTE_MEMORY_TYPE;
        // SAFETY: the driver writes one unsigned int, and only looks the
        // pointer up; it doesn't read the memory.
        let found = self.context.bind_to_thread().is_ok()
            && unsafe {
                sys::cuPointerGetAttribute((&raw mut kind).cast(), attribute, bytes.as_ptr() as u64)
            }
            .result()
            .is_ok();
        found && kind == sys::CUmemorytype::CU_MEMORYTYPE_HOST as u32
    }
}

/// A launch for rows of `row_len` samples: a thread for each sample, and up
/// to 65,535 rows of blocks, each stepping down until the last row.
fn grid(row_len: u32, rows: u32) -> LaunchConfig {
    LaunchConfig {
        grid_dim: (row_len.div_ceil(BLOCK), rows.clamp(1, 65_535), 1),
        block_dim: (BLOCK, 1, 1),
        shared_mem_bytes: 0,
    }
}

/// Keeps memory that kernels free in the device's pool for the next
/// allocation, rather than giving it back to the driver at each sync.
fn keep_freed_memory(context: &CudaContext) -> Result<(), DriverError> {
    let attribute = sys::CUmemPool_attribute::CU_MEMPOOL_ATTR_RELEASE_THRESHOLD;
    let mut keep = u64::MAX;
    // SAFETY: the device comes from the context, and the release threshold
    // takes a u64.
    unsafe {
        let pool = result::device::get_default_mem_pool(context.cu_device())?;
        result::mem_pool::set_attribute(pool, attribute, (&raw mut keep).cast())
    }
}

/// Pinned host memory for the frame pool (P03's [`SlabAllocator`]). If
/// pinning fails, it warns and gives ordinary memory.
#[derive(Debug, Clone)]
pub struct PinnedAllocator {
    context: Arc<CudaContext>,
}

/// A slab of pinned memory, and where it is.
struct PinnedSlab {
    _memory: PinnedHostSlice<u8>,
    start: *mut u8,
    len: usize,
}

// SAFETY: the slab owns its memory, and hands it out only through `&self`
// and `&mut self`, so sharing or moving it is as safe as for a `Box<[u8]>`.
unsafe impl Send for PinnedSlab {}
// SAFETY: as above.
unsafe impl Sync for PinnedSlab {}

impl Slab for PinnedSlab {
    fn bytes(&self) -> &[u8] {
        // SAFETY: `start` points at `len` bytes, zeroed when allocated, that
        // the slab owns for as long as it lives.
        unsafe { std::slice::from_raw_parts(self.start, self.len) }
    }

    fn bytes_mut(&mut self) -> &mut [u8] {
        // SAFETY: as above, and `&mut self` makes the borrow unique.
        unsafe { std::slice::from_raw_parts_mut(self.start, self.len) }
    }
}

impl PinnedAllocator {
    fn pinned(&self, len: usize) -> Result<PinnedSlab, DriverError> {
        // Portable, and not write-combined, which the CPU reads slowly.
        let flags = sys::CU_MEMHOSTALLOC_PORTABLE;
        // SAFETY: the memory starts uninitialised, and is zeroed next,
        // before anything reads it.
        let mut memory = unsafe { self.context.alloc_pinned_with_flags::<u8>(len, flags) }?;
        memory.as_mut_slice()?.fill(0);
        let start = memory.as_mut_ptr()?;
        Ok(PinnedSlab {
            _memory: memory,
            start,
            len,
        })
    }
}

impl SlabAllocator for PinnedAllocator {
    fn allocate(&self, len: usize) -> Box<dyn Slab> {
        match self.pinned(len) {
            Ok(slab) => Box::new(slab),
            Err(error) => {
                tracing::warn!(
                    "pinned memory isn't available, so a frame uses ordinary memory: {error}"
                );
                HeapAllocator.allocate(len)
            }
        }
    }
}
