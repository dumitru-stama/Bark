//! Provider Plugin Adapter
//!
//! Bridges the plugin API's ProviderSession to the app's PanelProvider trait.

use std::path::{Path, PathBuf};

// Re-export common types from the plugin API crate
pub use bark_plugin_api::{
    DialogField, DialogFieldType, FileEntry as PluginFileEntry, ProviderConfig,
    ProviderError as ProviderPluginError, ProviderPlugin, ProviderPluginInfo,
    ProviderResult as ProviderPluginResult, ProviderSession,
};

use crate::fs::FileEntry;
use crate::providers::{PanelProvider, ProviderError, ProviderInfo, ProviderResult, ProviderType};

/// Wrapper to adapt ProviderSession to PanelProvider trait
#[allow(dead_code)]
pub struct PluginProviderAdapter {
    session: Box<dyn ProviderSession>,
    provider_info: ProviderInfo,
    short_label_value: Option<String>,
}

#[allow(dead_code)]
impl PluginProviderAdapter {
    pub fn new(session: Box<dyn ProviderSession>, plugin_info: &ProviderPluginInfo) -> Self {
        let display_name = session.display_name();
        let short_label_value = session.short_label();
        Self {
            provider_info: ProviderInfo {
                name: display_name,
                description: plugin_info.description.clone(),
                provider_type: ProviderType::Plugin,
                icon: plugin_info.icon,
            },
            short_label_value,
            session,
        }
    }

    /// Get the provider info
    pub fn get_info(&self) -> ProviderInfo {
        self.provider_info.clone()
    }

    /// Get display name
    pub fn get_display_name(&self) -> &str {
        &self.provider_info.name
    }
}

/// Convert plugin FileEntry to app FileEntry
fn convert_file_entry(entry: PluginFileEntry) -> FileEntry {
    FileEntry {
        name: entry.name,
        path: entry.path,
        is_dir: entry.is_dir,
        size: entry.size,
        modified: entry.modified,
        is_hidden: entry.is_hidden,
        permissions: entry.permissions,
        is_symlink: entry.is_symlink,
        symlink_target: entry.symlink_target,
        owner: entry.owner,
        group: entry.group,
    }
}

/// Convert plugin error to provider error
fn convert_error(e: ProviderPluginError) -> ProviderError {
    match e {
        ProviderPluginError::Connection(s) => ProviderError::Connection(s),
        ProviderPluginError::Auth(s) => ProviderError::Auth(s),
        ProviderPluginError::NotFound(s) => ProviderError::NotFound(s),
        ProviderPluginError::PermissionDenied(s) => ProviderError::PermissionDenied(s),
        ProviderPluginError::PasswordRequired(s) => ProviderError::PasswordRequired(s),
        ProviderPluginError::PluginError(s) => ProviderError::Other(s),
        ProviderPluginError::ConfigError(s) => ProviderError::Other(s),
        ProviderPluginError::Other(s) => ProviderError::Other(s),
    }
}

impl PanelProvider for PluginProviderAdapter {
    fn info(&self) -> &ProviderInfo {
        &self.provider_info
    }

    fn is_connected(&self) -> bool {
        self.session.is_connected()
    }

    fn connect(&mut self) -> ProviderResult<()> {
        // Already connected when created
        Ok(())
    }

    fn disconnect(&mut self) {
        self.session.disconnect();
    }

    fn list_directory(&mut self, path: &str) -> ProviderResult<Vec<FileEntry>> {
        self.session
            .list_directory(path)
            .map(|entries| entries.into_iter().map(convert_file_entry).collect())
            .map_err(convert_error)
    }

    fn read_file(&mut self, path: &str) -> ProviderResult<Vec<u8>> {
        self.session.read_file(path).map_err(convert_error)
    }

    fn write_file(&mut self, path: &str, data: &[u8]) -> ProviderResult<()> {
        self.session.write_file(path, data).map_err(convert_error)
    }

    fn delete(&mut self, path: &str) -> ProviderResult<()> {
        self.session.delete(path).map_err(convert_error)
    }

    fn delete_recursive(&mut self, path: &str) -> ProviderResult<()> {
        self.session.delete_recursive(path).map_err(convert_error)
    }

    fn rename(&mut self, from: &str, to: &str) -> ProviderResult<()> {
        self.session.rename(from, to).map_err(convert_error)
    }

    fn mkdir(&mut self, path: &str) -> ProviderResult<()> {
        self.session.mkdir(path).map_err(convert_error)
    }

    fn copy_file(&mut self, from: &str, to: &str) -> ProviderResult<()> {
        self.session.copy_file(from, to).map_err(convert_error)
    }

    fn set_attributes(
        &mut self,
        path: &str,
        modified: Option<std::time::SystemTime>,
        permissions: u32,
    ) -> ProviderResult<()> {
        self.session.set_attributes(path, modified, permissions).map_err(convert_error)
    }

    fn get_free_space(&self, path: &str) -> Option<u64> {
        self.session.get_free_space(path)
    }

    fn is_local(&self) -> bool {
        false
    }

    fn short_label(&self) -> Option<String> {
        self.short_label_value.clone()
    }

    fn home_path(&self) -> String {
        self.session.home_path()
    }

    fn normalize_path(&self, path: &str) -> String {
        self.session.normalize_path(path)
    }

    fn parent_path(&self, path: &str) -> Option<String> {
        self.session.parent_path(path)
    }

    fn join_path(&self, base: &str, name: &str) -> String {
        self.session.join_path(base, name)
    }

    fn to_local_path(&self, _path: &str) -> Option<PathBuf> {
        None
    }

    fn from_local_path(&self, _path: &Path) -> Option<String> {
        None
    }

    fn set_password(&mut self, password: &str) -> crate::providers::ProviderResult<()> {
        self.session.set_password(password).map_err(convert_error)
    }
}
