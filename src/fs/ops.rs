//! Filesystem operations

use std::fs;
use std::io;
use std::path::Path;

use super::entry::FileEntry;

/// Read directory contents and return a list of FileEntry
pub fn read_directory(path: &Path) -> io::Result<Vec<FileEntry>> {
    let mut entries = Vec::new();

    // Add parent directory entry if not at root
    if let Some(parent) = path.parent() {
        entries.push(FileEntry::parent_entry(parent.to_path_buf()));
    }

    // Read directory contents
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        match FileEntry::from_path(&entry.path()) {
            Ok(file_entry) => entries.push(file_entry),
            Err(_) => {
                // Skip entries we can't read (permission denied, etc.)
                // Could log this in the future
            }
        }
    }

    Ok(entries)
}
