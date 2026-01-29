//! Panel manager (handles dual-pane logic)

use std::path::PathBuf;

use crate::git::{self, GitStatus};
use crate::panel::{Panel, Side, SortConfig, SortDirection, SortField, ViewMode};
use crate::config::Config;

/// Manages the two file panels and their interaction
pub struct PanelManager {
    pub left_panel: Panel,
    pub right_panel: Panel,
    pub active_side: Side,
    
    /// Git status for left panel's directory
    pub left_git_status: Option<GitStatus>,
    /// Git status for right panel's directory
    pub right_git_status: Option<GitStatus>,
    
    /// Path for which left git status was computed
    left_git_path: Option<PathBuf>,
    /// Path for which right git status was computed
    right_git_path: Option<PathBuf>,
}

impl PanelManager {
    pub fn new(config: &Config) -> Self {
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

        Self {
            left_panel,
            right_panel,
            active_side: Side::Left,
            left_git_status: left_git,
            right_git_status: right_git,
            left_git_path: Some(left_path),
            right_git_path: Some(right_path),
        }
    }

    /// Get a reference to the active panel
    pub fn active_panel(&self) -> &Panel {
        match self.active_side {
            Side::Left => &self.left_panel,
            Side::Right => &self.right_panel,
        }
    }

    /// Get a mutable reference to the active panel
    pub fn active_panel_mut(&mut self) -> &mut Panel {
        match self.active_side {
            Side::Left => &mut self.left_panel,
            Side::Right => &mut self.right_panel,
        }
    }

    /// Get a reference to the inactive panel
    pub fn inactive_panel(&self) -> &Panel {
        match self.active_side {
            Side::Left => &self.right_panel,
            Side::Right => &self.left_panel,
        }
    }

    /// Get a mutable reference to the inactive panel
    pub fn inactive_panel_mut(&mut self) -> &mut Panel {
        match self.active_side {
            Side::Left => &mut self.right_panel,
            Side::Right => &mut self.left_panel,
        }
    }

    /// Toggle active panel
    pub fn toggle_panel(&mut self) {
        self.active_side = match self.active_side {
            Side::Left => Side::Right,
            Side::Right => Side::Left,
        };
    }

    /// Refresh all panels (re-read directory contents)
    pub fn refresh_panels(&mut self) {
        self.left_panel.refresh();
        self.right_panel.refresh();
    }

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

        // Add to the inactive panel's temp mode
        self.inactive_panel_mut().enter_temp_mode(paths);

        // Clear selection after adding to temp
        self.active_panel_mut().selected.clear();
    }
}
