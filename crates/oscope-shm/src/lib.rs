//! Named shared memory, and the layout of the frame ring, on Windows and Linux.
//!
//! Layer L1 of ADR-0005.
//! Unsafe code is allowed here, for shared memory (plan §4.3).
//!
//! The desktop app's fast path (D21, ADR-0015):
//!
//! - **[`RingWriter`] and [`RingReader`]:** the preview ring, a header and a
//!   number of slots in named shared memory, each slot a message behind a
//!   seqlock. One process writes; others read the newest message without
//!   locks, and never a torn one.
//! - **[`write_private`]:** a file only the account the process runs as can
//!   read, for the local token.
//!
//! And a Python worker's frames (ADR-0020): **[`Segment`],** a block of
//! named shared memory another process maps by name.

mod map;
mod private;
mod ring;
mod segment;

pub use private::write_private;
pub use ring::{Read, RingConfig, RingReader, RingWriter};
pub use segment::Segment;

/// What shared memory can't do.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ShmError {
    /// A name that isn't 1 to 64 letters, digits, `-` and `_`.
    #[error("{0:?} isn't a name for shared memory: 1 to 64 letters, digits, - and _")]
    Name(String),
    /// A ring needs at least 2 slots, of at least a byte.
    #[error("a ring of {} slots of {} bytes: at least 2 of 1", .0.slots, .0.slot_bytes)]
    Config(RingConfig),
    /// The shared memory couldn't be made or opened.
    #[error("the shared memory {name}: {source}")]
    Io {
        /// Its name.
        name: String,
        /// Why.
        source: std::io::Error,
    },
    /// The shared memory isn't a ring.
    #[error("{0} isn't a preview ring")]
    NotARing(String),
    /// A ring of a layout this crate doesn't know.
    #[error("a ring of version {0}, where 1 is known")]
    Version(u64),
    /// A message larger than a slot.
    #[error("a message of {length} bytes, where a slot takes {most}")]
    TooLarge {
        /// Its length.
        length: usize,
        /// A slot's.
        most: usize,
    },
    /// The writer overtook every try at a read.
    #[error("the writer overtook {0} reads in a row")]
    Overtaken(u32),
}

impl ShmError {
    fn io(name: &str, source: std::io::Error) -> ShmError {
        ShmError::Io {
            name: name.to_owned(),
            source,
        }
    }

    /// Whether no shared memory of that name exists.
    pub fn is_not_found(&self) -> bool {
        matches!(self, ShmError::Io { source, .. } if source.kind() == std::io::ErrorKind::NotFound)
    }
}
