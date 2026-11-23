//! A Linux file readable only by its owner (mode 0600), private from
//! the moment it is created.

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

fn create(path: &Path) -> io::Result<File> {
    use std::os::unix::fs::OpenOptionsExt;
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
}
