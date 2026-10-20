//! Replacing files safely on the SD card.
//!
//! FAT (FatFs, via ESP-IDF) won't rename onto an existing file, so the usual
//! "write a temporary file, rename it over the old one" needs a delete in
//! between. If power fails in that gap, only the temporary file exists;
//! `read` falls back to it.

use std::fs;
use std::io;
use std::path::Path;

/// Write `data` to `path`, replacing any existing file.
pub fn write(path: &Path, data: &[u8]) -> io::Result<()> {
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, data)?;
    if fs::rename(&tmp, path).is_err() {
        // Target exists (FAT): remove it, then move the new file into place.
        match fs::remove_file(path) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
        fs::rename(&tmp, path)?;
    }
    Ok(())
}

/// Read a file written by `write`, recovering from an interrupted replace.
pub fn read(path: &Path) -> io::Result<Vec<u8>> {
    match fs::read(path) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => fs::read(path.with_extension("tmp")),
        other => other,
    }
}
