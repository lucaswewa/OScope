//! Segments: blocks of named shared memory that another process maps by
//! name, to share frames with it (ADR-0020).

use crate::ShmError;
use crate::map::{Mapping, check_name};

/// A block of named shared memory that this process made, and another maps
/// by [`os_name`](Self::os_name): a Python worker's frames, in and out
/// (ADR-0020).
///
/// The maker owns it. The name goes when this is dropped, and the memory
/// once the other process has unmapped it too. The two processes take
/// turns: one writes, then tells the other, which then reads.
pub struct Segment {
    mapping: Mapping,
    name: String,
}

impl std::fmt::Debug for Segment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let len = self.mapping.len();
        f.debug_struct("Segment")
            .field("name", &self.name)
            .field("len", &len)
            .finish()
    }
}

impl Segment {
    /// New shared memory of `len` bytes, zeroed, named `name`: 1 to 64
    /// letters, digits, `-` and `_`. Refused if the name is in use.
    pub fn create(name: &str, len: usize) -> Result<Segment, ShmError> {
        check_name(name)?;
        // Windows maps no empty memory.
        let mapping = Mapping::create(name, len.max(1)).map_err(|e| ShmError::io(name, e))?;
        Ok(Segment {
            mapping,
            name: name.to_owned(),
        })
    }

    /// Its name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The name another process maps it by: on Windows, a named file
    /// mapping, `Local\oscope-{name}`.
    #[cfg(windows)]
    pub fn os_name(&self) -> String {
        format!("Local\\oscope-{}", self.name)
    }

    /// The name another process maps it by: on Linux, a file,
    /// `/dev/shm/oscope-{name}`.
    #[cfg(unix)]
    pub fn os_name(&self) -> String {
        format!("/dev/shm/oscope-{}", self.name)
    }

    /// Its length in bytes.
    pub fn len(&self) -> usize {
        self.mapping.len()
    }

    /// Whether it has no bytes.
    pub fn is_empty(&self) -> bool {
        self.mapping.len() == 0
    }

    /// Copies `samples` into it, from its start.
    pub fn write_f32(&mut self, samples: &[f32]) -> Result<(), ShmError> {
        let length = std::mem::size_of_val(samples);
        if length > self.len() {
            let most = self.len();
            return Err(ShmError::TooLarge { length, most });
        }
        // SAFETY: the mapping holds at least `length` bytes, and is this
        // segment's; the other process doesn't touch it on this side's turn.
        unsafe {
            std::ptr::copy_nonoverlapping(
                samples.as_ptr().cast::<u8>(),
                self.mapping.base(),
                length,
            );
        }
        Ok(())
    }

    /// Copies its first samples into `samples`, filling it.
    pub fn read_f32(&self, samples: &mut [f32]) -> Result<(), ShmError> {
        let length = std::mem::size_of_val(samples);
        if length > self.len() {
            let most = self.len();
            return Err(ShmError::TooLarge { length, most });
        }
        // SAFETY: as in `write_f32`: at least `length` bytes, on this
        // side's turn; any bytes are a valid `f32`.
        unsafe {
            std::ptr::copy_nonoverlapping(
                self.mapping.base(),
                samples.as_mut_ptr().cast::<u8>(),
                length,
            );
        }
        Ok(())
    }
}
