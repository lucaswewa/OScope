//! A file only its owner can read: the local token (ADR-0015). On Linux,
//! mode 0600; on Windows, a protected access list with one entry, full
//! control for the account the process runs as. Either way the file is
//! private from the moment it is created.

use std::fs::File;
use std::io::{self, Write};
use std::path::Path;

/// Writes `contents` to `path`, replacing any file there, readable and
/// writable by the account the process runs as alone.
pub fn write_private(path: &Path, contents: &[u8]) -> io::Result<()> {
    match std::fs::remove_file(path) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => return Err(error),
        _ => {}
    }
    let mut file = create(path)?;
    file.write_all(contents)?;
    file.sync_all()
}

#[cfg(unix)]
fn create(path: &Path) -> io::Result<File> {
    use std::os::unix::fs::OpenOptionsExt;
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
}

#[cfg(windows)]
fn create(path: &Path) -> io::Result<File> {
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::io::FromRawHandle;
    use std::ptr;

    use windows_sys::Win32::Foundation::{GENERIC_WRITE, INVALID_HANDLE_VALUE, LocalFree};
    use windows_sys::Win32::Security::Authorization::{
        ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
    };
    use windows_sys::Win32::Security::SECURITY_ATTRIBUTES;
    use windows_sys::Win32::Storage::FileSystem::{CREATE_NEW, CreateFileW, FILE_ATTRIBUTE_NORMAL};

    // A protected access list (`P`: nothing inherited), with one entry:
    // full control (`FA`) for this account.
    let sddl: Vec<u16> = format!("D:P(A;;FA;;;{})", account()?)
        .encode_utf16()
        .chain([0])
        .collect();
    let mut descriptor = ptr::null_mut();
    // SAFETY: parses a NUL-terminated SDDL string into a security
    // descriptor that the system allocates, freed below with `LocalFree`.
    let parsed = unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            sddl.as_ptr(),
            SDDL_REVISION_1,
            &mut descriptor,
            ptr::null_mut(),
        )
    };
    if parsed == 0 {
        return Err(io::Error::last_os_error());
    }
    let attributes = SECURITY_ATTRIBUTES {
        nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: descriptor,
        bInheritHandle: 0,
    };
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain([0]).collect();
    // SAFETY: creates a new file, failing if one exists, with the security
    // descriptor above; the path and the attributes outlive the call.
    let handle = unsafe {
        CreateFileW(
            wide.as_ptr(),
            GENERIC_WRITE,
            0,
            &attributes,
            CREATE_NEW,
            FILE_ATTRIBUTE_NORMAL,
            ptr::null_mut(),
        )
    };
    // SAFETY: the descriptor was allocated by the conversion above, and is
    // no longer needed once the file exists.
    unsafe { LocalFree(descriptor) };
    if handle == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: `handle` is a file just opened for writing, owned by nothing
    // else: the `File` takes it over, and closes it.
    Ok(unsafe { File::from_raw_handle(handle) })
}

/// The account the process runs as, as a SID string such as `S-1-5-21-…`.
#[cfg(windows)]
fn account() -> io::Result<String> {
    use std::ptr;

    use windows_sys::Win32::Foundation::{CloseHandle, LocalFree};
    use windows_sys::Win32::Security::Authorization::ConvertSidToStringSidW;
    use windows_sys::Win32::Security::{GetTokenInformation, TOKEN_QUERY, TOKEN_USER, TokenUser};
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    let mut token = ptr::null_mut();
    // SAFETY: opens the current process's own token, for querying; closed
    // below.
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let mut size = 0;
    // SAFETY: asks only for the size the token's user needs.
    unsafe { GetTokenInformation(token, TokenUser, ptr::null_mut(), 0, &mut size) };
    // As 8-byte words, so that the TOKEN_USER in it is aligned.
    let mut buffer = vec![0_u64; (size as usize).div_ceil(8)];
    // SAFETY: fills `buffer`, of at least `size` bytes, with the token's
    // user.
    let filled = unsafe {
        GetTokenInformation(
            token,
            TokenUser,
            buffer.as_mut_ptr().cast(),
            size,
            &mut size,
        )
    };
    // SAFETY: the token is open, and closed only here.
    unsafe { CloseHandle(token) };
    if filled == 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: the buffer now holds a TOKEN_USER, aligned, whose SID points
    // into the buffer, which outlives its use here.
    let sid = unsafe { (*buffer.as_ptr().cast::<TOKEN_USER>()).User.Sid };
    let mut text = ptr::null_mut();
    // SAFETY: formats the SID as a string the system allocates, freed below.
    if unsafe { ConvertSidToStringSidW(sid, &mut text) } == 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: `text` is a NUL-terminated wide string from the call above.
    let length = (0..).take_while(|&i| unsafe { *text.add(i) } != 0).count();
    // SAFETY: `text` holds `length` wide characters before its NUL.
    let account = String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(text, length) });
    // SAFETY: `text` was allocated by `ConvertSidToStringSidW`.
    unsafe { LocalFree(text.cast()) };
    Ok(account)
}
