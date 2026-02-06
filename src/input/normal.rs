//! Normal mode and command input handling

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::state::app::App;
use crate::state::mode::Mode;
use crate::state::panel::{SortField, ViewMode};

pub fn handle_normal_mode(app: &mut App, key: KeyEvent) {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let shift = key.modifiers.contains(KeyModifiers::SHIFT);
    let alt = key.modifiers.contains(KeyModifiers::ALT);

    // Shell area scrolling with Ctrl+Arrow/Page or Alt+Arrow/Page
    // (Alt variants for macOS where Ctrl+Up triggers Mission Control)
    if (ctrl || alt) && !app.cmd.focused {
        match key.code {
            KeyCode::Up => {
                app.cmd.scroll_up(1);
                return;
            }
            KeyCode::Down => {
                app.cmd.scroll_down(1);
                return;
            }
            KeyCode::PageUp => {
                app.cmd.scroll_up(10);
                return;
            }
            KeyCode::PageDown => {
                app.cmd.scroll_down(10);
                return;
            }
            _ => {}
        }
    }

    // Resizing with Shift+Arrow keys (not configurable - special UI function)
    if shift {
        match key.code {
            KeyCode::Up => {
                app.grow_shell();
                return;
            }
            KeyCode::Down => {
                app.shrink_shell();
                return;
            }
            KeyCode::Left => {
                app.shrink_left_panel();
                return;
            }
            KeyCode::Right => {
                app.grow_left_panel();
                return;
            }
            _ => {}
        }
    }

    // If in command mode, handle command input
    if app.cmd.focused {
        handle_command_input(app, key);
        return;
    }

    // Quick search mode
    if let Some(mut search) = app.quick_search.take() {
        let (should_keep, fall_through) = match key.code {
            KeyCode::Esc => {
                // Cancel search
                (false, false)
            }
            KeyCode::Enter => {
                // Confirm search and execute Enter on selected file
                (false, true)
            }
            KeyCode::Backspace => {
                // Remove last character
                search.pop();
                if !search.is_empty() {
                    app.active_panel_mut().jump_to_prefix(&search);
                    (true, false)
                } else {
                    (false, false)
                }
            }
            KeyCode::Char(c) if !ctrl && !alt => {
                // Add character to search and jump
                search.push(c);
                app.active_panel_mut().jump_to_prefix(&search);
                (true, false)
            }
            _ => {
                // Any other key exits search mode and is processed normally
                (false, true)
            }
        };
        if should_keep {
            app.quick_search = Some(search);
            return;
        }
        if !fall_through {
            return;
        }
        // Fall through to process the key normally (Enter, or other keys)
    }

    // Check configurable keybindings first
    // Application
    if app.key_matches("quit", &key) {
        app.should_quit = true;
        return;
    }
    if app.key_matches("quit_alt", &key) {
        app.should_quit = true;
        return;
    }
    if app.key_matches("shell_toggle", &key) {
        if app.config.general.shell_history_mode
            || crate::persistent_shell::is_windows_10_or_older()
        {
            app.mode = Mode::ShellHistoryView { scroll: 0 };
        } else {
            app.mode = Mode::ShellVisible;
        }
        return;
    }

    // Source selectors (drives/quick access/connections) (check before help since Alt+F1 vs F1)
    // Support Alt+F1/F2, Ctrl+F1/F2, Shift+F1/F2, and Alt+1/2
    if app.key_matches("drive_left", &key) || app.key_matches("source_left", &key)
        || app.key_matches("source_left_shift", &key) || app.key_matches("source_left_alt", &key) {
        app.show_source_selector(crate::state::Side::Left);
        return;
    }
    if app.key_matches("drive_right", &key) || app.key_matches("source_right", &key)
        || app.key_matches("source_right_shift", &key) || app.key_matches("source_right_alt", &key) {
        app.show_source_selector(crate::state::Side::Right);
        return;
    }

    // ESC - exit archive if inside one
    if matches!(key.code, KeyCode::Esc) && app.active_panel().is_in_archive() {
        app.active_panel_mut().exit_archive();
        return;
    }

    // Help (F1)
    if app.key_matches("help", &key) {
        app.mode = Mode::Help { scroll: 0 };
        return;
    }

    // User Menu (F2)
    if app.key_matches("user_menu", &key) {
        app.show_user_menu();
        return;
    }

    // File operations
    if app.key_matches("view", &key) {
        if let Some(entry) = app.active_panel().selected() {
            let path = entry.path.clone();
            if entry.is_dir {
                app.compute_dir_size(&path);
            } else if app.config.general.view_plugin_first {
                app.view_file_with_plugins(&path);
            } else {
                app.view_file(&path);
            }
        }
        return;
    }
    if app.key_matches("edit", &key) {
        let path = app.active_panel().selected()
            .filter(|e| !e.is_dir)
            .map(|e| e.path.clone());
        if let Some(path) = path {
            app.edit_file(&path);
        }
        return;
    }
    if app.key_matches("copy", &key) {
        app.copy_selected();
        return;
    }
    // Move is disabled inside archives (read-only)
    if app.key_matches("move", &key) && !app.active_panel().is_in_archive() {
        app.move_selected();
        return;
    }
    // Mkdir is disabled inside archives (read-only)
    if app.key_matches("mkdir", &key) && !app.active_panel().is_in_archive() {
        app.show_mkdir_dialog();
        return;
    }
    // Delete is disabled inside archives (read-only)
    if app.key_matches("delete", &key) && !app.active_panel().is_in_archive() {
        app.delete_selected();
        return;
    }

    // Sorting (Ctrl+F-keys)
    if app.key_matches("sort_name_f", &key) {
        app.active_panel_mut().set_sort(SortField::Name);
        return;
    }
    if app.key_matches("sort_ext_f", &key) {
        app.active_panel_mut().set_sort(SortField::Extension);
        return;
    }
    if app.key_matches("sort_time_f", &key) {
        app.active_panel_mut().set_sort(SortField::Modified);
        return;
    }
    if app.key_matches("sort_size_f", &key) {
        app.active_panel_mut().set_sort(SortField::Size);
        return;
    }
    if app.key_matches("sort_unsorted_f", &key) {
        app.active_panel_mut().set_sort(SortField::Unsorted);
        return;
    }

    // Sorting (Ctrl+letter)
    if app.key_matches("sort_name", &key) {
        app.active_panel_mut().set_sort(SortField::Name);
        return;
    }
    if app.key_matches("sort_extension", &key) {
        app.active_panel_mut().set_sort(SortField::Extension);
        return;
    }
    if app.key_matches("sort_time", &key) {
        app.active_panel_mut().set_sort(SortField::Modified);
        return;
    }
    if app.key_matches("sort_size", &key) {
        app.active_panel_mut().set_sort(SortField::Size);
        return;
    }

    // Toggle hidden files
    if app.key_matches("toggle_hidden", &key) {
        app.active_panel_mut().toggle_hidden();
        return;
    }

    // Toggle view mode
    if app.key_matches("toggle_view_mode", &key) {
        let panel = app.active_panel_mut();
        let new_mode = match panel.view_mode {
            ViewMode::Brief => ViewMode::Full,
            ViewMode::Full => ViewMode::Brief,
        };
        panel.set_view_mode(new_mode);
        return;
    }

    // Refresh all panels
    if app.key_matches("refresh", &key) {
        app.refresh_panels();
        return;
    }

    // Find files
    if app.key_matches("find_files", &key) {
        app.show_find_files_dialog();
        return;
    }

    // Quick search
    if app.key_matches("quick_search", &key) {
        app.quick_search = Some(String::new());
        return;
    }

    // Command history
    if app.key_matches("command_history", &key) || app.key_matches("command_history_alt", &key) {
        app.show_command_history();
        return;
    }

    // Add to temp panel
    if app.key_matches("add_to_temp", &key) {
        app.add_to_temp_panel();
        return;
    }

    // Add current directory to favorites
    if app.key_matches("add_favorite", &key) {
        app.add_current_to_favorites();
        return;
    }

    // Remove from temp panel (only in temp mode)
    if app.key_matches("remove_from_temp", &key) && app.active_panel().is_temp_mode() {
        app.active_panel_mut().remove_current_from_temp();
        return;
    }

    // Selection
    if app.key_matches("select_toggle", &key) {
        let panel = app.active_panel_mut();
        panel.clear_error();
        panel.toggle_select();
        return;
    }
    if app.key_matches("select_pattern", &key) || app.key_matches("select_pattern_alt", &key) {
        app.show_select_files_dialog();
        return;
    }
    #[cfg(not(windows))]
    if app.key_matches("permissions", &key) {
        app.show_permissions_dialog();
        return;
    }
    #[cfg(not(windows))]
    if app.key_matches("chown", &key) {
        app.show_chown_dialog();
        return;
    }
    if app.key_matches("unselect_all", &key) {
        let count = app.active_panel().selected.len();
        if count > 0 {
            app.active_panel_mut().clear_selection();
            app.add_shell_output(format!("Unmarked {} file(s)", count));
        }
        return;
    }

    // Insert into command line
    if app.key_matches("insert_filename", &key) && app.config.general.edit_mode_always {
        if let Some(entry) = app.active_panel().selected()
            && entry.name != ".." {
                let s = shell_escape(&entry.name) + " ";
                app.cmd.insert_str(&s);
            }
        return;
    }
    if app.key_matches("insert_path", &key) && app.config.general.edit_mode_always {
        let path_str = app.active_panel().path.to_string_lossy();
        let s = shell_escape(&path_str) + " ";
        app.cmd.insert_str(&s);
        return;
    }
    if app.key_matches("insert_fullpath", &key) && app.config.general.edit_mode_always {
        if let Some(entry) = app.active_panel().selected()
            && entry.name != ".." {
                let path_str = entry.path.to_string_lossy();
                let s = shell_escape(&path_str) + " ";
                app.cmd.insert_str(&s);
            }
        return;
    }

    // Vim-style page navigation
    if app.key_matches("page_up_vim", &key) {
        let panel = app.active_panel_mut();
        panel.clear_error();
        panel.page_up();
        return;
    }
    if app.key_matches("page_down_vim", &key) {
        let panel = app.active_panel_mut();
        panel.clear_error();
        panel.page_down();
        return;
    }

    // Non-configurable keys (basic UI operations)
    match key.code {
        // Enter command mode with ':' (only needed when edit_mode_always is false)
        KeyCode::Char(':') if !app.config.general.edit_mode_always => {
            app.cmd.focused = true;
        }

        // Panel switching (only when command line is empty) / Tab completion
        KeyCode::Tab => {
            if app.cmd.input.is_empty() {
                app.toggle_panel();
            } else {
                // Built-in command completion
                app.complete_command();
            }
        }

        // Escape exits temp mode or clears command line
        KeyCode::Esc => {
            if app.active_panel().is_temp_mode() {
                app.active_panel_mut().exit_temp_mode();
            } else if app.config.general.edit_mode_always {
                app.cmd.clear_input();
            }
        }

        // Directory traversal / command execution / file opening
        KeyCode::Enter => {
            if !app.cmd.input.is_empty() {
                // Non-empty command line - execute it
                app.cmd.focused = true;
                app.execute_command();
            } else {
                // Get file info and cwd before mutable borrow
                let file_info = app.active_panel().selected().map(|entry| {
                    (entry.path.clone(), entry.name.clone(), entry.is_dir, entry.is_executable())
                });
                let cwd = app.active_panel().path.clone();
                let is_local = app.active_panel().is_local();

                if let Some((path, name, is_dir, is_exec)) = file_info {
                    // Check if a provider plugin handles this file extension
                    if !is_dir && is_local && app.plugins.find_provider_by_extension(&path).is_some() {
                        app.open_extension_provider(path, name);
                        return;
                    }

                    // Try enter_selected - handles directories
                    // Returns true if it entered a directory
                    let entered = app.active_panel_mut().enter_selected();

                    if !entered && !is_dir {
                        // Regular file - check handlers and executables
                        // Check for matching file handler first
                        if let Some(command) = app.config.find_handler(&name, &path) {
                            app.mode = Mode::RunningCommand { command, cwd };
                        } else if is_exec && app.config.general.run_executables {
                            // Run executable
                            let command = shell_escape(&path.to_string_lossy());
                            app.mode = Mode::RunningCommand { command, cwd };
                        }
                        // Otherwise do nothing (use F3 to view, F4 to edit)
                    }
                }
            }
        }
        KeyCode::Backspace => {
            if app.config.general.edit_mode_always {
                // In edit_mode_always, backspace deletes char before cursor
                app.cmd.delete_char_before();
            } else {
                // Go to parent directory
                app.active_panel_mut().go_parent();
            }
        }

        // Arrow key navigation (always works)
        KeyCode::Up => {
            let panel = app.active_panel_mut();
            panel.clear_error();
            panel.move_up();
        }
        KeyCode::Down => {
            let panel = app.active_panel_mut();
            panel.clear_error();
            panel.move_down();
        }
        KeyCode::Left => {
            let panel = app.active_panel_mut();
            panel.clear_error();
            panel.move_left();
        }
        KeyCode::Right => {
            let panel = app.active_panel_mut();
            panel.clear_error();
            panel.move_right();
        }
        KeyCode::PageUp => {
            let panel = app.active_panel_mut();
            panel.clear_error();
            panel.page_up();
        }
        KeyCode::PageDown => {
            let panel = app.active_panel_mut();
            panel.clear_error();
            panel.page_down();
        }
        KeyCode::Home => {
            let panel = app.active_panel_mut();
            panel.clear_error();
            panel.move_home();
        }
        KeyCode::End => {
            let panel = app.active_panel_mut();
            panel.clear_error();
            panel.move_end();
        }

        // Vim-style navigation (only when edit_mode_always is false)
        KeyCode::Char('k') if !app.config.general.edit_mode_always => {
            let panel = app.active_panel_mut();
            panel.clear_error();
            panel.move_up();
        }
        KeyCode::Char('j') if !app.config.general.edit_mode_always => {
            let panel = app.active_panel_mut();
            panel.clear_error();
            panel.move_down();
        }
        KeyCode::Char('h') if !app.config.general.edit_mode_always => {
            let panel = app.active_panel_mut();
            panel.clear_error();
            panel.move_left();
        }
        KeyCode::Char('l') if !app.config.general.edit_mode_always => {
            let panel = app.active_panel_mut();
            panel.clear_error();
            panel.move_right();
        }
        KeyCode::Char('g') if !app.config.general.edit_mode_always => {
            let panel = app.active_panel_mut();
            panel.clear_error();
            panel.move_home();
        }
        KeyCode::Char('G') if !app.config.general.edit_mode_always => {
            let panel = app.active_panel_mut();
            panel.clear_error();
            panel.move_end();
        }

        // 'q' to quit only when edit_mode_always is false
        KeyCode::Char('q') if key.modifiers.is_empty() && !app.config.general.edit_mode_always => {
            app.should_quit = true;
        }

        // In edit_mode_always, regular characters go to command line
        KeyCode::Char(c) if app.config.general.edit_mode_always && !ctrl && !alt => {
            app.cmd.insert_char(c);
        }

        _ => {}
    }
}

fn handle_command_input(app: &mut App, key: KeyEvent) {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let alt = key.modifiers.contains(KeyModifiers::ALT);

    // Shell area scrolling (Alt+Arrow/Page or Ctrl+Page)
    if alt {
        match key.code {
            KeyCode::Up => { app.cmd.scroll_up(1); return; }
            KeyCode::Down => { app.cmd.scroll_down(1); return; }
            KeyCode::PageUp => { app.cmd.scroll_up(10); return; }
            KeyCode::PageDown => { app.cmd.scroll_down(10); return; }
            _ => {}
        }
    }
    if ctrl {
        match key.code {
            KeyCode::PageUp => { app.cmd.scroll_up(10); return; }
            KeyCode::PageDown => { app.cmd.scroll_down(10); return; }
            _ => {}
        }
    }

    match key.code {
        // Cancel command input
        KeyCode::Esc => {
            app.cmd.focused = false;
            app.cmd.clear_input();
            app.cmd.history_index = None;
            app.cmd.history_temp.clear();
            app.reset_completion();
        }

        // Insert filename into command line (Ctrl+F)
        KeyCode::Char('f') if ctrl => {
            if let Some(entry) = app.active_panel().selected()
                && entry.name != ".." {
                    let s = shell_escape(&entry.name) + " ";
                    app.cmd.insert_str(&s);
                }
        }

        // Insert current folder path into command line (Ctrl+P for Path)
        KeyCode::Char('p') if ctrl => {
            let path_str = app.active_panel().path.to_string_lossy();
            let s = shell_escape(&path_str) + " ";
            app.cmd.insert_str(&s);
        }

        // Insert full path into command line (Alt+Enter) - only in edit_mode_always
        KeyCode::Enter if alt && app.config.general.edit_mode_always => {
            if let Some(entry) = app.active_panel().selected()
                && entry.name != ".." {
                    let path_str = entry.path.to_string_lossy();
                    let s = shell_escape(&path_str) + " ";
                    app.cmd.insert_str(&s);
                }
        }

        // Execute command (plain Enter)
        KeyCode::Enter => {
            app.cmd.scroll_to_bottom();
            app.reset_completion();
            if !app.cmd.input.is_empty() {
                app.execute_command();
            } else {
                app.cmd.focused = false;
            }
        }

        // Tab completion
        KeyCode::Tab => {
            app.complete_command();
        }

        // Cursor movement
        KeyCode::Left if ctrl => {
            app.cmd.cursor_word_left();
        }
        KeyCode::Right if ctrl => {
            app.cmd.cursor_word_right();
        }
        KeyCode::Left => {
            app.cmd.cursor_left();
        }
        KeyCode::Right => {
            app.cmd.cursor_right();
        }
        KeyCode::Home => {
            app.cmd.cursor_home();
        }
        KeyCode::End => {
            app.cmd.cursor_end();
        }

        // Delete character before cursor
        KeyCode::Backspace if ctrl => {
            app.reset_completion();
            app.cmd.delete_word_before();
        }
        KeyCode::Backspace => {
            app.reset_completion();
            app.cmd.delete_char_before();
        }

        // Delete character at cursor
        KeyCode::Delete => {
            app.reset_completion();
            app.cmd.delete_char_at();
        }

        // History navigation
        KeyCode::Up => {
            app.reset_completion();
            app.history_up();
        }

        KeyCode::Down => {
            app.reset_completion();
            app.history_down();
        }

        // Ctrl+O still works in command mode
        KeyCode::Char('o') if ctrl => {
            if app.config.general.shell_history_mode
                || crate::persistent_shell::is_windows_10_or_older()
            {
                app.mode = Mode::ShellHistoryView { scroll: 0 };
            } else {
                app.mode = Mode::ShellVisible;
            }
        }

        // Ctrl+A — move to start of line
        KeyCode::Char('a') if ctrl => {
            app.cmd.cursor_home();
        }

        // Ctrl+E — move to end of line
        KeyCode::Char('e') if ctrl => {
            app.cmd.cursor_end();
        }

        // Ctrl+W — delete word before cursor
        KeyCode::Char('w') if ctrl => {
            app.reset_completion();
            app.cmd.delete_word_before();
        }

        // Ctrl+U — delete to start of line
        KeyCode::Char('u') if ctrl => {
            app.reset_completion();
            app.cmd.delete_to_start();
        }

        // Ctrl+K — delete to end of line
        KeyCode::Char('k') if ctrl => {
            app.reset_completion();
            app.cmd.delete_to_end();
        }

        // Type character (must be after ctrl combinations)
        KeyCode::Char(c) if !ctrl => {
            app.cmd.scroll_to_bottom();
            app.reset_completion();
            app.cmd.insert_char(c);
        }

        _ => {}
    }
}

/// Escape a string for shell use (wrap in quotes if needed)
pub fn shell_escape(s: &str) -> String {
    // Check if the string needs quoting
    let needs_quoting = s.chars().any(|c| {
        c.is_whitespace() || matches!(c, '"' | '\'' | '\\' | '$' | '`' | '!' | '*' | '?' | '[' | ']' | '(' | ')' | '{' | '}' | '|' | '&' | ';' | '<' | '>' | '#')
    });

    if needs_quoting {
        // Use double quotes and escape internal double quotes and backslashes
        let escaped: String = s.chars().map(|c| {
            match c {
                '"' => "\\\"".to_string(),
                '\\' => "\\\\".to_string(),
                '$' => "\\$".to_string(),
                '`' => "\\`".to_string(),
                _ => c.to_string(),
            }
        }).collect();
        format!("\"{}\"", escaped)
    } else {
        s.to_string()
    }
}
