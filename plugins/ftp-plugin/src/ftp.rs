//! FTP/FTPS provider plugin for Bark file manager
//!
//! This plugin provides FTP and FTPS (FTP over TLS) remote filesystem access.

use std::io::Cursor;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::SystemTime;

use bark_plugin_api::*;
use suppaftp::types::Mode;
use suppaftp::{FtpStream, NativeTlsConnector, NativeTlsFtpStream};

/// Wrapper enum to hold either a plain FTP stream or a TLS-enabled stream
enum FtpConnection {
    Plain(FtpStream),
    Tls(NativeTlsFtpStream),
}

impl FtpConnection {
    fn list(&mut self, path: Option<&str>) -> suppaftp::FtpResult<Vec<String>> {
        match self {
            FtpConnection::Plain(s) => s.list(path),
            FtpConnection::Tls(s) => s.list(path),
        }
    }

    fn retr_as_buffer(&mut self, path: &str) -> suppaftp::FtpResult<Cursor<Vec<u8>>> {
        match self {
            FtpConnection::Plain(s) => s.retr_as_buffer(path),
            FtpConnection::Tls(s) => s.retr_as_buffer(path),
        }
    }

    fn put_file(
        &mut self,
        path: &str,
        reader: &mut impl std::io::Read,
    ) -> suppaftp::FtpResult<u64> {
        match self {
            FtpConnection::Plain(s) => s.put_file(path, reader),
            FtpConnection::Tls(s) => s.put_file(path, reader),
        }
    }

    fn rm(&mut self, path: &str) -> suppaftp::FtpResult<()> {
        match self {
            FtpConnection::Plain(s) => s.rm(path),
            FtpConnection::Tls(s) => s.rm(path),
        }
    }

    fn rmdir(&mut self, path: &str) -> suppaftp::FtpResult<()> {
        match self {
            FtpConnection::Plain(s) => s.rmdir(path),
            FtpConnection::Tls(s) => s.rmdir(path),
        }
    }

    fn rename(&mut self, from: &str, to: &str) -> suppaftp::FtpResult<()> {
        match self {
            FtpConnection::Plain(s) => s.rename(from, to),
            FtpConnection::Tls(s) => s.rename(from, to),
        }
    }

    fn mkdir(&mut self, path: &str) -> suppaftp::FtpResult<()> {
        match self {
            FtpConnection::Plain(s) => s.mkdir(path),
            FtpConnection::Tls(s) => s.mkdir(path),
        }
    }

    fn quit(&mut self) -> suppaftp::FtpResult<()> {
        match self {
            FtpConnection::Plain(s) => s.quit(),
            FtpConnection::Tls(s) => s.quit(),
        }
    }
}

/// FTP provider plugin
pub struct FtpProviderPlugin {
    info: ProviderPluginInfo,
}

impl FtpProviderPlugin {
    /// Create a new FTP provider plugin
    pub fn new() -> Self {
        Self {
            info: ProviderPluginInfo::provider(
                "FTP Provider",
                "1.0.0",
                vec!["ftp".to_string(), "ftps".to_string()],
            )
            .with_description("FTP/FTPS remote filesystem access")
            .with_icon('\u{1F4E1}'), // satellite antenna emoji
        }
    }
}

impl Default for FtpProviderPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl ProviderPlugin for FtpProviderPlugin {
    fn info(&self) -> &ProviderPluginInfo {
        &self.info
    }

    fn get_dialog_fields(&self) -> Vec<DialogField> {
        vec![
            DialogField {
                id: "name".to_string(),
                label: "Connection Name".to_string(),
                field_type: DialogFieldType::Text,
                default_value: None,
                placeholder: Some("My FTP Server".to_string()),
                required: false,
                help_text: Some("Optional name for this connection".to_string()),
            },
            DialogField {
                id: "host".to_string(),
                label: "Host".to_string(),
                field_type: DialogFieldType::Text,
                default_value: None,
                placeholder: Some("ftp.example.com".to_string()),
                required: true,
                help_text: None,
            },
            DialogField {
                id: "port".to_string(),
                label: "Port".to_string(),
                field_type: DialogFieldType::Number,
                default_value: Some("21".to_string()),
                placeholder: None,
                required: false,
                help_text: None,
            },
            DialogField {
                id: "user".to_string(),
                label: "Username".to_string(),
                field_type: DialogFieldType::Text,
                default_value: Some("anonymous".to_string()),
                placeholder: None,
                required: true,
                help_text: None,
            },
            DialogField {
                id: "password".to_string(),
                label: "Password".to_string(),
                field_type: DialogFieldType::Password,
                default_value: None,
                placeholder: None,
                required: false,
                help_text: None,
            },
            DialogField {
                id: "path".to_string(),
                label: "Initial Path".to_string(),
                field_type: DialogFieldType::Text,
                default_value: Some("/".to_string()),
                placeholder: Some("/".to_string()),
                required: false,
                help_text: None,
            },
            DialogField {
                id: "passive_mode".to_string(),
                label: "Passive Mode".to_string(),
                field_type: DialogFieldType::Checkbox,
                default_value: Some("true".to_string()),
                placeholder: None,
                required: false,
                help_text: Some("Required for most firewalls/NAT".to_string()),
            },
            DialogField {
                id: "use_tls".to_string(),
                label: "Use TLS (FTPS)".to_string(),
                field_type: DialogFieldType::Checkbox,
                default_value: Some("false".to_string()),
                placeholder: None,
                required: false,
                help_text: Some("Enable secure connection".to_string()),
            },
        ]
    }

    fn validate_config(&self, config: &ProviderConfig) -> ProviderResult<()> {
        if config.get("host").map(|s| s.is_empty()).unwrap_or(true) {
            return Err(ProviderError::ConfigError("Host is required".to_string()));
        }
        if config.get("user").map(|s| s.is_empty()).unwrap_or(true) {
            return Err(ProviderError::ConfigError(
                "Username is required".to_string(),
            ));
        }
        Ok(())
    }

    fn connect(&self, config: &ProviderConfig) -> ProviderResult<Box<dyn ProviderSession>> {
        let host = config
            .get("host")
            .ok_or_else(|| ProviderError::ConfigError("Host is required".to_string()))?
            .to_string();
        let port: u16 = config.get_int("port").unwrap_or(21) as u16;
        let user = config.get("user").unwrap_or("anonymous").to_string();
        let password = config.get("password").unwrap_or("anonymous@").to_string();
        let initial_path = config.get("path").map(|s| s.to_string());
        let passive_mode = config.get_bool("passive_mode");
        let use_tls = config.get_bool("use_tls");

        let addr = format!("{}:{}", host, port);

        let stream = if use_tls {
            // Connect with TLS
            let ftp_tls = NativeTlsFtpStream::connect(&addr).map_err(map_ftp_error)?;

            let tls_connector = suppaftp::native_tls::TlsConnector::new()
                .map_err(|e| ProviderError::Connection(format!("TLS setup failed: {}", e)))?;
            let mut ftp_tls = ftp_tls
                .into_secure(NativeTlsConnector::from(tls_connector), &host)
                .map_err(map_ftp_error)?;

            if passive_mode {
                ftp_tls.set_mode(Mode::Passive);
            }

            ftp_tls
                .login(&user, &password)
                .map_err(map_ftp_error)?;

            ftp_tls
                .transfer_type(suppaftp::types::FileType::Binary)
                .map_err(map_ftp_error)?;

            FtpConnection::Tls(ftp_tls)
        } else {
            // Connect without TLS
            let mut ftp = FtpStream::connect(&addr).map_err(map_ftp_error)?;

            if passive_mode {
                ftp.set_mode(Mode::Passive);
            }

            ftp.login(&user, &password).map_err(map_ftp_error)?;

            ftp.transfer_type(suppaftp::types::FileType::Binary)
                .map_err(map_ftp_error)?;

            FtpConnection::Plain(ftp)
        };

        let display = if port != 21 {
            format!("{}@{}:{}", user, host, port)
        } else {
            format!("{}@{}", user, host)
        };

        let scheme = if use_tls { "ftps" } else { "ftp" };
        let display_name = format!("{}://{}", scheme, display);

        Ok(Box::new(FtpProviderSession {
            stream: Mutex::new(Some(stream)),
            display_name,
            home_path: initial_path.unwrap_or_else(|| "/".to_string()),
        }))
    }
}

/// Active FTP session
pub struct FtpProviderSession {
    stream: Mutex<Option<FtpConnection>>,
    display_name: String,
    home_path: String,
}

impl FtpProviderSession {
    /// Get mutable access to the stream
    fn with_stream<T, F>(&self, f: F) -> ProviderResult<T>
    where
        F: FnOnce(&mut FtpConnection) -> ProviderResult<T>,
    {
        let mut guard = self
            .stream
            .lock()
            .map_err(|_| ProviderError::PluginError("Failed to lock FTP stream".to_string()))?;
        let stream = guard.as_mut().ok_or_else(|| {
            ProviderError::Connection("FTP connection closed".to_string())
        })?;
        f(stream)
    }

    /// Parse Unix-style LIST output to FileEntry
    fn parse_list_line(line: &str, base_path: &str) -> Option<FileEntry> {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 9 {
            return None;
        }

        let perms = parts[0];
        let is_dir = perms.starts_with('d');
        let is_symlink = perms.starts_with('l');

        let size: u64 = parts[4].parse().unwrap_or(0);

        let name_parts: Vec<&str> = parts[8..].to_vec();
        let name_str = name_parts.join(" ");

        let (name, symlink_target) = if is_symlink {
            if let Some(idx) = name_str.find(" -> ") {
                (
                    name_str[..idx].to_string(),
                    Some(PathBuf::from(&name_str[idx + 4..])),
                )
            } else {
                (name_str, None)
            }
        } else {
            (name_str, None)
        };

        if name == "." || name == ".." {
            return None;
        }

        let is_hidden = name.starts_with('.');
        let permissions = parse_unix_permissions(perms);
        let modified = parse_ftp_date(parts[5], parts[6], parts[7]);

        let full_path = if base_path == "/" {
            format!("/{}", name)
        } else {
            format!("{}/{}", base_path.trim_end_matches('/'), name)
        };

        Some(
            FileEntry::new(name, PathBuf::from(&full_path), is_dir, if is_dir { 0 } else { size })
                .with_modified(modified)
                .with_hidden(is_hidden)
                .with_permissions(permissions)
                .with_symlink(symlink_target)
                .with_ownership(
                    parts.get(2).unwrap_or(&"").to_string(),
                    parts.get(3).unwrap_or(&"").to_string(),
                ),
        )
    }
}

impl ProviderSession for FtpProviderSession {
    fn display_name(&self) -> String {
        self.display_name.clone()
    }

    fn is_connected(&self) -> bool {
        self.stream
            .lock()
            .map(|guard| guard.is_some())
            .unwrap_or(false)
    }

    fn disconnect(&mut self) {
        if let Ok(mut guard) = self.stream.lock()
            && let Some(mut stream) = guard.take()
        {
            let _ = stream.quit();
        }
    }

    fn list_directory(&mut self, path: &str) -> ProviderResult<Vec<FileEntry>> {
        let path = if path.is_empty() { "/" } else { path };
        let normalized_path = self.normalize_path(path);

        let parent_path = if normalized_path != "/" {
            self.parent_path(&normalized_path)
        } else {
            None
        };

        let list = self.with_stream(|stream| {
            stream.list(Some(&normalized_path)).map_err(map_ftp_error)
        })?;

        let mut entries = Vec::new();

        // Add parent directory entry if not at root
        if let Some(parent) = parent_path {
            entries.push(FileEntry::parent(PathBuf::from(&parent)));
        }

        for line in list {
            if let Some(entry) = Self::parse_list_line(&line, &normalized_path) {
                entries.push(entry);
            }
        }

        Ok(entries)
    }

    fn read_file(&mut self, path: &str) -> ProviderResult<Vec<u8>> {
        self.with_stream(|stream| {
            let cursor = stream.retr_as_buffer(path).map_err(map_ftp_error)?;
            Ok(cursor.into_inner())
        })
    }

    fn write_file(&mut self, path: &str, data: &[u8]) -> ProviderResult<()> {
        self.with_stream(|stream| {
            let mut cursor = std::io::Cursor::new(data);
            stream.put_file(path, &mut cursor).map_err(map_ftp_error)?;
            Ok(())
        })
    }

    fn delete(&mut self, path: &str) -> ProviderResult<()> {
        self.with_stream(|stream| {
            // Try to delete as file first
            match stream.rm(path) {
                Ok(()) => Ok(()),
                Err(_) => {
                    // Maybe it's a directory
                    stream.rmdir(path).map_err(map_ftp_error)
                }
            }
        })
    }

    fn delete_recursive(&mut self, path: &str) -> ProviderResult<()> {
        // List contents and delete recursively
        let entries = self.list_directory(path)?;

        for entry in entries {
            if entry.name != ".." {
                let entry_path = entry.path.to_string_lossy().into_owned();
                if entry.is_dir {
                    self.delete_recursive(&entry_path)?;
                } else {
                    self.delete(&entry_path)?;
                }
            }
        }

        // Now delete the empty directory
        self.with_stream(|stream| stream.rmdir(path).map_err(map_ftp_error))
    }

    fn rename(&mut self, from: &str, to: &str) -> ProviderResult<()> {
        self.with_stream(|stream| stream.rename(from, to).map_err(map_ftp_error))
    }

    fn mkdir(&mut self, path: &str) -> ProviderResult<()> {
        self.with_stream(|stream| stream.mkdir(path).map_err(map_ftp_error))
    }

    fn copy_file(&mut self, from: &str, to: &str) -> ProviderResult<()> {
        // FTP doesn't have native copy, so read and write
        let data = self.read_file(from)?;
        self.write_file(to, &data)
    }

    fn home_path(&self) -> String {
        self.home_path.clone()
    }
}

impl Drop for FtpProviderSession {
    fn drop(&mut self) {
        self.disconnect();
    }
}

/// Convert suppaftp error to plugin error
fn map_ftp_error(e: suppaftp::FtpError) -> ProviderError {
    match &e {
        suppaftp::FtpError::ConnectionError(e) => ProviderError::Connection(e.to_string()),
        suppaftp::FtpError::SecureError(msg) => {
            ProviderError::Connection(format!("TLS error: {}", msg))
        }
        suppaftp::FtpError::UnexpectedResponse(resp) => {
            let code = resp.status.code();
            let body_str = String::from_utf8_lossy(&resp.body).to_string();
            if code == 530 {
                ProviderError::Auth("Login incorrect".to_string())
            } else if code == 550 {
                ProviderError::NotFound(body_str)
            } else if code == 553 || code == 451 {
                ProviderError::PermissionDenied(body_str)
            } else {
                ProviderError::Other(format!("FTP error {}: {}", code, body_str))
            }
        }
        _ => ProviderError::Other(e.to_string()),
    }
}

/// Parse Unix permission string (e.g., "drwxr-xr-x") to numeric
fn parse_unix_permissions(perms: &str) -> u32 {
    let chars: Vec<char> = perms.chars().collect();
    if chars.len() < 10 {
        return 0;
    }

    let mut mode: u32 = 0;

    if chars[1] == 'r' {
        mode |= 0o400;
    }
    if chars[2] == 'w' {
        mode |= 0o200;
    }
    if chars[3] == 'x' || chars[3] == 's' {
        mode |= 0o100;
    }

    if chars[4] == 'r' {
        mode |= 0o040;
    }
    if chars[5] == 'w' {
        mode |= 0o020;
    }
    if chars[6] == 'x' || chars[6] == 's' {
        mode |= 0o010;
    }

    if chars[7] == 'r' {
        mode |= 0o004;
    }
    if chars[8] == 'w' {
        mode |= 0o002;
    }
    if chars[9] == 'x' || chars[9] == 't' {
        mode |= 0o001;
    }

    mode
}

/// Parse FTP date format
fn parse_ftp_date(month: &str, day: &str, time_or_year: &str) -> Option<SystemTime> {
    use std::time::{Duration, UNIX_EPOCH};

    let month_num = match month.to_lowercase().as_str() {
        "jan" => 1,
        "feb" => 2,
        "mar" => 3,
        "apr" => 4,
        "may" => 5,
        "jun" => 6,
        "jul" => 7,
        "aug" => 8,
        "sep" => 9,
        "oct" => 10,
        "nov" => 11,
        "dec" => 12,
        _ => return None,
    };

    let day_num: u32 = day.parse().ok()?;

    let (year, hour, minute) = if time_or_year.contains(':') {
        let time_parts: Vec<&str> = time_or_year.split(':').collect();
        let h: u32 = time_parts.first()?.parse().ok()?;
        let m: u32 = time_parts.get(1)?.parse().ok()?;
        (2025, h, m)
    } else {
        let y: i32 = time_or_year.parse().ok()?;
        (y, 0, 0)
    };

    let days_since_epoch = days_since_unix_epoch(year, month_num, day_num)?;
    let seconds =
        (days_since_epoch as u64) * 86400 + (hour as u64) * 3600 + (minute as u64) * 60;

    Some(UNIX_EPOCH + Duration::from_secs(seconds))
}

/// Calculate days since Unix epoch (1970-01-01)
fn days_since_unix_epoch(year: i32, month: u32, day: u32) -> Option<i64> {
    if year < 1970 {
        return None;
    }

    let month_days = [0, 31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let is_leap = |y: i32| y % 4 == 0 && (y % 100 != 0 || y % 400 == 0);

    let mut days: i64 = 0;
    for y in 1970..year {
        days += if is_leap(y) { 366 } else { 365 };
    }

    for m in 1..month {
        days += month_days[m as usize] as i64;
        if m == 2 && is_leap(year) {
            days += 1;
        }
    }

    days += (day - 1) as i64;

    Some(days)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_permissions() {
        assert_eq!(parse_unix_permissions("drwxr-xr-x"), 0o755);
        assert_eq!(parse_unix_permissions("-rw-r--r--"), 0o644);
        assert_eq!(parse_unix_permissions("-rwxrwxrwx"), 0o777);
    }
}
