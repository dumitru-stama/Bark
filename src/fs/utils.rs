use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Preserve file attributes (permissions, modification time) from src to dest.
/// Best-effort — errors are silently ignored since the file data is already written.
fn preserve_attributes(src: &Path, dest: &Path) {
    if let Ok(meta) = std::fs::metadata(src) {
        // Preserve modification time
        if let Ok(mtime) = meta.modified() {
            let _ = filetime::set_file_mtime(dest, filetime::FileTime::from_system_time(mtime));
        }
        // Preserve permissions (Unix only — Windows permissions are handled by std::fs::copy)
        #[cfg(unix)]
        {
            let _ = std::fs::set_permissions(dest, meta.permissions());
        }
    }
}

/// Copy a file or directory recursively, preserving attributes
pub fn copy_path(src: &PathBuf, dest: &PathBuf) -> std::io::Result<()> {
    if src.is_dir() {
        copy_dir_recursive(src, dest)
    } else {
        std::fs::copy(src, dest)?;
        preserve_attributes(src, dest);
        Ok(())
    }
}

/// Copy a directory recursively, preserving attributes
fn copy_dir_recursive(src: &PathBuf, dest: &PathBuf) -> std::io::Result<()> {
    std::fs::create_dir_all(dest)?;

    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dest_path = dest.join(entry.file_name());

        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dest_path)?;
        } else {
            std::fs::copy(&src_path, &dest_path)?;
            preserve_attributes(&src_path, &dest_path);
        }
    }

    // Preserve directory attributes after copying contents
    // (done last so mtime isn't changed by creating children)
    preserve_attributes(src, dest);

    Ok(())
}

/// Move a file or directory
pub fn move_path(src: &PathBuf, dest: &PathBuf) -> std::io::Result<()> {
    // Try rename first (fast, works on same filesystem)
    match std::fs::rename(src, dest) {
        Ok(()) => Ok(()),
        Err(_) => {
            // Rename failed (likely cross-filesystem), do copy + delete
            copy_path(src, dest)?;
            if src.is_dir() {
                std::fs::remove_dir_all(src)?;
            } else {
                std::fs::remove_file(src)?;
            }
            Ok(())
        }
    }
}

/// Delete a file or directory
pub fn delete_path(path: &PathBuf) -> std::io::Result<()> {
    if path.is_dir() {
        std::fs::remove_dir_all(path)
    } else {
        std::fs::remove_file(path)
    }
}

/// Copy a single file with progress callback, reporting bytes copied after each chunk.
/// Returns the number of bytes copied.
pub fn copy_file_with_progress(
    src: &Path,
    dest: &Path,
    cancel: &Arc<AtomicBool>,
    progress: &dyn Fn(u64),
) -> std::io::Result<u64> {
    use std::io::{Read, Write};

    let mut reader = std::fs::File::open(src)?;
    let mut writer = std::fs::File::create(dest)?;
    let mut buf = [0u8; 64 * 1024];
    let mut total: u64 = 0;

    loop {
        if cancel.load(Ordering::Relaxed) {
            // Clean up partial file on cancel
            drop(writer);
            let _ = std::fs::remove_file(dest);
            return Err(std::io::Error::new(std::io::ErrorKind::Interrupted, "cancelled"));
        }

        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        writer.write_all(&buf[..n])?;
        total += n as u64;
        progress(n as u64);
    }

    Ok(total)
}

/// Copy a file or directory recursively with progress callback.
/// The callback receives the number of bytes just written in each chunk.
pub fn copy_path_with_progress(
    src: &Path,
    dest: &Path,
    cancel: &Arc<AtomicBool>,
    progress: &dyn Fn(u64),
) -> std::io::Result<()> {
    if src.is_dir() {
        copy_dir_recursive_progress(src, dest, cancel, progress)
    } else {
        copy_file_with_progress(src, dest, cancel, progress)?;
        preserve_attributes(src, dest);
        Ok(())
    }
}

fn copy_dir_recursive_progress(
    src: &Path,
    dest: &Path,
    cancel: &Arc<AtomicBool>,
    progress: &dyn Fn(u64),
) -> std::io::Result<()> {
    std::fs::create_dir_all(dest)?;

    for entry in std::fs::read_dir(src)? {
        if cancel.load(Ordering::Relaxed) {
            return Err(std::io::Error::new(std::io::ErrorKind::Interrupted, "cancelled"));
        }
        let entry = entry?;
        let src_path = entry.path();
        let dest_path = dest.join(entry.file_name());

        if src_path.is_dir() {
            copy_dir_recursive_progress(&src_path, &dest_path, cancel, progress)?;
        } else {
            copy_file_with_progress(&src_path, &dest_path, cancel, progress)?;
            preserve_attributes(&src_path, &dest_path);
        }
    }

    // Preserve directory attributes after contents (so mtime isn't changed by creating children)
    preserve_attributes(src, dest);

    Ok(())
}

/// Move a file or directory with progress callback.
pub fn move_path_with_progress(
    src: &Path,
    dest: &Path,
    cancel: &Arc<AtomicBool>,
    progress: &dyn Fn(u64),
) -> std::io::Result<()> {
    // Try rename first (fast, same filesystem)
    match std::fs::rename(src, dest) {
        Ok(()) => {
            // Report the full size since rename is instant
            let size = if src.is_dir() {
                // Already moved, report 0 (can't stat after rename)
                0
            } else {
                dest.metadata().map(|m| m.len()).unwrap_or(0)
            };
            if size > 0 {
                progress(size);
            }
            Ok(())
        }
        Err(_) => {
            // Cross-filesystem: copy with progress, then delete
            copy_path_with_progress(src, dest, cancel, progress)?;
            if src.is_dir() {
                std::fs::remove_dir_all(src)?;
            } else {
                std::fs::remove_file(src)?;
            }
            Ok(())
        }
    }
}

/// Calculate total bytes for a list of source paths (for progress tracking).
pub fn calculate_total_bytes(sources: &[PathBuf]) -> u64 {
    sources.iter().map(|p| path_size(p)).sum()
}

fn path_size(path: &Path) -> u64 {
    if path.is_dir() {
        std::fs::read_dir(path)
            .map(|entries| {
                entries
                    .filter_map(|e| e.ok())
                    .map(|e| path_size(&e.path()))
                    .sum()
            })
            .unwrap_or(0)
    } else {
        path.metadata().map(|m| m.len()).unwrap_or(0)
    }
}
