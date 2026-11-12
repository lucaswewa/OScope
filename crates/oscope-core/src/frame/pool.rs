//! The frame pool: a fixed set of equal slabs of memory, reused frame after
//! frame (plan §5.5, ADR-0008).

use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use schemars::JsonSchema;
use serde::Serialize;

/// The memory for one frame.
pub trait Slab: Send + Sync {
    /// Its bytes.
    fn bytes(&self) -> &[u8];
    /// Its bytes, to fill.
    fn bytes_mut(&mut self) -> &mut [u8];
}

/// Where a pool's slabs come from: the heap, or pinned memory once CUDA is in
/// (P07).
pub trait SlabAllocator {
    /// A slab of `len` bytes.
    fn allocate(&self, len: usize) -> Box<dyn Slab>;
}

/// Slabs on the heap.
#[derive(Clone, Copy, Debug, Default)]
pub struct HeapAllocator;

struct HeapSlab(Box<[u8]>);

impl Slab for HeapSlab {
    fn bytes(&self) -> &[u8] {
        &self.0
    }

    fn bytes_mut(&mut self) -> &mut [u8] {
        &mut self.0
    }
}

impl SlabAllocator for HeapAllocator {
    fn allocate(&self, len: usize) -> Box<dyn Slab> {
        Box::new(HeapSlab(vec![0; len].into_boxed_slice()))
    }
}

struct Shared {
    slab_len: usize,
    slabs: usize,
    free: Mutex<Vec<Box<dyn Slab>>>,
    exhausted: AtomicU64,
}

impl Shared {
    fn free(&self) -> MutexGuard<'_, Vec<Box<dyn Slab>>> {
        self.free.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// A fixed set of equal slabs, all allocated when the pool is made. Cloning
/// the pool shares them.
#[derive(Clone)]
pub struct FramePool {
    shared: Arc<Shared>,
}

/// How a pool is being used.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct PoolStats {
    /// How many slabs it has.
    pub slabs: usize,
    /// The bytes in each slab.
    pub slab_len: usize,
    /// How many are taken now.
    pub in_use: usize,
    /// How many times a slab was asked for when none was free.
    pub exhausted: u64,
}

impl FramePool {
    /// A pool of `slabs` slabs of `slab_len` bytes each, from `allocator`.
    pub fn new(slab_len: usize, slabs: usize, allocator: &dyn SlabAllocator) -> Self {
        let free = (0..slabs).map(|_| allocator.allocate(slab_len)).collect();
        Self {
            shared: Arc::new(Shared {
                slab_len,
                slabs,
                free: Mutex::new(free),
                exhausted: AtomicU64::new(0),
            }),
        }
    }

    /// A free slab to fill, or `None` if every slab is in use. The frame is
    /// then lost: the pool counts it, and the producer goes on with the next.
    pub fn take(&self) -> Option<FrameBuffer> {
        let slab = self.shared.free().pop();
        if slab.is_none() {
            self.shared.exhausted.fetch_add(1, Ordering::Relaxed);
        }
        slab.map(|slab| FrameBuffer {
            slab: Some(slab),
            pool: Arc::clone(&self.shared),
        })
    }

    /// How the pool is being used.
    pub fn stats(&self) -> PoolStats {
        PoolStats {
            slabs: self.shared.slabs,
            slab_len: self.shared.slab_len,
            in_use: self.shared.slabs - self.shared.free().len(),
            exhausted: self.shared.exhausted.load(Ordering::Relaxed),
        }
    }
}

/// A slab taken from a pool, to fill. It goes back to the pool when it is
/// dropped; or, once it is sealed into a frame, when the frame's last clone
/// is.
pub struct FrameBuffer {
    slab: Option<Box<dyn Slab>>,
    pool: Arc<Shared>,
}

impl FrameBuffer {
    /// Its bytes.
    pub fn bytes(&self) -> &[u8] {
        self.slab.as_deref().map_or(&[], Slab::bytes)
    }

    /// Its bytes, to fill.
    pub fn bytes_mut(&mut self) -> &mut [u8] {
        self.slab.as_deref_mut().map_or(&mut [], Slab::bytes_mut)
    }
}

impl Drop for FrameBuffer {
    fn drop(&mut self) {
        if let Some(slab) = self.slab.take() {
            self.pool.free().push(slab);
        }
    }
}

impl fmt::Debug for FrameBuffer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FrameBuffer")
            .field("len", &self.bytes().len())
            .finish()
    }
}
