//! Named shared memory: a Windows named file mapping in the session's
//! `Local\` namespace, or a file in `/dev/shm` on Linux. The creator owns the
//! name: on Linux, it removes the file when it is dropped; on Windows, the
//! mapping goes when its last handle closes.

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
    #[cfg(windows)]
    handle: windows_sys::Win32::Foundation::HANDLE,
    #[cfg(unix)]
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

#[cfg(windows)]
mod platform {
    use std::io;
    use std::ptr;

    use windows_sys::Win32::Foundation::{
        CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, INVALID_HANDLE_VALUE,
    };
    use windows_sys::Win32::System::Memory::{
        CreateFileMappingW, FILE_MAP_ALL_ACCESS, FILE_MAP_READ, MEMORY_BASIC_INFORMATION,
        MEMORY_MAPPED_VIEW_ADDRESS, MapViewOfFile, OpenFileMappingW, PAGE_READWRITE,
        UnmapViewOfFile, VirtualQuery,
    };

    use super::Mapping;

    /// `name` in the session's namespace, as a wide string.
    fn wide(name: &str) -> Vec<u16> {
        format!("Local\\oscope-{name}")
            .encode_utf16()
            .chain([0])
            .collect()
    }

    pub(super) fn create(name: &str, len: usize) -> io::Result<Mapping> {
        let name = wide(name);
        let (high, low) = ((len as u64 >> 32) as u32, len as u32);
        // SAFETY: an anonymous mapping backed by the paging file, with the
        // default security, sized `len`, named by a NUL-terminated string
        // that outlives the call.
        let handle = unsafe {
            CreateFileMappingW(
                INVALID_HANDLE_VALUE,
                ptr::null(),
                PAGE_READWRITE,
                high,
                low,
                name.as_ptr(),
            )
        };
        if handle.is_null() {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: reads the calling thread's last error, set by the call above.
        if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
            // SAFETY: `handle` is open, and closed only here.
            unsafe { CloseHandle(handle) };
            return Err(io::Error::from(io::ErrorKind::AlreadyExists));
        }
        // SAFETY: maps the whole of the mapping just made, for writing.
        let view = unsafe { MapViewOfFile(handle, FILE_MAP_ALL_ACCESS, 0, 0, len) };
        mapped(handle, view, len)
    }

    pub(super) fn open(name: &str) -> io::Result<Mapping> {
        let name = wide(name);
        // SAFETY: opens an existing mapping for reading, by a NUL-terminated
        // name that outlives the call.
        let handle = unsafe { OpenFileMappingW(FILE_MAP_READ, 0, name.as_ptr()) };
        if handle.is_null() {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: maps the whole of the open mapping, for reading.
        let view = unsafe { MapViewOfFile(handle, FILE_MAP_READ, 0, 0, 0) };
        if view.Value.is_null() {
            return mapped(handle, view, 0);
        }
        let mut info: MEMORY_BASIC_INFORMATION =
            // SAFETY: a plain C struct, which all zeros is a valid value of.
            unsafe { std::mem::zeroed() };
        let size = size_of::<MEMORY_BASIC_INFORMATION>();
        // SAFETY: queries the view just mapped, into `info`, of `size` bytes.
        let filled = unsafe { VirtualQuery(view.Value, &mut info, size) };
        let len = if filled == 0 { 0 } else { info.RegionSize };
        mapped(handle, view, len)
    }

    /// The mapping, or the error that left `view` empty, closing `handle`.
    fn mapped(
        handle: windows_sys::Win32::Foundation::HANDLE,
        view: MEMORY_MAPPED_VIEW_ADDRESS,
        len: usize,
    ) -> io::Result<Mapping> {
        if view.Value.is_null() {
            let error = io::Error::last_os_error();
            // SAFETY: `handle` is open, and closed only here.
            unsafe { CloseHandle(handle) };
            return Err(error);
        }
        Ok(Mapping {
            base: view.Value.cast(),
            len,
            handle,
        })
    }

    pub(super) fn close(mapping: &mut Mapping) {
        let view = MEMORY_MAPPED_VIEW_ADDRESS {
            Value: mapping.base.cast(),
        };
        // SAFETY: the view and the handle are this mapping's, still open,
        // and nothing uses them after this.
        unsafe {
            UnmapViewOfFile(view);
            CloseHandle(mapping.handle);
        }
    }
}

#[cfg(unix)]
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
