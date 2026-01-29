//! Application state

use std::path::{Path, PathBuf};

use crate::config::Config;
use crate::git::{self, GitStatus};
use crate::plugins::{PluginManager, StatusContext, ViewerContext};
use crate::providers::{PanelSource, ProviderType, ScpAuth, ScpConnectionInfo, get_panel_sources};
use crate::ui::Theme;
use crate::errors::AppError;
use crate::utils::{glob_to_regex, parse_hex_string, wildcard_to_regex};
use crate::fs::utils::{copy_path, move_path, delete_path};
use crate::ui::viewer_utils::compute_line_offsets;

use super::mode::{Mode, FileOperation, SimpleConfirmAction, ViewContent, BinaryViewMode};
use super::panel::{Panel, ViewMode, SortField, SortDirection, SortConfig};
use super::{Side, UiState, CommandState};

/// Main application state
pub struct App {
    // === Panel state ===
    pub left_panel: Panel,
    pub right_panel: Panel,
    pub active_panel: Side,

    // === Mode and control ===
    pub mode: Mode,
    pub should_quit: bool,

    // === UI layout state ===
    pub ui: UiState,

    // === Command line state ===
    pub cmd: CommandState,

    // === Git status ===
    /// Git status for left panel's directory
    pub left_git_status: Option<GitStatus>,
    /// Git status for right panel's directory
    pub right_git_status: Option<GitStatus>,
    /// Path for which left git status was computed
    left_git_path: Option<PathBuf>,
    /// Path for which right git status was computed
    right_git_path: Option<PathBuf>,

    // === Configuration ===
    /// Application configuration
    pub config: Config,
    /// Active color theme
    pub theme: Theme,
    /// Plugin manager
    pub plugins: PluginManager,

    // === Misc state ===
    /// Quick search string (Alt+S). When Some, we're in quick search mode.
    pub quick_search: Option<String>,
    /// Computed directory sizes (F3 on a directory computes and caches size)
    pub dir_sizes: std::collections::HashMap<PathBuf, u64>,

    // === Background tasks ===
    /// Currently running background task (if any)
    pub background_task: Option<super::background::BackgroundTask>,
    /// Cancel token for file operations (shared with background thread)
    pub cancel_token: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
}

impl App {
    /// Check if a key event matches a configurable action.
    #[inline]
    pub fn key_matches(&self, action: &str, key: &crossterm::event::KeyEvent) -> bool {
        self.config.keybindings.matches(action, key)
    }
}

// ============================================================================
// CORE / CONSTRUCTION
// ============================================================================

#[allow(dead_code)]
impl App {
    /// Create a new application
    pub fn new() -> Self {
        let config = Config::load();
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"));

        // Determine starting paths
        let left_path = if config.general.remember_path {
            config.general.last_left_path
                .as_ref()
                .map(PathBuf::from)
                .filter(|p: &PathBuf| p.exists())
                .unwrap_or_else(|| cwd.clone())
        } else {
            cwd.clone()
        };

        let right_path = if config.general.remember_path {
            config.general.last_right_path
                .as_ref()
                .map(PathBuf::from)
                .filter(|p: &PathBuf| p.exists())
                .unwrap_or_else(|| cwd.clone())
        } else {
            cwd.clone()
        };

        // Create panels
        let mut left_panel = Panel::new(left_path.clone());
        let mut right_panel = Panel::new(right_path.clone());

        // Apply view modes (use saved per-panel modes if remember_path, else default)
        let default_view = match config.display.view_mode.as_str() {
            "full" => ViewMode::Full,
            _ => ViewMode::Brief,
        };
        left_panel.view_mode = if config.general.remember_path {
            config.general.last_left_view.as_deref()
                .map(|v| if v == "full" { ViewMode::Full } else { ViewMode::Brief })
                .unwrap_or(default_view)
        } else {
            default_view
        };
        right_panel.view_mode = if config.general.remember_path {
            config.general.last_right_view.as_deref()
                .map(|v| if v == "full" { ViewMode::Full } else { ViewMode::Brief })
                .unwrap_or(default_view)
        } else {
            default_view
        };

        let sort_field = match config.sorting.field.as_str() {
            "extension" => SortField::Extension,
            "size" => SortField::Size,
            "modified" => SortField::Modified,
            "unsorted" => SortField::Unsorted,
            _ => SortField::Name,
        };
        let sort_direction = match config.sorting.direction.as_str() {
            "descending" => SortDirection::Descending,
            _ => SortDirection::Ascending,
        };
        left_panel.sort_config = SortConfig {
            field: sort_field,
            direction: sort_direction,
            dirs_first: config.sorting.dirs_first,
            uppercase_first: config.sorting.uppercase_first,
        };
        right_panel.sort_config = left_panel.sort_config;
        left_panel.resort();
        right_panel.resort();

        // Apply show_hidden setting
        left_panel.show_hidden = config.general.show_hidden;
        right_panel.show_hidden = config.general.show_hidden;
        if config.general.show_hidden {
            left_panel.refresh();
            right_panel.refresh();
        }

        let left_git = git::get_git_status(&left_path);
        let right_git = git::get_git_status(&right_path);

        // Build theme from config
        let theme = config.theme.build_theme();

        // Initialize plugin manager
        let mut plugins = PluginManager::new();

        // Load plugins from config directory only
        if let Some(config_dir) = crate::config::config_dir() {
            let config_plugins = config_dir.join("plugins");
            if config_plugins.exists() {
                let _ = plugins.load_from_directory(&config_plugins);
            }
        }

        Self {
            left_panel,
            right_panel,
            active_panel: Side::Left,
            mode: Mode::Normal,
            should_quit: false,
            ui: UiState::from_config(
                config.display.shell_height.max(1),
                config.display.panel_ratio,
            ),
            cmd: CommandState::with_history(crate::config::load_command_history()),
            left_git_status: left_git,
            right_git_status: right_git,
            left_git_path: Some(left_path),
            right_git_path: Some(right_path),
            config,
            theme,
            plugins,
            quick_search: None,
            dir_sizes: std::collections::HashMap::new(),
            background_task: None,
            cancel_token: None,
        }
    }

    /// Save current state to config (call before exit)
    pub fn save_state(&mut self) {
        let left_path = self.left_panel.path.to_string_lossy().to_string();
        let right_path = self.right_panel.path.to_string_lossy().to_string();
        let left_view = match self.left_panel.view_mode {
            ViewMode::Brief => "brief",
            ViewMode::Full => "full",
        };
        let right_view = match self.right_panel.view_mode {
            ViewMode::Brief => "brief",
            ViewMode::Full => "full",
        };
        self.config.save_panel_state(&left_path, &right_path, left_view, right_view);

        // Save command history
        crate::config::save_command_history(&self.cmd.history);

        // Write last directory for shell wrapper
        self.write_last_dir();
    }

    /// Write the active panel's directory to a file for shell integration
    /// The shell wrapper can read this and cd to it after exit
    fn write_last_dir(&self) {
        let last_dir = self.active_panel().path.to_string_lossy();

        // Try environment variable first, then default to ~/.bark_lastdir
        let file_path = std::env::var("BARK_LASTDIR")
            .unwrap_or_else(|_| {
                std::env::var("HOME")
                    .map(|h| format!("{}/.bark_lastdir", h))
                    .unwrap_or_else(|_| "/tmp/.bark_lastdir".to_string())
            });

        let _ = std::fs::write(&file_path, last_dir.as_bytes());
    }

    // ========================================================================
    // GIT STATUS
    // ========================================================================

    /// Update git status for panels if their paths changed
    pub fn update_git_status(&mut self) {
        // Update left panel git status if path changed
        if self.left_git_path.as_ref() != Some(&self.left_panel.path) {
            self.left_git_status = git::get_git_status(&self.left_panel.path);
            self.left_git_path = Some(self.left_panel.path.clone());
        }

        // Update right panel git status if path changed
        if self.right_git_path.as_ref() != Some(&self.right_panel.path) {
            self.right_git_status = git::get_git_status(&self.right_panel.path);
            self.right_git_path = Some(self.right_panel.path.clone());
        }
    }

    /// Force refresh git status (e.g., after file operations)
    pub fn refresh_git_status(&mut self) {
        self.left_git_status = git::get_git_status(&self.left_panel.path);
        self.right_git_status = git::get_git_status(&self.right_panel.path);
    }

    // ========================================================================
    // UI LAYOUT
    // ========================================================================

    /// Increase shell area height
    pub fn grow_shell(&mut self) {
        self.ui.grow_shell();
    }

    /// Decrease shell area height
    pub fn shrink_shell(&mut self) {
        self.ui.shrink_shell();
    }

    /// Increase left panel width (Shift+Right)
    pub fn grow_left_panel(&mut self) {
        self.ui.grow_left_panel();
    }

    /// Decrease left panel width (Shift+Left)
    pub fn shrink_left_panel(&mut self) {
        self.ui.shrink_left_panel();
    }

    // ========================================================================
    // COMMAND LINE / HISTORY
    // ========================================================================

    /// Navigate command history up (older)
    pub fn history_up(&mut self) {
        self.cmd.history_up();
    }

    /// Get list of available commands for completion
    fn available_commands(&self) -> Vec<&'static str> {
        vec![
            "config-save",
            "config-reload",
            "config-reset",
            "config-edit",
            "config-upgrade",
            "show-hidden",
            "show-settings",
            "set",
            "theme",
            "themes",
            "highlights",
            "help",
            "quit",
            "exit",
            "q",
        ]
    }

    /// Handle tab completion
    pub fn complete_command(&mut self) {
        // Get the current word being completed (first word of command line)
        let input = self.cmd.input.trim_start().to_string();
        let first_word_end = input.find(' ').unwrap_or(input.len());
        let prefix = input[..first_word_end].to_string();

        // If we're completing an argument (after space), don't complete
        if input.contains(' ') {
            return;
        }

        // Check if we should continue cycling through existing matches
        if let Some((orig_prefix, matches, idx)) = &mut self.cmd.completion_state
            && (*orig_prefix == prefix || matches.iter().any(|m| m == &self.cmd.input)) {
                // Continue cycling through matches
                if !matches.is_empty() {
                    *idx = (*idx + 1) % matches.len();
                    self.cmd.input = matches[*idx].clone();
                }
                return;
            }

        // Start new completion
        let prefix_lower = prefix.to_lowercase();
        let matches: Vec<String> = self.available_commands()
            .into_iter()
            .filter(|cmd| cmd.to_lowercase().starts_with(&prefix_lower))
            .map(|s| s.to_string())
            .collect();

        if matches.len() == 1 {
            // Single match - complete it
            self.cmd.input = matches[0].clone();
            self.cmd.completion_state = None;
        } else if !matches.is_empty() {
            // Multiple matches - show first and set up cycling
            self.cmd.input = matches[0].clone();
            self.cmd.completion_state = Some((prefix, matches, 0));
        }
        // No matches - do nothing
    }

    /// Reset completion state (call when user types something)
    pub fn reset_completion(&mut self) {
        self.cmd.completion_state = None;
    }

    /// Navigate command history down (newer)
    pub fn history_down(&mut self) {
        self.cmd.history_down();
    }

    /// Add a line to shell output history
    pub fn add_shell_output(&mut self, line: String) {
        self.cmd.add_output(line);
    }

    /// Compute and cache the total size of a directory
    pub fn compute_dir_size(&mut self, path: &PathBuf) {
        let size = Self::calculate_dir_size(path);
        self.dir_sizes.insert(path.clone(), size);
    }

    /// Recursively calculate directory size
    fn calculate_dir_size(path: &PathBuf) -> u64 {
        let mut total = 0u64;
        if let Ok(entries) = std::fs::read_dir(path) {
            for entry in entries.filter_map(|e| e.ok()) {
                let path = entry.path();
                if path.is_file() {
                    if let Ok(meta) = path.metadata() {
                        total += meta.len();
                    }
                } else if path.is_dir() {
                    total += Self::calculate_dir_size(&path);
                }
            }
        }
        total
    }

    /// Get cached directory size if available
    pub fn get_dir_size(&self, path: &PathBuf) -> Option<u64> {
        self.dir_sizes.get(path).copied()
    }

    // ========================================================================
    // PANEL NAVIGATION
    // ========================================================================

    /// Get a reference to the active panel
    pub fn active_panel(&self) -> &Panel {
        match self.active_panel {
            Side::Left => &self.left_panel,
            Side::Right => &self.right_panel,
        }
    }

    /// Get a mutable reference to the active panel
    pub fn active_panel_mut(&mut self) -> &mut Panel {
        match self.active_panel {
            Side::Left => &mut self.left_panel,
            Side::Right => &mut self.right_panel,
        }
    }

    /// Get a reference to the inactive panel
    pub fn inactive_panel(&self) -> &Panel {
        match self.active_panel {
            Side::Left => &self.right_panel,
            Side::Right => &self.left_panel,
        }
    }

    /// Get a mutable reference to the inactive panel
    pub fn inactive_panel_mut(&mut self) -> &mut Panel {
        match self.active_panel {
            Side::Left => &mut self.right_panel,
            Side::Right => &mut self.left_panel,
        }
    }

    /// Toggle active panel
    pub fn toggle_panel(&mut self) {
        self.active_panel = match self.active_panel {
            Side::Left => Side::Right,
            Side::Right => Side::Left,
        };
    }

    /// Refresh all panels (re-read directory contents)
    pub fn refresh_panels(&mut self) {
        self.left_panel.refresh();
        self.right_panel.refresh();
    }

    /// Add selected/marked files to the temp panel (in the other panel)
    /// If files are marked, add all marked files; otherwise add the file under cursor
    pub fn add_to_temp_panel(&mut self) {
        let paths: Vec<PathBuf> = if self.active_panel().selected.is_empty() {
            // No marked files - use file under cursor
            let Some(entry) = self.active_panel().selected() else {
                return;
            };

            // Don't add ".."
            if entry.name == ".." {
                return;
            }

            vec![entry.path.clone()]
        } else {
            // Use all marked files
            self.active_panel().selected.iter().cloned().collect()
        };

        if paths.is_empty() {
            return;
        }

        let count = paths.len();

        // Add to the inactive panel's temp mode
        self.inactive_panel_mut().enter_temp_mode(paths);

        // Clear selection after adding to temp
        self.active_panel_mut().selected.clear();

        if count > 1 {
            self.add_shell_output(format!("Added {} files to TEMP panel", count));
        }
    }

    /// Edit a file with external editor
    pub fn edit_file(&mut self, path: &std::path::Path) {
        let is_remote = self.active_panel().is_remote();

        if is_remote {
            // Remote file - download to temp location first
            let remote_path = path.to_string_lossy().to_string();
            let panel = self.active_panel_mut();

            match panel.read_file(&remote_path) {
                Ok(contents) => {
                    // Create temp file with same extension
                    let extension = path.extension()
                        .map(|e| format!(".{}", e.to_string_lossy()))
                        .unwrap_or_default();
                    let temp_path = std::env::temp_dir()
                        .join(format!("rc_remote_{}{}", std::process::id(), extension));

                    if let Err(e) = std::fs::write(&temp_path, contents) {
                        panel.error = Some(format!("Failed to create temp file: {}", e));
                        return;
                    }

                    let side = self.active_panel;
                    self.mode = Mode::Editing {
                        path: temp_path,
                        remote_info: Some((side, remote_path)),
                    };
                }
                Err(e) => {
                    panel.error = Some(format!("Failed to download file: {}", e));
                }
            }
        } else {
            // Local file - edit directly
            self.mode = Mode::Editing {
                path: path.to_path_buf(),
                remote_info: None,
            };
        }
    }

    /// Execute a command from the command line
    /// Returns true if it was a built-in command, false if it should be run as shell command
    pub fn execute_command(&mut self) {
        let command = std::mem::take(&mut self.cmd.input);
        self.cmd.focused = false;

        // Add to history and reset navigation
        let trimmed = command.trim();
        let history_len_before = self.cmd.history.len();
        self.cmd.add_to_history(trimmed.to_string());
        if self.cmd.history.len() > history_len_before {
            // New command added, save history immediately (in case of crash)
            crate::config::save_command_history(&self.cmd.history);
        }
        self.cmd.history_index = None;
        self.cmd.history_temp.clear();

        // Check for built-in commands first
        let trimmed = command.trim();

        // Handle built-in commands
        if let Some(result) = self.handle_builtin_command(trimmed) {
            if !result.is_empty() {
                self.add_shell_output(result);
            }
            return;
        }

        // Not a built-in command, run as shell command
        let cwd = match self.active_panel {
            Side::Left => self.left_panel.path.clone(),
            Side::Right => self.right_panel.path.clone(),
        };
        self.mode = Mode::RunningCommand { command, cwd };
    }

    /// Handle built-in commands, returns Some(message) if handled, None if not a built-in
    fn handle_builtin_command(&mut self, cmd: &str) -> Option<String> {
        let parts: Vec<&str> = cmd.splitn(2, ' ').collect();
        let command = parts[0].to_lowercase();
        let args = parts.get(1).map(|s| s.trim()).unwrap_or("");

        match command.as_str() {
            // Save config
            "config-save" => {
                // Sync runtime state to config before saving
                self.sync_config();
                match self.config.save() {
                    Ok(()) => Some("Configuration saved".to_string()),
                    Err(e) => Some(format!("Error saving config: {}", e)),
                }
            }

            // Reload config from file
            "config-reload" => {
                self.config = Config::load();
                self.apply_config();
                Some("Configuration reloaded".to_string())
            }

            // Edit config file in external editor
            "config-edit" => {
                if let Some(config_path) = crate::config::config_file() {
                    self.mode = Mode::Editing { path: config_path, remote_info: None };
                    Some(String::new())
                } else {
                    Some("Could not determine config file path".to_string())
                }
            }

            // Upgrade config: reload and save to add missing options with defaults
            "config-upgrade" => {
                // Loading applies #[serde(default)] for missing fields
                self.config = Config::load();

                // If handlers list is empty, populate with defaults
                if self.config.handlers.is_empty() {
                    self.config.handlers = crate::config::default_handlers();
                }

                self.apply_config();
                // Save back with all fields populated
                match self.config.save() {
                    Ok(()) => Some("Configuration upgraded with new options".to_string()),
                    Err(e) => Some(format!("Error saving upgraded config: {}", e)),
                }
            }

            // Reset config to default with full documentation
            "config-reset" => {
                match Config::reset_to_default() {
                    Ok(()) => {
                        self.config = Config::load();
                        self.apply_config();
                        Some("Configuration reset to default with full documentation".to_string())
                    }
                    Err(e) => Some(format!("Error resetting config: {}", e)),
                }
            }

            // Toggle hidden files
            "show-hidden" => {
                self.active_panel_mut().toggle_hidden();
                let state = if self.active_panel().show_hidden { "on" } else { "off" };
                Some(format!("Hidden files: {}", state))
            }

            // Set config option
            "set" => {
                Some(self.handle_set_command(args))
            }

            // Show current settings
            "show-settings" => {
                Some(self.show_settings())
            }

            // Quit
            "q" | "quit" | "exit" => {
                self.should_quit = true;
                Some(String::new())
            }

            // Help for built-in commands
            "help" | "?" => {
                Some(self.builtin_help())
            }

            // Theme switching
            "theme" => {
                if args.is_empty() {
                    let available = self.config.theme.available_themes().join(", ");
                    Some(format!("Current: {}. Available: {}", self.config.theme.preset, available))
                } else if let Some(theme) = self.config.theme.get_theme(args) {
                    self.theme = theme;
                    self.config.theme.preset = args.to_string();
                    Some(format!("Theme set to: {}", args))
                } else {
                    let available = self.config.theme.available_themes().join(", ");
                    Some(format!("Unknown theme: {}. Available: {}", args, available))
                }
            }

            // List available themes
            "themes" => {
                let available = self.config.theme.available_themes().join(", ");
                Some(format!("Available themes: {}", available))
            }

            // Debug: show loaded highlights
            "highlights" => {
                let count = self.theme.highlights.len();
                let config_count = self.config.theme.highlights.len();
                Some(format!("Highlights: {} compiled from {} rules", count, config_count))
            }

            // Change directory - syncs shell cd with panel
            "cd" => {
                let home_dir = std::env::var("HOME").ok().map(std::path::PathBuf::from);
                let other_panel_path = match self.active_panel {
                    Side::Left => self.right_panel.path.clone(),
                    Side::Right => self.left_panel.path.clone(),
                };

                let target = if args.is_empty() {
                    // cd with no args goes home
                    home_dir
                } else if args == "-" {
                    // cd - goes to other panel's directory
                    Some(other_panel_path)
                } else if args.starts_with('~') {
                    // Expand ~ to home directory
                    home_dir.map(|home| {
                        if args == "~" {
                            home
                        } else if let Some(suffix) = args.strip_prefix("~/") {
                            home.join(suffix)
                        } else {
                            // ~username not supported, just use as-is
                            std::path::PathBuf::from(args)
                        }
                    })
                } else {
                    let path = std::path::Path::new(args);
                    if path.is_absolute() {
                        Some(path.to_path_buf())
                    } else {
                        // Relative path from current panel
                        Some(self.active_panel().path.join(args))
                    }
                };

                match target {
                    Some(path) => {
                        let canonical = match path.canonicalize() {
                            Ok(p) => p,
                            Err(e) => return Some(format!("cd: {}: {}", args, e)),
                        };
                        if canonical.is_dir() {
                            self.active_panel_mut().change_directory(canonical);
                            self.refresh_git_status();
                            Some(String::new())
                        } else {
                            Some(format!("cd: {}: Not a directory", args))
                        }
                    }
                    None => Some("cd: Could not resolve path".to_string()),
                }
            }

            _ => None, // Not a built-in command
        }
    }

    /// Handle :set command
    fn handle_set_command(&mut self, args: &str) -> String {
        if args.is_empty() {
            return self.show_settings();
        }

        // Parse option=value or just option (for booleans, toggles)
        let parts: Vec<&str> = args.splitn(2, '=').collect();
        let option = parts[0].trim().to_lowercase();
        let value = parts.get(1).map(|s| s.trim());

        match option.as_str() {
            "hidden" | "show_hidden" => {
                let new_val = match value {
                    Some("true") | Some("1") | Some("on") | Some("yes") => true,
                    Some("false") | Some("0") | Some("off") | Some("no") => false,
                    None => !self.active_panel().show_hidden, // Toggle
                    _ => return format!("Invalid value for {}: use true/false", option),
                };
                self.active_panel_mut().show_hidden = new_val;
                self.active_panel_mut().refresh();
                format!("show_hidden = {}", new_val)
            }

            "hidden_both" | "show_hidden_both" => {
                let new_val = match value {
                    Some("true") | Some("1") | Some("on") | Some("yes") => true,
                    Some("false") | Some("0") | Some("off") | Some("no") => false,
                    None => !self.left_panel.show_hidden, // Toggle based on left
                    _ => return format!("Invalid value for {}: use true/false", option),
                };
                self.left_panel.show_hidden = new_val;
                self.right_panel.show_hidden = new_val;
                self.left_panel.refresh();
                self.right_panel.refresh();
                format!("show_hidden (both) = {}", new_val)
            }

            "shell_height" => {
                match value {
                    Some(v) => match v.parse::<u16>() {
                        Ok(h) if h >= 1 => {
                            self.ui.shell_height = h;
                            self.config.display.shell_height = h;
                            format!("shell_height = {}", h)
                        }
                        _ => "Invalid value for shell_height: must be >= 1".to_string(),
                    },
                    None => format!("shell_height = {}", self.ui.shell_height),
                }
            }

            "panel_ratio" | "ratio" => {
                match value {
                    Some(v) => match v.parse::<u16>() {
                        Ok(r) if (10..=90).contains(&r) => {
                            self.ui.left_panel_percent = r;
                            self.config.display.panel_ratio = r;
                            format!("panel_ratio = {}", r)
                        }
                        _ => "Invalid value for panel_ratio: must be 10-90".to_string(),
                    },
                    None => format!("panel_ratio = {}", self.ui.left_panel_percent),
                }
            }

            "view" | "view_mode" => {
                let current_mode = self.active_panel().view_mode;
                let new_mode = match value {
                    Some("brief") | Some("b") => ViewMode::Brief,
                    Some("full") | Some("f") => ViewMode::Full,
                    None => {
                        // Toggle
                        match current_mode {
                            ViewMode::Brief => ViewMode::Full,
                            ViewMode::Full => ViewMode::Brief,
                        }
                    }
                    _ => return format!("Invalid value for {}: use brief/full", option),
                };
                self.active_panel_mut().set_view_mode(new_mode);
                format!("view_mode = {:?}", new_mode)
            }

            "view_both" | "view_mode_both" => {
                let new_mode = match value {
                    Some("brief") | Some("b") => ViewMode::Brief,
                    Some("full") | Some("f") => ViewMode::Full,
                    None => {
                        // Toggle based on left panel
                        match self.left_panel.view_mode {
                            ViewMode::Brief => ViewMode::Full,
                            ViewMode::Full => ViewMode::Brief,
                        }
                    }
                    _ => return format!("Invalid value for {}: use brief/full", option),
                };
                self.left_panel.set_view_mode(new_mode);
                self.right_panel.set_view_mode(new_mode);
                format!("view_mode (both) = {:?}", new_mode)
            }

            "left_view" => {
                let new_mode = match value {
                    Some("brief") | Some("b") => ViewMode::Brief,
                    Some("full") | Some("f") => ViewMode::Full,
                    None => {
                        match self.left_panel.view_mode {
                            ViewMode::Brief => ViewMode::Full,
                            ViewMode::Full => ViewMode::Brief,
                        }
                    }
                    _ => return format!("Invalid value for {}: use brief/full", option),
                };
                self.left_panel.set_view_mode(new_mode);
                format!("left_view = {:?}", new_mode)
            }

            "right_view" => {
                let new_mode = match value {
                    Some("brief") | Some("b") => ViewMode::Brief,
                    Some("full") | Some("f") => ViewMode::Full,
                    None => {
                        match self.right_panel.view_mode {
                            ViewMode::Brief => ViewMode::Full,
                            ViewMode::Full => ViewMode::Brief,
                        }
                    }
                    _ => return format!("Invalid value for {}: use brief/full", option),
                };
                self.right_panel.set_view_mode(new_mode);
                format!("right_view = {:?}", new_mode)
            }

            "git" | "show_git" | "show_git_status" => {
                let new_val = match value {
                    Some("true") | Some("1") | Some("on") | Some("yes") => true,
                    Some("false") | Some("0") | Some("off") | Some("no") => false,
                    None => !self.config.display.show_git_status, // Toggle
                    _ => return format!("Invalid value for {}: use true/false", option),
                };
                self.config.display.show_git_status = new_val;
                format!("show_git_status = {}", new_val)
            }

            "dirs_first" => {
                let current_val = self.active_panel().sort_config.dirs_first;
                let new_val = match value {
                    Some("true") | Some("1") | Some("on") | Some("yes") => true,
                    Some("false") | Some("0") | Some("off") | Some("no") => false,
                    None => !current_val, // Toggle
                    _ => return format!("Invalid value for {}: use true/false", option),
                };
                self.active_panel_mut().sort_config.dirs_first = new_val;
                self.active_panel_mut().resort();
                format!("dirs_first = {}", new_val)
            }

            "dirs_first_both" => {
                let new_val = match value {
                    Some("true") | Some("1") | Some("on") | Some("yes") => true,
                    Some("false") | Some("0") | Some("off") | Some("no") => false,
                    None => !self.left_panel.sort_config.dirs_first, // Toggle based on left
                    _ => return format!("Invalid value for {}: use true/false", option),
                };
                self.left_panel.sort_config.dirs_first = new_val;
                self.right_panel.sort_config.dirs_first = new_val;
                self.left_panel.resort();
                self.right_panel.resort();
                format!("dirs_first (both) = {}", new_val)
            }

            "uppercase_first" => {
                let current_val = self.active_panel().sort_config.uppercase_first;
                let new_val = match value {
                    Some("true") | Some("1") | Some("on") | Some("yes") => true,
                    Some("false") | Some("0") | Some("off") | Some("no") => false,
                    None => !current_val, // Toggle
                    _ => return format!("Invalid value for {}: use true/false", option),
                };
                self.active_panel_mut().sort_config.uppercase_first = new_val;
                self.active_panel_mut().resort();
                format!("uppercase_first = {}", new_val)
            }

            "uppercase_first_both" => {
                let new_val = match value {
                    Some("true") | Some("1") | Some("on") | Some("yes") => true,
                    Some("false") | Some("0") | Some("off") | Some("no") => false,
                    None => !self.left_panel.sort_config.uppercase_first,
                    _ => return format!("Invalid value for {}: use true/false", option),
                };
                self.left_panel.sort_config.uppercase_first = new_val;
                self.right_panel.sort_config.uppercase_first = new_val;
                self.left_panel.resort();
                self.right_panel.resort();
                format!("uppercase_first (both) = {}", new_val)
            }

            "sort" | "sort_field" => {
                let field = match value {
                    Some("name") | Some("n") => SortField::Name,
                    Some("ext") | Some("extension") | Some("e") => SortField::Extension,
                    Some("size") | Some("s") => SortField::Size,
                    Some("modified") | Some("date") | Some("m") | Some("d") => SortField::Modified,
                    Some("unsorted") | Some("none") | Some("u") => SortField::Unsorted,
                    _ => return "Invalid sort field: use name/ext/size/modified/unsorted".to_string(),
                };
                self.active_panel_mut().sort_config.field = field;
                self.active_panel_mut().resort();
                format!("sort_field = {:?}", field)
            }

            "sort_both" | "sort_field_both" => {
                let field = match value {
                    Some("name") | Some("n") => SortField::Name,
                    Some("ext") | Some("extension") | Some("e") => SortField::Extension,
                    Some("size") | Some("s") => SortField::Size,
                    Some("modified") | Some("date") | Some("m") | Some("d") => SortField::Modified,
                    Some("unsorted") | Some("none") | Some("u") => SortField::Unsorted,
                    _ => return "Invalid sort field: use name/ext/size/modified/unsorted".to_string(),
                };
                self.left_panel.sort_config.field = field;
                self.right_panel.sort_config.field = field;
                self.left_panel.resort();
                self.right_panel.resort();
                format!("sort_field (both) = {:?}", field)
            }

            "theme" => {
                match value {
                    Some(name) => {
                        if let Some(theme) = self.config.theme.get_theme(name) {
                            self.theme = theme;
                            self.config.theme.preset = name.to_string();
                            format!("theme = {}", name)
                        } else {
                            let available = self.config.theme.available_themes().join(", ");
                            format!("Unknown theme: {}. Available: {}", name, available)
                        }
                    }
                    None => format!("theme = {}", self.config.theme.preset),
                }
            }

            "remember_path" | "remember" => {
                let new_val = match value {
                    Some("true") | Some("1") | Some("on") | Some("yes") => true,
                    Some("false") | Some("0") | Some("off") | Some("no") => false,
                    None => !self.config.general.remember_path, // Toggle
                    _ => return format!("Invalid value for {}: use true/false", option),
                };
                self.config.general.remember_path = new_val;
                format!("remember_path = {}", new_val)
            }

            _ => format!("Unknown option: {}. Type 'help' for available options.", option),
        }
    }

    /// Show current settings
    fn show_settings(&self) -> String {
        let left_view = match self.left_panel.view_mode {
            ViewMode::Brief => "brief",
            ViewMode::Full => "full",
        };
        let right_view = match self.right_panel.view_mode {
            ViewMode::Brief => "brief",
            ViewMode::Full => "full",
        };
        format!(
            "hidden={} left_view={} right_view={} shell_height={} panel_ratio={} git={} dirs_first={} uppercase_first={} sort={} theme={} remember_path={}",
            self.config.general.show_hidden,
            left_view,
            right_view,
            self.ui.shell_height,
            self.ui.left_panel_percent,
            self.config.display.show_git_status,
            self.config.sorting.dirs_first,
            self.config.sorting.uppercase_first,
            self.config.sorting.field,
            self.config.theme.preset,
            self.config.general.remember_path,
        )
    }

    /// Help text for built-in commands
    fn builtin_help(&self) -> String {
        "Built-in: config-save, config-reload, config-edit, config-upgrade, config-reset, show-hidden, show-settings, set <opt>=<val>, theme <name>, themes, q".to_string()
    }

    // ========================================================================
    // CONFIGURATION
    // ========================================================================

    /// Sync runtime state to config (before saving)
    fn sync_config(&mut self) {
        // Sync shell height
        self.config.display.shell_height = self.ui.shell_height;

        // Sync panel ratio
        self.config.display.panel_ratio = self.ui.left_panel_percent;

        // Sync view modes
        self.config.display.view_mode = match self.left_panel.view_mode {
            ViewMode::Brief => "brief",
            ViewMode::Full => "full",
        }.to_string();

        // Sync hidden
        self.config.general.show_hidden = self.left_panel.show_hidden;

        // Sync sorting
        self.config.sorting.field = match self.left_panel.sort_config.field {
            SortField::Name => "name",
            SortField::Extension => "extension",
            SortField::Size => "size",
            SortField::Modified => "modified",
            SortField::Unsorted => "unsorted",
        }.to_string();
        self.config.sorting.dirs_first = self.left_panel.sort_config.dirs_first;
        self.config.sorting.uppercase_first = self.left_panel.sort_config.uppercase_first;

        // Sync paths and per-panel view modes
        self.config.general.last_left_path = Some(self.left_panel.path.to_string_lossy().to_string());
        self.config.general.last_right_path = Some(self.right_panel.path.to_string_lossy().to_string());
        self.config.general.last_left_view = Some(match self.left_panel.view_mode {
            ViewMode::Brief => "brief",
            ViewMode::Full => "full",
        }.to_string());
        self.config.general.last_right_view = Some(match self.right_panel.view_mode {
            ViewMode::Brief => "brief",
            ViewMode::Full => "full",
        }.to_string());
    }

    /// Apply current config to panels
    fn apply_config(&mut self) {
        // Apply view mode
        let view_mode = match self.config.display.view_mode.as_str() {
            "full" => ViewMode::Full,
            _ => ViewMode::Brief,
        };
        self.left_panel.view_mode = view_mode;
        self.right_panel.view_mode = view_mode;

        // Apply sorting
        let sort_field = match self.config.sorting.field.as_str() {
            "extension" => SortField::Extension,
            "size" => SortField::Size,
            "modified" => SortField::Modified,
            "unsorted" => SortField::Unsorted,
            _ => SortField::Name,
        };
        let sort_direction = match self.config.sorting.direction.as_str() {
            "descending" => SortDirection::Descending,
            _ => SortDirection::Ascending,
        };
        self.left_panel.sort_config.field = sort_field;
        self.left_panel.sort_config.direction = sort_direction;
        self.left_panel.sort_config.dirs_first = self.config.sorting.dirs_first;
        self.left_panel.sort_config.uppercase_first = self.config.sorting.uppercase_first;
        self.right_panel.sort_config = self.left_panel.sort_config;
        self.left_panel.resort();
        self.right_panel.resort();

        // Apply hidden
        self.left_panel.show_hidden = self.config.general.show_hidden;
        self.right_panel.show_hidden = self.config.general.show_hidden;
        self.left_panel.refresh();
        self.right_panel.refresh();

        // Apply shell height
        self.ui.shell_height = self.config.display.shell_height.max(1);

        // Apply panel ratio
        self.ui.left_panel_percent = self.config.display.panel_ratio.clamp(10, 90);

        // Apply theme
        self.theme = self.config.theme.build_theme();
    }

    // ========================================================================
    // FILE OPERATIONS (Copy, Move, Delete)
    // ========================================================================

    /// Show copy confirmation dialog
    pub fn copy_selected(&mut self) {
        let (src_panel, dest_path) = match self.active_panel {
            Side::Left => (&self.left_panel, self.right_panel.path.clone()),
            Side::Right => (&self.right_panel, self.left_panel.path.clone()),
        };

        // Get files to copy (selected or current)
        let sources: Vec<PathBuf> = if src_panel.selected.is_empty() {
            src_panel.selected()
                .filter(|e| e.name != "..")
                .map(|e| e.path.clone())
                .into_iter()
                .collect()
        } else {
            src_panel.selected.iter().cloned().collect()
        };

        if sources.is_empty() {
            self.active_panel_mut().error = Some("No files to copy".to_string());
            return;
        }

        let dest_str = dest_path.to_string_lossy().to_string();
        let cursor_pos = dest_str.len();
        // Pre-select the destination text so typing replaces it
        self.ui.input_selected = !dest_str.is_empty();
        self.mode = Mode::Confirming {
            operation: FileOperation::Copy,
            sources,
            dest_input: dest_str,
            cursor_pos,
            focus: 0, // Start with input field focused
        };
    }

    /// Show move confirmation dialog
    pub fn move_selected(&mut self) {
        let (src_panel, dest_path) = match self.active_panel {
            Side::Left => (&self.left_panel, self.right_panel.path.clone()),
            Side::Right => (&self.right_panel, self.left_panel.path.clone()),
        };

        // Get files to move (selected or current)
        let sources: Vec<PathBuf> = if src_panel.selected.is_empty() {
            src_panel.selected()
                .filter(|e| e.name != "..")
                .map(|e| e.path.clone())
                .into_iter()
                .collect()
        } else {
            src_panel.selected.iter().cloned().collect()
        };

        if sources.is_empty() {
            self.active_panel_mut().error = Some("No files to move".to_string());
            return;
        }

        let dest_str = dest_path.to_string_lossy().to_string();
        let cursor_pos = dest_str.len();
        // Pre-select the destination text so typing replaces it
        self.ui.input_selected = !dest_str.is_empty();
        self.mode = Mode::Confirming {
            operation: FileOperation::Move,
            sources,
            dest_input: dest_str,
            cursor_pos,
            focus: 0, // Start with input field focused
        };
    }

    /// Show delete confirmation dialog
    pub fn delete_selected(&mut self) {
        let src_panel = match self.active_panel {
            Side::Left => &self.left_panel,
            Side::Right => &self.right_panel,
        };

        // Get files to delete (selected or current)
        let sources: Vec<PathBuf> = if src_panel.selected.is_empty() {
            src_panel.selected()
                .filter(|e| e.name != "..")
                .map(|e| e.path.clone())
                .into_iter()
                .collect()
        } else {
            src_panel.selected.iter().cloned().collect()
        };

        if sources.is_empty() {
            self.active_panel_mut().error = Some("No files to delete".to_string());
            return;
        }

        self.mode = Mode::Confirming {
            operation: FileOperation::Delete,
            sources,
            dest_input: String::new(), // Not used for delete
            cursor_pos: 0,
            focus: 1, // Start with Delete button focused (no input field)
        };
    }

    /// Apply file attributes (modification time, permissions) from a provider entry
    /// to a locally-written file. Best-effort — errors are silently ignored since
    /// the file data was already written successfully.
    #[allow(unused_variables)]
    fn apply_provider_attributes(dest: &Path, modified: Option<std::time::SystemTime>, permissions: u32) {
        if let Some(mtime) = modified {
            if let Ok(file) = std::fs::File::options().write(true).open(dest) {
                let _ = file.set_modified(mtime);
            }
        }
        #[cfg(unix)]
        if permissions != 0 {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(dest, std::fs::Permissions::from_mode(permissions));
        }
    }

    /// Execute the confirmed file operation
    pub fn execute_file_operation(&mut self, operation: FileOperation, sources: Vec<PathBuf>, dest: PathBuf) {
        // Block move and delete from archives (read-only)
        if self.active_panel().is_in_archive() {
            match &operation {
                FileOperation::Move => {
                    self.active_panel_mut().error = Some("Cannot move from archive (read-only)".into());
                    return;
                }
                FileOperation::Delete => {
                    self.active_panel_mut().error = Some("Cannot delete from archive (read-only)".into());
                    return;
                }
                FileOperation::Copy => {} // Copy from archive is fine
            }
        }

        let mut count = 0;
        let mut errors = Vec::new();

        // Resolve relative destination paths against the active panel's directory
        let dest = if dest.is_relative() {
            let base = self.active_panel().path.clone();
            base.join(&dest).canonicalize().unwrap_or_else(|_| base.join(&dest))
        } else {
            dest
        };

        // Single file to a non-directory destination = rename (use dest as full path)
        let is_rename = sources.len() == 1 && !dest.is_dir();

        // Check if source (active) panel is remote
        let src_is_remote = self.active_panel().is_remote();
        // Check if destination (inactive) panel is remote
        let dest_is_remote = match self.active_panel {
            Side::Left => self.right_panel.is_remote(),
            Side::Right => self.left_panel.is_remote(),
        };

        // For local-to-local copy/move, run in background with progress
        if !src_is_remote && !dest_is_remote && !matches!(operation, FileOperation::Delete) {
            let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
            self.cancel_token = Some(cancel.clone());
            let title = match &operation {
                FileOperation::Copy => "Copying",
                FileOperation::Move => "Moving",
                FileOperation::Delete => unreachable!(),
            }.to_string();
            let task = super::background::BackgroundTask::file_operation(
                operation, sources, dest, cancel,
            );
            self.background_task = Some(task);
            self.mode = Mode::FileOpProgress {
                title,
                bytes_done: 0,
                bytes_total: 0,
                current_file: String::new(),
                files_done: 0,
                files_total: 0,
                frame: 0,
            };
            return;
        }

        let dest_is_remote = match self.active_panel {
            Side::Left => self.right_panel.is_remote(),
            Side::Right => self.left_panel.is_remote(),
        };

        for src_path in &sources {
            let result: Result<(), String> = match &operation {
                FileOperation::Delete => {
                    if src_is_remote {
                        // Delete on remote
                        let path_str = src_path.to_string_lossy().to_string();
                        // Check if it's a directory by looking at the entries
                        let is_dir = self.active_panel().entries.iter()
                            .find(|e| e.path == *src_path)
                            .map(|e| e.is_dir)
                            .unwrap_or(false);
                        self.active_panel_mut().delete_path(&path_str, is_dir).map_err(|e| e.to_string())
                    } else {
                        // Delete local
                        delete_path(src_path).map_err(|e: std::io::Error| e.to_string())
                    }
                }
                FileOperation::Copy => {
                    let dest_file = if is_rename {
                        dest.clone()
                    } else {
                        dest.join(src_path.file_name().unwrap_or_default())
                    };

                    match (src_is_remote, dest_is_remote) {
                        (false, false) => {
                            // Local to local
                            copy_path(src_path, &dest_file).map_err(|e: std::io::Error| e.to_string())
                        }
                        (true, false) => {
                            // Remote to local: download
                            let path_str = src_path.to_string_lossy().to_string();
                            // Grab metadata from the source entry before writing
                            let (modified, permissions) = self.active_panel().entries.iter()
                                .find(|e| e.path == *src_path)
                                .map(|e| (e.modified, e.permissions))
                                .unwrap_or((None, 0));
                            match self.active_panel_mut().read_file(&path_str) {
                                Ok(data) => {
                                    match std::fs::write(&dest_file, data) {
                                        Ok(()) => {
                                            Self::apply_provider_attributes(&dest_file, modified, permissions);
                                            Ok(())
                                        }
                                        Err(e) => Err(e.to_string()),
                                    }
                                }
                                Err(e) => Err(e.to_string()),
                            }
                        }
                        (false, true) => {
                            // Local to remote: upload, then set attributes
                            let meta = std::fs::metadata(src_path).ok();
                            let modified = meta.as_ref().and_then(|m| m.modified().ok());
                            #[cfg(unix)]
                            let permissions = meta.as_ref().map(|m| {
                                use std::os::unix::fs::PermissionsExt;
                                m.permissions().mode()
                            }).unwrap_or(0);
                            #[cfg(not(unix))]
                            let permissions = 0u32;

                            match std::fs::read(src_path) {
                                Ok(data) => {
                                    let dest_str = dest_file.to_string_lossy().to_string();
                                    let panel = match self.active_panel {
                                        Side::Left => &mut self.right_panel,
                                        Side::Right => &mut self.left_panel,
                                    };
                                    match panel.write_file(&dest_str, &data) {
                                        Ok(()) => {
                                            let _ = panel.set_attributes(&dest_str, modified, permissions);
                                            Ok(())
                                        }
                                        Err(e) => Err(e.to_string()),
                                    }
                                }
                                Err(e) => Err(e.to_string()),
                            }
                        }
                        (true, true) => {
                            // Remote to remote: download then upload, preserve attributes
                            let path_str = src_path.to_string_lossy().to_string();
                            let (modified, permissions) = self.active_panel().entries.iter()
                                .find(|e| e.path == *src_path)
                                .map(|e| (e.modified, e.permissions))
                                .unwrap_or((None, 0));
                            match self.active_panel_mut().read_file(&path_str) {
                                Ok(data) => {
                                    let dest_str = dest_file.to_string_lossy().to_string();
                                    let panel = match self.active_panel {
                                        Side::Left => &mut self.right_panel,
                                        Side::Right => &mut self.left_panel,
                                    };
                                    match panel.write_file(&dest_str, &data) {
                                        Ok(()) => {
                                            let _ = panel.set_attributes(&dest_str, modified, permissions);
                                            Ok(())
                                        }
                                        Err(e) => Err(e.to_string()),
                                    }
                                }
                                Err(e) => Err(e.to_string()),
                            }
                        }
                    }
                }
                FileOperation::Move => {
                    let dest_file = if is_rename {
                        dest.clone()
                    } else {
                        dest.join(src_path.file_name().unwrap_or_default())
                    };

                    match (src_is_remote, dest_is_remote) {
                        (false, false) => {
                            // Local to local
                            move_path(src_path, &dest_file).map_err(|e: std::io::Error| e.to_string())
                        }
                        (true, false) => {
                            // Remote to local: download then delete remote
                            let path_str = src_path.to_string_lossy().to_string();
                            let (modified, permissions) = self.active_panel().entries.iter()
                                .find(|e| e.path == *src_path)
                                .map(|e| (e.modified, e.permissions))
                                .unwrap_or((None, 0));
                            match self.active_panel_mut().read_file(&path_str) {
                                Ok(data) => {
                                    match std::fs::write(&dest_file, &data) {
                                        Ok(()) => {
                                            Self::apply_provider_attributes(&dest_file, modified, permissions);
                                            self.active_panel_mut().delete_path(&path_str, false).map_err(|e| e.to_string())
                                        }
                                        Err(e) => Err(e.to_string()),
                                    }
                                }
                                Err(e) => Err(e.to_string()),
                            }
                        }
                        (false, true) => {
                            // Local to remote: upload with attributes, then delete local
                            let meta = std::fs::metadata(src_path).ok();
                            let modified = meta.as_ref().and_then(|m| m.modified().ok());
                            #[cfg(unix)]
                            let permissions = meta.as_ref().map(|m| {
                                use std::os::unix::fs::PermissionsExt;
                                m.permissions().mode()
                            }).unwrap_or(0);
                            #[cfg(not(unix))]
                            let permissions = 0u32;

                            match std::fs::read(src_path) {
                                Ok(data) => {
                                    let dest_str = dest_file.to_string_lossy().to_string();
                                    let panel = match self.active_panel {
                                        Side::Left => &mut self.right_panel,
                                        Side::Right => &mut self.left_panel,
                                    };
                                    match panel.write_file(&dest_str, &data) {
                                        Ok(()) => {
                                            let _ = panel.set_attributes(&dest_str, modified, permissions);
                                            std::fs::remove_file(src_path).map_err(|e| e.to_string())
                                        }
                                        Err(e) => Err(e.to_string()),
                                    }
                                }
                                Err(e) => Err(e.to_string()),
                            }
                        }
                        (true, true) => {
                            // Remote to remote: download/upload/delete, preserve attributes
                            let path_str = src_path.to_string_lossy().to_string();
                            let (modified, permissions) = self.active_panel().entries.iter()
                                .find(|e| e.path == *src_path)
                                .map(|e| (e.modified, e.permissions))
                                .unwrap_or((None, 0));
                            match self.active_panel_mut().read_file(&path_str) {
                                Ok(data) => {
                                    let dest_str = dest_file.to_string_lossy().to_string();
                                    let panel = match self.active_panel {
                                        Side::Left => &mut self.right_panel,
                                        Side::Right => &mut self.left_panel,
                                    };
                                    match panel.write_file(&dest_str, &data) {
                                        Ok(()) => {
                                            let _ = panel.set_attributes(&dest_str, modified, permissions);
                                            self.active_panel_mut().delete_path(&path_str, false).map_err(|e| e.to_string())
                                        }
                                        Err(e) => Err(e.to_string()),
                                    }
                                }
                                Err(e) => Err(e.to_string()),
                            }
                        }
                    }
                }
            };

            if let Err(e) = result {
                errors.push(format!("{}: {}", src_path.display(), e));
            } else {
                count += 1;
            }
        }

        // Clear selection after operation
        self.active_panel_mut().selected.clear();

        // Refresh both panels and git status
        self.left_panel.refresh();
        self.right_panel.refresh();
        self.refresh_git_status();

        // Show result
        let op_name = match operation {
            FileOperation::Copy => "Copied",
            FileOperation::Move => "Moved",
            FileOperation::Delete => "Deleted",
        };

        if !errors.is_empty() {
            self.active_panel_mut().error = Some(format!(
                "{} {}, {} errors: {}",
                op_name,
                count,
                errors.len(),
                errors.first().unwrap_or(&String::new())
            ));
        } else {
            self.add_shell_output(format!("{} {} file(s)", op_name, count));
        }
    }

    /// Start a file operation with overwrite conflict checking.
    /// Called from the confirmation dialog instead of execute_file_operation() directly.
    pub fn start_file_operation(&mut self, operation: FileOperation, sources: Vec<PathBuf>, dest: PathBuf) {
        // Resolve relative destination paths against the active panel's directory
        // (so typing just a filename in F5/F6 renames in the current directory)
        let dest = if dest.is_relative() {
            let base = self.active_panel().path.clone();
            base.join(&dest).canonicalize().unwrap_or_else(|_| base.join(&dest))
        } else {
            dest
        };

        // Only check for overwrites on local-to-local copy/move
        let src_is_remote = self.active_panel().is_remote();
        let dest_is_local = !match self.active_panel {
            Side::Left => self.right_panel.is_remote(),
            Side::Right => self.left_panel.is_remote(),
        };

        if !matches!(operation, FileOperation::Delete) && !src_is_remote && dest_is_local {
            let conflicts = Self::find_overwrite_conflicts(&sources, &dest);
            if !conflicts.is_empty() {
                self.mode = Mode::OverwriteConfirm {
                    operation,
                    all_sources: sources,
                    dest,
                    conflicts,
                    current_conflict: 0,
                    skip_set: std::collections::HashSet::new(),
                    overwrite_all: false,
                    focus: 0,
                };
                return;
            }
        }

        // No conflicts — proceed directly
        self.execute_file_operation(operation, sources, dest);
    }

    /// Find destination files that already exist (overwrite conflicts).
    fn find_overwrite_conflicts(sources: &[PathBuf], dest: &Path) -> Vec<PathBuf> {
        // Single file to a non-directory dest = rename; check if dest itself exists
        if sources.len() == 1 && !dest.is_dir() {
            if dest.exists() {
                return sources.to_vec();
            }
            return Vec::new();
        }
        sources.iter().filter_map(|src| {
            let name = src.file_name()?;
            let dest_file = dest.join(name);
            if dest_file.exists() { Some(src.clone()) } else { None }
        }).collect()
    }

    // ========================================================================
    // DIALOGS
    // ========================================================================

    /// Show mkdir dialog (F7)
    pub fn show_mkdir_dialog(&mut self) {
        self.mode = Mode::MakingDir {
            name_input: String::new(),
            cursor_pos: 0,
            focus: 0, // Start with input field focused
        };
    }

    /// Show the command history panel
    pub fn show_command_history(&mut self) {
        // Start with the last (most recent) command selected, or 0 if empty
        let last_idx = self.cmd.history.len().saturating_sub(1);
        // Calculate visible height: terminal_height - 6 (dialog margins) - 4 (border + help)
        let visible_height = self.ui.terminal_height.saturating_sub(10) as usize;
        // Scroll so selected item is visible at the bottom of the list
        let scroll = if visible_height > 0 {
            last_idx.saturating_sub(visible_height - 1)
        } else {
            0
        };
        self.mode = Mode::CommandHistory {
            selected: last_idx,
            scroll,
        };
    }

    /// Show the select files by pattern dialog
    pub fn show_select_files_dialog(&mut self) {
        self.ui.input_selected = true; // Select "*" so typing replaces it
        self.mode = Mode::SelectFiles {
            pattern_input: "*".to_string(),
            pattern_cursor: 1,
            include_dirs: true,
            focus: 0, // Start with pattern input focused
        };
    }

    /// Show the user menu (F2)
    pub fn show_user_menu(&mut self) {
        let rules = self.config.user_menu.clone();
        self.mode = Mode::UserMenu {
            rules,
            selected: 0,
            scroll: 0,
        };
    }

    /// Execute a user menu command, substituting placeholders
    pub fn execute_user_menu_command(&mut self, template: &str) {
        let panel = self.active_panel();
        let cwd = panel.path.clone();

        // Get current file info
        let (filename, name_no_ext, extension) = if let Some(entry) = panel.selected() {
            let name = entry.name.clone();
            let path = std::path::Path::new(&name);
            let stem = path.file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            let ext = path.extension()
                .map(|e| e.to_string_lossy().to_string())
                .unwrap_or_default();
            (name, stem, ext)
        } else {
            (String::new(), String::new(), String::new())
        };

        // Get selected files (space-separated, quoted)
        let selected_files: String = if panel.selected.is_empty() {
            // No selection - use current file
            if !filename.is_empty() {
                crate::input::shell_escape(&filename)
            } else {
                String::new()
            }
        } else {
            // Use all selected files
            panel.selected.iter()
                .filter_map(|p| p.file_name())
                .map(|n| crate::input::shell_escape(&n.to_string_lossy()))
                .collect::<Vec<_>>()
                .join(" ")
        };

        // Substitute placeholders
        let command = template
            .replace("!.!", &filename)
            .replace("%f", &filename)
            .replace("!.", &name_no_ext)
            .replace("%n", &name_no_ext)
            .replace("%e", &extension)
            .replace("%d", &cwd.to_string_lossy())
            .replace("%s", &selected_files);

        // Run the command
        self.mode = Mode::RunningCommand { command, cwd };
    }

    /// Execute select files by pattern
    pub fn execute_select_files(&mut self, pattern: &str, include_dirs: bool) {
        if pattern.is_empty() {
            self.active_panel_mut().error = Some("Pattern cannot be empty".to_string());
            return;
        }

        // Convert glob pattern (* and ?) to regex
        let regex_pattern = glob_to_regex(pattern, false); // case insensitive
        let regex = match regex::Regex::new(&regex_pattern) {
            Ok(r) => r,
            Err(e) => {
                self.active_panel_mut().error = Some(format!("Invalid pattern: {}", e));
                return;
            }
        };

        let panel = self.active_panel_mut();
        let mut count = 0;

        // Select matching entries
        for entry in &panel.entries {
            // Skip ".."
            if entry.name == ".." {
                continue;
            }

            // Skip directories if not including them
            if entry.is_dir && !include_dirs {
                continue;
            }

            // Check if name matches pattern
            if regex.is_match(&entry.name) {
                panel.selected.insert(entry.path.clone());
                count += 1;
            }
        }

        if count > 0 {
            self.add_shell_output(format!("Selected {} item(s) matching '{}'", count, pattern));
        } else {
            self.active_panel_mut().error = Some(format!("No items matching '{}'", pattern));
        }
    }

    pub fn show_find_files_dialog(&mut self) {
        let current_path = self.active_panel().path.to_string_lossy().to_string();
        let path_len = current_path.len();
        self.ui.input_selected = true; // Select "*" so typing replaces it
        self.mode = Mode::FindFiles {
            pattern_input: "*".to_string(),
            pattern_cursor: 1,
            pattern_case_sensitive: false,
            content_input: String::new(),
            content_cursor: 0,
            content_case_sensitive: false,
            path_input: current_path,
            path_cursor: path_len,
            recursive: true,
            focus: 0, // Start with pattern input focused
        };
    }

    /// Execute find files search
    pub fn execute_find_files(
        &mut self,
        pattern: &str,
        pattern_case_sensitive: bool,
        content: &str,
        content_case_sensitive: bool,
        start_path: &Path,
        recursive: bool,
    ) {
        if pattern.is_empty() {
            self.active_panel_mut().error = Some("Pattern cannot be empty".to_string());
            return;
        }

        // Convert glob pattern (* and ?) to regex
        let regex_pattern = glob_to_regex(pattern, pattern_case_sensitive);
        let regex = match regex::Regex::new(&regex_pattern) {
            Ok(r) => r,
            Err(e) => {
                self.active_panel_mut().error = Some(format!("Invalid pattern: {}", e));
                return;
            }
        };

        // Collect matching files
        let mut matches: Vec<PathBuf> = Vec::new();
        let start = if start_path.is_dir() {
            start_path.to_path_buf()
        } else {
            self.active_panel().path.clone()
        };

        self.find_files_recursive(&start, &regex, content, content_case_sensitive, recursive, &mut matches, 0);

        if matches.is_empty() {
            if content.is_empty() {
                self.active_panel_mut().error = Some(format!("No files matching '{}' found", pattern));
            } else {
                self.active_panel_mut().error = Some(format!("No files matching '{}' containing '{}' found", pattern, content));
            }
        } else {
            // Show summary in shell
            let count = matches.len();
            if content.is_empty() {
                self.add_shell_output(format!("Found {} file(s) matching '{}':", count, pattern));
            } else {
                self.add_shell_output(format!("Found {} file(s) matching '{}' containing '{}':", count, pattern, content));
            }

            // Output each file path to shell for easy copy-paste
            for path in &matches {
                self.add_shell_output(path.to_string_lossy().to_string());
            }

            // Enter temp mode with the found files in the OTHER panel
            self.inactive_panel_mut().enter_temp_mode(matches);

            // Switch to the other panel so user sees the results
            self.toggle_panel();
        }
    }

    /// Recursively find files matching the pattern and optionally containing text
    #[allow(clippy::too_many_arguments)]
    fn find_files_recursive(
        &self,
        dir: &Path,
        regex: &regex::Regex,
        content_search: &str,
        content_case_sensitive: bool,
        recursive: bool,
        matches: &mut Vec<PathBuf>,
        depth: usize,
    ) {
        // Limit recursion depth to prevent stack overflow
        if depth > 100 {
            return;
        }

        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return,
        };

        for entry in entries.flatten() {
            let path = entry.path();
            let file_name = entry.file_name().to_string_lossy().to_string();

            // Check if file name matches pattern
            if regex.is_match(&file_name) && path.is_file() {
                // If content search is specified, check file content
                if content_search.is_empty()
                    || Self::file_contains_text(&path, content_search, content_case_sensitive)
                {
                    matches.push(path.clone());
                }
            }

            // Recurse into directories
            if recursive && path.is_dir() && file_name != "." && file_name != ".." {
                self.find_files_recursive(&path, regex, content_search, content_case_sensitive, recursive, matches, depth + 1);
            }
        }
    }

    /// Check if a file contains the given text
    fn file_contains_text(path: &Path, search_text: &str, case_sensitive: bool) -> bool {
        // Read file content - skip binary files and large files
        let metadata = match std::fs::metadata(path) {
            Ok(m) => m,
            Err(_) => return false,
        };

        // Skip files larger than 10MB
        if metadata.len() > 10 * 1024 * 1024 {
            return false;
        }

        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => return false, // Skip binary files or unreadable files
        };

        if case_sensitive {
            content.contains(search_text)
        } else {
            content.to_lowercase().contains(&search_text.to_lowercase())
        }
    }

    /// Create the directory with the given name
    pub fn create_directory(&mut self, name: &str) {
        if name.is_empty() {
            self.active_panel_mut().error = Some("Directory name cannot be empty".to_string());
            return;
        }

        let current_dir = self.active_panel().path.clone();
        let is_remote = self.active_panel().is_remote();

        let result = if is_remote {
            // Create directory on remote
            let new_dir = if current_dir.to_string_lossy() == "/" {
                format!("/{}", name)
            } else {
                format!("{}/{}", current_dir.to_string_lossy().trim_end_matches('/'), name)
            };
            self.active_panel_mut().mkdir(&new_dir)
        } else {
            // Create directory locally
            let new_dir_path = current_dir.join(name);
            std::fs::create_dir(&new_dir_path).map_err(AppError::from)
        };

        match result {
            Ok(()) => {
                self.add_shell_output(format!("Created directory: {}", name));
                // Refresh the current panel and position cursor on the new directory
                self.active_panel_mut().refresh();
                self.active_panel_mut().jump_to_prefix(name);
                self.refresh_git_status();
            }
            Err(e) => {
                self.active_panel_mut().error = Some(format!("Failed to create directory: {}", e));
            }
        }
    }

    // ========================================================================
    // SCP / REMOTE CONNECTIONS
    // ========================================================================

    /// Show source selector for the specified panel (drives, quick access, connections)
    pub fn show_source_selector(&mut self, target: Side) {
        // Get saved SCP connections and favorites from config
        // Build provider plugin summaries from loaded plugins
        let plugin_summaries: Vec<crate::providers::ProviderPluginSummary> = self.plugins
            .list_provider_plugins()
            .iter()
            .map(|info| crate::providers::ProviderPluginSummary {
                name: info.name.clone(),
                schemes: info.schemes.clone(),
                icon: info.icon,
            })
            .collect();
        let sources = get_panel_sources(
            &self.config.connections,
            &self.config.plugin_connections,
            &self.config.favorites,
            &plugin_summaries,
        );

        if sources.is_empty() {
            return;
        }

        self.mode = Mode::SourceSelector {
            target_panel: target,
            sources,
            selected: 0,
        };
    }

    /// Add the current panel's directory to favorites
    pub fn add_current_to_favorites(&mut self) {
        let panel = match self.active_panel {
            Side::Left => &self.left_panel,
            Side::Right => &self.right_panel,
        };

        // Only works for local providers
        if !panel.is_local() {
            self.add_shell_output("Cannot add remote directory to favorites".to_string());
            return;
        }

        let path = panel.path.to_string_lossy().to_string();
        let name = panel.path.file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| path.clone());

        match self.config.add_favorite(name.clone(), path.clone()) {
            Ok(true) => {
                self.add_shell_output(format!("Added '{}' to favorites", name));
            }
            Ok(false) => {
                self.add_shell_output(format!("'{}' is already in favorites", name));
            }
            Err(e) => {
                self.add_shell_output(format!("Failed to add favorite: {}", e));
            }
        }
    }

    /// Remove a source from the selector (favorites or connections)
    pub fn remove_source_at_selection(&mut self) {
        let Mode::SourceSelector { sources, selected, .. } = &self.mode else {
            return;
        };

        let selected_idx = *selected;
        if selected_idx >= sources.len() {
            return;
        }

        let source = &sources[selected_idx];
        match source {
            PanelSource::QuickAccess { path, is_favorite: true, .. } => {
                // Remove favorite
                let path = path.clone();
                if let Err(e) = self.config.remove_favorite(&path) {
                    self.add_shell_output(format!("Failed to remove favorite: {}", e));
                } else {
                    self.add_shell_output("Favorite removed".to_string());
                    // Refresh sources list
                    let target = if let Mode::SourceSelector { target_panel, .. } = &self.mode {
                        *target_panel
                    } else {
                        return;
                    };
                    self.show_source_selector(target);
                }
            }
            PanelSource::Provider { connection_name, .. } => {
                // Remove SCP connection
                let name = connection_name.clone();
                if let Err(e) = self.config.remove_connection(&name) {
                    self.add_shell_output(format!("Failed to remove connection: {}", e));
                } else {
                    self.add_shell_output("Connection removed".to_string());
                    // Refresh sources list
                    let target = if let Mode::SourceSelector { target_panel, .. } = &self.mode {
                        *target_panel
                    } else {
                        return;
                    };
                    self.show_source_selector(target);
                }
            }
            _ => {
                // Can't remove built-in items (Home, Root, drives)
                self.add_shell_output("Cannot remove built-in items".to_string());
            }
        }
    }

    /// Handle selection of a panel source
    pub fn select_source(&mut self, target: Side, source: &PanelSource) {
        let panel = match target {
            Side::Left => &mut self.left_panel,
            Side::Right => &mut self.right_panel,
        };

        match source {
            PanelSource::Drive { letter, .. } => {
                // Windows drive selection - switch to local filesystem
                let new_path = PathBuf::from(format!("{}\\", letter));
                if new_path.exists() {
                    panel.set_local_provider(new_path);
                    panel.cursor = 0;
                    panel.scroll_offset = 0;
                }
            }
            PanelSource::QuickAccess { path, .. } => {
                // Quick access path (home, root, etc.) - switch to local filesystem
                let new_path = PathBuf::from(path);
                if new_path.exists() {
                    panel.set_local_provider(new_path);
                    panel.cursor = 0;
                    panel.scroll_offset = 0;
                }
            }
            PanelSource::Provider { connection_string, connection_name, info } => {
                // Connect based on provider type
                match info.provider_type {
                    ProviderType::Scp => {
                        let conn_str = connection_string.clone();
                        self.connect_saved_scp(target, &conn_str);
                    }
                    ProviderType::Plugin => {
                        // Plugin connection — find saved connection and open dialog
                        let name = connection_name.clone();
                        let scheme = connection_string.clone(); // We store scheme in connection_string for plugin connections
                        self.edit_plugin_connection(target, &scheme, &name);
                    }
                    _ => {
                        panel.error = Some(format!("{:?} connections not yet supported", info.provider_type));
                    }
                }
            }
            PanelSource::NewConnection { provider_type } => {
                match provider_type {
                    ProviderType::Scp => {
                        self.show_scp_connect_dialog(target);
                    }
                    _ => {
                        panel.error = Some(format!("{:?} connections not yet supported", provider_type));
                    }
                }
            }
            PanelSource::NewPluginConnection { scheme, .. } => {
                let scheme = scheme.clone();
                self.show_plugin_connect_dialog(target, &scheme);
            }
        }
    }

    /// Show the SCP connection dialog
    pub fn show_scp_connect_dialog(&mut self, target: Side) {
        self.mode = Mode::ScpConnect {
            target_panel: target,
            name_input: String::new(),
            name_cursor: 0,
            user_input: String::new(),
            user_cursor: 0,
            host_input: String::new(),
            host_cursor: 0,
            port_input: "22".to_string(),
            port_cursor: 2,
            path_input: String::new(),
            path_cursor: 0,
            password_input: String::new(),
            password_cursor: 0,
            focus: 2, // Start on host field
            error: None,
        };
    }

    /// Show the SCP connection dialog pre-filled for editing an existing connection
    pub fn edit_scp_connection(&mut self, target: Side, connection_name: &str) {
        // Find the connection in config
        let Some(conn) = self.config.connections.iter().find(|c| c.name == connection_name) else {
            self.add_shell_output(format!("Connection '{}' not found", connection_name));
            return;
        };

        self.ui.input_selected = true; // Select name field so typing replaces it
        self.mode = Mode::ScpConnect {
            target_panel: target,
            name_input: conn.name.clone(),
            name_cursor: conn.name.len(),
            user_input: conn.user.clone(),
            user_cursor: conn.user.len(),
            host_input: conn.host.clone(),
            host_cursor: conn.host.len(),
            port_input: conn.port.to_string(),
            port_cursor: conn.port.to_string().len(),
            path_input: conn.path.clone().unwrap_or_default(),
            path_cursor: conn.path.as_ref().map(|p| p.len()).unwrap_or(0),
            password_input: String::new(),
            password_cursor: 0,
            focus: 0, // Start on name field for editing
            error: None,
        };
    }

    /// Delete a saved SCP connection by name
    pub fn delete_scp_connection(&mut self, connection_name: &str) {
        match self.config.remove_connection(connection_name) {
            Ok(()) => {
                self.add_shell_output(format!("Deleted connection '{}'", connection_name));
            }
            Err(e) => {
                self.add_shell_output(format!("Failed to delete connection: {}", e));
            }
        }
    }

    /// Execute a simple confirmation action
    pub fn execute_simple_confirm_action(&mut self, action: SimpleConfirmAction) {
        match action {
            SimpleConfirmAction::DeleteConnection { name } => {
                self.delete_scp_connection(&name);
            }
            SimpleConfirmAction::DeletePluginConnection { scheme, name } => {
                self.delete_plugin_connection(&scheme, &name);
            }
            SimpleConfirmAction::DeleteFavorite { path } => {
                if let Err(e) = self.config.remove_favorite(&path) {
                    self.add_shell_output(format!("Failed to remove favorite: {}", e));
                } else {
                    self.add_shell_output("Favorite removed".to_string());
                }
            }
        }
    }

    /// Save an SCP connection from the dialog
    pub fn save_scp_connection(&mut self) {
        let Mode::ScpConnect {
            name_input,
            user_input,
            host_input,
            port_input,
            path_input,
            ..
        } = &self.mode else {
            return;
        };

        // Validate inputs
        if name_input.trim().is_empty() {
            if let Mode::ScpConnect { error, .. } = &mut self.mode {
                *error = Some("Name is required".to_string());
            }
            return;
        }
        if host_input.trim().is_empty() {
            if let Mode::ScpConnect { error, .. } = &mut self.mode {
                *error = Some("Host is required".to_string());
            }
            return;
        }

        let port: u16 = port_input.parse().unwrap_or(22);
        let path = if path_input.trim().is_empty() {
            None
        } else {
            Some(path_input.trim().to_string())
        };

        let conn = crate::config::SavedConnection {
            name: name_input.trim().to_string(),
            user: if user_input.trim().is_empty() {
                "root".to_string()
            } else {
                user_input.trim().to_string()
            },
            host: host_input.trim().to_string(),
            port,
            path,
        };

        // Save to config
        match self.config.add_connection(conn) {
            Ok(()) => {
                self.mode = Mode::Normal;
            }
            Err(e) => {
                if let Mode::ScpConnect { error, .. } = &mut self.mode {
                    *error = Some(format!("Failed to save: {}", e));
                }
            }
        }
    }

    /// Connect to an SCP server from the dialog (spawns background task)
    pub fn connect_scp(&mut self) {
        use super::background::BackgroundTask;

        let Mode::ScpConnect {
            target_panel,
            user_input,
            host_input,
            port_input,
            path_input,
            password_input,
            ..
        } = &self.mode else {
            return;
        };

        // Validate inputs
        if host_input.trim().is_empty() {
            if let Mode::ScpConnect { error, .. } = &mut self.mode {
                *error = Some("Host is required".to_string());
            }
            return;
        }

        let user = if user_input.trim().is_empty() {
            "root".to_string()
        } else {
            user_input.trim().to_string()
        };
        let host = host_input.trim().to_string();
        let port: u16 = port_input.parse().unwrap_or(22);
        let initial_path = if path_input.trim().is_empty() {
            format!("/home/{}", user)
        } else {
            path_input.trim().to_string()
        };
        let password = password_input.clone();
        let target = *target_panel;

        // Create connection info with appropriate auth
        let conn_info = if password.is_empty() {
            ScpConnectionInfo::with_agent(user.clone(), host.clone()).port(port)
        } else {
            ScpConnectionInfo::with_password(user.clone(), host.clone(), password).port(port)
        };

        let display_name = format!("{}@{}", user, host);

        // Spawn background connection task
        let task = BackgroundTask::connect_scp(
            conn_info,
            target,
            initial_path,
            display_name.clone(),
            None, // No connection string for manual connections
        );

        self.background_task = Some(task);
        self.mode = Mode::BackgroundTask {
            title: "Connecting".to_string(),
            message: format!("Connecting to {}...", display_name),
            frame: 0,
        };
    }

    /// Connect to a saved SCP connection (tries key auth first, prompts for password on failure)
    pub fn connect_saved_scp(&mut self, target: Side, connection_string: &str) {
        use super::background::BackgroundTask;

        // Parse the connection string (scp://user@host:port/path)
        let Some(conn_info) = ScpConnectionInfo::from_uri(connection_string) else {
            let error_msg = format!("Invalid SCP connection URI: {}", connection_string);
            self.add_shell_output(error_msg.clone());
            let panel = match target {
                Side::Left => &mut self.left_panel,
                Side::Right => &mut self.right_panel,
            };
            panel.error = Some(error_msg);
            return;
        };

        let initial_path = conn_info.initial_path.clone()
            .unwrap_or_else(|| format!("/home/{}", conn_info.user));
        let display_name = conn_info.display_name();
        let conn_str = connection_string.to_string();

        // Spawn background connection task
        let task = BackgroundTask::connect_scp(
            conn_info,
            target,
            initial_path,
            display_name.clone(),
            Some(conn_str), // Connection string for password retry
        );

        self.background_task = Some(task);
        self.mode = Mode::BackgroundTask {
            title: "Connecting".to_string(),
            message: format!("Connecting to {}...", display_name),
            frame: 0,
        };
    }

    /// Connect to SCP with password (called after password prompt)
    pub fn connect_scp_with_password(&mut self) {
        use super::background::BackgroundTask;

        let Mode::ScpPasswordPrompt {
            target_panel,
            connection_string,
            display_name,
            password_input,
            ..
        } = &self.mode else {
            return;
        };

        let target = *target_panel;
        let password = password_input.clone();
        let display = display_name.clone();
        let conn_str = connection_string.clone();

        // Parse the connection string
        let Some(mut conn_info) = ScpConnectionInfo::from_uri(&conn_str) else {
            self.mode = Mode::Normal;
            self.add_shell_output("Invalid connection string".to_string());
            return;
        };

        let initial_path = conn_info.initial_path.clone()
            .unwrap_or_else(|| format!("/home/{}", conn_info.user));

        // Set password auth
        conn_info.auth = ScpAuth::Password(password);

        // Spawn background connection task
        let task = BackgroundTask::connect_scp(
            conn_info,
            target,
            initial_path,
            display.clone(),
            None, // No retry on password auth failure
        );

        self.background_task = Some(task);
        self.mode = Mode::BackgroundTask {
            title: "Connecting".to_string(),
            message: format!("Connecting to {}...", display),
            frame: 0,
        };
    }

    // ========================================================================
    // PLUGIN CONNECTIONS (generic for any provider plugin)
    // ========================================================================

    /// Edit a saved plugin connection (populate dialog from saved fields)
    pub fn edit_plugin_connection(&mut self, target: Side, scheme: &str, name: &str) {
        if let Some(conn) = self.config.plugin_connections.iter().find(|c| c.name == name && c.scheme == scheme) {
            let mut preset = conn.fields.clone();
            preset.insert("name".to_string(), conn.name.clone());
            // Find which fields are password-type by querying the plugin
            let password_field = self.plugins.find_provider_by_scheme(scheme)
                .and_then(|p| {
                    p.get_dialog_fields().iter()
                        .find(|f| f.field_type == bark_plugin_api::DialogFieldType::Password)
                        .map(|f| f.id.clone())
                });
            self.show_plugin_connect_dialog_with_values(target, scheme, preset, password_field.as_deref());
        }
    }

    /// Delete a saved plugin connection by scheme and name
    pub fn delete_plugin_connection(&mut self, scheme: &str, name: &str) {
        if let Err(e) = self.config.remove_plugin_connection(scheme, name) {
            self.add_shell_output(format!("Failed to delete connection: {}", e));
        } else {
            self.add_shell_output(format!("Deleted connection: {}", name));
        }
    }

    /// Show the generic plugin connection dialog
    pub fn show_plugin_connect_dialog(&mut self, target: Side, scheme: &str) {
        // Find the plugin by scheme
        let plugin = match self.plugins.find_provider_by_scheme(scheme) {
            Some(p) => p,
            None => {
                self.add_shell_output(format!("Plugin for scheme '{}' not found", scheme));
                return;
            }
        };

        let fields = plugin.get_dialog_fields();

        // Initialize values with defaults
        let values: Vec<String> = fields
            .iter()
            .map(|f| f.default_value.clone().unwrap_or_default())
            .collect();

        // Initialize cursors at end of default values
        let cursors: Vec<usize> = values.iter().map(|v| v.len()).collect();

        self.mode = Mode::PluginConnect {
            target_panel: target,
            plugin_scheme: scheme.to_string(),
            plugin_name: plugin.info().name.clone(),
            fields,
            values,
            cursors,
            focus: 0,
            error: None,
        };
    }

    /// Show the generic plugin connection dialog with pre-populated values
    /// Used for editing saved connections
    pub fn show_plugin_connect_dialog_with_values(
        &mut self,
        target: Side,
        scheme: &str,
        preset_values: std::collections::HashMap<String, String>,
        focus_field: Option<&str>,
    ) {
        // Find the plugin by scheme
        let plugin = match self.plugins.find_provider_by_scheme(scheme) {
            Some(p) => p,
            None => {
                self.add_shell_output(format!("Plugin for scheme '{}' not found", scheme));
                return;
            }
        };

        let fields = plugin.get_dialog_fields();

        // Initialize values: use preset if available, otherwise use default
        let values: Vec<String> = fields
            .iter()
            .map(|f| {
                preset_values
                    .get(&f.id)
                    .cloned()
                    .unwrap_or_else(|| f.default_value.clone().unwrap_or_default())
            })
            .collect();

        // Initialize cursors at end of values
        let cursors: Vec<usize> = values.iter().map(|v| v.len()).collect();

        // Find focus index by field id
        let focus = focus_field
            .and_then(|field_id| fields.iter().position(|f| f.id == field_id))
            .unwrap_or(0);

        self.mode = Mode::PluginConnect {
            target_panel: target,
            plugin_scheme: scheme.to_string(),
            plugin_name: plugin.info().name.clone(),
            fields,
            values,
            cursors,
            focus,
            error: None,
        };
    }

    /// Connect using a plugin from the dialog
    pub fn connect_plugin(&mut self) {
        use super::background::BackgroundTask;
        use crate::plugins::provider_api::ProviderConfig;

        let Mode::PluginConnect {
            target_panel,
            plugin_scheme,
            fields,
            values,
            error,
            ..
        } = &mut self.mode
        else {
            return;
        };

        // Find the plugin
        let plugin = match self.plugins.find_provider_by_scheme(plugin_scheme) {
            Some(p) => p,
            None => {
                *error = Some(format!("Plugin '{}' not found", plugin_scheme));
                return;
            }
        };

        // Build config from values
        let mut config = ProviderConfig::new();
        for (field, value) in fields.iter().zip(values.iter()) {
            config.set(&field.id, value);
        }

        // Validate
        if let Err(e) = plugin.validate_config(&config) {
            *error = Some(e.to_string());
            return;
        }

        // Get connection info for display
        let display_name = if let Some(name) = config.get("name") {
            if name.is_empty() {
                plugin.info().name.clone()
            } else {
                name.to_string()
            }
        } else {
            plugin.info().name.clone()
        };

        let target = *target_panel;

        // Use background task for all plugin connections
        let task = BackgroundTask::connect_plugin(
            plugin,
            config,
            target,
            display_name.clone(),
        );

        self.background_task = Some(task);
        self.mode = Mode::BackgroundTask {
            title: "Connecting".to_string(),
            message: format!("Connecting to {}...", display_name),
            frame: 0,
        };
    }

    /// Save a plugin connection (placeholder - requires config storage)
    pub fn save_plugin_connection(&mut self) {
        let Mode::PluginConnect {
            plugin_scheme,
            fields,
            values,
            error,
            ..
        } = &mut self.mode
        else {
            return;
        };

        // Build connection name from the "name" field
        let name = fields
            .iter()
            .zip(values.iter())
            .find(|(f, _)| f.id == "name")
            .map(|(_, v)| v.clone())
            .unwrap_or_default();

        if name.is_empty() {
            *error = Some("Connection name is required to save".to_string());
            return;
        }

        // Store all non-password fields as generic key-value pairs
        let mut saved_fields = std::collections::HashMap::new();
        for (field, value) in fields.iter().zip(values.iter()) {
            // Don't save password fields
            if field.field_type == bark_plugin_api::DialogFieldType::Password {
                continue;
            }
            if field.id != "name" {
                saved_fields.insert(field.id.clone(), value.clone());
            }
        }

        let scheme = plugin_scheme.clone();

        let saved = crate::config::SavedPluginConnection {
            name: name.clone(),
            scheme: scheme.clone(),
            fields: saved_fields,
        };

        if let Err(e) = self.config.add_plugin_connection(saved) {
            *error = Some(format!("Failed to save: {}", e));
            return;
        }

        self.add_shell_output(format!("Saved {} connection: {}", scheme.to_uppercase(), name));
        self.mode = Mode::Normal;
    }

    /// Change the drive for a panel (legacy, kept for compatibility)
    pub fn change_drive(&mut self, target: Side, drive: &str) {
        let new_path = PathBuf::from(format!("{}\\", drive));
        let panel = match target {
            Side::Left => &mut self.left_panel,
            Side::Right => &mut self.right_panel,
        };

        if new_path.exists() {
            panel.path = new_path;
            panel.refresh();
            panel.cursor = 0;
            panel.scroll_offset = 0;
        }
    }

    // ========================================================================
    // ARCHIVE OPENING
    // ========================================================================

    /// Open a file via an extension-mode provider plugin (e.g., archive plugin)
    pub fn open_extension_provider(&mut self, file_path: std::path::PathBuf, file_name: String) {
        use super::background::BackgroundTask;

        let plugin = match self.plugins.find_provider_by_extension(&file_path) {
            Some(p) => p,
            None => return,
        };

        let target = self.active_panel;

        let task = BackgroundTask::connect_extension_plugin(
            plugin,
            file_path,
            file_name.clone(),
            target,
        );

        self.background_task = Some(task);
        self.mode = Mode::BackgroundTask {
            title: "Opening".to_string(),
            message: format!("Opening {}...", file_name),
            frame: 0,
        };
    }

    // ========================================================================
    // FILE VIEWER
    // ========================================================================

    /// View a file's contents
    pub fn view_file(&mut self, path: &std::path::Path) {
        // Check if the active panel is local - if so, use mmap
        if self.active_panel().is_local() {
            self.view_file_mmap(path);
        } else {
            self.view_file_remote(path);
        }
    }

    /// View a local file using memory mapping (efficient for large files)
    fn view_file_mmap(&mut self, path: &std::path::Path) {
        use std::fs::File;
        use std::sync::Arc;
        use memmap2::Mmap;

        let file = match File::open(path) {
            Ok(f) => f,
            Err(e) => {
                self.active_panel_mut().error = Some(format!(
                    "Cannot open '{}': {}",
                    path.to_string_lossy(),
                    e
                ));
                return;
            }
        };

        // Get file size to handle empty files
        let metadata = match file.metadata() {
            Ok(m) => m,
            Err(e) => {
                self.active_panel_mut().error = Some(format!(
                    "Cannot read metadata for '{}': {}",
                    path.to_string_lossy(),
                    e
                ));
                return;
            }
        };

        // Handle empty files specially (mmap doesn't work on empty files)
        if metadata.len() == 0 {
            self.mode = Mode::Viewing {
                content: ViewContent::Text(String::new()),
                scroll: 0,
                path: path.to_path_buf(),
                binary_mode: BinaryViewMode::Cp437,
                search_matches: Vec::new(),
                current_match: None,
            };
            return;
        }

        // Create memory map
        let mmap = match unsafe { Mmap::map(&file) } {
            Ok(m) => Arc::new(m),
            Err(_) => {
                // Fall back to regular read if mmap fails
                self.view_file_remote(path);
                return;
            }
        };

        // Check if content is valid UTF-8
        let bytes: &[u8] = &mmap;
        let is_text = std::str::from_utf8(bytes).is_ok();

        // Compute line offsets for text files (scan once, enables O(1) line lookup)
        let line_offsets = if is_text {
            compute_line_offsets(bytes)
        } else {
            Vec::new()
        };

        let binary_mode = if is_text {
            BinaryViewMode::Cp437  // Text view
        } else {
            BinaryViewMode::Hex    // Binary view
        };

        self.mode = Mode::Viewing {
            content: ViewContent::MappedFile {
                mmap,
                is_text,
                line_offsets,
            },
            scroll: 0,
            path: path.to_path_buf(),
            binary_mode,
            search_matches: Vec::new(),
            current_match: None,
        };
    }

    /// View a remote file (fully loaded into memory)
    fn view_file_remote(&mut self, path: &std::path::Path) {
        let path_str = path.to_string_lossy().to_string();
        let bytes_result = self.active_panel_mut().read_file(&path_str)
            .map_err(std::io::Error::other);

        match bytes_result {
            Ok(bytes) => {
                // Try to interpret as UTF-8 text first
                match String::from_utf8(bytes.clone()) {
                    Ok(text) => {
                        self.mode = Mode::Viewing {
                            content: ViewContent::Text(text),
                            scroll: 0,
                            path: path.to_path_buf(),
                            binary_mode: BinaryViewMode::Cp437,  // Default to text view
                            search_matches: Vec::new(),
                            current_match: None,
                        };
                    }
                    Err(_) => {
                        // Not valid UTF-8, show as binary
                        self.mode = Mode::Viewing {
                            content: ViewContent::Binary(bytes),
                            scroll: 0,
                            path: path.to_path_buf(),
                            binary_mode: BinaryViewMode::Hex,
                            search_matches: Vec::new(),
                            current_match: None,
                        };
                    }
                }
            }
            Err(e) => {
                self.active_panel_mut().error = Some(format!(
                    "Cannot read '{}': {}",
                    path.to_string_lossy(),
                    e
                ));
            }
        }
    }

    /// View a file using a plugin or fall back to built-in viewer
    pub fn view_file_with_plugins(&mut self, path: &std::path::Path) {
        // Check if any plugin can handle this file
        if let Some(plugin) = self.plugins.find_viewer(path) {
            // Try to render with the plugin
            let context = ViewerContext {
                path: path.to_path_buf(),
                width: self.ui.terminal_width as usize,
                height: self.ui.viewer_height,
                scroll: 0,
            };

            if let Some(result) = plugin.render(&context) {
                self.mode = Mode::ViewingPlugin {
                    plugin_name: plugin.info().name.clone(),
                    path: path.to_path_buf(),
                    scroll: 0,
                    lines: result.lines,
                    total_lines: result.total_lines,
                };
                return;
            }
        }

        // Fall back to built-in viewer
        self.view_file(path);
    }

    /// Re-render plugin viewer content (for scrolling or resizing)
    pub fn refresh_plugin_viewer(&mut self) {
        if let Mode::ViewingPlugin { plugin_name, path, scroll, .. } = &self.mode {
            let plugin_name = plugin_name.clone();
            let path = path.clone();
            let scroll = *scroll;

            // Find the plugin again and re-render
            if let Some(plugin) = self.plugins.find_viewer(&path)
                && plugin.info().name == plugin_name {
                    let context = ViewerContext {
                        path: path.clone(),
                        width: self.ui.terminal_width as usize,
                        height: self.ui.viewer_height,
                        scroll,
                    };

                    if let Some(result) = plugin.render(&context) {
                        self.mode = Mode::ViewingPlugin {
                            plugin_name,
                            path,
                            scroll,
                            lines: result.lines,
                            total_lines: result.total_lines,
                        };
                    }
                }
        }
    }

    /// Get status context for plugins
    pub fn get_status_context(&self) -> StatusContext {
        let panel = self.active_panel();
        let entry = panel.selected();

        StatusContext {
            path: panel.path.clone(),
            selected_file: entry.map(|e| e.name.clone()),
            selected_path: entry.map(|e| e.path.clone()),
            is_dir: entry.map(|e| e.is_dir).unwrap_or(false),
            file_size: entry.map(|e| e.size).unwrap_or(0),
            selected_count: panel.selected_count(),
        }
    }

    /// Get plugin status bar outputs
    pub fn get_plugin_status(&self) -> Vec<(String, String)> {
        let context = self.get_status_context();
        self.plugins.render_status(&context)
    }

    /// Get list of viewer plugins and whether they can handle the given file
    pub fn get_viewer_plugins_for_file(&self, path: &std::path::Path) -> Vec<(String, bool)> {
        self.plugins.list_viewer_plugins(path)
    }

    /// Show the viewer plugin menu
    pub fn show_viewer_plugin_menu(&mut self) {
        // Only works in Viewing mode
        let Mode::Viewing { content, scroll, path, binary_mode, .. } = &self.mode else {
            return;
        };

        let path = path.clone();
        let content = content.clone();
        let binary_mode = *binary_mode;
        let original_scroll = *scroll;

        // Get available plugins
        let plugins = self.get_viewer_plugins_for_file(&path);

        self.mode = Mode::ViewerPluginMenu {
            path,
            content,
            binary_mode,
            original_scroll,
            plugins,
            selected: 0, // Start with "Built-in viewer" selected
        };
    }

    /// Select a plugin from the viewer menu
    pub fn select_viewer_plugin(&mut self, index: usize) {
        let Mode::ViewerPluginMenu { path, content, binary_mode, original_scroll, plugins, .. } = &self.mode else {
            return;
        };

        let path = path.clone();
        let content = content.clone();
        let binary_mode = *binary_mode;
        let original_scroll = *original_scroll;
        let plugins = plugins.clone();

        if index == 0 {
            // Built-in viewer
            self.mode = Mode::Viewing {
                content,
                scroll: original_scroll,
                path,
                binary_mode,
                search_matches: Vec::new(),
                current_match: None,
            };
        } else if let Some((plugin_name, can_handle)) = plugins.get(index - 1) {
            if *can_handle {
                // Use this plugin
                let context = ViewerContext {
                    path: path.clone(),
                    width: self.ui.terminal_width as usize,
                    height: self.ui.viewer_height,
                    scroll: 0,
                };

                if let Some(plugin) = self.plugins.find_viewer_by_name(plugin_name)
                    && let Some(result) = plugin.render(&context) {
                        self.mode = Mode::ViewingPlugin {
                            plugin_name: plugin_name.clone(),
                            path,
                            scroll: 0,
                            lines: result.lines,
                            total_lines: result.total_lines,
                        };
                        return;
                    }
            }
            // If plugin failed, return to built-in viewer
            self.mode = Mode::Viewing {
                content,
                scroll: original_scroll,
                path,
                binary_mode,
                search_matches: Vec::new(),
                current_match: None,
            };
        }
    }

    /// Cancel the viewer plugin menu and return to built-in viewer
    pub fn cancel_viewer_plugin_menu(&mut self) {
        let Mode::ViewerPluginMenu { path, content, binary_mode, original_scroll, .. } = &self.mode else {
            return;
        };

        self.mode = Mode::Viewing {
            content: content.clone(),
            scroll: *original_scroll,
            path: path.clone(),
            binary_mode: *binary_mode,
            search_matches: Vec::new(),
            current_match: None,
        };
    }

    /// Show the viewer search dialog (/)
    pub fn show_viewer_search(&mut self) {
        let Mode::Viewing { content, scroll, path, binary_mode, search_matches, current_match } = &self.mode else {
            return;
        };

        self.mode = Mode::ViewerSearch {
            content: content.clone(),
            scroll: *scroll,
            path: path.clone(),
            binary_mode: *binary_mode,
            prev_matches: search_matches.clone(),
            prev_current: *current_match,
            text_input: String::new(),
            text_cursor: 0,
            case_sensitive: false,
            hex_input: String::new(),
            hex_cursor: 0,
            focus: 0,
        };
    }

    /// Execute viewer search
    pub fn execute_viewer_search(&mut self) {
        let Mode::ViewerSearch {
            content, path, binary_mode,
            text_input, case_sensitive, hex_input, ..
        } = &self.mode else {
            return;
        };

        let content = content.clone();
        let path = path.clone();
        let binary_mode = *binary_mode;
        let text_input = text_input.clone();
        let case_sensitive = *case_sensitive;
        let hex_input = hex_input.clone();

        // Get the bytes to search in
        let bytes: &[u8] = match &content {
            ViewContent::Text(text) => text.as_bytes(),
            ViewContent::Binary(data) => data,
            ViewContent::MappedFile { mmap, .. } => mmap,
        };

        let mut matches: Vec<(usize, usize)> = Vec::new();

        // Try hex search first if hex input is provided
        if !hex_input.is_empty() {
            if let Some(hex_bytes) = parse_hex_string(&hex_input)
                && !hex_bytes.is_empty() {
                    // Search for hex pattern
                    let mut pos = 0;
                    while pos + hex_bytes.len() <= bytes.len() {
                        if bytes[pos..].starts_with(&hex_bytes) {
                            matches.push((pos, hex_bytes.len()));
                            pos += 1; // Allow overlapping matches
                        } else {
                            pos += 1;
                        }
                    }
                }
        }
        // Otherwise do text search
        else if !text_input.is_empty() {
            // Convert wildcard pattern to search
            let search_pattern = text_input.trim();
            if !search_pattern.is_empty() {
                // Check if pattern contains wildcards
                if search_pattern.contains('*') {
                    // Build regex from wildcard pattern
                    let regex_str = wildcard_to_regex(search_pattern, case_sensitive);
                    if let Ok(regex) = regex::Regex::new(&regex_str) {
                        for m in regex.find_iter(&String::from_utf8_lossy(bytes)) {
                            matches.push((m.start(), m.len()));
                        }
                    }
                } else {
                    // Simple substring search
                    // IMPORTANT: We must search in the original bytes to get correct offsets.
                    // Converting to lowercase can change byte lengths for Unicode characters.
                    let search_bytes = search_pattern.as_bytes();
                    let search_len = search_bytes.len();

                    if case_sensitive {
                        // Case-sensitive: direct byte comparison
                        let mut pos = 0;
                        while pos + search_len <= bytes.len() {
                            if bytes[pos..].starts_with(search_bytes) {
                                matches.push((pos, search_len));
                                pos += 1;
                            } else {
                                pos += 1;
                            }
                        }
                    } else {
                        // Case-insensitive: compare using ASCII case folding on original bytes
                        let search_lower: Vec<u8> = search_bytes.iter()
                            .map(|b| b.to_ascii_lowercase())
                            .collect();

                        let mut pos = 0;
                        while pos + search_len <= bytes.len() {
                            let matches_here = bytes[pos..pos + search_len]
                                .iter()
                                .zip(search_lower.iter())
                                .all(|(a, b)| a.to_ascii_lowercase() == *b);

                            if matches_here {
                                matches.push((pos, search_len));
                            }
                            pos += 1;
                        }
                    }
                }
            }
        }

        // Calculate scroll position to show first match
        let term_width = self.ui.terminal_width as usize;
        let (scroll, current_match) = if matches.is_empty() {
            (0, None)
        } else {
            let first_match_offset = matches[0].0;
            let scroll = content.byte_offset_to_line(first_match_offset, binary_mode, term_width);
            (scroll, Some(0))
        };

        self.mode = Mode::Viewing {
            content,
            scroll,
            path,
            binary_mode,
            search_matches: matches,
            current_match,
        };
    }

    /// Cancel viewer search and return to viewing
    pub fn cancel_viewer_search(&mut self) {
        let Mode::ViewerSearch {
            content, scroll, path, binary_mode, prev_matches, prev_current, ..
        } = &self.mode else {
            return;
        };

        self.mode = Mode::Viewing {
            content: content.clone(),
            scroll: *scroll,
            path: path.clone(),
            binary_mode: *binary_mode,
            search_matches: prev_matches.clone(),
            current_match: *prev_current,
        };
    }

    /// Navigate to next search match
    pub fn viewer_next_match(&mut self) {
        let term_width = self.ui.terminal_width as usize;

        let Mode::Viewing { search_matches, current_match, scroll, content, binary_mode, .. } = &mut self.mode else {
            return;
        };

        if search_matches.is_empty() {
            return;
        }

        let new_idx = match *current_match {
            Some(idx) => (idx + 1) % search_matches.len(),
            None => 0,
        };

        *current_match = Some(new_idx);
        let match_offset = search_matches[new_idx].0;
        *scroll = content.byte_offset_to_line(match_offset, *binary_mode, term_width);
    }

    /// Navigate to previous search match
    pub fn viewer_prev_match(&mut self) {
        let term_width = self.ui.terminal_width as usize;

        let Mode::Viewing { search_matches, current_match, scroll, content, binary_mode, .. } = &mut self.mode else {
            return;
        };

        if search_matches.is_empty() {
            return;
        }

        let new_idx = match *current_match {
            Some(idx) => {
                if idx == 0 {
                    search_matches.len() - 1
                } else {
                    idx - 1
                }
            }
            None => search_matches.len() - 1,
        };

        *current_match = Some(new_idx);
        let match_offset = search_matches[new_idx].0;
        *scroll = content.byte_offset_to_line(match_offset, *binary_mode, term_width);
    }

    /// Convert byte offset to line number for scrolling
    fn byte_offset_to_line_for_search(&self, offset: usize, content: &ViewContent, binary_mode: BinaryViewMode) -> usize {
        content.byte_offset_to_line(offset, binary_mode, self.ui.terminal_width as usize)
    }
}

// ============================================================================
// BACKGROUND TASKS
// ============================================================================

impl App {
    /// Cancel the current background task
    pub fn cancel_background_task(&mut self) {
        self.background_task = None;
        self.mode = Mode::Normal;
        self.add_shell_output("Connection cancelled".to_string());
    }

    /// Check if a background task has completed and handle the result
    pub fn poll_background_task(&mut self) {
        use super::background::TaskResult;

        let result = self.background_task.as_ref().and_then(|t| t.try_recv());

        if let Some(result) = result {
            self.background_task = None;

            match result {
                TaskResult::ScpConnected { target, provider, initial_path, display_name } => {
                    let panel = match target {
                        Side::Left => &mut self.left_panel,
                        Side::Right => &mut self.right_panel,
                    };
                    panel.set_provider(provider, &initial_path);
                    self.add_shell_output(format!("Connected to {}", display_name));
                    self.mode = Mode::Normal;
                }
                TaskResult::ScpFailed { target, error, prompt_password, connection_string, display_name } => {
                    if prompt_password {
                        // Auth failed - show password prompt
                        self.add_shell_output(format!("Key auth failed for {}, prompting for password...", display_name));
                        self.mode = Mode::ScpPasswordPrompt {
                            target_panel: target,
                            connection_string: connection_string.unwrap_or_default(),
                            display_name,
                            password_input: String::new(),
                            cursor_pos: 0,
                            focus: 0,
                            error: None,
                        };
                    } else {
                        // Show error
                        self.add_shell_output(format!("Connection failed: {}", error));
                        self.mode = Mode::Normal;
                    }
                }
                TaskResult::PluginConnected { target, provider, initial_path, display_name, is_extension_mode, source_path, source_name } => {
                    let panel = match target {
                        Side::Left => &mut self.left_panel,
                        Side::Right => &mut self.right_panel,
                    };
                    if is_extension_mode {
                        // Extension-mode: enter like an archive (preserves parent state for ESC exit)
                        let sp = source_path.unwrap_or_else(|| std::path::PathBuf::from(&initial_path));
                        let sn = source_name.unwrap_or_else(|| display_name.clone());
                        panel.switch_to_extension_provider(provider, &sp, &sn);
                    } else {
                        // Scheme-mode: regular provider switch
                        panel.set_provider(provider, &initial_path);
                        self.add_shell_output(format!("Connected to {}", display_name));
                    }
                    self.mode = Mode::Normal;
                }
                TaskResult::PluginFailed { error, display_name, .. } => {
                    self.add_shell_output(format!("Connection to {} failed: {}", display_name, error));
                    self.mode = Mode::Normal;
                }
                TaskResult::FileOpCompleted(result) => {
                    self.cancel_token = None;
                    // Clear selection after operation
                    self.active_panel_mut().selected.clear();
                    // Refresh both panels and git status
                    self.left_panel.refresh();
                    self.right_panel.refresh();
                    self.refresh_git_status();

                    if !result.errors.is_empty() {
                        self.active_panel_mut().error = Some(format!(
                            "{} {}, {} errors: {}",
                            result.op_name,
                            result.count,
                            result.errors.len(),
                            result.errors.first().unwrap_or(&String::new())
                        ));
                    } else {
                        self.add_shell_output(format!("{} {} file(s)", result.op_name, result.count));
                    }
                    self.mode = Mode::Normal;
                }
            }
        }
    }

    /// Poll file operation progress and update mode state
    pub fn poll_file_operation(&mut self) {
        // Drain progress channel for the latest update
        if let Some(task) = &self.background_task {
            if let Some(progress_rx) = &task.progress_rx {
                let mut latest = None;
                while let Ok(progress) = progress_rx.try_recv() {
                    latest = Some(progress);
                }
                if let Some(progress) = latest {
                    if let Mode::FileOpProgress {
                        bytes_done, bytes_total, current_file, files_done, files_total, ..
                    } = &mut self.mode {
                        *bytes_done = progress.bytes_done;
                        *bytes_total = progress.bytes_total;
                        *current_file = progress.current_file;
                        *files_done = progress.files_done;
                        *files_total = progress.files_total;
                    }
                }
            }
        }

        // Check if the operation has completed
        self.poll_background_task();
    }

    /// Cancel a running file operation
    pub fn cancel_file_operation(&mut self) {
        if let Some(cancel) = &self.cancel_token {
            cancel.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        // The thread will finish and send FileOpCompleted; poll_background_task handles cleanup
    }

    /// Advance the spinner animation frame
    pub fn tick_spinner(&mut self) {
        if let Mode::BackgroundTask { frame, .. } = &mut self.mode {
            *frame = (*frame + 1) % 10;
        }
        if let Mode::FileOpProgress { frame, .. } = &mut self.mode {
            *frame = (*frame + 1) % 10;
        }
    }
}

