//! Configuration management

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use crate::ui::ThemeConfig;

/// Main configuration structure
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// General settings
    pub general: GeneralConfig,
    /// Display settings
    pub display: DisplayConfig,
    /// Sorting settings
    pub sorting: SortingConfig,
    /// Editor settings
    pub editor: EditorConfig,
    /// Confirmation settings
    pub confirmations: ConfirmConfig,
    /// Theme settings
    pub theme: ThemeConfig,
    /// Keyboard shortcuts
    #[serde(default)]
    pub keybindings: KeyBindings,
    /// File handler rules (pattern -> command)
    #[serde(default = "default_handlers")]
    pub handlers: Vec<FileHandler>,
    /// Saved SCP/SFTP connections
    #[serde(default)]
    pub connections: Vec<SavedConnection>,
    /// Saved plugin connections (FTP, WebDAV, S3, etc. - any provider plugin)
    #[serde(default)]
    pub plugin_connections: Vec<SavedPluginConnection>,
    /// Favorite/bookmarked local paths
    #[serde(default)]
    pub favorites: Vec<FavoritePath>,
    /// User menu rules (custom commands)
    #[serde(default)]
    pub user_menu: Vec<UserMenuRule>,
}

/// A saved SCP/SFTP connection (password not stored for security)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedConnection {
    /// Display name for the connection
    pub name: String,
    /// Username for SSH
    pub user: String,
    /// Hostname or IP address
    pub host: String,
    /// Port (default 22)
    #[serde(default = "default_ssh_port")]
    pub port: u16,
    /// Initial remote path (optional)
    #[serde(default)]
    pub path: Option<String>,
}

/// A favorite/bookmarked local path
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FavoritePath {
    /// Display name for the favorite
    pub name: String,
    /// Full path to the directory
    pub path: String,
}

/// A user menu rule for custom commands
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserMenuRule {
    /// Display name for the rule
    pub name: String,
    /// Command template with placeholders:
    /// - !.! or %f = current filename
    /// - !. or %n = filename without extension
    /// - %e = extension only
    /// - %d = current directory
    /// - %s = selected files (space-separated)
    pub command: String,
    /// Optional hotkey letter (single character) for quick execution
    #[serde(default)]
    pub hotkey: Option<String>,
}

/// A saved plugin connection (generic key-value storage for any provider plugin)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedPluginConnection {
    /// Display name for the connection
    pub name: String,
    /// URI scheme identifying the plugin (e.g., "ftp", "s3")
    pub scheme: String,
    /// Connection fields as key-value pairs (plugin-specific)
    #[serde(default)]
    pub fields: HashMap<String, String>,
}


fn default_ssh_port() -> u16 {
    22
}

/// Keyboard shortcut configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct KeyBindings {
    /// Custom keybindings (action -> key)
    #[serde(flatten)]
    pub bindings: HashMap<String, String>,
}

impl KeyBindings {
    /// Get the key binding for an action, falling back to default
    pub fn get(&self, action: &str) -> &str {
        self.bindings.get(action)
            .map(|s| s.as_str())
            .unwrap_or_else(|| default_keybinding(action))
    }

    /// Check if a key event matches an action
    pub fn matches(&self, action: &str, key: &crossterm::event::KeyEvent) -> bool {
        use crossterm::event::KeyCode;

        let binding = self.get(action);
        parse_key_binding(binding)
            .map(|(code, mods)| {
                // Check modifiers match
                if key.modifiers != mods {
                    return false;
                }
                // Check key code - for Char, compare case-insensitively
                match (&key.code, &code) {
                    (KeyCode::Char(a), KeyCode::Char(b)) => {
                        a.eq_ignore_ascii_case(b)
                    }
                    _ => key.code == code,
                }
            })
            .unwrap_or(false)
    }
}

/// Parse a key binding string like "Ctrl+C", "Alt+F1", "F10", etc.
pub fn parse_key_binding(s: &str) -> Option<(crossterm::event::KeyCode, crossterm::event::KeyModifiers)> {
    use crossterm::event::KeyModifiers;

    let s = s.trim();
    let mut modifiers = KeyModifiers::NONE;
    let mut parts: Vec<&str> = s.split('+').collect();

    // Process modifiers (all but last part)
    while parts.len() > 1 {
        let modifier = parts.remove(0).to_lowercase();
        match modifier.as_str() {
            "ctrl" | "control" => modifiers |= KeyModifiers::CONTROL,
            "alt" => modifiers |= KeyModifiers::ALT,
            "shift" => modifiers |= KeyModifiers::SHIFT,
            _ => return None, // Unknown modifier
        }
    }

    // The last part is the key
    let key_str = parts[0];
    let code = parse_key_code(key_str)?;

    Some((code, modifiers))
}

/// Parse a key code string
fn parse_key_code(s: &str) -> Option<crossterm::event::KeyCode> {
    use crossterm::event::KeyCode;

    let s_lower = s.to_lowercase();

    // Function keys
    if s_lower.starts_with('f') && s_lower.len() >= 2
        && let Ok(n) = s_lower[1..].parse::<u8>()
            && (1..=12).contains(&n) {
                return Some(KeyCode::F(n));
            }

    // Named keys
    match s_lower.as_str() {
        "esc" | "escape" => Some(KeyCode::Esc),
        "enter" | "return" => Some(KeyCode::Enter),
        "tab" => Some(KeyCode::Tab),
        "backtab" => Some(KeyCode::BackTab),
        "backspace" | "bs" => Some(KeyCode::Backspace),
        "delete" | "del" => Some(KeyCode::Delete),
        "insert" | "ins" => Some(KeyCode::Insert),
        "home" => Some(KeyCode::Home),
        "end" => Some(KeyCode::End),
        "pageup" | "pgup" => Some(KeyCode::PageUp),
        "pagedown" | "pgdn" => Some(KeyCode::PageDown),
        "up" => Some(KeyCode::Up),
        "down" => Some(KeyCode::Down),
        "left" => Some(KeyCode::Left),
        "right" => Some(KeyCode::Right),
        "space" => Some(KeyCode::Char(' ')),
        // Single character
        _ if s.len() == 1 => Some(KeyCode::Char(s.chars().next().unwrap())),
        _ => None,
    }
}

/// File handler rule: maps a regex pattern to a command
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileHandler {
    /// Regex pattern to match against filename (e.g., "\\.mp4$", "\\.pdf$")
    pub pattern: String,
    /// Command to run. Use {} as placeholder for the file path.
    /// Example: "vlc {}" or "xdg-open {}"
    pub command: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GeneralConfig {
    /// Show hidden files (starting with .)
    pub show_hidden: bool,
    /// Follow symlinks when navigating
    pub follow_symlinks: bool,
    /// Save panel paths on exit and restore on start
    pub remember_path: bool,
    /// Always keep command line in edit mode (no vim-style navigation)
    pub edit_mode_always: bool,
    /// Run executable files when pressing Enter on them
    pub run_executables: bool,
    /// Automatically save config changes (panel state, settings)
    pub autosave: bool,
    /// Override shell executable (empty = auto-detect)
    /// On Unix: defaults to $SHELL or /bin/sh
    /// On Windows: auto-detects pwsh > powershell > cmd.exe
    #[serde(default)]
    pub shell: String,
    /// Last left panel path (auto-saved)
    pub last_left_path: Option<String>,
    /// Last right panel path (auto-saved)
    pub last_right_path: Option<String>,
    /// Last left panel view mode (auto-saved)
    pub last_left_view: Option<String>,
    /// Last right panel view mode (auto-saved)
    pub last_right_view: Option<String>,
    /// Try plugin viewers before built-in viewer on F3
    #[serde(default)]
    pub view_plugin_first: bool,
    /// Max size (MB) for remote transfers before confirmation prompt (0 = no limit)
    #[serde(default = "default_remote_transfer_limit_mb")]
    pub remote_transfer_limit_mb: u64,
    /// Use shell history viewer instead of interactive PTY shell on Ctrl+O
    #[serde(default)]
    pub shell_history_mode: bool,
}

fn default_remote_transfer_limit_mb() -> u64 {
    512
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DisplayConfig {
    /// Default view mode: "brief" or "full"
    pub view_mode: String,
    /// Initial shell area height (lines)
    pub shell_height: u16,
    /// Left panel width as percentage (10-90, default 50)
    pub panel_ratio: u16,
    /// Show git status in status bar
    pub show_git_status: bool,
    /// Show Python virtual environment in status bar
    pub show_python_env: bool,
    /// Date format: "relative" or "absolute"
    pub date_format: String,
    /// Show directory prefix (/ on Unix, \ on Windows) before folder names
    pub show_dir_prefix: bool,
    /// Show current date on the right panel top border
    pub show_date: bool,
    /// Show current time on the right panel top border
    pub show_time: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SortingConfig {
    /// Sort field: "name", "extension", "size", "modified", "unsorted"
    pub field: String,
    /// Sort direction: "ascending" or "descending"
    pub direction: String,
    /// Always show directories before files
    pub dirs_first: bool,
    /// Sort uppercase-first names before lowercase-first names
    pub uppercase_first: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
#[derive(Default)]
pub struct EditorConfig {
    /// External editor command (empty = use $VISUAL/$EDITOR/default)
    pub command: String,
    /// External viewer command (empty = use built-in)
    pub viewer: String,
    /// External hex editor command (used by HexEditor plugin)
    #[serde(default = "default_hex_editor")]
    pub hex_editor: String,
}

fn default_hex_editor() -> String {
    "jinx".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ConfirmConfig {
    /// Confirm before delete
    pub delete: bool,
    /// Confirm before overwrite
    pub overwrite: bool,
    /// Confirm before exit
    pub exit: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            general: GeneralConfig::default(),
            display: DisplayConfig::default(),
            sorting: SortingConfig::default(),
            editor: EditorConfig::default(),
            confirmations: ConfirmConfig::default(),
            theme: ThemeConfig::default(),
            keybindings: KeyBindings::default(),
            handlers: default_handlers(),
            connections: Vec::new(),
            plugin_connections: Vec::new(),
            favorites: Vec::new(),
            user_menu: Vec::new(),
        }
    }
}

/// Get the default key binding for an action
pub fn default_keybinding(action: &str) -> &'static str {
    match action {
        // Application
        "quit" => "F10",
        "quit_alt" => "Ctrl+C",
        "help" => "F1",
        "shell_toggle" => "Ctrl+O",

        // Panel navigation
        "toggle_panel" => "Tab",
        "move_up" => "Up",
        "move_down" => "Down",
        "move_left" => "Left",
        "move_right" => "Right",
        "page_up" => "PageUp",
        "page_down" => "PageDown",
        "go_home" => "Home",
        "go_end" => "End",
        "go_parent" => "Backspace",
        "enter" => "Enter",

        // File operations
        "user_menu" => "F2",
        "view" => "F3",
        "edit" => "F4",
        "copy" => "F5",
        "move" => "F6",
        "mkdir" => "F7",
        "delete" => "F8",

        // Selection
        "select_toggle" => "Insert",
        "select_pattern" => "Ctrl+A",
        "select_pattern_alt" => "Alt+A",
        "unselect_all" => "Ctrl+U",

        // Temp panel
        "add_to_temp" => "Alt+T",
        "remove_from_temp" => "Delete",

        // Sorting
        "sort_name" => "Ctrl+N",
        "sort_extension" => "Ctrl+F4",
        "sort_time" => "Ctrl+T",
        "sort_size" => "Ctrl+S",
        "sort_name_f" => "Ctrl+F3",
        "sort_ext_f" => "Ctrl+F4",
        "sort_time_f" => "Ctrl+F5",
        "sort_size_f" => "Ctrl+F6",
        "sort_unsorted_f" => "Ctrl+F7",

        // Display
        "toggle_hidden" => "Ctrl+H",
        "toggle_view_mode" => "Alt+M",
        "refresh" => "Ctrl+R",

        // Search
        "find_files" => "Alt+/",
        "quick_search" => "Alt+S",

        // Command line
        "insert_filename" => "Ctrl+F",
        "insert_path" => "Ctrl+P",
        "insert_fullpath" => "Alt+Enter",
        "command_history" => "Alt+H",
        "command_history_alt" => "F9",
        "recall_history" => "Ctrl+E",

        // Source selector (drives/quick access/connections)
        "drive_left" => "Alt+F1",
        "drive_right" => "Alt+F2",
        "source_left" => "Ctrl+F1",   // Alternative for desktop environments where Alt+F1 conflicts
        "source_right" => "Ctrl+F2",  // Alternative for desktop environments where Alt+F2 conflicts
        "source_left_shift" => "Shift+F1",  // Another alternative
        "source_right_shift" => "Shift+F2", // Another alternative
        "source_left_alt" => "Alt+1",       // Quick access with Alt+number
        "source_right_alt" => "Alt+2",      // Quick access with Alt+number
        "add_favorite" => "Ctrl+D",   // Add current directory to favorites

        // Page navigation (vim-style)
        "page_up_vim" => "Ctrl+B",
        "page_down_vim" => "Ctrl+F",

        // Permissions (Unix only)
        "permissions" => "Ctrl+X",

        // Owner/Group (Unix only)
        "chown" => "Ctrl+G",

        // Viewer
        "viewer_save" => "Ctrl+S",

        // Overlay plugins
        "overlay_plugins" => "Alt+P",

        // Unknown action
        _ => "",
    }
}

/// Returns the platform-appropriate default open command
fn default_open_command() -> &'static str {
    #[cfg(target_os = "linux")]
    { "setsid xdg-open {}" }
    #[cfg(target_os = "macos")]
    { "open {}" }
    #[cfg(target_os = "windows")]
    { "explorer {}" }
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    { "setsid xdg-open {}" }
}

/// Default file handlers
pub fn default_handlers() -> Vec<FileHandler> {
    let cmd = default_open_command().to_string();
    vec![
        FileHandler {
            pattern: r"\.(jpg|jpeg|png|gif|bmp|webp|svg|ico|tiff|tif|avif)$".to_string(),
            command: cmd.clone(),
        },
        FileHandler {
            pattern: r"\.(mp4|mkv|avi|mov|webm|flv|wmv|m4v)$".to_string(),
            command: cmd.clone(),
        },
        FileHandler {
            pattern: r"\.(mp3|flac|ogg|wav|aac|wma|m4a|opus)$".to_string(),
            command: cmd.clone(),
        },
        FileHandler {
            pattern: r"\.pdf$".to_string(),
            command: cmd,
        },
    ]
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            show_hidden: true,
            follow_symlinks: true,
            remember_path: false,
            edit_mode_always: true,
            run_executables: true,
            autosave: false,
            shell: String::new(),
            last_left_path: None,
            last_right_path: None,
            last_left_view: None,
            last_right_view: None,
            view_plugin_first: false,
            remote_transfer_limit_mb: 512,
            shell_history_mode: false,
        }
    }
}

impl Default for DisplayConfig {
    fn default() -> Self {
        Self {
            view_mode: "brief".to_string(),
            shell_height: 1,
            panel_ratio: 50,
            show_git_status: true,
            show_python_env: true,
            date_format: "absolute".to_string(),
            show_dir_prefix: false,
            show_date: true,
            show_time: true,
        }
    }
}

impl Default for SortingConfig {
    fn default() -> Self {
        Self {
            field: "name".to_string(),
            direction: "ascending".to_string(),
            dirs_first: true,
            uppercase_first: true,
        }
    }
}


impl Default for ConfirmConfig {
    fn default() -> Self {
        Self {
            delete: true,
            overwrite: true,
            exit: false,
        }
    }
}

/// Get the config directory path for the current platform
pub fn config_dir() -> Option<PathBuf> {
    #[cfg(target_os = "linux")]
    {
        // Linux: ~/.config/bark
        dirs_next().map(|p| p.join("bark"))
    }

    #[cfg(target_os = "macos")]
    {
        // macOS: prefer ~/.config/bark (consistent with other CLI tools like fish, nvim, etc.)
        // Fall back to ~/Library/Application Support/bark for existing installs
        let home = std::env::var("HOME").ok().map(PathBuf::from);
        if let Some(ref h) = home {
            let xdg_path = h.join(".config/bark");
            let legacy_path = h.join("Library/Application Support/bark");
            if xdg_path.exists() {
                return Some(xdg_path);
            }
            if legacy_path.exists() {
                return Some(legacy_path);
            }
        }
        // New installs: default to ~/.config/bark
        home.map(|h| h.join(".config/bark"))
    }

    #[cfg(target_os = "windows")]
    {
        // Windows: %APPDATA%\bark
        std::env::var("APPDATA")
            .ok()
            .map(|p| PathBuf::from(p).join("bark"))
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        // Fallback: ~/.config/bark
        std::env::var("HOME").ok().map(|p| PathBuf::from(p).join(".config/bark"))
    }
}

#[cfg(target_os = "linux")]
fn dirs_next() -> Option<PathBuf> {
    // Check XDG_CONFIG_HOME first, then fall back to ~/.config
    std::env::var("XDG_CONFIG_HOME")
        .ok()
        .map(PathBuf::from)
        .or_else(|| std::env::var("HOME").ok().map(|p| PathBuf::from(p).join(".config")))
}

/// Get the config file path
pub fn config_file() -> Option<PathBuf> {
    config_dir().map(|p| p.join("config.toml"))
}

/// Get the command history file path
pub fn history_file() -> Option<PathBuf> {
    config_dir().map(|p| p.join("history"))
}

/// Load command history from file
pub fn load_command_history() -> Vec<String> {
    let Some(path) = history_file() else {
        return Vec::new();
    };

    match fs::read_to_string(&path) {
        Ok(content) => content.lines().map(|s| s.to_string()).collect(),
        Err(_) => Vec::new(),
    }
}

/// Save command history to file
pub fn save_command_history(history: &[String]) {
    let Some(path) = history_file() else {
        return;
    };

    // Ensure config directory exists
    if let Some(dir) = path.parent() {
        let _ = fs::create_dir_all(dir);
    }

    // Keep only last 1000 commands
    let history_to_save: Vec<&str> = history.iter()
        .rev()
        .take(1000)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .map(|s| s.as_str())
        .collect();

    let content = history_to_save.join("\n");
    let _ = fs::write(&path, content);
}

/// Default config file content with comments
fn default_config() -> String {
    let open_cmd = default_open_command();
    format!(r##"# Bark Configuration
# This file is auto-generated. Edit as needed.

[general]
# Show hidden files (starting with .)
show_hidden = true

# Follow symlinks when navigating
follow_symlinks = true

# Save and restore panel paths between sessions
remember_path = false

# Always keep command line in edit mode (typing goes to command line, not vim navigation)
edit_mode_always = true

# Run executable files when pressing Enter on them
run_executables = true

# Automatically save config changes to disk (panel state, settings)
# When false, use :config-save to save manually
autosave = false

# Shell to use for Ctrl+O interactive shell (leave empty for auto-detect)
# Auto-detect: Unix uses $SHELL or /bin/sh; Windows tries pwsh > powershell > cmd.exe
# Examples: "bash", "zsh", "fish", "pwsh", "cmd.exe"
shell = ""

# Try plugin viewers before built-in viewer when pressing F3
# When false (default), F3 opens the built-in hex/text viewer; use F2 to pick a plugin
# When true, F3 tries matching plugins first and falls back to the built-in viewer
view_plugin_first = false

# Maximum size (MB) for remote file transfers before showing a confirmation prompt
# Set to 0 to disable the size guard (no confirmation regardless of size)
remote_transfer_limit_mb = 512

[display]
# Default view mode: "brief" (two columns) or "full" (detailed list)
view_mode = "brief"

# Initial shell area height in lines (1 = just command line)
shell_height = 1

# Left panel width as percentage (10-90, use Shift+Left/Right to adjust)
panel_ratio = 50

# Show git branch and status in status bar
show_git_status = true

# Show Python virtual environment in status bar
show_python_env = true

# Date format: "absolute" (2024-01-15 10:30) or "relative" (2 hours ago)
date_format = "absolute"

# Show directory prefix (/ or \\) before folder names
show_dir_prefix = false

# Show current date on the right panel top border
show_date = true

# Show current time on the right panel top border
show_time = true

[sorting]
# Sort field: "name", "extension", "size", "modified", "unsorted"
field = "name"

# Sort direction: "ascending" or "descending"
direction = "ascending"

# Always show directories before files
dirs_first = true

# Sort uppercase-first names before lowercase-first names
uppercase_first = true

[editor]
# External editor command (leave empty to use $VISUAL, $EDITOR, or default)
# Examples: "vim", "nano", "code --wait", "hx"
command = ""

# External viewer command (leave empty to use built-in viewer)
# Example: "less", "bat"
viewer = ""

[confirmations]
# Show confirmation dialog before deleting files
delete = true

# Show confirmation dialog before overwriting files
overwrite = true

# Show confirmation dialog before exiting
exit = false

[theme]
# Color scheme preset: "dark", "classic", "light", or any custom theme name
# Use :theme <name> at runtime to switch themes
preset = "dark"

# Quick color overrides for the active theme (uncomment and modify as needed)
# Colors can be: named (red, blue, cyan, etc.), hex (#RRGGBB), or rgb(R,G,B)
#
# Available color names: black, red, green, yellow, blue, magenta, cyan, white,
#   gray, dark_gray, light_red, light_green, light_yellow, light_blue,
#   light_magenta, light_cyan
#
# [theme.colors]
# ## Panel colors
# panel_border_active = "cyan"       # Border color when panel is active
# panel_border_inactive = "gray"     # Border color when panel is inactive
# panel_header = "yellow"            # Panel title/path text color
# panel_header_bg = "#5f8787"        # Panel title background
# panel_column_separator = "gray"    # Separator between columns in brief mode
# panel_background = "#3a3a3a"       # Panel background color
# temp_panel_background = "#4b4632"  # Background for TEMP panel (search results)
# remote_panel_background = "#323a46" # Background for remote panels (SCP/SFTP)
#
# ## File list colors
# file_normal = "#dcdcdc"            # Normal file text color
# file_directory = "#abaf87"         # Directory text color
# file_selected = "yellow"           # Selected/marked file color
# cursor_bg = "#005f5f"              # Cursor background color
# cursor_fg = "#dcdcdc"              # Cursor text color
#
# ## Status bar colors
# status_bg = "#2d2d2d"              # Status bar background
# status_fg = "#abb2bf"              # Status bar text color
# status_error_bg = "#b43c3c"        # Error message background
# status_error_fg = "white"          # Error message text color
# git_clean = "#98c379"              # Git status color (clean)
# git_dirty = "yellow"               # Git status color (dirty/modified)
#
# ## File viewer colors
# viewer_header_bg = "cyan"          # Viewer header background
# viewer_header_fg = "black"         # Viewer header text color
# viewer_content_bg = "#3a3a3a"      # Viewer content background
# viewer_content_fg = "#abb2bf"      # Viewer content text color
# viewer_line_number = "gray"        # Line number color
# viewer_footer_bg = "cyan"          # Viewer footer background
# viewer_footer_fg = "black"         # Viewer footer text color
#
# ## Help viewer colors
# help_header_bg = "cyan"
# help_header_fg = "black"
# help_content_bg = "#3a3a3a"
# help_content_fg = "#abb2bf"
# help_highlight = "yellow"          # Highlighted text in help
# help_footer_bg = "cyan"
# help_footer_fg = "black"
#
# ## Dialog colors
# dialog_copy_bg = "#1e3228"         # Copy dialog background (greenish)
# dialog_copy_border = "#98c379"
# dialog_move_bg = "#1e2837"         # Move dialog background (bluish)
# dialog_move_border = "#61afef"
# dialog_delete_bg = "#372323"       # Delete dialog background (reddish)
# dialog_delete_border = "#e06c75"
# dialog_mkdir_bg = "#3c2819"        # Mkdir dialog background (orangish)
# dialog_mkdir_border = "#d28c3c"
# dialog_title = "white"
# dialog_text = "#abb2bf"
# dialog_warning = "yellow"
# dialog_input_focused_bg = "#4c5263"
# dialog_input_focused_fg = "white"
# dialog_input_unfocused_fg = "gray"
# dialog_button_focused_bg = "cyan"
# dialog_button_focused_fg = "black"
# dialog_button_unfocused = "gray"
# dialog_delete_button_focused_bg = "#e06c75"
# dialog_delete_button_focused_fg = "white"
# dialog_help = "gray"

# Define custom themes below. Each theme can inherit from a base theme.
# Example custom themes (uncomment to use):
#
# [theme.themes.gruvbox]
# base = "dark"
# panel_border_active = "#fabd2f"
# panel_border_inactive = "#665c54"
# file_directory = "#83a598"
# file_selected = "#fe8019"
# cursor_bg = "#504945"
# cursor_fg = "#ebdbb2"
# status_bg = "#3c3836"
# status_fg = "#ebdbb2"
#
# [theme.themes.solarized]
# base = "light"
# panel_border_active = "#268bd2"
# panel_border_inactive = "#93a1a1"
# file_directory = "#2aa198"
# cursor_bg = "#eee8d5"
# cursor_fg = "#657b83"
#
# [theme.themes.nord]
# base = "dark"
# panel_border_active = "#88c0d0"
# panel_border_inactive = "#4c566a"
# file_directory = "#81a1c1"
# file_selected = "#ebcb8b"
# cursor_bg = "#434c5e"
# cursor_fg = "#eceff4"
# status_bg = "#3b4252"
# status_fg = "#e5e9f0"

# File highlighting rules (first match wins)
# Special patterns: "executable", "symlink"
# Regular patterns: regex matched against filename
# Default rules are built-in; add your own to customize:
#
# [[theme.highlights]]
# pattern = "\\.pdf$"
# color = "lightblue"
#
# [[theme.highlights]]
# pattern = "\\.md$"
# color = "#87CEEB"
# prefix = ">"
#
# [[theme.highlights]]
# pattern = "executable"
# color = "green"
# prefix = "*"

# Keyboard shortcuts
# Format: action = "Key" or action = "Modifier+Key"
# Modifiers: Ctrl, Alt, Shift (can combine: Ctrl+Shift+X)
# Keys: F1-F12, Enter, Esc, Tab, Space, Backspace, Delete, Insert,
#       Home, End, PageUp, PageDown, Up, Down, Left, Right,
#       or any single character (a, b, /, etc.)
#
# Uncomment and change any binding below:
# [keybindings]
# ## Application
# quit = "F10"                    # Exit application
# quit_alt = "Ctrl+C"             # Alternative quit
# help = "F1"                     # Show help
# shell_toggle = "Ctrl+O"         # Toggle shell mode
#
# ## Panel navigation
# toggle_panel = "Tab"            # Switch active panel
# move_up = "Up"                  # Move cursor up
# move_down = "Down"              # Move cursor down
# move_left = "Left"              # Move left (other column in brief mode)
# move_right = "Right"            # Move right
# page_up = "PageUp"              # Page up
# page_down = "PageDown"          # Page down
# go_home = "Home"                # Go to first entry
# go_end = "End"                  # Go to last entry
# go_parent = "Backspace"         # Go to parent directory
# enter = "Enter"                 # Enter directory / execute
#
# ## File operations
# view = "F3"                     # View file
# edit = "F4"                     # Edit file
# copy = "F5"                     # Copy file(s)
# move = "F6"                     # Move/rename file(s)
# mkdir = "F7"                    # Create directory
# delete = "F8"                   # Delete file(s)
#
# ## Selection
# select_toggle = "Insert"        # Toggle file selection
# select_pattern = "Ctrl+A"       # Select files by pattern
# select_pattern_alt = "Alt+A"    # Alternative select by pattern
# unselect_all = "Ctrl+U"         # Unselect all files
#
# ## TEMP panel
# add_to_temp = "Alt+T"           # Add file to TEMP panel
# remove_from_temp = "Delete"     # Remove from TEMP (not disk)
#
# ## Sorting
# sort_name = "Ctrl+N"            # Sort by name
# sort_extension = "Ctrl+F4"      # Sort by extension
# sort_time = "Ctrl+T"            # Sort by modification time
# sort_size = "Ctrl+S"            # Sort by size
# sort_name_f = "Ctrl+F3"         # Sort by name (F-key)
# sort_ext_f = "Ctrl+F4"          # Sort by extension (F-key)
# sort_time_f = "Ctrl+F5"         # Sort by time (F-key)
# sort_size_f = "Ctrl+F6"         # Sort by size (F-key)
# sort_unsorted_f = "Ctrl+F7"     # Unsorted (F-key)
#
# ## Display
# toggle_hidden = "Ctrl+H"        # Toggle hidden files
# toggle_view_mode = "Alt+M"      # Toggle Brief/Full view
# refresh = "Ctrl+R"              # Refresh all panels
#
# ## Search
# find_files = "Alt+/"            # Find files dialog
# quick_search = "Alt+S"          # Quick search (type to jump)
#
# ## Command line
# insert_filename = "Ctrl+F"      # Insert filename into command
# insert_path = "Ctrl+P"          # Insert current path into command
# insert_fullpath = "Alt+Enter"   # Insert full path into command
# command_history = "Alt+H"       # Show command history
# command_history_alt = "F9"      # Alternative command history
# recall_history = "Ctrl+E"      # Recall last command for editing
#
# ## Source selector (drives/quick access/remote connections)
# drive_left = "Alt+F1"           # Source selector for left panel
# drive_right = "Alt+F2"          # Source selector for right panel
# source_left = "Ctrl+F1"         # Alternative (for DEs where Alt+F1 conflicts)
# source_right = "Ctrl+F2"        # Alternative (for DEs where Alt+F2 conflicts)
# source_left_shift = "Shift+F1"  # Another alternative
# source_right_shift = "Shift+F2" # Another alternative
# source_left_alt = "Alt+1"       # Quick access with Alt+number
# source_right_alt = "Alt+2"      # Quick access with Alt+number
#
# ## Vim-style navigation
# page_up_vim = "Ctrl+B"          # Page up (vim)
# page_down_vim = "Ctrl+F"        # Page down (vim)
#
# ## Viewer
# viewer_save = "Ctrl+S"           # Save plugin viewer output to file

# File handlers: map file patterns to commands
# Pattern is a regex matched against the filename
# Command uses {{}} as placeholder for the full file path
# First matching handler wins

# Default handlers (open with OS default application)
[[handlers]]
pattern = "\\.(jpg|jpeg|png|gif|bmp|webp|svg|ico|tiff|tif|avif)$"
command = "{open_cmd}"

[[handlers]]
pattern = "\\.(mp4|mkv|avi|mov|webm|flv|wmv|m4v)$"
command = "{open_cmd}"

[[handlers]]
pattern = "\\.(mp3|flac|ogg|wav|aac|wma|m4a|opus)$"
command = "{open_cmd}"

[[handlers]]
pattern = "\\.pdf$"
command = "{open_cmd}"

# Add more handlers below:
#
# [[handlers]]
# pattern = "\\.(doc|docx|odt|xls|xlsx|ppt|pptx)$"
# command = "libreoffice {{}}"

# Saved SCP/SFTP connections
# Connections are saved automatically when you use the "Save" button in the
# connection dialog (Ctrl+F1/F2 -> New SCP Connection).
# Password is NOT stored - you'll be prompted or SSH agent/keys are used.
#
# [[connections]]
# name = "My Server"
# user = "username"
# host = "example.com"
# port = 22
# path = "/home/username"

# User Menu (F2) - Custom commands
# Define your own quick-access commands with optional hotkeys.
# Placeholders in command:
#   !.! or %f = current filename
#   !. or %n  = filename without extension
#   %e        = extension only
#   %d        = current directory
#   %s        = selected files (space-separated)
#
# [[user_menu]]
# name = "Strip binary"
# command = "strip !.!"
# hotkey = "s"
#
# [[user_menu]]
# name = "Make executable"
# command = "chmod +x %f"
# hotkey = "x"
"##, open_cmd = open_cmd)
}

impl Config {
    /// Load configuration from file, creating default if it doesn't exist
    pub fn load() -> Self {
        let Some(config_path) = config_file() else {
            eprintln!("Warning: Could not determine config directory");
            return Config::default();
        };

        // Create config directory if it doesn't exist
        if let Some(config_dir) = config_path.parent()
            && !config_dir.exists()
                && let Err(e) = fs::create_dir_all(config_dir) {
                    eprintln!("Warning: Could not create config directory: {}", e);
                    return Config::default();
                }

        // Create default config if it doesn't exist
        if !config_path.exists()
            && let Err(e) = fs::write(&config_path, &default_config()) {
                eprintln!("Warning: Could not create config file: {}", e);
                return Config::default();
            }

        // Read and parse config
        match fs::read_to_string(&config_path) {
            Ok(content) => match toml_edit::de::from_str(&content) {
                Ok(config) => config,
                Err(e) => {
                    eprintln!("Warning: Could not parse config file: {}", e);
                    eprintln!("Using default configuration");
                    Config::default()
                }
            },
            Err(e) => {
                eprintln!("Warning: Could not read config file: {}", e);
                Config::default()
            }
        }
    }

    /// Save configuration to file (preserving comments)
    pub fn save(&self) -> Result<(), Box<dyn std::error::Error>> {
        let config_path = config_file().ok_or("Could not determine config path")?;

        // Create config directory if needed
        if let Some(config_dir) = config_path.parent() {
            fs::create_dir_all(config_dir)?;
        }

        // Try to preserve comments by reading existing file and updating values
        let content = if config_path.exists() {
            match fs::read_to_string(&config_path) {
                Ok(existing) => self.update_toml_preserving_comments(&existing)?,
                Err(_) => toml_edit::ser::to_string_pretty(self)?,
            }
        } else {
            toml_edit::ser::to_string_pretty(self)?
        };

        fs::write(&config_path, &content)?;

        Ok(())
    }

    /// Update TOML content while preserving comments and formatting
    fn update_toml_preserving_comments(&self, existing: &str) -> Result<String, Box<dyn std::error::Error>> {
        use toml_edit::{DocumentMut, value};

        let mut doc: DocumentMut = existing.parse()?;

        // Update [general] section
        if let Some(general) = doc.get_mut("general").and_then(|v| v.as_table_mut()) {
            general["show_hidden"] = value(self.general.show_hidden);
            general["follow_symlinks"] = value(self.general.follow_symlinks);
            general["remember_path"] = value(self.general.remember_path);
            general["edit_mode_always"] = value(self.general.edit_mode_always);
            general["run_executables"] = value(self.general.run_executables);
            general["autosave"] = value(self.general.autosave);
            general["view_plugin_first"] = value(self.general.view_plugin_first);
            general["remote_transfer_limit_mb"] = value(self.general.remote_transfer_limit_mb as i64);

            // Update last paths (these are optional)
            if let Some(ref path) = self.general.last_left_path {
                general["last_left_path"] = value(path.as_str());
            }
            if let Some(ref path) = self.general.last_right_path {
                general["last_right_path"] = value(path.as_str());
            }
            if let Some(ref view) = self.general.last_left_view {
                general["last_left_view"] = value(view.as_str());
            }
            if let Some(ref view) = self.general.last_right_view {
                general["last_right_view"] = value(view.as_str());
            }
        }

        // Update [display] section
        if let Some(display) = doc.get_mut("display").and_then(|v| v.as_table_mut()) {
            display["view_mode"] = value(&self.display.view_mode);
            display["shell_height"] = value(self.display.shell_height as i64);
            display["panel_ratio"] = value(self.display.panel_ratio as i64);
            display["show_git_status"] = value(self.display.show_git_status);
            display["show_python_env"] = value(self.display.show_python_env);
            display["date_format"] = value(&self.display.date_format);
            display["show_dir_prefix"] = value(self.display.show_dir_prefix);
            display["show_date"] = value(self.display.show_date);
            display["show_time"] = value(self.display.show_time);
        }

        // Update [sorting] section
        if let Some(sorting) = doc.get_mut("sorting").and_then(|v| v.as_table_mut()) {
            sorting["field"] = value(&self.sorting.field);
            sorting["direction"] = value(&self.sorting.direction);
            sorting["dirs_first"] = value(self.sorting.dirs_first);
            sorting["uppercase_first"] = value(self.sorting.uppercase_first);
        }

        // Update [editor] section
        if let Some(editor) = doc.get_mut("editor").and_then(|v| v.as_table_mut()) {
            editor["command"] = value(&self.editor.command);
            editor["viewer"] = value(&self.editor.viewer);
            editor["hex_editor"] = value(&self.editor.hex_editor);
        }

        // Update [confirmations] section
        if let Some(conf) = doc.get_mut("confirmations").and_then(|v| v.as_table_mut()) {
            conf["delete"] = value(self.confirmations.delete);
            conf["overwrite"] = value(self.confirmations.overwrite);
            conf["exit"] = value(self.confirmations.exit);
        }

        // Update [theme] section - just the preset
        if let Some(theme) = doc.get_mut("theme").and_then(|v| v.as_table_mut()) {
            theme["preset"] = value(&self.theme.preset);
        }

        // Update [[connections]] array (SCP connections)
        // Remove existing and rebuild
        doc.remove("connections");
        if !self.connections.is_empty() {
            let mut aot = toml_edit::ArrayOfTables::new();
            for conn in &self.connections {
                let mut tbl = toml_edit::Table::new();
                tbl.insert("name", value(&conn.name));
                tbl.insert("user", value(&conn.user));
                tbl.insert("host", value(&conn.host));
                tbl.insert("port", value(conn.port as i64));
                if let Some(ref path) = conn.path {
                    tbl.insert("path", value(path.as_str()));
                }
                aot.push(tbl);
            }
            doc.insert("connections", toml_edit::Item::ArrayOfTables(aot));
        }

        // Update [[plugin_connections]] array
        doc.remove("plugin_connections");
        if !self.plugin_connections.is_empty() {
            let mut aot = toml_edit::ArrayOfTables::new();
            for conn in &self.plugin_connections {
                let mut tbl = toml_edit::Table::new();
                tbl.insert("name", value(&conn.name));
                tbl.insert("scheme", value(&conn.scheme));
                // Store all fields as a sub-table
                let mut fields_tbl = toml_edit::InlineTable::new();
                for (k, v) in &conn.fields {
                    fields_tbl.insert(k.as_str(), value(v.as_str()).into_value().unwrap());
                }
                tbl.insert("fields", toml_edit::Item::Value(toml_edit::Value::InlineTable(fields_tbl)));
                aot.push(tbl);
            }
            doc.insert("plugin_connections", toml_edit::Item::ArrayOfTables(aot));
        }

        // Update [[handlers]] array
        doc.remove("handlers");
        if !self.handlers.is_empty() {
            let mut aot = toml_edit::ArrayOfTables::new();
            for handler in &self.handlers {
                let mut tbl = toml_edit::Table::new();
                tbl.insert("pattern", value(&handler.pattern));
                tbl.insert("command", value(&handler.command));
                aot.push(tbl);
            }
            doc.insert("handlers", toml_edit::Item::ArrayOfTables(aot));
        }

        // Update [[favorites]] array
        doc.remove("favorites");
        if !self.favorites.is_empty() {
            let mut aot = toml_edit::ArrayOfTables::new();
            for fav in &self.favorites {
                let mut tbl = toml_edit::Table::new();
                tbl.insert("name", value(&fav.name));
                tbl.insert("path", value(&fav.path));
                aot.push(tbl);
            }
            doc.insert("favorites", toml_edit::Item::ArrayOfTables(aot));
        }

        // Update [[user_menu]] array
        doc.remove("user_menu");
        if !self.user_menu.is_empty() {
            let mut aot = toml_edit::ArrayOfTables::new();
            for rule in &self.user_menu {
                let mut tbl = toml_edit::Table::new();
                tbl.insert("name", value(&rule.name));
                tbl.insert("command", value(&rule.command));
                if let Some(ref hotkey) = rule.hotkey {
                    tbl.insert("hotkey", value(hotkey.as_str()));
                }
                aot.push(tbl);
            }
            doc.insert("user_menu", toml_edit::Item::ArrayOfTables(aot));
        }

        Ok(doc.to_string())
    }

    /// Reset config file to default with full documentation
    pub fn reset_to_default() -> Result<(), Box<dyn std::error::Error>> {
        let config_path = config_file().ok_or("Could not determine config path")?;

        // Create config directory if needed
        if let Some(config_dir) = config_path.parent() {
            fs::create_dir_all(config_dir)?;
        }

        fs::write(&config_path, &default_config())?;

        Ok(())
    }

    /// Save panel state (paths and view modes) for remember_path feature
    pub fn save_panel_state(
        &mut self,
        left_path: &str,
        right_path: &str,
        left_view: &str,
        right_view: &str,
    ) {
        if self.general.remember_path {
            self.general.last_left_path = Some(left_path.to_string());
            self.general.last_right_path = Some(right_path.to_string());
            self.general.last_left_view = Some(left_view.to_string());
            self.general.last_right_view = Some(right_view.to_string());
            // Only save to disk if autosave is enabled
            if self.general.autosave {
                let _ = self.save();
            }
        }
    }

    /// Find a handler command for the given filename
    /// Returns the command with {} replaced by the quoted file path
    pub fn find_handler(&self, filename: &str, file_path: &std::path::Path) -> Option<String> {
        for handler in &self.handlers {
            if let Ok(re) = regex::Regex::new(&handler.pattern)
                && re.is_match(filename) {
                    // Quote the path for shell safety
                    let quoted_path = shell_quote(&file_path.to_string_lossy());
                    let command = handler.command.replace("{}", &quoted_path);
                    return Some(command);
                }
        }
        None
    }

    /// Get saved SCP connections
    #[allow(dead_code)]
    pub fn get_scp_connections(&self) -> Vec<crate::providers::ScpConnectionInfo> {
        self.connections.iter().map(|c| {
            let mut info = crate::providers::ScpConnectionInfo::with_agent(
                c.user.clone(),
                c.host.clone(),
            );
            info.port = c.port;
            info.initial_path = c.path.clone();
            info
        }).collect()
    }

    /// Add a new SCP connection and save config
    pub fn add_connection(&mut self, conn: SavedConnection) -> Result<(), Box<dyn std::error::Error>> {
        // Check if connection with same name exists
        if let Some(idx) = self.connections.iter().position(|c| c.name == conn.name) {
            // Update existing
            self.connections[idx] = conn;
        } else {
            self.connections.push(conn);
        }
        self.save()
    }

    /// Remove an SCP connection by name
    pub fn remove_connection(&mut self, name: &str) -> Result<(), Box<dyn std::error::Error>> {
        self.connections.retain(|c| c.name != name);
        self.save()
    }

    /// Add or update a saved plugin connection and save config
    pub fn add_plugin_connection(&mut self, conn: SavedPluginConnection) -> Result<(), Box<dyn std::error::Error>> {
        // Check if connection with same name and scheme exists
        if let Some(idx) = self.plugin_connections.iter().position(|c| c.name == conn.name && c.scheme == conn.scheme) {
            self.plugin_connections[idx] = conn;
        } else {
            self.plugin_connections.push(conn);
        }
        self.save()
    }

    /// Remove a plugin connection by scheme and name
    pub fn remove_plugin_connection(&mut self, scheme: &str, name: &str) -> Result<(), Box<dyn std::error::Error>> {
        self.plugin_connections.retain(|c| !(c.name == name && c.scheme == scheme));
        self.save()
    }

    /// Add a favorite path. Returns Ok(true) if added, Ok(false) if already exists.
    pub fn add_favorite(&mut self, name: String, path: String) -> Result<bool, Box<dyn std::error::Error>> {
        // Check if already exists (by path)
        if self.favorites.iter().any(|f| f.path == path) {
            return Ok(false); // Already a favorite
        }
        self.favorites.push(FavoritePath { name, path });
        self.save()?;
        Ok(true)
    }

    /// Remove a favorite by path
    pub fn remove_favorite(&mut self, path: &str) -> Result<(), Box<dyn std::error::Error>> {
        self.favorites.retain(|f| f.path != path);
        self.save()
    }

    /// Add or update a user menu rule and save config.
    /// If editing_index is Some, updates the rule at that index.
    /// Otherwise adds a new rule.
    pub fn save_user_menu_rule(&mut self, rule: UserMenuRule, editing_index: Option<usize>) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(idx) = editing_index {
            if idx < self.user_menu.len() {
                self.user_menu[idx] = rule;
            }
        } else {
            self.user_menu.push(rule);
        }
        self.save()
    }

    /// Remove a user menu rule by index
    pub fn remove_user_menu_rule(&mut self, index: usize) -> Result<(), Box<dyn std::error::Error>> {
        if index < self.user_menu.len() {
            self.user_menu.remove(index);
        }
        self.save()
    }
}

/// Quote a string for shell use
#[cfg(unix)]
fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace("'", "'\\''"))
}

/// Quote a string for cmd.exe use (double quotes, escape internal double quotes)
#[cfg(windows)]
fn shell_quote(s: &str) -> String {
    format!("\"{}\"", s.replace('"', "\\\""))
}
