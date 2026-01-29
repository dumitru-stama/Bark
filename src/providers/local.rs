//! Local filesystem provider

use std::fs;
use std::path::{Path, PathBuf};

use crate::fs::FileEntry;
use super::{PanelProvider, ProviderError, ProviderInfo, ProviderResult, ProviderType};

/// Provider for local filesystem operations
#[derive(Debug)]
pub struct LocalProvider {
    info: ProviderInfo,
}

impl Default for LocalProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl LocalProvider {
    /// Create a new local provider
    pub fn new() -> Self {
        Self {
            info: ProviderInfo {
                name: "Local".to_string(),
                description: "Local filesystem".to_string(),
                provider_type: ProviderType::Local,
                icon: Some('📁'),
            },
        }
    }
}

impl PanelProvider for LocalProvider {
    fn info(&self) -> &ProviderInfo {
        &self.info
    }

    fn is_connected(&self) -> bool {
        true // Local filesystem is always "connected"
    }

    fn connect(&mut self) -> ProviderResult<()> {
        Ok(()) // No-op for local
    }

    fn disconnect(&mut self) {
        // No-op for local
    }

    fn list_directory(&mut self, path: &str) -> ProviderResult<Vec<FileEntry>> {
        let path = Path::new(path);
        crate::fs::read_directory(path).map_err(ProviderError::from)
    }

    fn read_file(&mut self, path: &str) -> ProviderResult<Vec<u8>> {
        fs::read(path).map_err(ProviderError::from)
    }

    fn write_file(&mut self, path: &str, data: &[u8]) -> ProviderResult<()> {
        fs::write(path, data).map_err(ProviderError::from)
    }

    fn delete(&mut self, path: &str) -> ProviderResult<()> {
        let path = Path::new(path);
        if path.is_dir() {
            fs::remove_dir(path).map_err(ProviderError::from)
        } else {
            fs::remove_file(path).map_err(ProviderError::from)
        }
    }

    fn delete_recursive(&mut self, path: &str) -> ProviderResult<()> {
        let path = Path::new(path);
        if path.is_dir() {
            fs::remove_dir_all(path).map_err(ProviderError::from)
        } else {
            fs::remove_file(path).map_err(ProviderError::from)
        }
    }

    fn rename(&mut self, from: &str, to: &str) -> ProviderResult<()> {
        fs::rename(from, to).map_err(ProviderError::from)
    }

    fn mkdir(&mut self, path: &str) -> ProviderResult<()> {
        fs::create_dir_all(path).map_err(ProviderError::from)
    }

    fn copy_file(&mut self, from: &str, to: &str) -> ProviderResult<()> {
        fs::copy(from, to)?;
        // Preserve modification time (permissions already copied by fs::copy on Unix)
        if let Ok(meta) = fs::metadata(from) {
            if let Ok(mtime) = meta.modified() {
                let _ = filetime::set_file_mtime(
                    Path::new(to),
                    filetime::FileTime::from_system_time(mtime),
                );
            }
        }
        Ok(())
    }

    fn set_attributes(
        &mut self,
        path: &str,
        modified: Option<std::time::SystemTime>,
        permissions: u32,
    ) -> ProviderResult<()> {
        let dest = Path::new(path);
        if let Some(mtime) = modified {
            let _ = filetime::set_file_mtime(dest, filetime::FileTime::from_system_time(mtime));
        }
        #[cfg(unix)]
        if permissions != 0 {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(dest, fs::Permissions::from_mode(permissions));
        }
        Ok(())
    }

    fn get_free_space(&self, path: &str) -> Option<u64> {
        get_free_space_for_path(Path::new(path))
    }

    fn is_local(&self) -> bool {
        true
    }

    fn home_path(&self) -> String {
        #[cfg(unix)]
        {
            std::env::var("HOME").unwrap_or_else(|_| "/".to_string())
        }
        #[cfg(windows)]
        {
            std::env::var("USERPROFILE").unwrap_or_else(|_| "C:\\".to_string())
        }
        #[cfg(not(any(unix, windows)))]
        {
            "/".to_string()
        }
    }

    fn normalize_path(&self, path: &str) -> String {
        let path = Path::new(path);
        path.canonicalize()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|_| path.to_string_lossy().into_owned())
    }

    fn parent_path(&self, path: &str) -> Option<String> {
        Path::new(path)
            .parent()
            .map(|p| p.to_string_lossy().into_owned())
    }

    fn join_path(&self, base: &str, name: &str) -> String {
        Path::new(base)
            .join(name)
            .to_string_lossy()
            .into_owned()
    }

    fn to_local_path(&self, path: &str) -> Option<PathBuf> {
        Some(PathBuf::from(path))
    }

    fn from_local_path(&self, path: &Path) -> Option<String> {
        Some(path.to_string_lossy().into_owned())
    }
}

/// Get free space for a path
#[cfg(unix)]
#[allow(dead_code)]
fn get_free_space_for_path(path: &Path) -> Option<u64> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    let path_cstr = CString::new(path.as_os_str().as_bytes()).ok()?;

    unsafe {
        let mut stat: libc::statvfs = std::mem::zeroed();
        if libc::statvfs(path_cstr.as_ptr(), &mut stat) == 0 {
            Some(stat.f_bavail as u64 * stat.f_frsize as u64)
        } else {
            None
        }
    }
}

#[cfg(windows)]
fn get_free_space_for_path(path: &Path) -> Option<u64> {
    use std::os::windows::ffi::OsStrExt;

    let path_str: Vec<u16> = path.as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    let mut free_bytes: u64 = 0;
    let mut total_bytes: u64 = 0;
    let mut total_free_bytes: u64 = 0;

    unsafe {
        unsafe extern "system" {
            fn GetDiskFreeSpaceExW(
                lpDirectoryName: *const u16,
                lpFreeBytesAvailableToCaller: *mut u64,
                lpTotalNumberOfBytes: *mut u64,
                lpTotalNumberOfFreeBytes: *mut u64,
            ) -> i32;
        }

        if GetDiskFreeSpaceExW(
            path_str.as_ptr(),
            &mut free_bytes,
            &mut total_bytes,
            &mut total_free_bytes,
        ) != 0 {
            Some(free_bytes)
        } else {
            None
        }
    }
}

#[cfg(not(any(unix, windows)))]
fn get_free_space_for_path(_path: &Path) -> Option<u64> {
    None
}
