//! Archive browsing logic
//!
//! Supports: zip, tar, tar.gz, tar.bz2, tar.xz, 7z, rar, and single-file xz/gz/bz2

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, Cursor, Read};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use bark_plugin_api::FileEntry;

/// Normalize archive paths: replace backslashes, strip leading "./"
fn normalize_archive_path(p: &str) -> String {
    p.replace('\\', "/").trim_start_matches("./").to_string()
}

/// Supported archive types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchiveType {
    Zip,
    Tar,
    TarGz,
    TarBz2,
    TarXz,
    Tar7z,
    SevenZip,
    Rar,
    Xz,
    Gzip,
    Bzip2,
}

impl ArchiveType {
    /// Detect archive type from file extension
    pub fn from_path(path: &Path) -> Option<Self> {
        let name = path.file_name()?.to_string_lossy().to_lowercase();

        if name.ends_with(".zip") || name.ends_with(".jar") || name.ends_with(".war") || name.ends_with(".apk") {
            Some(ArchiveType::Zip)
        } else if name.ends_with(".tar.gz") || name.ends_with(".tgz") {
            Some(ArchiveType::TarGz)
        } else if name.ends_with(".tar.bz2") || name.ends_with(".tbz2") || name.ends_with(".tbz") {
            Some(ArchiveType::TarBz2)
        } else if name.ends_with(".tar.xz") || name.ends_with(".txz") {
            Some(ArchiveType::TarXz)
        } else if name.ends_with(".tar.7z") {
            Some(ArchiveType::Tar7z)
        } else if name.ends_with(".tar") {
            Some(ArchiveType::Tar)
        } else if name.ends_with(".7z") {
            Some(ArchiveType::SevenZip)
        } else if name.ends_with(".rar") {
            Some(ArchiveType::Rar)
        } else if name.ends_with(".xz") {
            Some(ArchiveType::Xz)
        } else if name.ends_with(".gz") {
            Some(ArchiveType::Gzip)
        } else if name.ends_with(".bz2") {
            Some(ArchiveType::Bzip2)
        } else {
            None
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            ArchiveType::Zip => "ZIP",
            ArchiveType::Tar => "TAR",
            ArchiveType::TarGz => "TAR.GZ",
            ArchiveType::TarBz2 => "TAR.BZ2",
            ArchiveType::TarXz => "TAR.XZ",
            ArchiveType::Tar7z => "TAR.7Z",
            ArchiveType::SevenZip => "7Z",
            ArchiveType::Rar => "RAR",
            ArchiveType::Xz => "XZ",
            ArchiveType::Gzip => "GZ",
            ArchiveType::Bzip2 => "BZ2",
        }
    }

    /// Get all supported extensions for plugin-info
    pub fn all_extensions() -> Vec<String> {
        vec![
            ".zip".into(), ".jar".into(), ".war".into(), ".apk".into(),
            ".tar.gz".into(), ".tgz".into(),
            ".tar.bz2".into(), ".tbz2".into(), ".tbz".into(),
            ".tar.xz".into(), ".txz".into(),
            ".tar.7z".into(),
            ".tar".into(),
            ".7z".into(),
            ".rar".into(),
            ".xz".into(),
            ".gz".into(),
            ".bz2".into(),
        ]
    }
}

/// Cached entry information from archive
#[derive(Debug, Clone)]
struct ArchiveEntry {
    path: String,
    is_dir: bool,
    size: u64,
    modified: Option<SystemTime>,
    permissions: u32,
}

/// Compression type for tar archives
#[derive(Debug, Clone, Copy)]
enum Compression {
    Gzip,
    Bzip2,
    Xz,
}

/// Archive session — holds the loaded archive state
#[allow(dead_code)]
pub struct ArchiveSession {
    archive_path: PathBuf,
    archive_type: ArchiveType,
    entries: Vec<ArchiveEntry>,
    display_name: String,
    password: Option<String>,
}

#[allow(dead_code)]
impl ArchiveSession {
    /// Open an archive file, optionally with a password
    pub fn open(archive_path: PathBuf, password: Option<String>) -> Result<Self, String> {
        let archive_type = ArchiveType::from_path(&archive_path)
            .ok_or_else(|| format!("Unknown archive type: {}", archive_path.display()))?;

        let archive_name = archive_path.file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "Archive".to_string());

        let display_name = format!("{} [{}]", archive_name, archive_type.display_name());

        let mut session = Self {
            archive_path,
            archive_type,
            entries: Vec::new(),
            display_name,
            password,
        };

        session.load_entries()?;
        Ok(session)
    }

    pub fn display_name(&self) -> &str {
        &self.display_name
    }

    pub fn set_password(&mut self, password: Option<String>) {
        self.password = password;
    }

    pub fn short_label(&self) -> String {
        format!("[{}]", self.archive_type.display_name())
    }

    fn load_entries(&mut self) -> Result<(), String> {
        self.entries = match self.archive_type {
            ArchiveType::Zip => self.load_zip_entries()?,
            ArchiveType::Tar => self.load_tar_entries(None)?,
            ArchiveType::TarGz => self.load_tar_entries(Some(Compression::Gzip))?,
            ArchiveType::TarBz2 => self.load_tar_entries(Some(Compression::Bzip2))?,
            ArchiveType::TarXz => self.load_tar_entries(Some(Compression::Xz))?,
            ArchiveType::Tar7z => self.load_tar7z_entries()?,
            ArchiveType::SevenZip => self.load_7z_entries()?,
            ArchiveType::Rar => self.load_rar_entries()?,
            ArchiveType::Xz => self.load_single_file_entries(".xz"),
            ArchiveType::Gzip => self.load_single_file_entries(".gz"),
            ArchiveType::Bzip2 => self.load_single_file_entries(".bz2"),
        };
        Ok(())
    }

    fn load_zip_entries(&self) -> Result<Vec<ArchiveEntry>, String> {
        let file = File::open(&self.archive_path)
            .map_err(|e| format!("Failed to open archive: {}", e))?;
        let mut archive = zip::ZipArchive::new(file)
            .map_err(|e| format!("Failed to open ZIP: {}", e))?;

        let mut entries = Vec::new();
        let mut seen_dirs: std::collections::HashSet<String> = std::collections::HashSet::new();

        for i in 0..archive.len() {
            // Use by_index_decrypt when password is available, by_index otherwise
            let file = if let Some(pw) = &self.password {
                match archive.by_index_decrypt(i, pw.as_bytes()) {
                    Ok(f) => f,
                    Err(zip::result::ZipError::InvalidPassword) => {
                        return Err("PASSWORD_REQUIRED:Wrong password".to_string());
                    }
                    Err(e) => return Err(format!("Failed to read ZIP entry: {}", e)),
                }
            } else {
                match archive.by_index(i) {
                    Ok(f) => f,
                    Err(zip::result::ZipError::InvalidPassword) => {
                        return Err("PASSWORD_REQUIRED:Archive is encrypted".to_string());
                    }
                    Err(zip::result::ZipError::UnsupportedArchive(ref msg)) if msg.contains("encrypted") || msg.contains("password") => {
                        return Err("PASSWORD_REQUIRED:Archive is encrypted".to_string());
                    }
                    Err(e) => return Err(format!("Failed to read ZIP entry: {}", e)),
                }
            };

            let path = normalize_archive_path(file.name())
                .trim_end_matches('/')
                .to_string();
            if path.is_empty() || path == "." {
                continue;
            }
            let is_dir = file.is_dir();
            let size = file.size();
            let permissions = file.unix_mode().unwrap_or(0);

            // Add implicit parent directories
            let mut current = String::new();
            for component in path.split('/') {
                if !current.is_empty() {
                    current.push('/');
                }
                current.push_str(component);

                if current != path && !seen_dirs.contains(&current) {
                    seen_dirs.insert(current.clone());
                    entries.push(ArchiveEntry {
                        path: current.clone(),
                        is_dir: true,
                        size: 0,
                        modified: None,
                        permissions: 0,
                    });
                }
            }

            let modified = file.last_modified().and_then(|dt| {
                // Convert zip::DateTime (year, month, day, hour, minute, second) to SystemTime
                // Using a simplified days-from-epoch calculation
                let year = dt.year() as i64;
                let month = dt.month() as i64;
                let day = dt.day() as i64;
                let days = {
                    // Days from Unix epoch to the given date (simplified calculation)
                    let y = if month <= 2 { year - 1 } else { year };
                    let m = if month <= 2 { month + 9 } else { month - 3 };
                    let c = y / 100;
                    let ya = y - 100 * c;
                    (146097 * c) / 4 + (1461 * ya) / 4 + (153 * m + 2) / 5 + day - 719469
                };
                let secs = days * 86400
                    + dt.hour() as i64 * 3600
                    + dt.minute() as i64 * 60
                    + dt.second() as i64;
                if secs >= 0 {
                    SystemTime::UNIX_EPOCH.checked_add(std::time::Duration::from_secs(secs as u64))
                } else {
                    None
                }
            });

            if !is_dir || !seen_dirs.contains(&path) {
                if is_dir {
                    seen_dirs.insert(path.clone());
                }
                entries.push(ArchiveEntry {
                    path,
                    is_dir,
                    size,
                    modified,
                    permissions,
                });
            }
        }

        Ok(entries)
    }

    fn load_tar_entries(&self, compression: Option<Compression>) -> Result<Vec<ArchiveEntry>, String> {
        let file = File::open(&self.archive_path)
            .map_err(|e| format!("Failed to open archive: {}", e))?;
        let reader = BufReader::new(file);

        let mut entries = Vec::new();
        let mut seen_dirs: std::collections::HashSet<String> = std::collections::HashSet::new();

        match compression {
            None => {
                let mut archive = tar::Archive::new(reader);
                Self::read_tar_entries(&mut archive, &mut entries, &mut seen_dirs)?;
            }
            Some(Compression::Gzip) => {
                let decoder = flate2::read::GzDecoder::new(reader);
                let mut archive = tar::Archive::new(decoder);
                Self::read_tar_entries(&mut archive, &mut entries, &mut seen_dirs)?;
            }
            Some(Compression::Bzip2) => {
                let decoder = bzip2::read::BzDecoder::new(reader);
                let mut archive = tar::Archive::new(decoder);
                Self::read_tar_entries(&mut archive, &mut entries, &mut seen_dirs)?;
            }
            Some(Compression::Xz) => {
                let decoder = xz2::read::XzDecoder::new(reader);
                let mut archive = tar::Archive::new(decoder);
                Self::read_tar_entries(&mut archive, &mut entries, &mut seen_dirs)?;
            }
        }

        Ok(entries)
    }

    fn read_tar_entries<R: Read>(
        archive: &mut tar::Archive<R>,
        entries: &mut Vec<ArchiveEntry>,
        seen_dirs: &mut std::collections::HashSet<String>,
    ) -> Result<(), String> {
        for entry_result in archive.entries()
            .map_err(|e| format!("Failed to read TAR: {}", e))?
        {
            let entry = entry_result
                .map_err(|e| format!("Failed to read TAR entry: {}", e))?;

            let path = normalize_archive_path(
                &entry.path()
                    .map_err(|e| format!("Invalid path in TAR: {}", e))?
                    .to_string_lossy()
            ).trim_end_matches('/').to_string();

            if path.is_empty() || path == "." {
                continue;
            }

            let is_dir = entry.header().entry_type().is_dir();
            let size = entry.header().size().unwrap_or(0);
            let modified = entry.header().mtime().ok().and_then(|t| {
                SystemTime::UNIX_EPOCH.checked_add(std::time::Duration::from_secs(t))
            });
            let permissions = entry.header().mode().unwrap_or(0);

            // Add implicit parent directories
            let mut current = String::new();
            for component in path.split('/') {
                if !current.is_empty() {
                    current.push('/');
                }
                current.push_str(component);

                if current != path && !seen_dirs.contains(&current) {
                    seen_dirs.insert(current.clone());
                    entries.push(ArchiveEntry {
                        path: current.clone(),
                        is_dir: true,
                        size: 0,
                        modified: None,
                        permissions: 0,
                    });
                }
            }

            if !is_dir || !seen_dirs.contains(&path) {
                if is_dir {
                    seen_dirs.insert(path.clone());
                }
                entries.push(ArchiveEntry {
                    path,
                    is_dir,
                    size,
                    modified,
                    permissions,
                });
            }
        }

        Ok(())
    }

    fn load_7z_entries(&self) -> Result<Vec<ArchiveEntry>, String> {
        let pw = match &self.password {
            Some(p) => sevenz_rust::Password::from(p.as_str()),
            None => sevenz_rust::Password::empty(),
        };
        let reader = sevenz_rust::SevenZReader::open(
            &self.archive_path,
            pw,
        ).map_err(|e| {
            let msg = format!("{}", e);
            if msg.contains("password") || msg.contains("Password") || msg.contains("decrypt") || msg.contains("BadPassword") {
                if self.password.is_some() {
                    format!("PASSWORD_REQUIRED:Wrong password")
                } else {
                    format!("PASSWORD_REQUIRED:Archive is encrypted")
                }
            } else {
                format!("Failed to open 7z: {}", e)
            }
        })?;

        let archive_entries = &reader.archive().files;

        let mut entries = Vec::new();
        let mut seen_dirs: std::collections::HashSet<String> = std::collections::HashSet::new();

        for sz_entry in archive_entries {
            let path = normalize_archive_path(sz_entry.name())
                .trim_end_matches('/')
                .to_string();
            if path.is_empty() || path == "." {
                continue;
            }

            let is_dir = sz_entry.is_directory();
            let size = sz_entry.size();
            let modified = {
                let st: SystemTime = sz_entry.last_modified_date().into();
                if st > SystemTime::UNIX_EPOCH {
                    Some(st)
                } else {
                    None
                }
            };

            // Add implicit parent directories
            let mut current = String::new();
            for component in path.split('/') {
                if !current.is_empty() {
                    current.push('/');
                }
                current.push_str(component);

                if current != path && !seen_dirs.contains(&current) {
                    seen_dirs.insert(current.clone());
                    entries.push(ArchiveEntry {
                        path: current.clone(),
                        is_dir: true,
                        size: 0,
                        modified: None,
                        permissions: 0,
                    });
                }
            }

            if !is_dir || !seen_dirs.contains(&path) {
                if is_dir {
                    seen_dirs.insert(path.clone());
                }
                entries.push(ArchiveEntry {
                    path,
                    is_dir,
                    size,
                    modified,
                    permissions: 0,
                });
            }
        }

        Ok(entries)
    }

    /// Decompress a .tar.7z: extract the tar stream from the 7z, then parse as tar
    fn decompress_7z_to_bytes(&self) -> Result<Vec<u8>, String> {
        let pw = match &self.password {
            Some(p) => sevenz_rust::Password::from(p.as_str()),
            None => sevenz_rust::Password::empty(),
        };
        let mut reader = sevenz_rust::SevenZReader::open(
            &self.archive_path,
            pw,
        ).map_err(|e| {
            let msg = format!("{}", e);
            if msg.contains("password") || msg.contains("Password") || msg.contains("decrypt") || msg.contains("BadPassword") {
                if self.password.is_some() {
                    format!("PASSWORD_REQUIRED:Wrong password")
                } else {
                    format!("PASSWORD_REQUIRED:Archive is encrypted")
                }
            } else {
                format!("Failed to open 7z: {}", e)
            }
        })?;

        let mut tar_data: Option<Vec<u8>> = None;

        reader.for_each_entries(|_entry, reader| {
            let mut data = Vec::new();
            reader.read_to_end(&mut data)
                .map_err(|e| sevenz_rust::Error::other(format!("Failed to read: {}", e)))?;
            // Take the first (and typically only) entry — the tar stream
            if tar_data.is_none() {
                tar_data = Some(data);
            }
            Ok(true)
        }).map_err(|e| format!("Failed to decompress 7z: {}", e))?;

        tar_data.ok_or_else(|| "No entries found in 7z archive".to_string())
    }

    fn load_tar7z_entries(&self) -> Result<Vec<ArchiveEntry>, String> {
        let tar_data = self.decompress_7z_to_bytes()?;
        let cursor = Cursor::new(tar_data);
        let mut archive = tar::Archive::new(cursor);
        let mut entries = Vec::new();
        let mut seen_dirs = std::collections::HashSet::new();
        Self::read_tar_entries(&mut archive, &mut entries, &mut seen_dirs)?;
        Ok(entries)
    }

    fn extract_tar7z_file(&self, path: &str) -> Result<Vec<u8>, String> {
        let tar_data = self.decompress_7z_to_bytes()?;
        let cursor = Cursor::new(tar_data);
        let mut archive = tar::Archive::new(cursor);
        Self::find_and_extract_tar_file(&mut archive, path)
    }

    fn load_rar_entries(&self) -> Result<Vec<ArchiveEntry>, String> {
        let archive = if let Some(pw) = &self.password {
            unrar::Archive::with_password(&self.archive_path, pw)
                .open_for_listing()
        } else {
            unrar::Archive::new(&self.archive_path)
                .open_for_listing()
        }.map_err(|e| {
            let msg = format!("{}", e);
            if msg.contains("password") || msg.contains("Password") || msg.contains("encrypted") || msg.contains("ERAR_BAD_PASSWORD") || msg.contains("BadPassword") {
                if self.password.is_some() {
                    "PASSWORD_REQUIRED:Wrong password".to_string()
                } else {
                    "PASSWORD_REQUIRED:Archive is encrypted".to_string()
                }
            } else {
                format!("Failed to open RAR: {}", e)
            }
        })?;

        let mut entries = Vec::new();
        let mut seen_dirs: std::collections::HashSet<String> = std::collections::HashSet::new();

        for entry_result in archive {
            let header = entry_result
                .map_err(|e| format!("Failed to read RAR entry: {}", e))?;

            let path = normalize_archive_path(&header.filename.to_string_lossy())
                .trim_end_matches('/')
                .to_string();
            if path.is_empty() || path == "." {
                continue;
            }

            let is_dir = header.is_directory();
            let size = header.unpacked_size;

            // Convert DOS file_time to SystemTime
            // DOS time format: bits 15-11=hours, 10-5=minutes, 4-0=seconds/2
            // DOS date format: bits 15-9=year-1980, 8-5=month, 4-0=day
            let modified = {
                let ft = header.file_time as u64;
                if ft > 0 {
                    let time_part = (ft & 0xFFFF) as u32;
                    let date_part = ((ft >> 16) & 0xFFFF) as u32;
                    let second = ((time_part & 0x1F) * 2) as i64;
                    let minute = ((time_part >> 5) & 0x3F) as i64;
                    let hour = ((time_part >> 11) & 0x1F) as i64;
                    let day = (date_part & 0x1F) as i64;
                    let month = ((date_part >> 5) & 0x0F) as i64;
                    let year = (((date_part >> 9) & 0x7F) + 1980) as i64;

                    let days = {
                        let y = if month <= 2 { year - 1 } else { year };
                        let m = if month <= 2 { month + 9 } else { month - 3 };
                        let c = y / 100;
                        let ya = y - 100 * c;
                        (146097 * c) / 4 + (1461 * ya) / 4 + (153 * m + 2) / 5 + day - 719469
                    };
                    let secs = days * 86400 + hour * 3600 + minute * 60 + second;
                    if secs >= 0 {
                        SystemTime::UNIX_EPOCH.checked_add(std::time::Duration::from_secs(secs as u64))
                    } else {
                        None
                    }
                } else {
                    None
                }
            };

            // Add implicit parent directories
            let mut current = String::new();
            for component in path.split('/') {
                if !current.is_empty() {
                    current.push('/');
                }
                current.push_str(component);

                if current != path && !seen_dirs.contains(&current) {
                    seen_dirs.insert(current.clone());
                    entries.push(ArchiveEntry {
                        path: current.clone(),
                        is_dir: true,
                        size: 0,
                        modified: None,
                        permissions: 0,
                    });
                }
            }

            if !is_dir || !seen_dirs.contains(&path) {
                if is_dir {
                    seen_dirs.insert(path.clone());
                }
                entries.push(ArchiveEntry {
                    path,
                    is_dir,
                    size,
                    modified,
                    permissions: header.file_attr,
                });
            }
        }

        Ok(entries)
    }

    fn extract_rar_file(&self, path: &str) -> Result<Vec<u8>, String> {
        let archive = if let Some(pw) = &self.password {
            unrar::Archive::with_password(&self.archive_path, pw)
                .open_for_processing()
        } else {
            unrar::Archive::new(&self.archive_path)
                .open_for_processing()
        }.map_err(|e| format!("Failed to open RAR: {}", e))?;

        let target_path = normalize_archive_path(path)
            .trim_end_matches('/')
            .to_string();

        let mut cursor = archive;
        loop {
            let header_result = cursor.read_header();
            match header_result {
                Ok(Some(header)) => {
                    let entry_path = normalize_archive_path(&header.entry().filename.to_string_lossy())
                        .trim_end_matches('/')
                        .to_string();

                    if entry_path == target_path {
                        let (data, _next) = header.read()
                            .map_err(|e| {
                                let msg = format!("{}", e);
                                if msg.contains("password") || msg.contains("Password") || msg.contains("encrypted") || msg.contains("BadPassword") || msg.contains("ERAR_BAD_PASSWORD") || msg.contains("BAD_DATA") {
                                    if self.password.is_some() {
                                        "PASSWORD_REQUIRED:Wrong password".to_string()
                                    } else {
                                        "PASSWORD_REQUIRED:File is encrypted".to_string()
                                    }
                                } else {
                                    format!("Failed to extract: {}", e)
                                }
                            })?;
                        return Ok(data);
                    } else {
                        cursor = header.skip()
                            .map_err(|e| format!("Failed to skip entry: {}", e))?;
                    }
                }
                Ok(None) => break,
                Err(e) => {
                    let msg = format!("{}", e);
                    if msg.contains("password") || msg.contains("Password") || msg.contains("encrypted") || msg.contains("BadPassword") || msg.contains("ERAR_BAD_PASSWORD") {
                        return Err(if self.password.is_some() {
                            "PASSWORD_REQUIRED:Wrong password".to_string()
                        } else {
                            "PASSWORD_REQUIRED:File is encrypted".to_string()
                        });
                    }
                    return Err(format!("Failed to read RAR header: {}", e));
                }
            }
        }

        Err(format!("File not found: {}", path))
    }

    fn load_single_file_entries(&self, extension: &str) -> Vec<ArchiveEntry> {
        let archive_name = self.archive_path.file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();

        let decompressed_name = if archive_name.to_lowercase().ends_with(extension) {
            archive_name[..archive_name.len() - extension.len()].to_string()
        } else {
            archive_name
        };

        let size = std::fs::metadata(&self.archive_path)
            .map(|m| m.len())
            .unwrap_or(0);

        vec![ArchiveEntry {
            path: decompressed_name,
            is_dir: false,
            size,
            modified: std::fs::metadata(&self.archive_path)
                .ok()
                .and_then(|m| m.modified().ok()),
            permissions: 0,
        }]
    }

    /// List entries at a given directory path within the archive
    pub fn list_directory(&self, dir_path: &str) -> Vec<FileEntry> {
        let dir_path = dir_path.trim_matches('/');
        let prefix = if dir_path.is_empty() { String::new() } else { format!("{}/", dir_path) };

        let mut seen: HashMap<String, FileEntry> = HashMap::new();

        // Always add ".." entry
        let parent = if dir_path.is_empty() {
            ""
        } else if let Some(pos) = dir_path.rfind('/') {
            &dir_path[..pos]
        } else {
            ""
        };
        seen.insert("..".to_string(), FileEntry::new("..".into(), PathBuf::from(parent), true, 0));

        for entry in &self.entries {
            let entry_path = entry.path.trim_matches('/');

            if dir_path.is_empty() {
                if entry_path.contains('/') {
                    let first_component = entry_path.split('/').next().unwrap();
                    if !seen.contains_key(first_component) {
                        let fe = FileEntry::new(
                            first_component.into(),
                            PathBuf::from(first_component),
                            true,
                            0,
                        ).with_modified(entry.modified)
                            .with_permissions(entry.permissions)
                            .with_hidden(first_component.starts_with('.'));
                        seen.insert(first_component.to_string(), fe);
                    }
                    continue;
                }
            } else if !entry_path.starts_with(&prefix) {
                continue;
            }

            let relative = if dir_path.is_empty() {
                entry_path.to_string()
            } else {
                entry_path.strip_prefix(&prefix).unwrap_or(entry_path).to_string()
            };

            if relative.contains('/') {
                let first_component = relative.split('/').next().unwrap();
                if !seen.contains_key(first_component) {
                    let full_path = if dir_path.is_empty() {
                        first_component.to_string()
                    } else {
                        format!("{}/{}", dir_path, first_component)
                    };
                    let fe = FileEntry::new(
                        first_component.into(),
                        PathBuf::from(full_path),
                        true,
                        0,
                    ).with_modified(entry.modified)
                        .with_permissions(entry.permissions)
                        .with_hidden(first_component.starts_with('.'));
                    seen.insert(first_component.to_string(), fe);
                }
                continue;
            }

            if !relative.is_empty() && !seen.contains_key(&relative) {
                let fe = FileEntry::new(
                    relative.clone(),
                    PathBuf::from(&entry.path),
                    entry.is_dir,
                    entry.size,
                ).with_modified(entry.modified)
                    .with_permissions(entry.permissions)
                    .with_hidden(relative.starts_with('.'));
                seen.insert(relative, fe);
            }
        }

        let mut result: Vec<FileEntry> = seen.into_values().collect();
        result.sort_by(|a, b| {
            if a.name == ".." { return std::cmp::Ordering::Less; }
            if b.name == ".." { return std::cmp::Ordering::Greater; }
            match (a.is_dir, b.is_dir) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
            }
        });

        result
    }

    /// Extract a file from the archive and return its contents
    pub fn read_file(&self, path: &str) -> Result<Vec<u8>, String> {
        let path = path.trim_start_matches('/');
        match self.archive_type {
            ArchiveType::Zip => self.extract_zip_file(path),
            ArchiveType::Tar => self.extract_tar_file(path, None),
            ArchiveType::TarGz => self.extract_tar_file(path, Some(Compression::Gzip)),
            ArchiveType::TarBz2 => self.extract_tar_file(path, Some(Compression::Bzip2)),
            ArchiveType::TarXz => self.extract_tar_file(path, Some(Compression::Xz)),
            ArchiveType::Tar7z => self.extract_tar7z_file(path),
            ArchiveType::SevenZip => self.extract_7z_file(path),
            ArchiveType::Rar => self.extract_rar_file(path),
            ArchiveType::Xz => self.extract_xz_file(),
            ArchiveType::Gzip => self.extract_gzip_file(),
            ArchiveType::Bzip2 => self.extract_bzip2_file(),
        }
    }

    fn extract_zip_file(&self, path: &str) -> Result<Vec<u8>, String> {
        let file = File::open(&self.archive_path)
            .map_err(|e| format!("Failed to open archive: {}", e))?;
        let mut archive = zip::ZipArchive::new(file)
            .map_err(|e| format!("Failed to open ZIP: {}", e))?;

        let mut zip_file = if let Some(pw) = &self.password {
            archive.by_name_decrypt(path, pw.as_bytes())
                .map_err(|e| match e {
                    zip::result::ZipError::InvalidPassword => "PASSWORD_REQUIRED:Wrong password".to_string(),
                    _ => format!("File not found: {}", path),
                })?
        } else {
            archive.by_name(path)
                .map_err(|_| format!("File not found: {}", path))?
        };

        let mut contents = Vec::new();
        zip_file.read_to_end(&mut contents)
            .map_err(|e| format!("Failed to read: {}", e))?;

        Ok(contents)
    }

    fn extract_tar_file(&self, path: &str, compression: Option<Compression>) -> Result<Vec<u8>, String> {
        let file = File::open(&self.archive_path)
            .map_err(|e| format!("Failed to open archive: {}", e))?;
        let reader = BufReader::new(file);

        match compression {
            None => {
                let mut archive = tar::Archive::new(reader);
                Self::find_and_extract_tar_file(&mut archive, path)
            }
            Some(Compression::Gzip) => {
                let decoder = flate2::read::GzDecoder::new(reader);
                let mut archive = tar::Archive::new(decoder);
                Self::find_and_extract_tar_file(&mut archive, path)
            }
            Some(Compression::Bzip2) => {
                let decoder = bzip2::read::BzDecoder::new(reader);
                let mut archive = tar::Archive::new(decoder);
                Self::find_and_extract_tar_file(&mut archive, path)
            }
            Some(Compression::Xz) => {
                let decoder = xz2::read::XzDecoder::new(reader);
                let mut archive = tar::Archive::new(decoder);
                Self::find_and_extract_tar_file(&mut archive, path)
            }
        }
    }

    fn find_and_extract_tar_file<R: Read>(archive: &mut tar::Archive<R>, path: &str) -> Result<Vec<u8>, String> {
        for entry_result in archive.entries()
            .map_err(|e| format!("Failed to read TAR: {}", e))?
        {
            let mut entry = entry_result
                .map_err(|e| format!("Failed to read TAR entry: {}", e))?;

            let entry_path = entry.path()
                .map_err(|e| format!("Invalid path in TAR: {}", e))?
                .to_string_lossy()
                .trim_end_matches('/')
                .to_string();

            if entry_path == path {
                let mut contents = Vec::new();
                entry.read_to_end(&mut contents)
                    .map_err(|e| format!("Failed to read: {}", e))?;
                return Ok(contents);
            }
        }

        Err(format!("File not found: {}", path))
    }

    fn extract_7z_file(&self, path: &str) -> Result<Vec<u8>, String> {
        let pw = match &self.password {
            Some(p) => sevenz_rust::Password::from(p.as_str()),
            None => sevenz_rust::Password::empty(),
        };
        let mut reader = sevenz_rust::SevenZReader::open(
            &self.archive_path,
            pw,
        ).map_err(|e| format!("Failed to open 7z: {}", e))?;

        let target_path = normalize_archive_path(path)
            .trim_end_matches('/')
            .to_string();

        let mut found_data: Option<Vec<u8>> = None;

        reader.for_each_entries(|entry, reader| {
            let entry_path = normalize_archive_path(entry.name())
                .trim_end_matches('/')
                .to_string();
            let mut data = Vec::new();
            reader.read_to_end(&mut data)
                .map_err(|e| sevenz_rust::Error::other(format!("Failed to read: {}", e)))?;
            if entry_path == target_path {
                found_data = Some(data);
            }
            Ok(true)
        }).map_err(|e| format!("Failed to read 7z: {}", e))?;

        found_data.ok_or_else(|| format!("File not found: {}", path))
    }

    fn extract_xz_file(&self) -> Result<Vec<u8>, String> {
        let file = File::open(&self.archive_path)
            .map_err(|e| format!("Failed to open: {}", e))?;
        let mut decoder = xz2::read::XzDecoder::new(file);
        let mut contents = Vec::new();
        decoder.read_to_end(&mut contents)
            .map_err(|e| format!("Failed to decompress XZ: {}", e))?;
        Ok(contents)
    }

    fn extract_gzip_file(&self) -> Result<Vec<u8>, String> {
        let file = File::open(&self.archive_path)
            .map_err(|e| format!("Failed to open: {}", e))?;
        let mut decoder = flate2::read::GzDecoder::new(file);
        let mut contents = Vec::new();
        decoder.read_to_end(&mut contents)
            .map_err(|e| format!("Failed to decompress GZIP: {}", e))?;
        Ok(contents)
    }

    fn extract_bzip2_file(&self) -> Result<Vec<u8>, String> {
        let file = File::open(&self.archive_path)
            .map_err(|e| format!("Failed to open: {}", e))?;
        let mut decoder = bzip2::read::BzDecoder::new(file);
        let mut contents = Vec::new();
        decoder.read_to_end(&mut contents)
            .map_err(|e| format!("Failed to decompress BZIP2: {}", e))?;
        Ok(contents)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_7z_lists_all_entries() {
        let path = PathBuf::from("../../examples/pingg-04.7z");
        if !path.exists() {
            return; // skip if test archive not available
        }
        let session = ArchiveSession::open(path, None).expect("Failed to open archive");
        // Archive has 50 files + 12 folders = 62 entries
        assert_eq!(session.entries.len(), 62, "Expected 62 raw entries");

        let root = session.list_directory("");
        // Root: 9 files + 4 dirs + ".." = 14
        assert_eq!(root.len(), 14, "Expected 14 root entries");
    }

    #[test]
    fn test_7z_read_file() {
        let path = PathBuf::from("../../examples/pingg-04.7z");
        if !path.exists() {
            return;
        }
        let session = ArchiveSession::open(path, None).expect("Failed to open archive");
        let data = session.read_file("main.c").expect("Failed to read main.c");
        assert_eq!(data.len(), 4765, "main.c should be 4765 bytes");
    }
}
