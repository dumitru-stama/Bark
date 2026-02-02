//! Panel data structures and logic

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::fs::FileEntry;
use crate::providers::{LocalProvider, PanelProvider};
use crate::errors::AppResult;

/// How files are displayed in a panel
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum ViewMode {
    /// Two columns, name only (classic Norton Commander look)
    #[default]
    Brief,
    /// Single column with full details
    Full,
}

/// Sort field for file listing
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum SortField {
    #[default]
    Name,
    Extension,
    Size,
    Modified,
    Unsorted,
}

/// Sort direction
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum SortDirection {
    #[default]
    Ascending,
    Descending,
}

/// Sort configuration
#[derive(Clone, Copy, Debug)]
pub struct SortConfig {
    pub field: SortField,
    pub direction: SortDirection,
    pub dirs_first: bool,
    /// Sort uppercase-first names before lowercase-first names (within dirs/files groups)
    pub uppercase_first: bool,
}

impl Default for SortConfig {
    fn default() -> Self {
        Self {
            field: SortField::Name,
            direction: SortDirection::Ascending,
            dirs_first: true,
            uppercase_first: true,
        }
    }
}

/// Saved panel state for restoring from temp mode
#[derive(Debug, Clone)]
pub struct SavedPanelState {
    pub path: PathBuf,
    pub cursor: usize,
    pub scroll_offset: usize,
}

/// Info needed to return to a previous provider (e.g., when exiting an archive)
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ParentProviderInfo {
    /// Path in the parent provider where we entered from
    pub path: PathBuf,
    /// Name of the archive file (to position cursor on it)
    pub entry_name: String,
    /// Whether the parent was local
    pub was_local: bool,
    /// Cursor position when we entered the archive
    pub cursor: usize,
    /// Scroll offset when we entered the archive
    pub scroll_offset: usize,
}

/// A single file panel
pub struct Panel {
    /// Current directory path (local path or remote path string)
    pub path: PathBuf,
    /// Raw entries from filesystem
    pub entries: Vec<FileEntry>,
    /// Indices into entries, in sorted display order
    pub sorted_indices: Vec<usize>,
    /// Cursor position (index into sorted_indices)
    pub cursor: usize,
    /// Scroll offset for display
    pub scroll_offset: usize,
    /// View mode (Brief or Full)
    pub view_mode: ViewMode,
    /// Sort configuration
    pub sort_config: SortConfig,
    /// Error message if directory couldn't be read
    pub error: Option<String>,
    /// Last known visible height (rows available for file listing)
    /// Updated during rendering
    pub visible_height: usize,
    /// Selected file paths
    pub selected: HashSet<PathBuf>,
    /// Show hidden files (starting with .)
    pub show_hidden: bool,
    /// Show directory prefix (/ or \) before folder names
    pub show_dir_prefix: bool,
    /// Whether this panel is in temporary mode (showing search results, etc.)
    pub temp_mode: bool,
    /// Saved state to restore when exiting temp mode
    pub saved_state: Option<SavedPanelState>,
    /// Filesystem provider (local or remote)
    provider: Box<dyn PanelProvider>,
    /// Info about the parent provider (set when entering an archive)
    parent_provider: Option<ParentProviderInfo>,
}

impl std::fmt::Debug for Panel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Panel")
            .field("path", &self.path)
            .field("entries", &self.entries.len())
            .field("cursor", &self.cursor)
            .field("is_remote", &self.is_remote())
            .finish()
    }
}

#[allow(dead_code)]
impl Panel {
    /// Create a new panel for the given directory
    pub fn new(path: PathBuf) -> Self {
        let mut panel = Self {
            path: path.clone(),
            entries: Vec::new(),
            sorted_indices: Vec::new(),
            cursor: 0,
            scroll_offset: 0,
            view_mode: ViewMode::default(),
            sort_config: SortConfig::default(),
            error: None,
            visible_height: 20, // Will be updated during first render
            selected: HashSet::new(),
            show_hidden: true,
            show_dir_prefix: false,
            temp_mode: false,
            saved_state: None,
            provider: Box::new(LocalProvider::new()),
            parent_provider: None,
        };
        panel.refresh();
        panel
    }

    /// Check if currently inside an archive
    pub fn is_in_archive(&self) -> bool {
        self.parent_provider.is_some()
    }

    /// Get archive source path and name (if inside an archive)
    pub fn archive_source(&self) -> Option<(std::path::PathBuf, String)> {
        self.parent_provider.as_ref().map(|info| {
            let full_path = info.path.join(&info.entry_name);
            (full_path, info.entry_name.clone())
        })
    }

    /// Temporarily extract the provider, replacing it with a dummy LocalProvider.
    /// Use `restore_provider()` to put the real provider back.
    pub fn take_provider(&mut self) -> Box<dyn PanelProvider> {
        std::mem::replace(&mut self.provider, Box::new(LocalProvider::new()))
    }

    /// Restore a previously taken provider.
    pub fn restore_provider(&mut self, provider: Box<dyn PanelProvider>) {
        self.provider = provider;
    }

    /// Check if this panel is browsing a remote filesystem
    pub fn is_remote(&self) -> bool {
        !self.provider.is_local()
    }

    /// Check if this panel is browsing a local filesystem
    pub fn is_local(&self) -> bool {
        self.provider.is_local()
    }

    /// Set a provider for this panel (local or remote)
    pub fn set_provider(&mut self, provider: Box<dyn PanelProvider>, initial_path: &str) {
        self.provider.disconnect();
        self.provider = provider;
        self.path = PathBuf::from(initial_path);
        self.refresh();
    }

    /// Switch back to local filesystem
    pub fn set_local_provider(&mut self, path: PathBuf) {
        self.provider.disconnect();
        self.provider = Box::new(LocalProvider::new());
        self.path = path;
        self.refresh();
    }

    /// Get provider info for display
    pub fn provider_name(&self) -> String {
        self.provider.info().name.clone()
    }

    /// Get short label for panel header (e.g., "[ZIP]" for archives)
    pub fn provider_short_label(&self) -> Option<String> {
        self.provider.short_label()
    }

    /// Write a file via the provider
    pub fn write_file(&mut self, path: &str, data: &[u8]) -> AppResult<()> {
        Ok(self.provider.write_file(path, data)?)
    }

    /// Set file attributes (modification time, permissions) via the provider
    pub fn set_attributes(&mut self, path: &str, modified: Option<std::time::SystemTime>, permissions: u32) -> AppResult<()> {
        Ok(self.provider.set_attributes(path, modified, permissions)?)
    }

    /// Delete a file/directory via the provider
    pub fn delete_path(&mut self, path: &str, recursive: bool) -> AppResult<()> {
        if recursive {
            Ok(self.provider.delete_recursive(path)?)
        } else {
            Ok(self.provider.delete(path)?)
        }
    }

    /// Rename/move a file or directory via the provider
    pub fn rename_path(&mut self, from: &str, to: &str) -> AppResult<()> {
        Ok(self.provider.rename(from, to)?)
    }

    /// Create a directory via the provider
    pub fn mkdir(&mut self, path: &str) -> AppResult<()> {
        Ok(self.provider.mkdir(path)?)
    }

    /// Read a file via the provider
    pub fn read_file(&mut self, path: &str) -> AppResult<Vec<u8>> {
        Ok(self.provider.read_file(path)?)
    }

    /// Refresh directory contents
    pub fn refresh(&mut self) {
        // Don't refresh if in temp mode - entries are virtual
        if self.temp_mode {
            return;
        }

        // Use provider for all filesystem operations
        let path_str = self.path.to_string_lossy().to_string();
        let result = self.provider.list_directory(&path_str);

        match result {
            Ok(entries) => {
                // Filter hidden files if show_hidden is false
                self.entries = if self.show_hidden {
                    entries
                } else {
                    entries.into_iter()
                        .filter(|e| e.name == ".." || !e.name.starts_with('.'))
                        .collect()
                };
                self.error = None;
                self.resort();
                // Keep cursor in bounds
                if self.cursor >= self.sorted_indices.len() {
                    self.cursor = self.sorted_indices.len().saturating_sub(1);
                }
            }
            Err(e) => {
                self.error = Some(e.to_string());
                self.entries.clear();
                self.sorted_indices.clear();
                self.cursor = 0;
            }
        }
    }

    /// Toggle show hidden files
    pub fn toggle_hidden(&mut self) {
        self.show_hidden = !self.show_hidden;
        self.refresh();
    }

    /// Enter temp mode with the given file paths
    pub fn enter_temp_mode(&mut self, paths: Vec<PathBuf>) {
        if self.temp_mode {
            // Already in temp mode, just add files
            self.add_to_temp(paths);
            return;
        }

        // Save current state
        self.saved_state = Some(SavedPanelState {
            path: self.path.clone(),
            cursor: self.cursor,
            scroll_offset: self.scroll_offset,
        });

        // Switch to temp mode
        self.temp_mode = true;
        self.path = PathBuf::from("[TEMP]");
        self.entries.clear();
        self.sorted_indices.clear();
        self.selected.clear();
        self.cursor = 0;
        self.scroll_offset = 0;
        self.error = None;

        // Add files to temp panel
        self.add_to_temp(paths);
    }

    /// Add files to temp panel
    fn add_to_temp(&mut self, paths: Vec<PathBuf>) {
        for path in paths {
            // Create FileEntry for each path using the existing from_path method
            if let Ok(entry) = FileEntry::from_path(&path) {
                // Check if entry already exists
                if !self.entries.iter().any(|e| e.path == entry.path) {
                    self.entries.push(entry);
                }
            }
        }

        self.resort();
        if self.cursor >= self.sorted_indices.len() {
            self.cursor = self.sorted_indices.len().saturating_sub(1);
        }
    }

    /// Exit temp mode and restore previous state
    pub fn exit_temp_mode(&mut self) {
        if !self.temp_mode {
            return;
        }

        self.temp_mode = false;

        // Clear temp panel contents
        self.entries.clear();
        self.sorted_indices.clear();
        self.selected.clear();

        if let Some(saved) = self.saved_state.take() {
            self.path = saved.path;
            self.cursor = saved.cursor;
            self.scroll_offset = saved.scroll_offset;
            self.refresh();
        }
    }

    /// Check if panel is in temp mode
    pub fn is_temp_mode(&self) -> bool {
        self.temp_mode
    }

    /// Remove the current entry from temp panel (doesn't delete from disk)
    /// Returns true if an entry was removed
    pub fn remove_current_from_temp(&mut self) -> bool {
        if !self.temp_mode {
            return false;
        }

        let Some(entry) = self.selected() else {
            return false;
        };

        let path_to_remove = entry.path.clone();

        // Find and remove the entry
        if let Some(pos) = self.entries.iter().position(|e| e.path == path_to_remove) {
            self.entries.remove(pos);
            self.resort();

            // Adjust cursor if needed
            if self.cursor >= self.sorted_indices.len() {
                self.cursor = self.sorted_indices.len().saturating_sub(1);
            }

            // If no entries left, exit temp mode
            if self.entries.is_empty() {
                self.exit_temp_mode();
            }

            return true;
        }

        false
    }

    /// Classify a name's first character into a sort tier:
    /// 0 = dot-prefixed, 1 = uppercase-first, 2 = lowercase-first.
    fn name_tier(name: &str) -> u8 {
        match name.chars().next() {
            Some('.') => 0,
            Some(c) if c.is_uppercase() => 1,
            _ => 2,
        }
    }

    /// Compare two names by first-character tier: dot < uppercase < lowercase.
    /// Returns Equal if both are in the same tier.
    fn uppercase_first_cmp(a: &str, b: &str) -> std::cmp::Ordering {
        Self::name_tier(a).cmp(&Self::name_tier(b))
    }

    /// Re-sort entries based on current sort configuration
    pub fn resort(&mut self) {
        // Build indices
        self.sorted_indices = (0..self.entries.len()).collect();

        // Sort indices based on entries
        let entries = &self.entries;
        let config = &self.sort_config;

        self.sorted_indices.sort_by(|&a, &b| {
            let ea = &entries[a];
            let eb = &entries[b];

            // ".." always comes first
            if ea.name == ".." {
                return std::cmp::Ordering::Less;
            }
            if eb.name == ".." {
                return std::cmp::Ordering::Greater;
            }

            // Directories first (if enabled)
            if config.dirs_first && ea.is_dir != eb.is_dir {
                return if ea.is_dir {
                    std::cmp::Ordering::Less
                } else {
                    std::cmp::Ordering::Greater
                };
            }

            // Sort by field
            let cmp = match config.field {
                SortField::Name => {
                    let base = ea.name.to_lowercase().cmp(&eb.name.to_lowercase());
                    if config.uppercase_first {
                        Self::uppercase_first_cmp(&ea.name, &eb.name).then(base)
                    } else {
                        base
                    }
                }
                SortField::Extension => {
                    let ext_a = ea.extension().unwrap_or("");
                    let ext_b = eb.extension().unwrap_or("");
                    let ext_cmp = ext_a.to_lowercase().cmp(&ext_b.to_lowercase());
                    let name_cmp = ea.name.to_lowercase().cmp(&eb.name.to_lowercase());
                    if config.uppercase_first {
                        ext_cmp
                            .then(Self::uppercase_first_cmp(ext_a, ext_b))
                            .then(name_cmp)
                            .then(Self::uppercase_first_cmp(&ea.name, &eb.name))
                    } else {
                        ext_cmp.then(name_cmp)
                    }
                }
                SortField::Size => ea.size.cmp(&eb.size),
                SortField::Modified => ea.modified.cmp(&eb.modified),
                SortField::Unsorted => std::cmp::Ordering::Equal,
            };

            // Apply direction
            match config.direction {
                SortDirection::Ascending => cmp,
                SortDirection::Descending => cmp.reverse(),
            }
        });
    }

    /// Get the currently selected entry
    pub fn selected(&self) -> Option<&FileEntry> {
        self.sorted_indices
            .get(self.cursor)
            .and_then(|&idx| self.entries.get(idx))
    }

    /// Get entry at a given display index
    pub fn entry_at(&self, display_index: usize) -> Option<&FileEntry> {
        self.sorted_indices
            .get(display_index)
            .and_then(|&idx| self.entries.get(idx))
    }

    /// Total number of entries (including ..)
    pub fn entry_count(&self) -> usize {
        self.sorted_indices.len()
    }

    /// Count of directories (excluding ..)
    pub fn dir_count(&self) -> usize {
        self.entries
            .iter()
            .filter(|e| e.is_dir && e.name != "..")
            .count()
    }

    /// Count of files
    pub fn file_count(&self) -> usize {
        self.entries.iter().filter(|e| !e.is_dir).count()
    }

    /// Total size of all files
    pub fn total_size(&self) -> u64 {
        self.entries.iter().map(|e| e.size).sum()
    }

    /// Number of items visible at once based on view mode
    fn visible_items(&self) -> usize {
        match self.view_mode {
            ViewMode::Brief => self.visible_height * 2, // Two columns
            ViewMode::Full => self.visible_height,
        }
    }

    /// Ensure scroll offset keeps cursor visible
    fn adjust_scroll(&mut self) {
        let visible = self.visible_items();
        if visible == 0 {
            return;
        }

        // If cursor is before visible area, scroll up
        if self.cursor < self.scroll_offset {
            self.scroll_offset = self.cursor;
        }
        // If cursor is after visible area, scroll down
        else if self.cursor >= self.scroll_offset + visible {
            self.scroll_offset = self.cursor - visible + 1;
        }
    }

    /// Move cursor up by one
    pub fn move_up(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
            self.adjust_scroll();
        }
    }

    /// Move cursor down by one
    pub fn move_down(&mut self) {
        if self.cursor + 1 < self.entry_count() {
            self.cursor += 1;
            self.adjust_scroll();
        }
    }

    /// Move cursor left (Brief mode: same row, left column)
    /// If already in left column, page up
    pub fn move_left(&mut self) {
        match self.view_mode {
            ViewMode::Brief => {
                let col_height = self.visible_height;
                if col_height == 0 {
                    return;
                }
                // Calculate which column and row we're in (relative to scroll)
                let visible_pos = self.cursor.saturating_sub(self.scroll_offset);
                let in_right_column = visible_pos >= col_height;

                if in_right_column {
                    // Move to same row in left column
                    self.cursor -= col_height;
                } else {
                    // Already in left column, page up (move by 2 columns worth)
                    let page_size = col_height * 2;
                    self.cursor = self.cursor.saturating_sub(page_size);
                }
                self.adjust_scroll();
            }
            ViewMode::Full => {
                // In Full mode, left does nothing (single column)
            }
        }
    }

    /// Move cursor right (Brief mode: same row, right column)
    /// If already in right column, page down
    pub fn move_right(&mut self) {
        match self.view_mode {
            ViewMode::Brief => {
                let col_height = self.visible_height;
                let count = self.entry_count();
                if col_height == 0 || count == 0 {
                    return;
                }
                // Calculate which column we're in (relative to scroll)
                let visible_pos = self.cursor.saturating_sub(self.scroll_offset);
                let in_left_column = visible_pos < col_height;

                if in_left_column && self.cursor + col_height < count {
                    // Move to same row in right column
                    self.cursor += col_height;
                } else {
                    // Already in right column, page down (move by 2 columns worth)
                    let page_size = col_height * 2;
                    self.cursor = (self.cursor + page_size).min(count - 1);
                }
                self.adjust_scroll();
            }
            ViewMode::Full => {
                // In Full mode, right does nothing (single column)
            }
        }
    }

    /// Move cursor up by one page
    pub fn page_up(&mut self) {
        let page_size = self.visible_items().max(1);
        self.cursor = self.cursor.saturating_sub(page_size);
        self.adjust_scroll();
    }

    /// Move cursor down by one page
    pub fn page_down(&mut self) {
        let page_size = self.visible_items().max(1);
        let count = self.entry_count();
        self.cursor = (self.cursor + page_size).min(count.saturating_sub(1));
        self.adjust_scroll();
    }

    /// Move cursor to first entry
    pub fn move_home(&mut self) {
        self.cursor = 0;
        self.adjust_scroll();
    }

    /// Move cursor to last entry
    pub fn move_end(&mut self) {
        let count = self.entry_count();
        if count > 0 {
            self.cursor = count - 1;
        }
        self.adjust_scroll();
    }

    /// Jump to first entry whose name starts with prefix (case-insensitive)
    /// Returns true if a match was found
    pub fn jump_to_prefix(&mut self, prefix: &str) -> bool {
        if prefix.is_empty() {
            return false;
        }
        let prefix_lower = prefix.to_lowercase();
        // Iterate through sorted/displayed order
        for (display_idx, &entry_idx) in self.sorted_indices.iter().enumerate() {
            if let Some(entry) = self.entries.get(entry_idx)
                && entry.name.to_lowercase().starts_with(&prefix_lower) {
                    self.cursor = display_idx;
                    self.adjust_scroll();
                    return true;
                }
        }
        false
    }

    /// Set view mode
    pub fn set_view_mode(&mut self, mode: ViewMode) {
        if self.view_mode != mode {
            self.view_mode = mode;
            // Reset scroll when changing view mode
            self.scroll_offset = 0;
            self.adjust_scroll();
        }
    }

    /// Set sort field, toggling direction if same field
    pub fn set_sort(&mut self, field: SortField) {
        if self.sort_config.field == field {
            // Toggle direction
            self.sort_config.direction = match self.sort_config.direction {
                SortDirection::Ascending => SortDirection::Descending,
                SortDirection::Descending => SortDirection::Ascending,
            };
        } else {
            // New field, default to ascending
            self.sort_config.field = field;
            self.sort_config.direction = SortDirection::Ascending;
        }

        // Remember current selection
        let selected_path = self.selected().map(|e| e.path.clone());

        // Re-sort
        self.resort();

        // Try to restore cursor to same entry
        if let Some(path) = selected_path {
            for (i, &idx) in self.sorted_indices.iter().enumerate() {
                if self.entries[idx].path == path {
                    self.cursor = i;
                    break;
                }
            }
        }

        self.adjust_scroll();
    }

    /// Enter the selected directory
    /// Returns true if directory was entered, false otherwise
    /// Note: Archives are handled separately via background task in normal.rs
    pub fn enter_selected(&mut self) -> bool {
        let Some(entry) = self.selected() else {
            return false;
        };

        if !entry.is_dir {
            return false;
        }

        // If in temp mode, exit temp mode and navigate to the directory
        if self.temp_mode {
            let new_path = entry.path.clone();
            self.temp_mode = false;
            self.saved_state = None;
            return self.change_directory(new_path);
        }

        // If selecting "..", use go_parent() to get cursor positioning
        if entry.name == ".." {
            return self.go_parent();
        }

        let new_path = entry.path.clone();
        self.change_directory(new_path)
    }

    /// Switch to an extension-mode provider (e.g., archive plugin)
    /// Called from background task completion when a plugin handles a file extension
    pub fn switch_to_extension_provider(
        &mut self,
        provider: Box<dyn PanelProvider>,
        source_path: &Path,
        source_name: &str,
    ) {
        // Save info about where we came from
        // The parent path is the directory containing the source file
        let parent_path = source_path.parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| self.path.clone());

        self.parent_provider = Some(ParentProviderInfo {
            path: parent_path,
            entry_name: source_name.to_string(),
            was_local: self.provider.is_local(),
            cursor: self.cursor,
            scroll_offset: self.scroll_offset,
        });

        // Switch to extension provider
        self.provider.disconnect();
        self.provider = provider;
        self.path = PathBuf::from("/");
        self.cursor = 0;
        self.scroll_offset = 0;
        self.selected.clear();
        self.refresh();
    }

    /// Reconnect an extension-mode provider (e.g., after entering password for encrypted archive)
    /// Replaces the current provider but preserves parent_provider info so ESC still works.
    pub fn set_provider_password(&mut self, password: &str) -> crate::providers::ProviderResult<()> {
        self.provider.set_password(password)
    }

    pub fn reconnect_extension_provider(&mut self, provider: Box<dyn PanelProvider>) {
        self.provider.disconnect();
        self.provider = provider;
        self.path = PathBuf::from("/");
        self.cursor = 0;
        self.scroll_offset = 0;
        self.selected.clear();
        self.refresh();
    }

    /// Go to parent directory
    /// Returns true if successful, false if already at root
    pub fn go_parent(&mut self) -> bool {
        // Check if we're at the root of an archive - if so, exit the archive
        let path_str = self.path.to_string_lossy();
        let at_archive_root = path_str == "/" || path_str.is_empty();

        if at_archive_root && self.parent_provider.is_some() {
            return self.exit_archive();
        }

        let Some(parent) = self.path.parent() else {
            // If we're truly at root and in an archive, exit it
            if self.parent_provider.is_some() {
                return self.exit_archive();
            }
            return false;
        };

        // For archive providers, also check if parent would be empty/root
        let parent_str = parent.to_string_lossy();
        if (parent_str.is_empty() || parent_str == "/") && self.parent_provider.is_some() {
            if self.path.to_string_lossy() == "/" {
                // Already at root, exit archive
                return self.exit_archive();
            }
            // Going to root of archive - fall through to normal navigation
            // so cursor-restore logic below handles it
        }

        // Remember current directory name to position cursor on it after going up
        let current_name = self.path.file_name()
            .map(|s| s.to_string_lossy().into_owned());

        let parent_path = if self.parent_provider.is_some()
            && (parent_str.is_empty() || parent_str == "/")
        {
            PathBuf::from("/")
        } else {
            parent.to_path_buf()
        };

        if self.change_directory(parent_path) {
            // Try to position cursor on the directory we just left
            if let Some(name) = current_name {
                for (i, &idx) in self.sorted_indices.iter().enumerate() {
                    if self.entries[idx].name == name {
                        self.cursor = i;
                        self.adjust_scroll();
                        break;
                    }
                }
            }
            true
        } else {
            false
        }
    }

    /// Exit from an archive back to the parent provider
    pub fn exit_archive(&mut self) -> bool {
        let Some(parent_info) = self.parent_provider.take() else {
            return false;
        };

        // Switch back to local provider
        self.provider.disconnect();
        self.provider = Box::new(LocalProvider::new());
        self.path = parent_info.path;
        self.selected.clear();
        self.refresh();

        // Restore cursor and scroll position to exactly where we were
        self.cursor = parent_info.cursor.min(self.sorted_indices.len().saturating_sub(1));
        self.scroll_offset = parent_info.scroll_offset;
        self.adjust_scroll();

        true
    }

    /// Change to a new directory
    /// Returns true if successful
    pub fn change_directory(&mut self, new_path: PathBuf) -> bool {
        // Use provider for all filesystem operations
        let path_str = new_path.to_string_lossy().to_string();
        let result = self.provider.list_directory(&path_str);

        match result {
            Ok(entries) => {
                self.path = new_path;
                // Filter hidden files if show_hidden is false
                self.entries = if self.show_hidden {
                    entries
                } else {
                    entries.into_iter()
                        .filter(|e| e.name == ".." || !e.name.starts_with('.'))
                        .collect()
                };
                self.error = None;
                self.selected.clear();  // Clear selection when changing directory
                self.resort();
                self.cursor = 0;
                self.scroll_offset = 0;
                true
            }
            Err(e) => {
                self.error = Some(format!(
                    "Cannot enter '{}': {}",
                    new_path.to_string_lossy(),
                    e
                ));
                false
            }
        }
    }

    /// Clear any error message
    pub fn clear_error(&mut self) {
        self.error = None;
    }

    /// Toggle selection of the current entry and move to the next
    pub fn toggle_select(&mut self) {
        if let Some(entry) = self.selected() {
            // Don't select ".."
            if entry.name == ".." {
                self.move_down();
                return;
            }

            let path = entry.path.clone();
            if self.selected.contains(&path) {
                self.selected.remove(&path);
            } else {
                self.selected.insert(path);
            }
        }
        // Move to next entry
        self.move_down();
    }

    /// Check if a path is selected
    pub fn is_selected(&self, path: &PathBuf) -> bool {
        self.selected.contains(path)
    }

    /// Get all selected entries (or current entry if none selected)
    pub fn get_selected_entries(&self) -> Vec<&FileEntry> {
        if self.selected.is_empty() {
            // If nothing selected, return current entry (if it's not "..")
            if let Some(entry) = self.selected()
                && entry.name != ".." {
                    return vec![entry];
                }
            vec![]
        } else {
            self.entries
                .iter()
                .filter(|e| self.selected.contains(&e.path))
                .collect()
        }
    }

    /// Clear all selections
    pub fn clear_selection(&mut self) {
        self.selected.clear();
    }

    /// Count of selected files
    pub fn selected_count(&self) -> usize {
        self.selected.len()
    }

    /// Total size of selected files
    pub fn selected_size(&self) -> u64 {
        self.entries
            .iter()
            .filter(|e| self.selected.contains(&e.path))
            .map(|e| e.size)
            .sum()
    }
}
