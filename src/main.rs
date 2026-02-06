//! Bark - A Norton Commander clone in Rust
//!
//! Stage 6: Status bar and function keys

use std::io::{self, stdout, Write};
use std::panic;
use std::time::Duration;

use crossterm::{
    cursor::{Hide, Show},
    event::{self, Event, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Modifier, Style},
    Terminal,
};

mod state;
mod async_io;
mod config;
mod errors;
mod fs;
mod git;
mod input;
mod plugins;
mod providers;
mod ui;
mod utils;
mod persistent_shell;
mod win_console;

use state::app::App;
use state::mode::Mode;
use state::Side;
use ui::{ArchivePasswordPromptDialog, CommandHistoryDialog, ConfirmDialog, DeleteIterativeDialog, SimpleConfirmDialog, SourceSelector, FileViewer, FindFilesDialog, HelpViewer, MkdirDialog, OverwriteConfirmDialog, PanelWidget, PluginViewer, ScpConnectDialog, ScpPasswordPromptDialog, SelectFilesDialog, ShellArea, ShellHistoryViewer, SpinnerDialog, StatusBar, ViewerPluginMenu, ViewerSearchDialog, UserMenuDialog, UserMenuEditDialog, FileOpProgressDialog};
use ui::dialog::{archive_password_prompt_cursor_position, dialog_cursor_position, mkdir_cursor_position, find_files_pattern_cursor_position, find_files_content_cursor_position, find_files_path_cursor_position, viewer_search_text_cursor_position, viewer_search_hex_cursor_position, select_files_cursor_position, scp_connect_cursor_position, scp_password_prompt_cursor_position, user_menu_edit_cursor_position, PluginConnectDialog, plugin_connect_cursor_position};
use input::get_help_text;

/// Set up panic hook to restore terminal on panic
fn setup_panic_hook() {
    let original_hook = panic::take_hook();
    panic::set_hook(Box::new(move |panic_info| {
        let _ = disable_raw_mode();
        let _ = execute!(stdout(), LeaveAlternateScreen);
        original_hook(panic_info);
    }));
}

/// Initialize the terminal for TUI mode
fn setup_terminal() -> io::Result<Terminal<CrosstermBackend<io::Stdout>>> {
    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    Terminal::new(backend)
}

/// Restore terminal to normal mode
fn restore_terminal() -> io::Result<()> {
    disable_raw_mode()?;
    execute!(stdout(), LeaveAlternateScreen)?;
    Ok(())
}

/// Main event loop
fn run(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, app: &mut App) -> io::Result<()> {
    // Spawn the persistent shell
    app.init_shell();

    let mut needs_redraw = true;

    loop {
        // Drain output from the persistent shell each iteration
        let shell_had_output = app.poll_shell();
        if shell_had_output {
            needs_redraw = true;
        }

        // Draw the UI (only when something changed)
        if needs_redraw {
            needs_redraw = false;
            terminal.draw(|frame| {
            let size = frame.area();

            // Update terminal dimensions for shell area resizing and hex viewer
            app.ui.terminal_height = size.height;
            app.ui.terminal_width = size.width;

            // Forward terminal size changes to the persistent PTY
            // (only when dimensions actually change — on Windows ConPTY,
            // redundant resize calls can trigger cmd.exe to redraw its banner)
            if (size.width, size.height) != (app.ui.last_pty_cols, app.ui.last_pty_rows) {
                app.ui.last_pty_cols = size.width;
                app.ui.last_pty_rows = size.height;
                if let Some(shell) = &app.shell {
                    shell.resize(size.width, size.height);
                }
            }

            match &app.mode {
                Mode::Viewing { content, scroll, path, binary_mode, search_matches, current_match } => {
                    // Full-screen file viewer with search highlighting
                    let viewer = FileViewer::new(content, *scroll, path, &app.theme, *binary_mode)
                        .with_search(search_matches, *current_match);
                    app.ui.viewer_height = FileViewer::content_height(size);
                    frame.render_widget(viewer, size);

                    // Show search status in footer if there are matches
                    if !search_matches.is_empty() {
                        let status = if let Some(idx) = current_match {
                            format!(" Match {}/{} (n=next, N=prev) ", idx + 1, search_matches.len())
                        } else {
                            format!(" {} matches (n=next, N=prev) ", search_matches.len())
                        };
                        let status_x = size.x + 1;
                        let status_y = size.y + size.height - 1;
                        let status_style = Style::default()
                            .bg(app.theme.viewer_header_bg)
                            .fg(app.theme.viewer_header_fg);
                        let buf = frame.buffer_mut();
                        buf.set_string(status_x, status_y, &status, status_style);
                    }
                }
                Mode::ViewingPlugin { plugin_name, path, scroll, lines, total_lines, status_message } => {
                    // Full-screen plugin viewer
                    let viewer = PluginViewer::new(plugin_name, path, lines, *scroll, *total_lines, status_message.as_deref(), &app.theme);
                    app.ui.viewer_height = PluginViewer::content_height(size);
                    frame.render_widget(viewer, size);
                }
                Mode::ViewerPluginMenu { path, content, binary_mode, original_scroll, plugins, selected } => {
                    // Show the built-in viewer underneath
                    let viewer = FileViewer::new(content, *original_scroll, path, &app.theme, *binary_mode);
                    app.ui.viewer_height = FileViewer::content_height(size);
                    frame.render_widget(viewer, size);

                    // Render plugin menu as overlay
                    let menu = ViewerPluginMenu::new(plugins, *selected, &app.theme);
                    frame.render_widget(menu, size);
                }
                Mode::ViewerSearch {
                    content, scroll, path, binary_mode,
                    text_input, text_cursor, case_sensitive,
                    hex_input, hex_cursor, focus, ..
                } => {
                    // Show the built-in viewer underneath
                    let viewer = FileViewer::new(content, *scroll, path, &app.theme, *binary_mode);
                    app.ui.viewer_height = FileViewer::content_height(size);
                    frame.render_widget(viewer, size);

                    // Render search dialog as overlay
                    let dialog = ViewerSearchDialog::new(
                        text_input,
                        *case_sensitive,
                        hex_input,
                        *focus,
                        app.ui.input_selected,
                        &app.theme,
                        0,    // match_count - not available until search is executed
                        None, // current_match - not available until search is executed
                    );
                    frame.render_widget(dialog, size);

                    // Position cursor in the focused input field
                    // Focus: 0=text, 1=case_sensitive, 2=hex, 3=search, 4=cancel
                    if *focus == 0 {
                        let (cx, cy) = viewer_search_text_cursor_position(size, text_input, *text_cursor);
                        frame.set_cursor_position((cx, cy));
                    } else if *focus == 2 {
                        let (cx, cy) = viewer_search_hex_cursor_position(size, hex_input, *hex_cursor);
                        frame.set_cursor_position((cx, cy));
                    }
                }
                Mode::ShellHistoryView { scroll } => {
                    let viewer = ShellHistoryViewer::new(&app.cmd.output, *scroll);
                    app.ui.viewer_height = ShellHistoryViewer::content_height(size);
                    frame.render_widget(viewer, size);
                }
                Mode::Help { scroll } => {
                    // Full-screen help viewer
                    let help = HelpViewer::new(get_help_text(), *scroll, &app.theme);
                    app.ui.viewer_height = HelpViewer::content_height(size);
                    frame.render_widget(help, size);
                }
                _ => {
                    // Normal panel view
                    // Main vertical layout: panels, status bar, shell area
                    let shell_height = app.ui.shell_height;
                    let main_chunks = Layout::default()
                        .direction(Direction::Vertical)
                        .constraints([
                            Constraint::Min(5),                    // Panels (takes remaining space)
                            Constraint::Length(1),                 // Status bar
                            Constraint::Length(shell_height),      // Shell area (resizable)
                        ])
                        .split(size);

                    // Split panel area horizontally (ratio adjustable with Shift+Left/Right)
                    let left_pct = app.ui.left_panel_percent;
                    let right_pct = 100 - left_pct;
                    let panel_chunks = Layout::default()
                        .direction(Direction::Horizontal)
                        .constraints([Constraint::Percentage(left_pct), Constraint::Percentage(right_pct)])
                        .split(main_chunks[0]);

                    // Draw left panel
                    let left_widget = PanelWidget::new(app.active_panel == Side::Left && !app.cmd.focused, &app.theme)
                        .with_dir_sizes(&app.dir_sizes);
                    frame.render_stateful_widget(left_widget, panel_chunks[0], &mut app.left_panel);

                    // Draw right panel
                    let right_widget = PanelWidget::new(app.active_panel == Side::Right && !app.cmd.focused, &app.theme)
                        .with_dir_sizes(&app.dir_sizes);
                    frame.render_stateful_widget(right_widget, panel_chunks[1], &mut app.right_panel);

                    // Draw quick search box overlaid on active panel's bottom border
                    if let Some(ref search) = app.quick_search {
                        let search_style = Style::default()
                            .bg(app.theme.cursor_bg)
                            .fg(app.theme.cursor_fg)
                            .add_modifier(Modifier::BOLD);
                        let display = format!(" Search: {}_ ", search);
                        // Position on the bottom border of active panel, left-aligned (after border corner)
                        let active_panel_area = match app.active_panel {
                            Side::Left => panel_chunks[0],
                            Side::Right => panel_chunks[1],
                        };
                        let search_x = active_panel_area.x + 2;
                        let search_y = active_panel_area.y + active_panel_area.height - 1;
                        let buf = frame.buffer_mut();
                        buf.set_string(search_x, search_y, &display, search_style);
                    }

                    // Update git status if panel paths changed (and git status is enabled)
                    if app.config.display.show_git_status {
                        app.update_git_status();
                    }

                    // Draw status bar (shows active panel's selected file info + git status + plugins)
                    let (active_panel, git_status) = match app.active_panel {
                        Side::Left => (&app.left_panel, app.left_git_status.as_ref()),
                        Side::Right => (&app.right_panel, app.right_git_status.as_ref()),
                    };
                    let git_for_status = if app.config.display.show_git_status {
                        git_status
                    } else {
                        None
                    };
                    let plugin_status = app.get_plugin_status();
                    let plugin_status_ref: Option<&[(String, String)]> = if plugin_status.is_empty() {
                        None
                    } else {
                        Some(&plugin_status)
                    };
                    let status_bar = StatusBar::new(active_panel, &app.theme)
                        .with_git(git_for_status)
                        .with_plugin_status(plugin_status_ref);
                    frame.render_widget(status_bar, main_chunks[1]);

                    // Draw shell area (history + command line at bottom)
                    let cwd = match app.active_panel {
                        Side::Left => app.left_panel.path.to_string_lossy(),
                        Side::Right => app.right_panel.path.to_string_lossy(),
                    };
                    let prompt = format!("{}> ", cwd);
                    let shell_area = ShellArea::new(&app.cmd.output, &app.cmd.input, &prompt, app.cmd.scroll_offset);
                    frame.render_widget(shell_area, main_chunks[2]);

                    // Position cursor for command mode (on the last line of shell area)
                    // Show cursor when command_focused OR edit_mode_always is on (in Normal mode)
                    let show_cursor = app.cmd.focused ||
                        (app.config.general.edit_mode_always && matches!(app.mode, Mode::Normal));
                    if show_cursor {
                        let cursor_x = main_chunks[2].x + prompt.len() as u16 + app.cmd.cursor_display_offset() as u16;
                        let cursor_y = main_chunks[2].y + main_chunks[2].height - 1;
                        frame.set_cursor_position((cursor_x, cursor_y));
                    }

                    // Render confirmation dialog if in confirming mode (overlay)
                    if let Mode::Confirming { operation, sources, dest_input, cursor_pos, focus, apply_all } = &app.mode {
                        let dialog = ConfirmDialog::new(operation, sources, dest_input, *cursor_pos, *focus, app.ui.input_selected, *apply_all, &app.theme);
                        frame.render_widget(dialog, size);

                        // Position cursor in dialog input field (only when input is focused)
                        if *focus == 0 {
                            let (cx, cy) = dialog_cursor_position(size, dest_input, *cursor_pos);
                            frame.set_cursor_position((cx, cy));
                        }
                    }

                    // Render iterative delete confirmation dialog (overlay)
                    if let Mode::DeleteIterative { items, current, apply_all, focus, .. } = &app.mode {
                        let dialog = DeleteIterativeDialog::new(items, *current, *apply_all, *focus, &app.theme);
                        frame.render_widget(dialog, size);
                    }

                    // Render source selector if in source selector mode (overlay on target panel)
                    if let Mode::SourceSelector { target_panel, sources, selected } = &app.mode {
                        let dialog = SourceSelector::new(sources, *selected, target_panel, &app.theme);
                        let panel_area = match target_panel {
                            Side::Left => panel_chunks[0],
                            Side::Right => panel_chunks[1],
                        };
                        frame.render_widget(dialog, panel_area);
                    }

                    // Render simple confirmation dialog (overlay)
                    if let Mode::SimpleConfirm { message, focus, .. } = &app.mode {
                        let dialog = SimpleConfirmDialog::new(message, *focus, &app.theme);
                        frame.render_widget(dialog, size);
                    }

                    // Render mkdir dialog if in mkdir mode (overlay)
                    if let Mode::MakingDir { name_input, cursor_pos, focus } = &app.mode {
                        let dialog = MkdirDialog::new(name_input, *cursor_pos, *focus, app.ui.input_selected, &app.theme);
                        frame.render_widget(dialog, size);

                        // Position cursor in dialog input field (only when input is focused)
                        if *focus == 0 {
                            let (cx, cy) = mkdir_cursor_position(size, name_input, *cursor_pos);
                            frame.set_cursor_position((cx, cy));
                        }
                    }

                    // Render command history dialog if in history mode (overlay)
                    if let Mode::CommandHistory { selected, scroll } = &app.mode {
                        let dialog = CommandHistoryDialog::new(
                            &app.cmd.history,
                            *selected,
                            *scroll,
                            &app.theme,
                        );
                        frame.render_widget(dialog, size);
                    }

                    // Render find files dialog if in find files mode (overlay)
                    if let Mode::FindFiles {
                        pattern_input,
                        pattern_cursor,
                        pattern_case_sensitive,
                        content_input,
                        content_cursor,
                        content_case_sensitive,
                        path_input,
                        path_cursor,
                        recursive,
                        focus,
                    } = &app.mode
                    {
                        let dialog = FindFilesDialog::new(
                            pattern_input,
                            *pattern_case_sensitive,
                            content_input,
                            *content_case_sensitive,
                            path_input,
                            *recursive,
                            *focus,
                            app.ui.input_selected,
                            &app.theme,
                        );
                        frame.render_widget(dialog, size);

                        // Position cursor in the focused input field
                        // Focus: 0=pattern, 1=pattern_case, 2=content, 3=content_case, 4=path, 5=recursive, 6=search, 7=cancel
                        if *focus == 0 {
                            let (cx, cy) = find_files_pattern_cursor_position(size, pattern_input, *pattern_cursor);
                            frame.set_cursor_position((cx, cy));
                        } else if *focus == 2 {
                            let (cx, cy) = find_files_content_cursor_position(size, content_input, *content_cursor);
                            frame.set_cursor_position((cx, cy));
                        } else if *focus == 4 {
                            let (cx, cy) = find_files_path_cursor_position(size, path_input, *path_cursor);
                            frame.set_cursor_position((cx, cy));
                        }
                    }

                    // Render select files dialog if in select files mode (overlay)
                    if let Mode::SelectFiles {
                        pattern_input,
                        pattern_cursor,
                        include_dirs,
                        focus,
                    } = &app.mode
                    {
                        let dialog = SelectFilesDialog::new(
                            pattern_input,
                            *include_dirs,
                            *focus,
                            app.ui.input_selected,
                            &app.theme,
                        );
                        frame.render_widget(dialog, size);

                        // Position cursor in the pattern input field
                        if *focus == 0 {
                            let (cx, cy) = select_files_cursor_position(size, pattern_input, *pattern_cursor);
                            frame.set_cursor_position((cx, cy));
                        }
                    }

                    // Render SCP connection dialog if in SCP connect mode (overlay)
                    if let Mode::ScpConnect {
                        name_input,
                        name_cursor,
                        user_input,
                        user_cursor,
                        host_input,
                        host_cursor,
                        port_input,
                        port_cursor,
                        path_input,
                        path_cursor,
                        password_input,
                        password_cursor,
                        focus,
                        error,
                        ..
                    } = &app.mode
                    {
                        let dialog = ScpConnectDialog::new(
                            name_input,
                            *name_cursor,
                            user_input,
                            *user_cursor,
                            host_input,
                            *host_cursor,
                            port_input,
                            *port_cursor,
                            path_input,
                            *path_cursor,
                            password_input,
                            *password_cursor,
                            *focus,
                            app.ui.input_selected,
                            error.as_deref(),
                            &app.theme,
                        );
                        frame.render_widget(dialog, size);

                        // Position cursor in the focused input field
                        if let Some((cx, cy)) = scp_connect_cursor_position(
                            size,
                            *focus,
                            name_input,
                            *name_cursor,
                            user_input,
                            *user_cursor,
                            host_input,
                            *host_cursor,
                            port_input,
                            *port_cursor,
                            path_input,
                            *path_cursor,
                            password_input,
                            *password_cursor,
                        ) {
                            frame.set_cursor_position((cx, cy));
                        }
                    }

                    // Render SCP password prompt dialog if in password prompt mode (overlay)
                    if let Mode::ScpPasswordPrompt {
                        display_name,
                        password_input,
                        cursor_pos,
                        focus,
                        error,
                        ..
                    } = &app.mode
                    {
                        let dialog = ScpPasswordPromptDialog::new(
                            display_name,
                            password_input,
                            *focus,
                            app.ui.input_selected,
                            error.as_deref(),
                            &app.theme,
                        );
                        frame.render_widget(dialog, size);

                        // Position cursor in the password input field (only when input is focused)
                        if *focus == 0 {
                            let (cx, cy) = scp_password_prompt_cursor_position(size, password_input, *cursor_pos);
                            frame.set_cursor_position((cx, cy));
                        }
                    }

                    // Render archive password prompt dialog if in archive password mode (overlay)
                    if let Mode::ArchivePasswordPrompt {
                        archive_name,
                        password_input,
                        cursor_pos,
                        focus,
                        error,
                        ..
                    } = &app.mode
                    {
                        let dialog = ArchivePasswordPromptDialog::new(
                            archive_name,
                            password_input,
                            *focus,
                            app.ui.input_selected,
                            error.as_deref(),
                            &app.theme,
                        );
                        frame.render_widget(dialog, size);

                        if *focus == 0 {
                            let (cx, cy) = archive_password_prompt_cursor_position(size, password_input, *cursor_pos, error.is_some());
                            frame.set_cursor_position((cx, cy));
                        }
                    }

                    // Render plugin connect dialog if in plugin connect mode (overlay)
                    if let Mode::PluginConnect {
                        plugin_name,
                        fields,
                        values,
                        cursors,
                        focus,
                        error,
                        ..
                    } = &app.mode
                    {
                        let dialog = PluginConnectDialog::new(
                            plugin_name,
                            fields,
                            values,
                            *focus,
                            error.as_deref(),
                            &app.theme,
                            app.ui.input_selected,
                        );
                        frame.render_widget(dialog, size);

                        // Position cursor in the focused input field
                        if let Some((cx, cy)) = plugin_connect_cursor_position(
                            size,
                            fields,
                            values,
                            cursors,
                            *focus,
                        ) {
                            frame.set_cursor_position((cx, cy));
                        }
                    }

                    // Render user menu dialog if in user menu mode (overlay)
                    if let Mode::UserMenu { rules, selected, scroll } = &app.mode {
                        let dialog = UserMenuDialog::new(rules, *selected, *scroll, &app.theme);
                        frame.render_widget(dialog, size);
                    }

                    // Render user menu edit dialog if in user menu edit mode (overlay)
                    if let Mode::UserMenuEdit {
                        editing_index,
                        name_input,
                        name_cursor,
                        command_input,
                        command_cursor,
                        hotkey_input,
                        hotkey_cursor,
                        focus,
                        error,
                    } = &app.mode
                    {
                        let dialog = UserMenuEditDialog::new(
                            *editing_index,
                            name_input,
                            command_input,
                            hotkey_input,
                            *focus,
                            app.ui.input_selected,
                            error.as_deref(),
                            &app.theme,
                        );
                        frame.render_widget(dialog, size);

                        // Position cursor in the focused input field
                        if let Some((cx, cy)) = user_menu_edit_cursor_position(
                            size,
                            *focus,
                            name_input,
                            *name_cursor,
                            command_input,
                            *command_cursor,
                            hotkey_input,
                            *hotkey_cursor,
                        ) {
                            frame.set_cursor_position((cx, cy));
                        }
                    }

                    // Render overwrite confirmation dialog (overlay)
                    if let Mode::OverwriteConfirm { conflicts, current_conflict, focus, .. } = &app.mode {
                        let filename = conflicts.get(*current_conflict)
                            .and_then(|p| p.file_name())
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_default();
                        let dialog = OverwriteConfirmDialog::new(&filename, *current_conflict, conflicts.len(), *focus, &app.theme);
                        frame.render_widget(dialog, size);
                    }

                    // Render file operation progress dialog (overlay)
                    if let Mode::FileOpProgress { title, bytes_done, bytes_total, current_file, files_done, files_total, frame: spinner_frame } = &app.mode {
                        let dialog = FileOpProgressDialog::new(
                            spinner_frame % 10, title, current_file,
                            *bytes_done, *bytes_total, *files_done, *files_total, &app.theme,
                        );
                        frame.render_widget(dialog, size);
                    }

                    // Render permissions dialog (Unix only)
                    #[cfg(not(windows))]
                    if let Mode::EditingPermissions {
                        paths, owner, group,
                        user_read, user_write, user_execute,
                        group_read, group_write, group_execute,
                        other_read, other_write, other_execute,
                        apply_recursive, has_dirs, focus,
                    } = &app.mode {
                        let perms = [
                            *user_read, *user_write, *user_execute,
                            *group_read, *group_write, *group_execute,
                            *other_read, *other_write, *other_execute,
                        ];
                        let dialog = ui::EditPermissionsDialog::new(
                            paths, owner, group, perms,
                            *apply_recursive, *has_dirs, *focus, &app.theme,
                        );
                        frame.render_widget(dialog, size);
                    }

                    // Render owner/group dialog (Unix only)
                    #[cfg(not(windows))]
                    if let Mode::EditingOwner {
                        paths, current_owner, current_group,
                        users, groups,
                        user_selected, user_scroll,
                        group_selected, group_scroll,
                        apply_recursive, has_dirs, focus,
                        ..
                    } = &app.mode {
                        let dialog = ui::EditOwnerDialog::new(
                            paths, current_owner, current_group,
                            users, groups,
                            *user_selected, *user_scroll,
                            *group_selected, *group_scroll,
                            *apply_recursive, *has_dirs, *focus, &app.theme,
                        );
                        frame.render_widget(dialog, size);
                    }

                    // Render spinner dialog for background tasks (overlay)
                    if let Mode::BackgroundTask { title, message, frame: spinner_frame, started } = &app.mode {
                        let elapsed = started.elapsed();
                        let elapsed_str = format!("{}:{:02}", elapsed.as_secs() / 60, elapsed.as_secs() % 60);
                        let msg_with_time = format!("{}\nElapsed: {}", message, elapsed_str);
                        let spinner = SpinnerDialog::new(*spinner_frame, title, &msg_with_time)
                            .border_style(Style::default().fg(app.theme.panel_border_active))
                            .content_style(Style::default().fg(app.theme.cursor_fg).bg(app.theme.cursor_bg));
                        frame.render_widget(spinner, size);
                    }
                }
            }
        })?;
        } // needs_redraw

        // Check if we need to run a command
        if let Mode::RunningCommand { command, cwd } = &app.mode {
            let command = command.clone();
            let cwd = cwd.clone();
            app.mode = Mode::Normal;

            // Strip optional ! prefix (legacy syntax, now all commands
            // run on the real terminal).
            let actual_command = command.strip_prefix('!')
                .map(|s| s.trim().to_string())
                .unwrap_or(command);

            // Echo command to shell area.
            let cmd_line = format!("{}> {}", cwd.display(), actual_command);
            app.add_shell_output(cmd_line);

            // ── Windows 10: run in background thread with spinner ──
            // On Windows 10 (and older) ConPTY buffers screen-buffer output
            // and doesn't flush to the reader pipe in real-time, so the
            // persistent shell approach doesn't work for TUI commands.
            // Spawn via background task so the TUI stays responsive and the
            // user can cancel.
            //
            // On Windows 11+ ConPTY works properly, so we leave the TUI and
            // run the command on the real terminal (same approach as Unix).
            #[cfg(windows)]
            {
                if persistent_shell::is_windows_10_or_older() {
                    use state::background::BackgroundTask;

                    let shell = persistent_shell::resolve_shell(&app.config.general.shell);
                    let flag = persistent_shell::shell_command_flag(&shell).to_string();

                    let child_holder = std::sync::Arc::new(std::sync::Mutex::new(None));
                    app.command_child = Some(child_holder.clone());

                    let task = BackgroundTask::run_command(
                        shell, flag, actual_command.clone(), cwd.clone(), child_holder,
                    );
                    app.background_task = Some(task);
                    app.mode = Mode::BackgroundTask {
                        title: "Running command".to_string(),
                        message: actual_command,
                        frame: 0,
                        started: std::time::Instant::now(),
                    };
                    needs_redraw = true;
                    continue;
                } else {
                    // Windows 11+: leave TUI, run on real terminal
                    let shell = persistent_shell::resolve_shell(&app.config.general.shell);
                    let flag = persistent_shell::shell_command_flag(&shell);

                    restore_terminal()?;
                    println!("{}> {}", cwd.display(), actual_command);

                    let status = std::process::Command::new(&shell)
                        .arg(flag)
                        .arg(&actual_command)
                        .current_dir(&cwd)
                        .status();

                    match status {
                        Ok(exit) => {
                            if exit.success() {
                                app.add_shell_output(format!("(completed successfully)"));
                            } else {
                                let msg = match exit.code() {
                                    Some(code) => format!("(exited with code {})", code),
                                    None => "(process terminated)".to_string(),
                                };
                                app.add_shell_output(msg);
                            }
                        }
                        Err(e) => {
                            app.add_shell_output(format!("Error: {}", e));
                        }
                    }

                    *terminal = setup_terminal()?;

                    // Refresh panels
                    app.left_panel.refresh();
                    app.right_panel.refresh();

                    needs_redraw = true;
                    continue;
                }
            }

            // ── Unix: run on real terminal with script for capture ──
            #[cfg(not(windows))]
            {
                let shell = persistent_shell::resolve_shell(&app.config.general.shell);
                let _flag = persistent_shell::shell_command_flag(&shell);

                restore_terminal()?;
                println!("{}> {}", cwd.display(), actual_command);

                // Fish 4.1+ sends a Device Attributes query that hangs in PTYs
                // that don't respond.  Disable it.
                let is_fish = shell.to_lowercase().contains("fish");

                // Fish uses `-c` for command strings; bash/zsh use `-ic`.
                let shell_cmd_flag = if is_fish { "-c" } else { "-ic" };

                #[cfg(target_os = "linux")]
                let capture_file = {
                    let tmp = format!("/tmp/bark_capture_{}", std::process::id());
                    let inner = format!("{} {} {}",
                        persistent_shell::shell_quote(&shell),
                        shell_cmd_flag,
                        persistent_shell::shell_quote(&actual_command));
                    let mut cmd = std::process::Command::new("script");
                    cmd.args(["-q", "-c", &inner, &tmp])
                        .current_dir(&cwd);
                    if is_fish { cmd.env("fish_features", "no-query-term"); }
                    let status = cmd.status();
                    if let Err(e) = status {
                        app.add_shell_output(format!("Error: {}", e));
                    }
                    tmp
                };

                #[cfg(target_os = "macos")]
                let capture_file = {
                    // macOS BSD script: script -q <file> command [args...]
                    // No -c flag on script itself.  Pass the shell with its
                    // command flag so it executes the command.
                    let tmp = format!("/tmp/bark_capture_{}", std::process::id());
                    let mut cmd = std::process::Command::new("script");
                    cmd.arg("-q")
                        .arg(&tmp)
                        .arg(&shell)
                        .arg(shell_cmd_flag)
                        .arg(&actual_command)
                        .current_dir(&cwd);
                    if is_fish { cmd.env("fish_features", "no-query-term"); }
                    let status = cmd.status();
                    if let Err(e) = status {
                        app.add_shell_output(format!("Error: {}", e));
                    }
                    tmp
                };

                #[cfg(not(any(target_os = "linux", target_os = "macos")))]
                let capture_file = {
                    let status = std::process::Command::new(&shell)
                        .arg(_flag)
                        .arg(&actual_command)
                        .current_dir(&cwd)
                        .status();
                    if let Err(e) = status {
                        app.add_shell_output(format!("Error: {}", e));
                    }
                    String::new()
                };

                // Read captured output into the shell area
                if !capture_file.is_empty() {
                    if let Ok(content) = std::fs::read_to_string(&capture_file) {
                        // Skip TUI program output (alternate screen sequences)
                        if !persistent_shell::is_tui_output(&content) {
                            for line in content.lines() {
                                let line = line.trim_end_matches('\r');
                                let line = if let Some(pos) = line.rfind('\r') {
                                    &line[pos + 1..]
                                } else {
                                    line
                                };
                                let clean = persistent_shell::strip_ansi(line);
                                if clean.starts_with("Script started on ")
                                    || clean.starts_with("Script done on ")
                                    || clean.is_empty()
                                {
                                    continue;
                                }
                                app.add_shell_output(line.to_string());
                            }
                        }
                    }
                    let _ = std::fs::remove_file(&capture_file);
                }

                *terminal = setup_terminal()?;

                // Refresh panels
                app.left_panel.refresh();
                app.right_panel.refresh();

                needs_redraw = true;
                continue;
            }
        }

        // Check if we need to spawn an interactive shell
        if let Mode::SpawnShell { cwd } = &app.mode {
            let cwd = cwd.clone();
            app.mode = Mode::Normal;

            let shell = persistent_shell::resolve_shell(&app.config.general.shell);

            restore_terminal()?;

            let mut cmd = std::process::Command::new(&shell);
            cmd.current_dir(&cwd);

            // Fish 4.1+ sends a Device Attributes query that hangs in PTYs
            // that don't respond.  Disable it.
            if shell.to_lowercase().contains("fish") {
                cmd.env("fish_features", "no-query-term");
            }

            match cmd.status() {
                Ok(exit) => {
                    if !exit.success() {
                        let msg = match exit.code() {
                            Some(code) => format!("(shell exited with code {})", code),
                            None => "(shell terminated)".to_string(),
                        };
                        app.add_shell_output(msg);
                    }
                }
                Err(e) => {
                    app.add_shell_output(format!("Error spawning shell: {}", e));
                }
            }

            *terminal = setup_terminal()?;

            // Refresh panels (user may have changed files)
            app.left_panel.refresh();
            app.right_panel.refresh();

            needs_redraw = true;
            continue;
        }

        // Check if we need to launch an external editor
        if let Mode::Editing { path, remote_info } = &app.mode {
            let path = path.clone();
            let remote_info = remote_info.clone();
            app.mode = Mode::Normal;

            // Leave TUI temporarily
            restore_terminal()?;

            // Launch editor: $VISUAL -> $EDITOR -> hx -> vi (Unix) / notepad (Windows)
            let editor = std::env::var("VISUAL")
                .or_else(|_| std::env::var("EDITOR"))
                .unwrap_or_else(|_| {
                    if cfg!(windows) {
                        "notepad".to_string()
                    } else {
                        // Try hx (Helix) first, fall back to vi
                        if std::process::Command::new("hx").arg("--version").output().is_ok() {
                            "hx".to_string()
                        } else {
                            "vi".to_string()
                        }
                    }
                });

            let status = std::process::Command::new(&editor)
                .arg(&path)
                .status();

            // Re-enter TUI
            *terminal = setup_terminal()?;

            // If this was a remote file, upload it back
            if let Some((panel_side, remote_path)) = remote_info {
                // Read the edited temp file
                match std::fs::read(&path) {
                    Ok(contents) => {
                        let panel = match panel_side {
                            Side::Left => &mut app.left_panel,
                            Side::Right => &mut app.right_panel,
                        };
                        if let Err(e) = panel.write_file(&remote_path, &contents) {
                            panel.error = Some(format!("Failed to upload file: {}", e));
                        }
                    }
                    Err(e) => {
                        app.active_panel_mut().error = Some(format!(
                            "Failed to read edited file: {}", e
                        ));
                    }
                }
                // Clean up temp file
                let _ = std::fs::remove_file(&path);
            }

            // Refresh the panel in case file changed
            app.active_panel_mut().refresh();

            if let Err(e) = status {
                app.active_panel_mut().error = Some(format!(
                    "Failed to run '{}': {}",
                    editor, e
                ));
            }

            needs_redraw = true;
            continue;
        }

        // Check if a viewer plugin needs terminal access (e.g. hex editor).
        // We run the command directly from Bark (not through the plugin
        // process) so the editor inherits Bark's real terminal handles
        // instead of piped stdin/stdout.  This is the same pattern as
        // Mode::Editing (F4).
        if let Mode::TerminalPlugin { path, .. } = &app.mode {
            let path = path.clone();
            app.mode = Mode::Normal;

            let editor = app.config.editor.hex_editor.clone();
            if !editor.is_empty() {
                // Leave TUI so the editor gets a clean terminal
                restore_terminal()?;

                let _ = std::process::Command::new(&editor)
                    .arg(&path)
                    .status();

                // Re-enter TUI
                *terminal = setup_terminal()?;
            }

            // Open the built-in viewer on the (possibly modified) file
            app.view_file(&path);
            app.active_panel_mut().refresh();

            needs_redraw = true;
            continue;
        }

        // Check if shell toggle is active (Ctrl+O) - interactive shell mode
        if matches!(app.mode, Mode::ShellVisible) {

            // Windows 10's ConPTY doesn't relay shell output, so the
            // interactive shell is unavailable.  Return to Normal mode.
            if persistent_shell::is_windows_10_or_older() {
                app.add_shell_output("Interactive shell (Ctrl+O) is not available on Windows 10 due to ConPTY limitations.".to_string());
                app.mode = Mode::Normal;
                needs_redraw = true;
                continue;
            }

            // Clear any stray content if not in command mode
            if !app.cmd.focused {
                app.cmd.clear_input();
            }

            // Ensure we have a persistent shell
            if app.shell.is_none() {
                app.init_shell();
            }

            if let Some(shell) = &mut app.shell {
                // Leave alternate screen to show the primary buffer
                disable_raw_mode()?;
                execute!(stdout(), LeaveAlternateScreen, Show)?;
                print!("\x1b[0m");
                let _ = io::stdout().flush();

                // Detect shell type for platform-specific behaviour.
                let shell_lower = shell.shell_name().to_lowercase();
                let is_powershell = shell_lower.contains("powershell")
                    || shell_lower.contains("pwsh");

                // Clear startup suppression so Ctrl+O output is captured.
                shell.suppress_output.store(false, std::sync::atomic::Ordering::Relaxed);

                if is_powershell {
                    // PowerShell: skip replay.  ConPTY's virtual screen
                    // buffer uses cursor-positioning sequences that would
                    // overwrite replayed content.  Just enter shell mode;
                    // TUI command history is still available via
                    // Ctrl+Up/Down in the TUI.
                    shell.set_visible(true);
                } else {
                    // cmd.exe and other shells: replay TUI command history
                    // on the primary screen.  Replay BEFORE set_visible so
                    // ConPTY redraw doesn't overwrite it.
                    print!("\x1b[2J\x1b[H"); // clear + home
                    for line in &app.cmd.output {
                        println!("{}", line);
                    }
                    let _ = io::stdout().flush();

                    shell.set_visible(true);
                }

                // Unix: send an empty Enter so the shell displays a fresh
                // prompt.  Runs after set_visible so the user sees it.
                // The reader thread captures the response as OutputLine,
                // which the drain loop filters out (ANSI-heavy prompt).
                #[cfg(not(windows))]
                { let _ = shell.send_command(""); }

                // Forward stdin to the persistent shell until Ctrl+O
                persistent_shell::run_forwarding_loop(shell)?;

                // Drain channel messages accumulated during Ctrl+O.
                // InputTracked (from write_bytes): always keep.
                // OutputLine (from reader thread): keep only real
                // command output.  Terminal rendering noise (prompts,
                // fish/zsh ⏎ indicator, syntax redraws) contains
                // cursor positioning (\x1b[NC, \x1b[N;NH) or mid-line
                // \r — real command output doesn't.
                // Windows: ConPTY redraws the entire screen buffer when
                // entering Ctrl+O shell mode, and PowerShell sends
                // incremental syntax-highlighting echoes per keystroke.
                // All of this arrives as OutputLine BEFORE the first
                // InputTracked.  Collect all messages, then only emit
                // OutputLine that appears after an InputTracked.
                #[cfg(windows)]
                {
                    let mut drained: Vec<persistent_shell::ShellMessage> = Vec::new();
                    while let Ok(msg) = shell.receiver.try_recv() {
                        drained.push(msg);
                    }

                    // Find first InputTracked index — everything before
                    // it is ConPTY redraw noise or char echo garbage.
                    let first_input = drained.iter().position(|m| {
                        matches!(m, persistent_shell::ShellMessage::InputTracked(_))
                    });

                    for (i, msg) in drained.into_iter().enumerate() {
                        match msg {
                            persistent_shell::ShellMessage::InputTracked(line) => {
                                // The last output line may be a bare prompt
                                // (e.g. "C:\path>") that is a prefix of this
                                // InputTracked line ("C:\path> whoami").
                                // Replace it to avoid a double-prompt.
                                if let Some(last) = app.cmd.output.last() {
                                    let trimmed = last.trim_end();
                                    if !trimmed.is_empty() && line.starts_with(trimmed) {
                                        let last_idx = app.cmd.output.len() - 1;
                                        app.cmd.output[last_idx] = line;
                                    } else {
                                        app.cmd.add_output(line);
                                    }
                                } else {
                                    app.cmd.add_output(line);
                                }
                            }
                            persistent_shell::ShellMessage::OutputLine(line) => {
                                let dominated = first_input.map_or(true, |fi| i < fi);
                                if dominated {
                                    continue;
                                }
                                let is_noise = line.contains("\x1b[?2004h")
                                    || has_cursor_move(&line);
                                let stripped = persistent_shell::strip_ansi(&line);
                                if !is_noise && stripped.len() > 1 {
                                    app.cmd.add_output(stripped);
                                }
                            }
                            _ => {}
                        }
                    }

                    // The last OutputLine is typically the next cmd.exe
                    // prompt (e.g. "C:\path>") that ConPTY renders after
                    // the command finishes.  This is redundant because the
                    // next InputTracked will include it.  Pop it if it
                    // looks like a bare prompt (ends with ">").
                    if let Some(last) = app.cmd.output.last() {
                        let trimmed = last.trim_end();
                        let ends_gt = trimmed.ends_with('>');
                        let has_space = trimmed.contains(' ');
                        let starts_ps = trimmed.starts_with("PS ");
                        if ends_gt && (!has_space || starts_ps) {
                            app.cmd.output.pop();
                        }
                    }
                }

                // Unix: no ConPTY redraw issue — filter inline as before.
                #[cfg(not(windows))]
                while let Ok(msg) = shell.receiver.try_recv() {
                    match msg {
                        persistent_shell::ShellMessage::InputTracked(line) => {
                            app.cmd.add_output(line);
                        }
                        persistent_shell::ShellMessage::OutputLine(line) => {
                            // Fish/zsh prompt redraws and syntax
                            // highlighting contain cursor-forward
                            // sequences (\x1b[NC) that real command
                            // output never has.  Also filter bracket
                            // paste mode toggles (\x1b[?2004h).
                            let is_noise = line.contains("\x1b[?2004h")
                                || has_cursor_move(&line);
                            let stripped = persistent_shell::strip_ansi(&line);
                            if !is_noise && stripped.len() > 1 {
                                app.cmd.add_output(stripped);
                            }
                        }
                        _ => {}
                    }
                }

                shell.set_visible(false);

                // Flush stale console input events and ensure VT processing
                win_console::flush_console_input();
                win_console::ensure_vt_processing();

                // Return to alternate screen
                execute!(stdout(), EnterAlternateScreen, Hide)?;
                enable_raw_mode()?;
                terminal.clear()?;

                // After set_visible(false), ConPTY sends redraw noise as
                // it re-renders its screen buffer.  Wait for it to settle,
                // then discard the noise.
                std::thread::sleep(Duration::from_millis(300));
                if let Some(ref shell) = app.shell {
                    while shell.receiver.try_recv().is_ok() {}
                }
            }

            app.mode = Mode::Normal;
            app.cmd.focused = false;

            // Refresh panels in case filesystem changed
            app.left_panel.refresh();
            app.right_panel.refresh();

            needs_redraw = true;
            continue;
        }

        // Poll for background task completion and tick spinner
        let has_animation = matches!(app.mode, Mode::BackgroundTask { .. } | Mode::FileOpProgress { .. });
        if matches!(app.mode, Mode::BackgroundTask { .. }) {
            app.poll_background_task();
            app.tick_spinner();
            needs_redraw = true;
        }

        // Poll file operation progress
        if matches!(app.mode, Mode::FileOpProgress { .. }) {
            app.poll_file_operation();
            app.tick_spinner();
            needs_redraw = true;
        }

        // Use shorter poll timeout for animations (spinner),
        // otherwise block until an event arrives.
        let poll_timeout = if has_animation {
            Duration::from_millis(50)
        } else {
            Duration::from_secs(5)
        };

        if event::poll(poll_timeout)? {
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    input::handle_key(app, key);
                    needs_redraw = true;
                }
                Event::Resize(_, _) => {
                    needs_redraw = true;
                }
                _ => {}
            }
        }

        if app.should_quit {
            // Kill any running background command process tree
            #[cfg(windows)]
            if let Some(child_arc) = app.command_child.take() {
                if let Ok(mut guard) = child_arc.lock() {
                    if let Some(child) = guard.take() {
                        let pid = child.id();
                        drop(child);
                        persistent_shell::kill_process_tree(pid);
                    }
                }
            }
            // Drop the background task so the thread is detached
            app.background_task = None;
            // Save state before exiting
            app.save_state();
            // Remove lock file before shell shutdown — on Windows,
            // shutdown() calls process::exit() and never returns.
            remove_lock_file();
            // Shut down the persistent shell (on Windows 11+ this
            // calls process::exit and never returns).
            if let Some(shell) = app.shell.take() {
                shell.shutdown();
            }
            // On Windows 10 there is no persistent shell, so
            // shutdown() above is skipped.  Exit explicitly to
            // ensure background threads don't keep the process alive.
            #[cfg(windows)]
            if persistent_shell::is_windows_10_or_older() {
                std::process::exit(0);
            }
            break;
        }
    }
    Ok(())
}

/// Path to the instance lock file
fn lock_file_path() -> std::path::PathBuf {
    let dir = config::config_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    let _ = std::fs::create_dir_all(&dir);
    dir.join("bark.lock")
}

/// Check if another instance is running by reading the lock file PID.
/// Returns Some(pid) if a live process holds the lock.
fn check_existing_instance() -> Option<u32> {
    let path = lock_file_path();
    let contents = std::fs::read_to_string(&path).ok()?;
    let pid: u32 = contents.trim().parse().ok()?;

    // Check if that PID is still alive
    #[cfg(unix)]
    {
        // signal 0 checks existence without sending a signal
        let alive = unsafe { libc::kill(pid as i32, 0) } == 0;
        if alive && pid != std::process::id() {
            return Some(pid);
        }
    }
    #[cfg(windows)]
    {
        // On Windows, just trust the lock file if PID differs from ours
        if pid != std::process::id() {
            return Some(pid);
        }
    }

    None
}

/// Write our PID to the lock file
fn write_lock_file() {
    let path = lock_file_path();
    let _ = std::fs::write(&path, std::process::id().to_string());
}

/// Remove the lock file on exit
fn remove_lock_file() {
    let path = lock_file_path();
    let _ = std::fs::remove_file(&path);
}

/// Show a TUI confirmation dialog asking whether to proceed when another instance is detected.
/// Returns true if the user chooses to continue.
fn confirm_duplicate_instance(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    pid: u32,
    theme: &ui::Theme,
) -> io::Result<bool> {
    let message = format!("Bark is already running (PID {}). Continue anyway?", pid);
    let mut focus: usize = 1; // Default to "No"

    loop {
        terminal.draw(|frame| {
            let size = frame.area();
            let dialog = SimpleConfirmDialog::new(&message, focus, theme);
            // Clear screen with a dark background first
            let bg = ratatui::widgets::Block::default()
                .style(Style::default().bg(ratatui::style::Color::Black));
            frame.render_widget(bg, size);
            frame.render_widget(dialog, size);
        })?;

        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()?
                && key.kind == KeyEventKind::Press
            {
                match key.code {
                    crossterm::event::KeyCode::Char('y') | crossterm::event::KeyCode::Char('Y') => {
                        return Ok(true);
                    }
                    crossterm::event::KeyCode::Char('n') | crossterm::event::KeyCode::Char('N')
                    | crossterm::event::KeyCode::Esc => {
                        return Ok(false);
                    }
                    crossterm::event::KeyCode::Enter => {
                        return Ok(focus == 0);
                    }
                    crossterm::event::KeyCode::Tab
                    | crossterm::event::KeyCode::Left
                    | crossterm::event::KeyCode::Right
                    | crossterm::event::KeyCode::BackTab => {
                        focus = if focus == 0 { 1 } else { 0 };
                    }
                    _ => {}
                }
            }
        }
    }
}

/// Returns true if the line contains cursor-positioning escape sequences
/// (cursor forward `\x1b[NC` or absolute position `\x1b[N;NH`), which
/// indicate terminal prompt rendering rather than real command output.
fn has_cursor_move(s: &str) -> bool {
    let bytes = s.as_bytes();
    let mut i = 0;
    while i + 2 < bytes.len() {
        if bytes[i] == 0x1b && bytes[i + 1] == b'[' {
            i += 2;
            // Skip digits and semicolons
            while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b';') {
                i += 1;
            }
            if i < bytes.len() && (bytes[i] == b'C' || bytes[i] == b'H') {
                return true;
            }
        } else {
            i += 1;
        }
    }
    false
}

fn main() -> io::Result<()> {
    setup_panic_hook();

    // Check for duplicate instance before setting up the full app
    let existing_pid = check_existing_instance();

    if let Some(pid) = existing_pid {
        // Need terminal to show the dialog
        let mut terminal = setup_terminal()?;
        let theme = ui::Theme::default();
        let proceed = confirm_duplicate_instance(&mut terminal, pid, &theme);
        match proceed {
            Ok(true) => {
                // User chose to continue — proceed with normal startup
            }
            _ => {
                restore_terminal()?;
                return Ok(());
            }
        }
        // Terminal is already set up, write lock and run
        write_lock_file();
        let mut app = App::new();
        let result = run(&mut terminal, &mut app);
        remove_lock_file();
        restore_terminal()?;
        return result;
    }

    // No duplicate — normal startup
    write_lock_file();
    let mut terminal = setup_terminal()?;
    let mut app = App::new();

    let result = run(&mut terminal, &mut app);

    remove_lock_file();
    restore_terminal()?;

    result
}
