//! SCP/SFTP remote filesystem provider
//!
//! Uses SSH2 protocol for secure file transfer.

use std::path::{Path, PathBuf};
use std::net::TcpStream;
use std::io::{Read, Write};

use crate::fs::FileEntry;
use super::{PanelProvider, ProviderError, ProviderInfo, ProviderResult, ProviderType};

/// Connection information for SCP
#[derive(Debug, Clone)]
pub struct ScpConnectionInfo {
    /// Username
    pub user: String,
    /// Hostname or IP address
    pub host: String,
    /// Port (default 22)
    pub port: u16,
    /// Initial path (optional)
    pub initial_path: Option<String>,
    /// Authentication method
    pub auth: ScpAuth,
}

/// Authentication method for SCP
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum ScpAuth {
    /// Password authentication
    Password(String),
    /// SSH key authentication
    Key {
        /// Path to private key file
        private_key: PathBuf,
        /// Passphrase for key (if encrypted)
        passphrase: Option<String>,
    },
    /// SSH agent authentication
    Agent,
}

#[allow(dead_code)]
impl ScpConnectionInfo {
    /// Create a new connection info with password auth
    pub fn with_password(user: String, host: String, password: String) -> Self {
        Self {
            user,
            host,
            port: 22,
            initial_path: None,
            auth: ScpAuth::Password(password),
        }
    }

    /// Create a new connection info with key auth
    pub fn with_key(user: String, host: String, key_path: PathBuf) -> Self {
        Self {
            user,
            host,
            port: 22,
            initial_path: None,
            auth: ScpAuth::Key {
                private_key: key_path,
                passphrase: None,
            },
        }
    }

    /// Create a new connection info with SSH agent auth
    pub fn with_agent(user: String, host: String) -> Self {
        Self {
            user,
            host,
            port: 22,
            initial_path: None,
            auth: ScpAuth::Agent,
        }
    }

    /// Set port
    pub fn port(mut self, port: u16) -> Self {
        self.port = port;
        self
    }

    /// Set initial path
    pub fn initial_path(mut self, path: String) -> Self {
        self.initial_path = Some(path);
        self
    }

    /// Get display name for this connection
    pub fn display_name(&self) -> String {
        if self.port != 22 {
            format!("{}@{}:{}", self.user, self.host, self.port)
        } else {
            format!("{}@{}", self.user, self.host)
        }
    }

    /// Convert to URI format
    pub fn to_uri(&self) -> String {
        let base = if self.port != 22 {
            format!("scp://{}@{}:{}", self.user, self.host, self.port)
        } else {
            format!("scp://{}@{}", self.user, self.host)
        };
        if let Some(path) = &self.initial_path {
            format!("{}{}", base, path)
        } else {
            base
        }
    }

    /// Parse from URI format (scp://user@host:port/path)
    pub fn from_uri(uri: &str) -> Option<Self> {
        let uri = uri.strip_prefix("scp://")?;

        // Split user@host:port/path
        let (user_host, path) = if let Some(idx) = uri.find('/') {
            (&uri[..idx], Some(&uri[idx..]))
        } else {
            (uri, None)
        };

        let (user, host_port) = user_host.split_once('@')?;

        let (host, port) = if let Some((h, p)) = host_port.split_once(':') {
            (h.to_string(), p.parse().ok()?)
        } else {
            (host_port.to_string(), 22)
        };

        Some(Self {
            user: user.to_string(),
            host,
            port,
            initial_path: path.map(|s| s.to_string()),
            // Auth will need to be set separately
            auth: ScpAuth::Agent,
        })
    }
}

/// SCP/SFTP provider using ssh2
pub struct ScpProvider {
    info: ProviderInfo,
    connection: ScpConnectionInfo,
    session: Option<ssh2::Session>,
    sftp: Option<ssh2::Sftp>,
}

impl ScpProvider {
    /// Create a new SCP provider
    pub fn new(connection: ScpConnectionInfo) -> Self {
        let name = connection.display_name();
        Self {
            info: ProviderInfo {
                name: format!("scp://{}", name),
                description: format!("SSH connection to {}", connection.host),
                provider_type: ProviderType::Scp,
                icon: Some('🔒'),
            },
            connection,
            session: None,
            sftp: None,
        }
    }

    /// Get the SFTP handle, connecting if necessary
    fn sftp(&mut self) -> ProviderResult<&mut ssh2::Sftp> {
        if self.sftp.is_none() {
            self.connect()?;
        }
        self.sftp.as_mut().ok_or_else(|| {
            ProviderError::Connection("SFTP session not available".to_string())
        })
    }

    /// Convert ssh2 error to provider error
    fn map_ssh_error(e: ssh2::Error) -> ProviderError {
        match e.code() {
            ssh2::ErrorCode::Session(_) => ProviderError::Connection(e.to_string()),
            _ => ProviderError::Other(e.to_string()),
        }
    }

    /// Convert FileEntry from SFTP stat
    fn file_entry_from_stat(
        name: String,
        path: String,
        stat: &ssh2::FileStat,
    ) -> FileEntry {
        use std::time::{Duration, UNIX_EPOCH};

        let is_dir = stat.is_dir();
        let is_symlink = stat.file_type() == ssh2::FileType::Symlink;
        let is_hidden = name.starts_with('.');

        FileEntry {
            name,
            path: PathBuf::from(&path),
            is_dir,
            size: if is_dir { 0 } else { stat.size.unwrap_or(0) },
            modified: stat.mtime.map(|t| UNIX_EPOCH + Duration::from_secs(t)),
            is_hidden,
            permissions: stat.perm.unwrap_or(0),
            is_symlink,
            symlink_target: None, // Would need separate readlink call
            owner: stat.uid.map(|u| u.to_string()).unwrap_or_default(),
            group: stat.gid.map(|g| g.to_string()).unwrap_or_default(),
        }
    }
}

impl PanelProvider for ScpProvider {
    fn info(&self) -> &ProviderInfo {
        &self.info
    }

    fn is_connected(&self) -> bool {
        self.session.as_ref().map(|s| s.authenticated()).unwrap_or(false)
    }

    fn connect(&mut self) -> ProviderResult<()> {
        // Connect TCP
        let addr = format!("{}:{}", self.connection.host, self.connection.port);
        let tcp = TcpStream::connect(&addr)
            .map_err(|e| ProviderError::Connection(format!("Failed to connect to {}: {}", addr, e)))?;

        // Create SSH session
        let mut session = ssh2::Session::new()
            .map_err(|e| ProviderError::Connection(format!("Failed to create session: {}", e)))?;

        session.set_tcp_stream(tcp);
        session.handshake()
            .map_err(|e| ProviderError::Connection(format!("SSH handshake failed: {}", e)))?;

        // Enable keepalive to prevent connection timeout (every 10 seconds)
        session.set_keepalive(true, 10);

        // Authenticate
        match &self.connection.auth {
            ScpAuth::Password(password) => {
                session.userauth_password(&self.connection.user, password)
                    .map_err(|e| ProviderError::Auth(format!("Password auth failed: {}", e)))?;
            }
            ScpAuth::Key { private_key, passphrase } => {
                session.userauth_pubkey_file(
                    &self.connection.user,
                    None,
                    private_key,
                    passphrase.as_deref(),
                ).map_err(|e| ProviderError::Auth(format!("Key auth failed: {}", e)))?;
            }
            ScpAuth::Agent => {
                let mut agent = session.agent()
                    .map_err(|e| ProviderError::Auth(format!("Failed to connect to SSH agent: {}", e)))?;
                agent.connect()
                    .map_err(|e| ProviderError::Auth(format!("Failed to connect to SSH agent: {}", e)))?;
                agent.list_identities()
                    .map_err(|e| ProviderError::Auth(format!("Failed to list agent identities: {}", e)))?;

                let mut authenticated = false;
                for identity in agent.identities().unwrap_or_default() {
                    if agent.userauth(&self.connection.user, &identity).is_ok() {
                        authenticated = true;
                        break;
                    }
                }
                if !authenticated {
                    return Err(ProviderError::Auth("No valid identity found in SSH agent".to_string()));
                }
            }
        }

        if !session.authenticated() {
            return Err(ProviderError::Auth("Authentication failed".to_string()));
        }

        // Open SFTP session
        let sftp = session.sftp()
            .map_err(|e| ProviderError::Connection(format!("Failed to open SFTP: {}", e)))?;

        self.session = Some(session);
        self.sftp = Some(sftp);

        Ok(())
    }

    fn disconnect(&mut self) {
        self.sftp = None;
        if let Some(session) = self.session.take() {
            let _ = session.disconnect(None, "Goodbye", None);
        }
    }

    fn list_directory(&mut self, path: &str) -> ProviderResult<Vec<FileEntry>> {
        // Handle empty path as root
        let path = if path.is_empty() { "/" } else { path };

        // Normalize the path first (before mutable borrow for sftp)
        let normalized_path = self.normalize_path(path);
        let path_str = normalized_path.as_str();

        // Calculate parent path before mutable borrow
        let parent_path = if path_str != "/" {
            self.parent_path(path_str)
        } else {
            None
        };

        // Now get SFTP handle (mutable borrow)
        let sftp = self.sftp()?;
        let path_obj = std::path::Path::new(path_str);

        let mut dir = sftp.opendir(path_obj)
            .map_err(Self::map_ssh_error)?;

        let mut entries = Vec::new();

        // Add parent directory entry if not at root
        if let Some(parent) = parent_path {
            entries.push(FileEntry {
                name: "..".to_string(),
                path: PathBuf::from(&parent),
                is_dir: true,
                size: 0,
                modified: None,
                is_hidden: false,
                permissions: 0,
                is_symlink: false,
                symlink_target: None,
                owner: String::new(),
                group: String::new(),
            });
        }

        // Read directory entries
        while let Ok((entry_path, stat)) = dir.readdir() {
            let name = entry_path.file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();

            // Skip . and .. and empty/problematic names
            if name.is_empty()
                || name == "."
                || name == ".."
                || name == "/"
                || name.trim().is_empty()
                || name.contains('/') // Skip any name containing path separator
            {
                continue;
            }

            let full_path = if path_str == "/" {
                format!("/{}", name)
            } else {
                format!("{}/{}", path_str.trim_end_matches('/'), name)
            };
            entries.push(Self::file_entry_from_stat(name, full_path, &stat));
        }

        Ok(entries)
    }

    fn read_file(&mut self, path: &str) -> ProviderResult<Vec<u8>> {
        let sftp = self.sftp()?;
        let mut file = sftp.open(std::path::Path::new(path))
            .map_err(Self::map_ssh_error)?;

        let mut contents = Vec::new();
        file.read_to_end(&mut contents)
            .map_err(ProviderError::Io)?;

        Ok(contents)
    }

    fn write_file(&mut self, path: &str, data: &[u8]) -> ProviderResult<()> {
        let sftp = self.sftp()?;
        let mut file = sftp.create(std::path::Path::new(path))
            .map_err(Self::map_ssh_error)?;

        file.write_all(data)
            .map_err(ProviderError::Io)?;

        Ok(())
    }

    fn delete(&mut self, path: &str) -> ProviderResult<()> {
        let sftp = self.sftp()?;
        let path_obj = std::path::Path::new(path);

        // Check if it's a directory
        match sftp.stat(path_obj) {
            Ok(stat) if stat.is_dir() => {
                sftp.rmdir(path_obj).map_err(Self::map_ssh_error)
            }
            _ => {
                sftp.unlink(path_obj).map_err(Self::map_ssh_error)
            }
        }
    }

    fn delete_recursive(&mut self, path: &str) -> ProviderResult<()> {
        let sftp = self.sftp()?;
        let path_obj = std::path::Path::new(path);

        match sftp.stat(path_obj) {
            Ok(stat) if stat.is_dir() => {
                // List and delete contents first
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
                let sftp = self.sftp()?;
                sftp.rmdir(path_obj).map_err(Self::map_ssh_error)
            }
            _ => {
                sftp.unlink(path_obj).map_err(Self::map_ssh_error)
            }
        }
    }

    fn rename(&mut self, from: &str, to: &str) -> ProviderResult<()> {
        let sftp = self.sftp()?;
        sftp.rename(
            std::path::Path::new(from),
            std::path::Path::new(to),
            None,
        ).map_err(Self::map_ssh_error)
    }

    fn mkdir(&mut self, path: &str) -> ProviderResult<()> {
        let sftp = self.sftp()?;
        sftp.mkdir(std::path::Path::new(path), 0o755)
            .map_err(Self::map_ssh_error)
    }

    fn copy_file(&mut self, from: &str, to: &str) -> ProviderResult<()> {
        // SFTP doesn't have a native copy, so we read and write
        let data = self.read_file(from)?;
        self.write_file(to, &data)
    }

    fn set_attributes(
        &mut self,
        path: &str,
        modified: Option<std::time::SystemTime>,
        permissions: u32,
    ) -> ProviderResult<()> {
        use std::time::UNIX_EPOCH;

        let sftp = self.sftp()?;
        let path_obj = std::path::Path::new(path);

        let mtime = modified.and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_secs());
        let perm = if permissions != 0 { Some(permissions) } else { None };

        // Only call setstat if we have something to set
        if mtime.is_some() || perm.is_some() {
            let stat = ssh2::FileStat {
                size: None,
                uid: None,
                gid: None,
                perm,
                atime: None,
                mtime,
            };
            sftp.setstat(path_obj, stat).map_err(Self::map_ssh_error)?;
        }

        Ok(())
    }

    fn get_free_space(&self, _path: &str) -> Option<u64> {
        // SFTP doesn't provide a standard way to get free space
        // Some servers support the statvfs extension, but it's not universal
        None
    }

    fn is_local(&self) -> bool {
        false
    }

    fn home_path(&self) -> String {
        self.connection.initial_path.clone().unwrap_or_else(|| "/".to_string())
    }

    fn normalize_path(&self, path: &str) -> String {
        // Simple normalization for Unix-style paths
        let mut parts: Vec<&str> = Vec::new();
        for part in path.split('/') {
            match part {
                "" | "." => {}
                ".." => { parts.pop(); }
                _ => parts.push(part),
            }
        }
        format!("/{}", parts.join("/"))
    }

    fn parent_path(&self, path: &str) -> Option<String> {
        let normalized = self.normalize_path(path);
        if normalized == "/" {
            return None;
        }
        let mut parts: Vec<&str> = normalized.split('/').filter(|s| !s.is_empty()).collect();
        parts.pop();
        if parts.is_empty() {
            Some("/".to_string())
        } else {
            Some(format!("/{}", parts.join("/")))
        }
    }

    fn join_path(&self, base: &str, name: &str) -> String {
        format!("{}/{}", base.trim_end_matches('/'), name)
    }

    fn to_local_path(&self, _path: &str) -> Option<PathBuf> {
        None // Remote paths can't be converted to local
    }

    fn from_local_path(&self, _path: &Path) -> Option<String> {
        None // Local paths can't be used with remote provider
    }
}

impl Drop for ScpProvider {
    fn drop(&mut self) {
        self.disconnect();
    }
}
