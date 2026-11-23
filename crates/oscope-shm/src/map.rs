//! Named shared memory backed by a Linux /dev/shm file. The creator owns
//! the name and removes it when dropped.

use std::io;

use crate::ShmError;

/// The longest name.
const MOST: usize = 64;

/// A name's form: 1 to 64 letters, digits, `-` and `_`.
pub(crate) fn check_name(name: &str) -> Result<(), ShmError> {
    let fine = |c: char| c.is_ascii_alphanumeric() || c == '-' || c == '_';
    match !name.is_empty() && name.len() <= MOST && name.chars().all(fine) {
        true => Ok(()),
        false => Err(ShmError::Name(name.to_owned())),
    }
}

/// A view of named shared memory, mapped while this lives.
pub(crate) struct Mapping {
    base: *mut u8,
    len: usize,
    path: Option<std::path::PathBuf>,
}

// SAFETY: the mapping is plain memory owned by this value until it is
// dropped; every access to it goes through atomics (`ring`).
unsafe impl Send for Mapping {}
// SAFETY: as for `Send`: shared access is only through atomic operations.
unsafe impl Sync for Mapping {}

impl Mapping {
    /// The view's first byte: page-aligned.
    pub(crate) fn base(&self) -> *mut u8 {
        self.base
    }

    /// Its length in bytes.
    pub(crate) fn len(&self) -> usize {
        self.len
    }
}

mod platform {
    use std::fs::{File, OpenOptions};
    use std::io;
    use std::os::fd::AsRawFd;
    use std::os::unix::fs::OpenOptionsExt;
    use std::path::PathBuf;
    use std::ptr;

    use super::Mapping;

    fn path(name: &str) -> PathBuf {
        PathBuf::from(format!("/dev/shm/oscope-{name}"))
    }

    pub(super) fn create(name: &str, len: usize) -> io::Result<Mapping> {
        let path = path(name);
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)?;
        file.set_len(len as u64)?;
        match map(&file, len, true) {
            Ok(mut mapping) => {
                // Removed when the mapping is dropped.
                mapping.path = Some(path);
                Ok(mapping)
            }
            Err(error) => {
                let _ = std::fs::remove_file(&path);
                Err(error)
            }
        }
    }

    pub(super) fn open(name: &str) -> io::Result<Mapping> {
        let file = File::open(path(name))?;
        let len = file.metadata()?.len() as usize;
        map(&file, len, false)
    }

    fn map(file: &File, len: usize, write: bool) -> io::Result<Mapping> {
        let protection = match write {
            true => libc::PROT_READ | libc::PROT_WRITE,
            false => libc::PROT_READ,
        };
        // SAFETY: maps `len` bytes of an open file, shared, at an address
        // the kernel chooses; the file may be closed once it is mapped.
        let base = unsafe {
            libc::mmap(
                ptr::null_mut(),
                len.max(1),
                protection,
                libc::MAP_SHARED,
                file.as_raw_fd(),
                0,
            )
        };
        if base == libc::MAP_FAILED {
            return Err(io::Error::last_os_error());
        }
        Ok(Mapping {
            base: base.cast(),
            len,
            path: None,
        })
    }

    pub(super) fn close(mapping: &mut Mapping) {
        // SAFETY: the mapping is this value's, still mapped, and nothing
        // uses it after this.
        unsafe { libc::munmap(mapping.base.cast(), mapping.len.max(1)) };
        if let Some(path) = &mapping.path {
            let _ = std::fs::remove_file(path);
        }
    }
}

impl Mapping {
    /// New shared memory of `len` bytes, zeroed, named `name`; refused if the
    /// name is in use.
    pub(crate) fn create(name: &str, len: usize) -> io::Result<Mapping> {
        platform::create(name, len)
    }

    /// The shared memory named `name`, for reading.
    pub(crate) fn open(name: &str) -> io::Result<Mapping> {
        platform::open(name)
    }
}

impl Drop for Mapping {
    fn drop(&mut self) {
        platform::close(self);
    }
}
