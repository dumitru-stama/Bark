//! Plugin API types and traits

use std::path::PathBuf;

/// Type of plugin
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum PluginType {
    /// Adds a section to the status bar
    StatusBar = 0,
    /// Custom file viewer for specific file types
    Viewer = 1,
}

impl TryFrom<u32> for PluginType {
    type Error = ();

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(PluginType::StatusBar),
            1 => Ok(PluginType::Viewer),
            _ => Err(()),
        }
    }
}

/// Information about a loaded plugin
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct PluginInfo {
    pub name: String,
    pub version: String,
    pub plugin_type: PluginType,
    pub source: PluginSource,
    /// Plugin needs direct terminal access (Bark will leave alternate screen
    /// and disable raw mode before calling render, then restore after).
    pub needs_terminal: bool,
}

/// Where the plugin was loaded from
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum PluginSource {
    Native(PathBuf),
    Script(PathBuf),
}

/// Context passed to status bar plugins
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct StatusContext {
    /// Current panel path
    pub path: PathBuf,
    /// Selected file name (if any)
    pub selected_file: Option<String>,
    /// Selected file full path (if any)
    pub selected_path: Option<PathBuf>,
    /// Whether the file is a directory
    pub is_dir: bool,
    /// File size (0 for directories)
    pub file_size: u64,
    /// Number of selected files
    pub selected_count: usize,
}

/// Context passed to viewer plugins
#[derive(Debug, Clone)]
pub struct ViewerContext {
    /// Path to the file to view
    pub path: PathBuf,
    /// Available width in characters
    pub width: usize,
    /// Available height in lines
    pub height: usize,
    /// Current scroll offset
    pub scroll: usize,
    /// Configuration values from Bark (flattened key-value pairs)
    pub config: std::collections::HashMap<String, String>,
}

/// Result from a status bar plugin
#[derive(Debug, Clone)]
pub struct StatusResult {
    /// Text to display in the status bar
    pub text: String,
}

/// Result from a viewer plugin's can_handle check
#[derive(Debug, Clone)]
pub struct ViewerCanHandleResult {
    pub can_handle: bool,
    /// Priority (higher = more preferred when multiple plugins match)
    pub priority: i32,
}

/// Result from a viewer plugin's render
#[derive(Debug, Clone)]
pub struct ViewerRenderResult {
    /// Lines of text to display
    pub lines: Vec<String>,
    /// Total number of lines (for scrolling)
    pub total_lines: usize,
}

/// Trait for status bar plugins
pub trait StatusBarPlugin: Send + Sync {
    /// Get plugin info
    fn info(&self) -> &PluginInfo;

    /// Render status bar section
    fn render(&self, context: &StatusContext) -> Option<StatusResult>;
}

/// Trait for viewer plugins
pub trait ViewerPlugin: Send + Sync {
    /// Get plugin info
    fn info(&self) -> &PluginInfo;

    /// Check if this plugin can handle the given file
    fn can_handle(&self, path: &std::path::Path) -> ViewerCanHandleResult;

    /// Render the file content
    fn render(&self, context: &ViewerContext) -> Option<ViewerRenderResult>;
}
