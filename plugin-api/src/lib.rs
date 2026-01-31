//! Bark Plugin API
//!
//! This crate defines the unified interface for all Bark plugins:
//! - **Provider plugins**: Remote filesystem access (FTP, S3, Google Drive, etc.)
//! - **Viewer plugins**: Custom file viewers (ELF, images, PDFs, etc.)
//! - **Status plugins**: Status bar extensions (system info, git status, etc.)
//!
//! All plugins are external executables that communicate via JSON over stdin/stdout.
//! Protocol:
//! - `--plugin-info`: Print plugin metadata as JSON
//! - stdin/stdout: JSON commands and responses

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::SystemTime;

// ============================================================================
// PLUGIN TYPE
// ============================================================================

/// Type of plugin
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginType {
    /// Remote filesystem provider (FTP, S3, etc.)
    Provider,
    /// Custom file viewer (ELF, images, etc.)
    Viewer,
    /// Status bar extension
    Status,
}

impl PluginType {
    pub fn as_str(&self) -> &'static str {
        match self {
            PluginType::Provider => "provider",
            PluginType::Viewer => "viewer",
            PluginType::Status => "status",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "provider" => Some(PluginType::Provider),
            "viewer" => Some(PluginType::Viewer),
            "status" => Some(PluginType::Status),
            _ => None,
        }
    }
}

// ============================================================================
// UNIFIED PLUGIN INFO
// ============================================================================

/// Information about any plugin type
#[derive(Debug, Clone)]
pub struct PluginInfo {
    /// Plugin name (e.g., "FTP Provider", "ELF Viewer")
    pub name: String,
    /// Plugin version
    pub version: String,
    /// Plugin type
    pub plugin_type: PluginType,
    /// Short description
    pub description: String,
    /// Icon character for display
    pub icon: Option<char>,
    /// Path to the plugin executable
    pub source: PathBuf,
    /// For provider plugins: URI schemes handled (e.g., ["ftp", "ftps"])
    pub schemes: Vec<String>,
    /// For viewer plugins: file extensions handled (e.g., ["elf", "so", "o"])
    pub extensions: Vec<String>,
    /// For viewer plugins: MIME types handled
    pub mime_types: Vec<String>,
}

impl PluginInfo {
    /// Create a new provider plugin info
    pub fn provider(name: impl Into<String>, version: impl Into<String>, schemes: Vec<String>) -> Self {
        Self {
            name: name.into(),
            version: version.into(),
            plugin_type: PluginType::Provider,
            description: String::new(),
            icon: None,
            source: PathBuf::new(),
            schemes,
            extensions: Vec::new(),
            mime_types: Vec::new(),
        }
    }

    /// Create a new viewer plugin info
    pub fn viewer(name: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            version: version.into(),
            plugin_type: PluginType::Viewer,
            description: String::new(),
            icon: None,
            source: PathBuf::new(),
            schemes: Vec::new(),
            extensions: Vec::new(),
            mime_types: Vec::new(),
        }
    }

    /// Create a new status plugin info
    pub fn status(name: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            version: version.into(),
            plugin_type: PluginType::Status,
            description: String::new(),
            icon: None,
            source: PathBuf::new(),
            schemes: Vec::new(),
            extensions: Vec::new(),
            mime_types: Vec::new(),
        }
    }

    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = desc.into();
        self
    }

    pub fn with_icon(mut self, icon: char) -> Self {
        self.icon = Some(icon);
        self
    }

    pub fn with_extensions(mut self, exts: Vec<String>) -> Self {
        self.extensions = exts;
        self
    }

    pub fn with_mime_types(mut self, mimes: Vec<String>) -> Self {
        self.mime_types = mimes;
        self
    }
}

/// Legacy alias for backwards compatibility
pub type ProviderPluginInfo = PluginInfo;

// ============================================================================
// FILE ENTRY
// ============================================================================

/// Represents a single file or directory entry returned by provider plugins
#[derive(Clone, Debug)]
pub struct FileEntry {
    /// File/directory name (not full path)
    pub name: String,
    /// Full path to the entry (within the provider's namespace)
    pub path: PathBuf,
    /// Whether this is a directory
    pub is_dir: bool,
    /// File size in bytes (0 for directories)
    pub size: u64,
    /// Last modification time
    pub modified: Option<SystemTime>,
    /// Whether this is a hidden file
    pub is_hidden: bool,
    /// Unix permission bits (0 if not available)
    pub permissions: u32,
    /// Whether this is a symbolic link
    pub is_symlink: bool,
    /// Target of symlink if applicable
    pub symlink_target: Option<PathBuf>,
    /// Owner user name (empty if not available)
    pub owner: String,
    /// Owner group name (empty if not available)
    pub group: String,
}

impl FileEntry {
    /// Create a new file entry with minimal required fields
    pub fn new(name: String, path: PathBuf, is_dir: bool, size: u64) -> Self {
        Self {
            name,
            path,
            is_dir,
            size,
            modified: None,
            is_hidden: false,
            permissions: 0,
            is_symlink: false,
            symlink_target: None,
            owner: String::new(),
            group: String::new(),
        }
    }

    /// Create a directory entry
    pub fn directory(name: String, path: PathBuf) -> Self {
        Self::new(name, path, true, 0)
    }

    /// Create a file entry
    pub fn file(name: String, path: PathBuf, size: u64) -> Self {
        Self::new(name, path, false, size)
    }

    /// Create a parent directory (..) entry
    pub fn parent(parent_path: PathBuf) -> Self {
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

    /// Set modification time
    pub fn with_modified(mut self, time: Option<SystemTime>) -> Self {
        self.modified = time;
        self
    }

    /// Set hidden flag
    pub fn with_hidden(mut self, hidden: bool) -> Self {
        self.is_hidden = hidden;
        self
    }

    /// Set permissions
    pub fn with_permissions(mut self, perms: u32) -> Self {
        self.permissions = perms;
        self
    }

    /// Set symlink info
    pub fn with_symlink(mut self, target: Option<PathBuf>) -> Self {
        self.is_symlink = target.is_some();
        self.symlink_target = target;
        self
    }

    /// Set owner and group
    pub fn with_ownership(mut self, owner: String, group: String) -> Self {
        self.owner = owner;
        self.group = group;
        self
    }

    /// Get the file extension (lowercase), if any
    pub fn extension(&self) -> Option<&str> {
        self.path.extension().and_then(|s| s.to_str())
    }
}

// ============================================================================
// DIALOG FIELDS (for provider plugins)
// ============================================================================

/// Describes a field for the connection dialog
#[derive(Debug, Clone)]
pub struct DialogField {
    /// Field identifier (used as config key)
    pub id: String,
    /// Display label
    pub label: String,
    /// Field type
    pub field_type: DialogFieldType,
    /// Default value (if any)
    pub default_value: Option<String>,
    /// Placeholder text
    pub placeholder: Option<String>,
    /// Whether this field is required
    pub required: bool,
    /// Help text shown below the field
    pub help_text: Option<String>,
}

/// Type of dialog field
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DialogFieldType {
    /// Single-line text input
    Text,
    /// Password input (masked)
    Password,
    /// Numeric input
    Number,
    /// Checkbox (boolean)
    Checkbox,
    /// Dropdown selection (value, label) pairs
    Select { options: Vec<(String, String)> },
    /// Multi-line text input
    TextArea,
    /// File path selector
    FilePath,
}

// ============================================================================
// CONFIGURATION (for provider plugins)
// ============================================================================

/// Configuration for creating a provider connection
#[derive(Debug, Clone, Default)]
pub struct ProviderConfig {
    /// Key-value configuration
    pub values: HashMap<String, String>,
    /// Connection name (for display/saving)
    pub name: String,
}

impl ProviderConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.values.get(key).map(|s| s.as_str())
    }

    pub fn get_bool(&self, key: &str) -> bool {
        self.values
            .get(key)
            .map(|v| matches!(v.as_str(), "true" | "1" | "yes" | "on"))
            .unwrap_or(false)
    }

    pub fn get_int(&self, key: &str) -> Option<i64> {
        self.values.get(key)?.parse().ok()
    }

    pub fn set(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.values.insert(key.into(), value.into());
    }

    pub fn set_bool(&mut self, key: impl Into<String>, value: bool) {
        self.values.insert(
            key.into(),
            if value { "true" } else { "false" }.to_string(),
        );
    }

    /// Convert to URI format for storage
    pub fn to_uri(&self, scheme: &str) -> String {
        let mut uri = format!("{}://", scheme);
        if let Some(host) = self.values.get("host") {
            uri.push_str(host);
            if let Some(port) = self.values.get("port")
                && !port.is_empty()
            {
                uri.push(':');
                uri.push_str(port);
            }
        }
        if let Some(path) = self.values.get("path") {
            if !path.is_empty() && !path.starts_with('/') {
                uri.push('/');
            }
            uri.push_str(path);
        }
        uri
    }
}

// ============================================================================
// VIEWER CONTEXT (for viewer plugins)
// ============================================================================

/// Context passed to viewer plugins for rendering
#[derive(Debug, Clone)]
pub struct ViewerContext {
    /// Path to the file being viewed
    pub path: PathBuf,
    /// Available width in characters
    pub width: usize,
    /// Available height in lines
    pub height: usize,
    /// Current scroll offset (line number)
    pub scroll: usize,
    /// File size in bytes
    pub file_size: u64,
}

/// Result from viewer rendering
#[derive(Debug, Clone)]
pub struct ViewerResult {
    /// Lines of content to display
    pub lines: Vec<String>,
    /// Total number of lines in the document
    pub total_lines: usize,
    /// Optional title override
    pub title: Option<String>,
}

// ============================================================================
// STATUS CONTEXT (for status plugins)
// ============================================================================

/// Context passed to status plugins
#[derive(Debug, Clone)]
pub struct StatusContext {
    /// Current directory path
    pub current_path: PathBuf,
    /// Currently selected file name (if any)
    pub selected_file: Option<String>,
    /// Number of selected files
    pub selected_count: usize,
    /// Total size of selected files
    pub selected_size: u64,
    /// Total number of files in current directory
    pub total_files: usize,
}

/// Result from status rendering
#[derive(Debug, Clone)]
pub struct StatusResult {
    /// Text to display in status bar
    pub text: String,
}

// ============================================================================
// ERRORS
// ============================================================================

/// Result type for plugin operations
pub type ProviderResult<T> = Result<T, ProviderError>;

/// Errors from plugins
#[derive(Debug, Clone)]
pub enum ProviderError {
    /// Connection failed
    Connection(String),
    /// Authentication failed
    Auth(String),
    /// File/directory not found
    NotFound(String),
    /// Permission denied
    PermissionDenied(String),
    /// Password required (e.g., encrypted archive)
    PasswordRequired(String),
    /// Plugin communication error
    PluginError(String),
    /// Configuration error
    ConfigError(String),
    /// Generic error
    Other(String),
}

impl std::fmt::Display for ProviderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProviderError::Connection(s) => write!(f, "Connection error: {}", s),
            ProviderError::Auth(s) => write!(f, "Authentication error: {}", s),
            ProviderError::NotFound(s) => write!(f, "Not found: {}", s),
            ProviderError::PermissionDenied(s) => write!(f, "Permission denied: {}", s),
            ProviderError::PasswordRequired(s) => write!(f, "Password required: {}", s),
            ProviderError::PluginError(s) => write!(f, "Plugin error: {}", s),
            ProviderError::ConfigError(s) => write!(f, "Configuration error: {}", s),
            ProviderError::Other(s) => write!(f, "{}", s),
        }
    }
}

impl std::error::Error for ProviderError {}

// ============================================================================
// PLUGIN TRAITS
// ============================================================================

/// Trait for provider plugins (remote filesystems)
pub trait ProviderPlugin: Send + Sync {
    /// Get plugin information
    fn info(&self) -> &PluginInfo;

    /// Get dialog fields for connection configuration
    fn get_dialog_fields(&self) -> Vec<DialogField>;

    /// Validate configuration before connecting
    fn validate_config(&self, config: &ProviderConfig) -> ProviderResult<()>;

    /// Create a new session/connection
    fn connect(&self, config: &ProviderConfig) -> ProviderResult<Box<dyn ProviderSession>>;
}

/// Trait for an active provider session
pub trait ProviderSession: Send {
    /// Get the display name for this session
    fn display_name(&self) -> String;

    /// Get a short label for panel headers (e.g., "[ZIP]", "[FTP]")
    /// Returns None if no special label is needed
    fn short_label(&self) -> Option<String> {
        None
    }

    /// Check if still connected
    fn is_connected(&self) -> bool;

    /// Disconnect/cleanup
    fn disconnect(&mut self);

    /// List directory contents
    fn list_directory(&mut self, path: &str) -> ProviderResult<Vec<FileEntry>>;

    /// Read file contents
    fn read_file(&mut self, path: &str) -> ProviderResult<Vec<u8>>;

    /// Write file contents
    fn write_file(&mut self, path: &str, data: &[u8]) -> ProviderResult<()>;

    /// Delete a file or empty directory
    fn delete(&mut self, path: &str) -> ProviderResult<()>;

    /// Delete recursively
    fn delete_recursive(&mut self, path: &str) -> ProviderResult<()>;

    /// Rename/move a file or directory
    fn rename(&mut self, from: &str, to: &str) -> ProviderResult<()>;

    /// Create a directory
    fn mkdir(&mut self, path: &str) -> ProviderResult<()>;

    /// Copy a file (within the same provider)
    fn copy_file(&mut self, from: &str, to: &str) -> ProviderResult<()>;

    /// Set file attributes (modification time, permissions) on a remote path.
    /// Best-effort — the default implementation is a no-op for plugins that
    /// don't support setting attributes.
    #[allow(unused_variables)]
    fn set_attributes(
        &mut self,
        path: &str,
        modified: Option<SystemTime>,
        permissions: u32,
    ) -> ProviderResult<()> {
        Ok(())
    }

    /// Get free space (if supported)
    fn get_free_space(&self, _path: &str) -> Option<u64> {
        None
    }

    /// Get home/initial path
    fn home_path(&self) -> String {
        "/".to_string()
    }

    /// Normalize a path
    fn normalize_path(&self, path: &str) -> String {
        let mut parts: Vec<&str> = Vec::new();
        for part in path.split('/') {
            match part {
                "" | "." => {}
                ".." => {
                    parts.pop();
                }
                _ => parts.push(part),
            }
        }
        format!("/{}", parts.join("/"))
    }

    /// Get parent path
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

    /// Join path components
    fn join_path(&self, base: &str, name: &str) -> String {
        format!("{}/{}", base.trim_end_matches('/'), name)
    }

    /// Set or update the password for this session (e.g., for encrypted archives).
    /// Default is a no-op. Plugins that support passwords should override this.
    #[allow(unused_variables)]
    fn set_password(&mut self, password: &str) -> ProviderResult<()> {
        Ok(())
    }
}

/// Trait for viewer plugins (custom file viewers)
pub trait ViewerPlugin: Send + Sync {
    /// Get plugin information
    fn info(&self) -> &PluginInfo;

    /// Check if this plugin can handle the given file
    /// Returns (can_handle, priority) - higher priority wins
    fn can_handle(&self, path: &std::path::Path, magic_bytes: &[u8]) -> (bool, i32);

    /// Render the file content
    fn render(&self, context: &ViewerContext, file_data: &[u8]) -> ProviderResult<ViewerResult>;
}

/// Trait for status bar plugins
pub trait StatusPlugin: Send + Sync {
    /// Get plugin information
    fn info(&self) -> &PluginInfo;

    /// Render the status bar content
    fn render(&self, context: &StatusContext) -> ProviderResult<StatusResult>;
}
