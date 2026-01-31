//! Plugin manager - loads and manages all plugins

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;

use crate::plugins::api::*;
use crate::plugins::provider_script::ScriptProviderPlugin;
use crate::plugins::script::ScriptPlugin;

use bark_plugin_api::{ProviderPlugin, ProviderPluginInfo};

/// Manages all loaded plugins
pub struct PluginManager {
    /// Status bar plugins
    status_plugins: Vec<Arc<dyn StatusBarPlugin>>,
    /// Viewer plugins
    viewer_plugins: Vec<Arc<dyn ViewerPlugin>>,
    /// Provider plugins (for remote filesystems like S3, GDrive, etc.)
    provider_plugins: Vec<Arc<dyn ProviderPlugin>>,
    /// Plugin directory
    plugin_dir: Option<PathBuf>,
}

#[allow(dead_code)]
impl PluginManager {
    /// Create a new plugin manager
    pub fn new() -> Self {
        PluginManager {
            status_plugins: Vec::new(),
            viewer_plugins: Vec::new(),
            provider_plugins: Vec::new(),
            plugin_dir: None,
        }
    }

    /// Load all plugins from a single directory
    pub fn load_from_directory(&mut self, dir: &Path) -> Vec<String> {
        self.plugin_dir = Some(dir.to_path_buf());
        let mut errors = Vec::new();

        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return errors,
        };

        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }

            // Check if it's an executable
            if !is_executable(&path) {
                continue;
            }

            // Query plugin type via --plugin-info
            let plugin_type = match query_plugin_type(&path) {
                Some(t) => t,
                None => continue,
            };

            match plugin_type.as_str() {
                "provider" => {
                    match ScriptProviderPlugin::load(&path) {
                        Ok(plugin) => self.provider_plugins.push(Arc::new(plugin)),
                        Err(e) => errors.push(format!("{}: {}", path.display(), e)),
                    }
                }
                "status" | "statusbar" | "status_bar" | "viewer" | "view" => {
                    match ScriptPlugin::load(&path) {
                        Ok(plugin) => self.register_script_plugin(plugin),
                        Err(e) => errors.push(format!("{}: {}", path.display(), e)),
                    }
                }
                _ => {
                    errors.push(format!("{}: unknown plugin type '{}'", path.display(), plugin_type));
                }
            }
        }

        errors
    }

    fn register_script_plugin(&mut self, plugin: ScriptPlugin) {
        let info = plugin.info().clone();
        let arc: Arc<ScriptPlugin> = Arc::new(plugin);

        match info.plugin_type {
            PluginType::StatusBar => self.status_plugins.push(arc.clone()),
            PluginType::Viewer => self.viewer_plugins.push(arc.clone()),
        }
    }

    /// Get all status bar plugin outputs
    pub fn render_status(&self, context: &StatusContext) -> Vec<(String, String)> {
        self.status_plugins
            .iter()
            .filter_map(|p| {
                p.render(context).map(|r| (p.info().name.clone(), r.text))
            })
            .collect()
    }

    /// Find a viewer plugin that can handle the given file
    pub fn find_viewer(&self, path: &Path) -> Option<Arc<dyn ViewerPlugin>> {
        let mut best: Option<(Arc<dyn ViewerPlugin>, i32)> = None;

        for plugin in &self.viewer_plugins {
            let result = plugin.can_handle(path);
            if result.can_handle {
                match &best {
                    None => best = Some((plugin.clone(), result.priority)),
                    Some((_, priority)) if result.priority > *priority => {
                        best = Some((plugin.clone(), result.priority));
                    }
                    _ => {}
                }
            }
        }

        best.map(|(p, _)| p)
    }

    /// Render file content using a viewer plugin
    pub fn render_viewer(&self, plugin: &dyn ViewerPlugin, context: &ViewerContext) -> Option<ViewerRenderResult> {
        plugin.render(context)
    }

    /// Get list of loaded plugins
    pub fn list_plugins(&self) -> Vec<&PluginInfo> {
        let mut plugins: Vec<&PluginInfo> = Vec::new();

        for p in &self.status_plugins {
            plugins.push(p.info());
        }
        for p in &self.viewer_plugins {
            if !plugins.iter().any(|i| std::ptr::eq(*i, p.info())) {
                plugins.push(p.info());
            }
        }

        plugins
    }

    /// Number of loaded plugins
    pub fn plugin_count(&self) -> usize {
        self.status_plugins.len() + self.viewer_plugins.len() + self.provider_plugins.len()
    }

    /// List viewer plugins that can handle the given file
    pub fn list_viewer_plugins(&self, path: &Path) -> Vec<(String, bool)> {
        self.viewer_plugins
            .iter()
            .filter_map(|p| {
                let result = p.can_handle(path);
                if result.can_handle {
                    Some((p.info().name.clone(), true))
                } else {
                    None
                }
            })
            .collect()
    }

    /// Find a viewer plugin by name
    pub fn find_viewer_by_name(&self, name: &str) -> Option<Arc<dyn ViewerPlugin>> {
        self.viewer_plugins
            .iter()
            .find(|p| p.info().name == name)
            .cloned()
    }

    /// Get all provider plugins
    pub fn provider_plugins(&self) -> &[Arc<dyn ProviderPlugin>] {
        &self.provider_plugins
    }

    /// Find provider plugins that handle a given URI scheme
    pub fn find_provider_by_scheme(&self, scheme: &str) -> Option<Arc<dyn ProviderPlugin>> {
        self.provider_plugins
            .iter()
            .find(|p| p.info().schemes.iter().any(|s| s.eq_ignore_ascii_case(scheme)))
            .cloned()
    }

    /// Find a provider plugin that handles the given file extension
    /// Checks the filename against each plugin's declared extensions list
    pub fn find_provider_by_extension(&self, path: &std::path::Path) -> Option<Arc<dyn ProviderPlugin>> {
        let name = path.file_name()?.to_string_lossy().to_lowercase();
        self.provider_plugins
            .iter()
            .find(|p| {
                p.info().extensions.iter().any(|ext| name.ends_with(&ext.to_lowercase()))
            })
            .cloned()
    }

    /// List all provider plugins with their info (scheme-based only, for source selector)
    pub fn list_provider_plugins(&self) -> Vec<&ProviderPluginInfo> {
        self.provider_plugins
            .iter()
            .map(|p| p.info())
            .filter(|info| !info.schemes.is_empty())
            .collect()
    }

    /// List all provider plugins (including extension-based)
    pub fn list_all_provider_plugins(&self) -> Vec<&ProviderPluginInfo> {
        self.provider_plugins
            .iter()
            .map(|p| p.info())
            .collect()
    }

    /// Get number of provider plugins
    pub fn provider_plugin_count(&self) -> usize {
        self.provider_plugins.len()
    }
}

impl Default for PluginManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Check if a file is executable
fn is_executable(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = std::fs::metadata(path) {
            if meta.permissions().mode() & 0o111 != 0 {
                return true;
            }
        }
        // Also allow common script extensions even without execute bit
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        matches!(ext, "py" | "rb" | "pl" | "sh" | "bash")
    }

    #[cfg(windows)]
    {
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        matches!(ext, "exe" | "py" | "bat" | "cmd" | "ps1" | "rb" | "pl")
    }

    #[cfg(not(any(unix, windows)))]
    {
        true
    }
}

/// Query a plugin executable for its type via --plugin-info
fn query_plugin_type(path: &Path) -> Option<String> {
    let output = Command::new(path)
        .arg("--plugin-info")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        let line = line.trim();
        if line.starts_with('{') {
            // Extract "type" field
            let pattern = "\"type\":";
            if let Some(start) = line.find(pattern) {
                let rest = line[start + pattern.len()..].trim_start();
                if rest.starts_with('"') {
                    let rest = &rest[1..];
                    if let Some(end) = rest.find('"') {
                        return Some(rest[..end].to_string());
                    }
                }
            }
        }
    }

    None
}
