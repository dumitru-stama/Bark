//! File entry representation

use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Represents a single file or directory entry
#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct FileEntry {
    /// File/directory name (not full path)
    pub name: String,
    /// Full path to the entry
    pub path: PathBuf,
    /// Whether this is a directory
    pub is_dir: bool,
    /// File size in bytes (0 for directories)
    pub size: u64,
    /// Last modification time
    pub modified: Option<SystemTime>,
    /// Whether this is a hidden file (starts with '.' on Unix)
    pub is_hidden: bool,
    /// Unix permission bits or Windows file attributes
    pub permissions: u32,
    /// Whether this is a symbolic link
    pub is_symlink: bool,
    /// Target of symlink if applicable
    pub symlink_target: Option<PathBuf>,
    /// Owner user name (Unix only)
    pub owner: String,
    /// Owner group name (Unix only)
    pub group: String,
}

impl FileEntry {
    /// Create a FileEntry from a path
    pub fn from_path(path: &Path) -> std::io::Result<Self> {
        let metadata = fs::symlink_metadata(path)?;
        let name = path
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.to_string_lossy().into_owned());

        let is_symlink = metadata.is_symlink();
        let symlink_target = if is_symlink {
            fs::read_link(path).ok()
        } else {
            None
        };

        // For symlinks, get the target's metadata for is_dir and size
        let target_metadata = if is_symlink {
            fs::metadata(path).ok()
        } else {
            Some(metadata.clone())
        };

        let is_dir = target_metadata
            .as_ref()
            .map(|m| m.is_dir())
            .unwrap_or(false);

        let size = if is_dir {
            0
        } else {
            target_metadata.as_ref().map(|m| m.len()).unwrap_or(0)
        };

        #[cfg(unix)]
        let is_hidden = name.starts_with('.');
        #[cfg(windows)]
        let is_hidden = {
            use std::os::windows::fs::MetadataExt;
            const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
            metadata.file_attributes() & FILE_ATTRIBUTE_HIDDEN != 0
        };
        #[cfg(not(any(unix, windows)))]
        let is_hidden = name.starts_with('.');

        #[cfg(unix)]
        let permissions = {
            use std::os::unix::fs::PermissionsExt;
            metadata.permissions().mode()
        };
        #[cfg(windows)]
        let permissions = {
            use std::os::windows::fs::MetadataExt;
            metadata.file_attributes()
        };
        #[cfg(not(any(unix, windows)))]
        let permissions = 0u32;

        // Get owner and group names (Unix only)
        #[cfg(unix)]
        let (owner, group) = {
            use std::os::unix::fs::MetadataExt;
            let uid = metadata.uid();
            let gid = metadata.gid();
            (get_username(uid), get_groupname(gid))
        };
        #[cfg(not(unix))]
        let (owner, group) = (String::new(), String::new());

        Ok(Self {
            name,
            path: path.to_path_buf(),
            is_dir,
            size,
            modified: metadata.modified().ok(),
            is_hidden,
            permissions,
            is_symlink,
            symlink_target,
            owner,
            group,
        })
    }

    /// Create the special ".." parent directory entry with actual parent metadata
    pub fn parent_entry(parent_path: PathBuf) -> Self {
        // Try to get actual metadata from the parent directory
        if let Ok(mut entry) = Self::from_path(&parent_path) {
            entry.name = "..".to_string();
            return entry;
        }

        // Fallback if we can't read the parent
        Self {
            name: "..".to_string(),
            path: parent_path,
            is_dir: true,
            size: 0,
            modified: None,
            is_hidden: false,
            permissions: 0,
            is_symlink: false,
            symlink_target: None,
            owner: String::new(),
            group: String::new(),
        }
    }

    /// Get the file extension (lowercase), if any
    pub fn extension(&self) -> Option<&str> {
        self.path.extension().and_then(|s| s.to_str())
    }

    /// Check if file is executable (Unix: any execute bit set, Windows: .exe/.bat/.cmd)
    pub fn is_executable(&self) -> bool {
        if self.is_dir {
            return false;
        }

        #[cfg(unix)]
        {
            // Check if any execute bit is set (owner, group, or other)
            const S_IXUSR: u32 = 0o100;
            const S_IXGRP: u32 = 0o010;
            const S_IXOTH: u32 = 0o001;
            self.permissions & (S_IXUSR | S_IXGRP | S_IXOTH) != 0
        }

        #[cfg(windows)]
        {
            // Check for common executable extensions
            let name_lower = self.name.to_lowercase();
            name_lower.ends_with(".exe")
                || name_lower.ends_with(".bat")
                || name_lower.ends_with(".cmd")
                || name_lower.ends_with(".com")
        }

        #[cfg(not(any(unix, windows)))]
        {
            false
        }
    }
}

/// Get username from uid (Unix only)
#[cfg(unix)]
fn get_username(uid: u32) -> String {
    use std::ffi::CStr;

    // SAFETY: getpwuid is safe to call with any uid value
    unsafe {
        let pw = libc::getpwuid(uid);
        if pw.is_null() {
            return uid.to_string();
        }
        let name = (*pw).pw_name;
        if name.is_null() {
            return uid.to_string();
        }
        CStr::from_ptr(name)
            .to_str()
            .map(|s| s.to_string())
            .unwrap_or_else(|_| uid.to_string())
    }
}

/// Get group name from gid (Unix only)
#[cfg(unix)]
fn get_groupname(gid: u32) -> String {
    use std::ffi::CStr;

    // SAFETY: getgrgid is safe to call with any gid value
    unsafe {
        let gr = libc::getgrgid(gid);
        if gr.is_null() {
            return gid.to_string();
        }
        let name = (*gr).gr_name;
        if name.is_null() {
            return gid.to_string();
        }
        CStr::from_ptr(name)
            .to_str()
            .map(|s| s.to_string())
            .unwrap_or_else(|_| gid.to_string())
    }
}
