//! Panel providers for different filesystem backends
//!
//! Providers abstract filesystem operations, allowing panels to work with:
//! - Local filesystems
//! - Remote filesystems via SCP/SFTP
//! - Other backends (FTP, cloud storage, etc.)

#![allow(dead_code)]

mod local;
mod scp;

pub use local::LocalProvider;
pub use scp::{ScpAuth, ScpProvider, ScpConnectionInfo};

use crate::fs::FileEntry;
use std::path::{Path, PathBuf};

use thiserror::Error;

/// Error type for provider operations
#[derive(Error, Debug)]
#[allow(dead_code)]
pub enum ProviderError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Connection error: {0}")]
    Connection(String),
    #[error("Authentication error: {0}")]
    Auth(String),
    #[error("Not found: {0}")]
    NotFound(String),
    #[error("Permission denied: {0}")]
    PermissionDenied(String),
    #[error("Password required: {0}")]
    PasswordRequired(String),
    #[error("Not supported: {0}")]
    NotSupported(String),
    #[error("{0}")]
    Other(String),
}

pub type ProviderResult<T> = Result<T, ProviderError>;

/// Information about a provider for display in the source selector
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ProviderInfo {
    /// Display name (e.g., "Local", "scp://user@host")
    pub name: String,
    /// Short description
    pub description: String,
    /// Provider type identifier
    pub provider_type: ProviderType,
    /// Icon/prefix for display (optional)
    pub icon: Option<char>,
}

/// Type of provider
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum ProviderType {
    /// Local filesystem
    Local,
    /// SCP/SFTP remote
    Scp,
    /// Archive (zip, tar, etc.)
    Archive,
    /// Cloud storage (reserved)
    Cloud,
    /// Plugin-provided provider
    Plugin,
}

/// Trait for panel filesystem providers
///
/// Implementors provide filesystem-like operations for panels.
/// All paths are provider-relative strings (e.g., "/home/user" for local,
/// "/remote/path" for SCP).
#[allow(dead_code)]
pub trait PanelProvider: Send {
    /// Get provider information for display
    fn info(&self) -> &ProviderInfo;

    /// Check if this provider is connected/ready
    fn is_connected(&self) -> bool;

    /// Connect to the provider (no-op for local)
    fn connect(&mut self) -> ProviderResult<()>;

    /// Disconnect from the provider (no-op for local)
    fn disconnect(&mut self);

    /// List directory contents
    fn list_directory(&mut self, path: &str) -> ProviderResult<Vec<FileEntry>>;

    /// Read file contents
    fn read_file(&mut self, path: &str) -> ProviderResult<Vec<u8>>;

    /// Write file contents
    fn write_file(&mut self, path: &str, data: &[u8]) -> ProviderResult<()>;

    /// Delete a file or empty directory
    fn delete(&mut self, path: &str) -> ProviderResult<()>;

    /// Delete a directory recursively
    fn delete_recursive(&mut self, path: &str) -> ProviderResult<()>;

    /// Rename/move a file or directory
    fn rename(&mut self, from: &str, to: &str) -> ProviderResult<()>;

    /// Create a directory
    fn mkdir(&mut self, path: &str) -> ProviderResult<()>;

    /// Copy a file (within the same provider)
    fn copy_file(&mut self, from: &str, to: &str) -> ProviderResult<()>;

    /// Set file attributes (modification time, permissions) on a remote path.
    /// Best-effort — the default implementation is a no-op for providers that
    /// don't support setting attributes.
    #[allow(unused_variables)]
    fn set_attributes(
        &mut self,
        path: &str,
        modified: Option<std::time::SystemTime>,
        permissions: u32,
    ) -> ProviderResult<()> {
        Ok(())
    }

    /// Get free space at path (if available)
    fn get_free_space(&self, path: &str) -> Option<u64>;

    /// Check if this is a local provider (for operations needing local paths)
    fn is_local(&self) -> bool;

    /// Get a short label for the panel header (e.g., "[ZIP]" for archives)
    /// Returns None for providers that don't need a special label
    fn short_label(&self) -> Option<String> {
        None
    }

    /// Get the current working directory / home path
    fn home_path(&self) -> String;

    /// Normalize a path for this provider
    fn normalize_path(&self, path: &str) -> String;

    /// Get parent path
    fn parent_path(&self, path: &str) -> Option<String>;

    /// Join path components
    fn join_path(&self, base: &str, name: &str) -> String;

    /// Convert provider path to local PathBuf (for local provider)
    /// Returns None for remote providers
    fn to_local_path(&self, path: &str) -> Option<PathBuf>;

    /// Convert local PathBuf to provider path (for local provider)
    /// Returns None for remote providers
    #[allow(clippy::wrong_self_convention)]
    fn from_local_path(&self, path: &Path) -> Option<String>;

    /// Set or update the password for this provider session.
    /// Default is a no-op. Archive/encrypted providers should override this.
    #[allow(unused_variables)]
    fn set_password(&mut self, password: &str) -> ProviderResult<()> {
        Ok(())
    }
}

/// Source entry for the panel source selector
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum PanelSource {
    /// A drive letter (Windows) or mount point
    Drive {
        letter: String,
        label: Option<String>,
    },
    /// Quick access path (home, root, favorites)
    QuickAccess {
        name: String,
        path: String,
        /// True if this is a user favorite (can be deleted)
        is_favorite: bool,
    },
    /// A saved provider connection
    Provider {
        info: ProviderInfo,
        /// Connection details for recreation
        connection_string: String,
        /// Name used in config (for edit/delete)
        connection_name: String,
    },
    /// Option to create a new connection (built-in)
    NewConnection {
        provider_type: ProviderType,
    },
    /// Option to create a new plugin connection
    NewPluginConnection {
        /// Plugin display name (e.g., "FTP Provider")
        plugin_name: String,
        /// URI scheme (e.g., "ftp")
        scheme: String,
        /// Icon from plugin info
        icon: Option<char>,
    },
}

#[allow(dead_code)]
impl PanelSource {
    /// Get display name for this source
    pub fn display_name(&self) -> String {
        match self {
            PanelSource::Drive { letter, label } => {
                if let Some(l) = label {
                    format!("{} ({})", letter, l)
                } else {
                    letter.clone()
                }
            }
            PanelSource::QuickAccess { name, .. } => name.clone(),
            PanelSource::Provider { info, .. } => info.name.clone(),
            PanelSource::NewConnection { provider_type } => {
                match provider_type {
                    ProviderType::Scp => "+ New SCP Connection...".to_string(),
                    ProviderType::Cloud => "+ New Cloud Connection...".to_string(),
                    ProviderType::Local => "Local".to_string(),
                    ProviderType::Archive => "Archive".to_string(),
                    ProviderType::Plugin => "+ New Plugin Connection...".to_string(),
                }
            }
            PanelSource::NewPluginConnection { plugin_name, .. } => {
                format!("+ New {} Connection...", plugin_name)
            }
        }
    }

    /// Get icon character for this source
    pub fn icon(&self) -> char {
        match self {
            PanelSource::Drive { .. } => '💾',
            PanelSource::QuickAccess { .. } => '📁',
            PanelSource::Provider { info, .. } => info.icon.unwrap_or('🌐'),
            PanelSource::NewConnection { .. } => '+',
            PanelSource::NewPluginConnection { icon, .. } => icon.unwrap_or('+'),
        }
    }
}

/// Info about a loaded provider plugin, used for generating source selector entries
pub struct ProviderPluginSummary {
    /// Plugin display name (e.g., "FTP Provider")
    pub name: String,
    /// URI schemes handled (e.g., ["ftp", "ftps"])
    pub schemes: Vec<String>,
    /// Icon character
    pub icon: Option<char>,
}

/// Get available panel sources for the source selector
pub fn get_panel_sources(
    saved_connections: &[crate::config::SavedConnection],
    plugin_connections: &[crate::config::SavedPluginConnection],
    favorites: &[crate::config::FavoritePath],
    provider_plugins: &[ProviderPluginSummary],
) -> Vec<PanelSource> {
    let mut sources = Vec::new();

    // Add drives (Windows) or quick access paths (Unix)
    #[cfg(windows)]
    {
        let drives = crate::utils::get_available_drives();
        for drive in drives {
            sources.push(PanelSource::Drive {
                letter: drive,
                label: None,
            });
        }
    }

    #[cfg(not(windows))]
    {
        // Add common quick access paths
        if let Ok(home) = std::env::var("HOME") {
            sources.push(PanelSource::QuickAccess {
                name: "Home".to_string(),
                path: home,
                is_favorite: false,
            });
        }
        sources.push(PanelSource::QuickAccess {
            name: "Root".to_string(),
            path: "/".to_string(),
            is_favorite: false,
        });
    }

    // Add user favorites
    for fav in favorites {
        sources.push(PanelSource::QuickAccess {
            name: format!("★ {}", fav.name),
            path: fav.path.clone(),
            is_favorite: true,
        });
    }

    // Add saved SCP connections
    for conn in saved_connections {
        let uri = format!(
            "scp://{}@{}{}{}",
            conn.user,
            conn.host,
            if conn.port != 22 { format!(":{}", conn.port) } else { String::new() },
            conn.path.as_deref().unwrap_or("")
        );

        let display_name = if conn.name.is_empty() {
            if conn.port != 22 {
                format!("SCP - {}@{}:{}", conn.user, conn.host, conn.port)
            } else {
                format!("SCP - {}@{}", conn.user, conn.host)
            }
        } else {
            format!("SCP - {}", conn.name)
        };

        sources.push(PanelSource::Provider {
            info: ProviderInfo {
                name: display_name,
                description: format!("SSH connection to {}", conn.host),
                provider_type: ProviderType::Scp,
                icon: Some('🔒'),
            },
            connection_string: uri,
            connection_name: conn.name.clone(),
        });
    }

    // Add saved plugin connections (FTP, WebDAV, S3, etc. — driven by loaded plugins)
    for conn in plugin_connections {
        // Only show if the plugin for this scheme is loaded
        let plugin_loaded = provider_plugins.iter().any(|p| p.schemes.iter().any(|s| s == &conn.scheme));
        if !plugin_loaded {
            continue;
        }

        let icon = provider_plugins.iter()
            .find(|p| p.schemes.iter().any(|s| s == &conn.scheme))
            .and_then(|p| p.icon);

        let display_name = format!("{} - {}", conn.scheme.to_uppercase(), conn.name);

        sources.push(PanelSource::Provider {
            info: ProviderInfo {
                name: display_name,
                description: format!("{} connection: {}", conn.scheme.to_uppercase(), conn.name),
                provider_type: ProviderType::Plugin,
                icon,
            },
            connection_string: conn.scheme.clone(),
            connection_name: conn.name.clone(),
        });
    }

    // Add built-in "New Connection" options
    sources.push(PanelSource::NewConnection {
        provider_type: ProviderType::Scp,
    });

    // Add "New Connection" for each loaded provider plugin
    for plugin in provider_plugins {
        let scheme = plugin.schemes.first().cloned().unwrap_or_default();
        if !scheme.is_empty() {
            sources.push(PanelSource::NewPluginConnection {
                plugin_name: plugin.name.clone(),
                scheme,
                icon: plugin.icon,
            });
        }
    }

    sources
}
