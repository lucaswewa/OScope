//! Frames: pixels in pooled memory, with their metadata (plan §5.5,
//! ADR-0008).
//!
//! A producer takes a [`FrameBuffer`] from a [`FramePool`], fills it, and
//! seals it into a [`Frame`] with its [`FrameMeta`]. Frames are shared, never
//! copied: cloning one shares its pixels, and the slab goes back to the pool
//! when the last clone is dropped. [`FrameView`]s look at a frame, or at a
//! rectangle of it, without copying.

mod format;
mod meta;
mod pool;
mod view;

use std::fmt;
use std::sync::Arc;

pub use format::{BayerColour, BayerPattern, PixelFormat};
pub use meta::{FrameMeta, LightState};
pub use pool::{FrameBuffer, FramePool, HeapAllocator, PoolStats, Slab, SlabAllocator};
pub use view::FrameView;

use crate::error::{Error, Result};

struct Inner {
    buffer: FrameBuffer,
    meta: FrameMeta,
}

/// A frame: its pixels, in a slab from a pool, and its metadata. Cloning it
/// shares both.
#[derive(Clone)]
pub struct Frame {
    inner: Arc<Inner>,
}

impl Frame {
    /// Seals a filled buffer with its metadata. The buffer must hold the
    /// frame's format and size.
    pub fn new(buffer: FrameBuffer, meta: FrameMeta) -> Result<Frame> {
        let needed = meta.format.frame_bytes(meta.width, meta.height);
        if buffer.bytes().len() < needed {
            return Err(Error::BufferTooSmall {
                needed,
                got: buffer.bytes().len(),
            });
        }
        Ok(Frame {
            inner: Arc::new(Inner { buffer, meta }),
        })
    }

    /// Its metadata.
    pub fn meta(&self) -> &FrameMeta {
        &self.inner.meta
    }

    /// Its pixels, tightly packed: exactly the bytes its format and size need.
    pub fn bytes(&self) -> &[u8] {
        let meta = &self.inner.meta;
        &self.inner.buffer.bytes()[..meta.format.frame_bytes(meta.width, meta.height)]
    }

    /// A view of plane `index`: of the frame itself, for a format with one
    /// plane. `None` past the last plane.
    pub fn plane(&self, index: usize) -> Option<FrameView<'_>> {
        let meta = &self.inner.meta;
        let len = meta.format.frame_bytes(meta.width, meta.height) / meta.format.planes();
        let bytes = self.bytes().get(index * len..(index + 1) * len)?;
        let format = match meta.format {
            PixelFormat::PlanarF32 { .. } => PixelFormat::PlanarF32 { planes: 1 },
            other => other,
        };
        FrameView::new(bytes, format, meta.width, meta.height).ok()
    }

    /// A view of all its pixels: of its first plane, for a planar format.
    pub fn view(&self) -> FrameView<'_> {
        self.plane(0).expect("a sealed frame holds its first plane")
    }

    /// Whether two frames share their pixels, as clones of one frame do.
    pub fn same_pixels(&self, other: &Frame) -> bool {
        Arc::ptr_eq(&self.inner, &other.inner)
    }
}

impl fmt::Debug for Frame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let meta = &self.inner.meta;
        f.debug_struct("Frame")
            .field("id", &meta.id)
            .field("format", &meta.format)
            .field("size", &(meta.width, meta.height))
            .finish_non_exhaustive()
    }
}
