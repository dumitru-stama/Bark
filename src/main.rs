//! Bark - A Norton Commander clone in Rust
//!
//! Stage 6: Status bar and function keys

use std::io::{self, stdout, Read, Write};
use std::panic;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use portable_pty::{native_pty_system, CommandBuilder, PtySize};

use crossterm::{
    cursor::{Hide, Show},
    event::{self, Event},
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

use state::app::App;
use state::mode::Mode;
use state::Side;
use ui::{CommandHistoryDialog, ConfirmDialog, SimpleConfirmDialog, SourceSelector, FileViewer, FindFilesDialog, HelpViewer, MkdirDialog, OverwriteConfirmDialog, PanelWidget, PluginViewer, ScpConnectDialog, ScpPasswordPromptDialog, SelectFilesDialog, ShellArea, SpinnerDialog, StatusBar, ViewerPluginMenu, ViewerSearchDialog, UserMenuDialog, UserMenuEditDialog, FileOpProgressDialog};
use ui::dialog::{dialog_cursor_position, mkdir_cursor_position, find_files_pattern_cursor_position, find_files_content_cursor_position, find_files_path_cursor_position, viewer_search_text_cursor_position, viewer_search_hex_cursor_position, select_files_cursor_position, scp_connect_cursor_position, scp_password_prompt_cursor_position, user_menu_edit_cursor_position, PluginConnectDialog, plugin_connect_cursor_position};
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
    loop {
        // Draw the UI
        terminal.draw(|frame| {
            let size = frame.area();

            // Update terminal dimensions for shell area resizing and hex viewer
            app.ui.terminal_height = size.height;
            app.ui.terminal_width = size.width;

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
                Mode::ViewingPlugin { plugin_name, path, scroll, lines, total_lines } => {
                    // Full-screen plugin viewer
                    let viewer = PluginViewer::new(plugin_name, path, lines, *scroll, *total_lines, &app.theme);
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
                    let shell_area = ShellArea::new(&app.cmd.output, &app.cmd.input, &prompt);
                    frame.render_widget(shell_area, main_chunks[2]);

                    // Position cursor for command mode (on the last line of shell area)
                    // Show cursor when command_focused OR edit_mode_always is on (in Normal mode)
                    let show_cursor = app.cmd.focused ||
                        (app.config.general.edit_mode_always && matches!(app.mode, Mode::Normal));
                    if show_cursor {
                        let cursor_x = main_chunks[2].x + prompt.len() as u16 + app.cmd.input.len() as u16;
                        let cursor_y = main_chunks[2].y + main_chunks[2].height - 1;
                        frame.set_cursor_position((cursor_x, cursor_y));
                    }

                    // Render confirmation dialog if in confirming mode (overlay)
                    if let Mode::Confirming { operation, sources, dest_input, cursor_pos, focus } = &app.mode {
                        let dialog = ConfirmDialog::new(operation, sources, dest_input, *cursor_pos, *focus, app.ui.input_selected, &app.theme);
                        frame.render_widget(dialog, size);

                        // Position cursor in dialog input field (only when input is focused)
                        if *focus == 0 {
                            let (cx, cy) = dialog_cursor_position(size, dest_input, *cursor_pos);
                            frame.set_cursor_position((cx, cy));
                        }
                    }

                    // Render source selector if in source selector mode (overlay)
                    if let Mode::SourceSelector { target_panel, sources, selected } = &app.mode {
                        let dialog = SourceSelector::new(sources, *selected, target_panel, &app.theme);
                        frame.render_widget(dialog, size);
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

                    // Render spinner dialog for background tasks (overlay)
                    if let Mode::BackgroundTask { title, message, frame: spinner_frame } = &app.mode {
                        let spinner = SpinnerDialog::new(*spinner_frame, title, message)
                            .border_style(Style::default().fg(app.theme.panel_border_active))
                            .content_style(Style::default().fg(app.theme.cursor_fg).bg(app.theme.cursor_bg));
                        frame.render_widget(spinner, size);
                    }
                }
            }
        })?;

        // Check if we need to run a command
        if let Mode::RunningCommand { command, cwd } = &app.mode {
            let command = command.clone();
            let cwd = cwd.clone();
            app.mode = Mode::Normal;

            // Check if command starts with ! for explicit full terminal access
            let (explicit_interactive, actual_command) = if let Some(rest) = command.strip_prefix('!') {
                (true, rest.trim().to_string())
            } else {
                (false, command)
            };

            // Add command to shell history
            let cmd_line = format!("{}> {}", cwd.display(), actual_command);
            app.add_shell_output(cmd_line);

            // Run command with PTY and auto-detect TUI programs
            run_command_with_pty_detection(
                &actual_command,
                &cwd,
                explicit_interactive,
                app,
                terminal,
            )?;

            // Refresh panels
            app.left_panel.refresh();
            app.right_panel.refresh();

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

            continue;
        }

        // Check if shell toggle is active (Ctrl+O) - interactive shell mode with PTY
        if matches!(app.mode, Mode::ShellVisible) {
            // Get current directory
            let cwd = match app.active_panel {
                Side::Left => app.left_panel.path.clone(),
                Side::Right => app.right_panel.path.clone(),
            };

            // Clear any stray content if not in command mode
            if !app.cmd.focused {
                app.cmd.input.clear();
            }

            // Leave alternate screen to show terminal
            disable_raw_mode()?;
            execute!(stdout(), LeaveAlternateScreen, Show)?;

            // Reset terminal attributes before printing history so
            // leftover colors from Bark's UI don't bleed through
            print!("\x1b[0m");
            let _ = io::stdout().flush();

            // Print shell history so user can see previous commands
            for line in &app.cmd.output {
                println!("{}", line);
            }

            // Run interactive shell with PTY, capture output
            let captured_lines = run_interactive_shell(&cwd, &app.config.general.shell)?;
            for line in captured_lines {
                app.add_shell_output(line);
            }

            // Return to alternate screen
            execute!(stdout(), EnterAlternateScreen, Hide)?;
            enable_raw_mode()?;
            terminal.clear()?;
            app.mode = Mode::Normal;
            app.cmd.focused = false;

            // Refresh panels in case filesystem changed
            app.left_panel.refresh();
            app.right_panel.refresh();

            continue;
        }

        // Poll for background task completion and tick spinner
        if matches!(app.mode, Mode::BackgroundTask { .. }) {
            app.poll_background_task();
            app.tick_spinner();
        }

        // Poll file operation progress
        if matches!(app.mode, Mode::FileOpProgress { .. }) {
            app.poll_file_operation();
            app.tick_spinner();
        }

        // Use shorter poll timeout for background tasks / file ops (smoother animation)
        let poll_timeout = if matches!(app.mode, Mode::BackgroundTask { .. } | Mode::FileOpProgress { .. }) {
            Duration::from_millis(50)
        } else {
            Duration::from_millis(100)
        };

        if event::poll(poll_timeout)?
            && let Event::Key(key) = event::read()?
        {
            input::handle_key(app, key);
        }

        if app.should_quit {
            // Save state before exiting
            app.save_state();
            break;
        }
    }
    Ok(())
}

/// Check if buffer contains alternate screen buffer escape sequence
#[allow(dead_code)]
fn has_alternate_screen(buf: &[u8]) -> bool {
    // Look for \x1b[?1049h or \x1b[?47h or \x1b[?1047h
    for i in 0..buf.len().saturating_sub(4) {
        if buf[i] == 0x1b && buf.get(i + 1) == Some(&b'[') && buf.get(i + 2) == Some(&b'?') {
            let rest = &buf[i + 3..];
            // Check for 1049h
            if rest.starts_with(b"1049h") {
                return true;
            }
            // Check for 1047h
            if rest.starts_with(b"1047h") {
                return true;
            }
            // Check for 47h
            if rest.starts_with(b"47h") {
                return true;
            }
        }
    }
    false
}

/// Strip ANSI escape sequences from text
fn strip_ansi(s: &str) -> String {
    let mut result = String::new();
    let mut chars = s.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '\x1b' {
            // Skip escape sequence
            if let Some(&next) = chars.peek() {
                if next == '[' {
                    chars.next();
                    // Skip until we hit a letter or ~
                    while let Some(&ch) = chars.peek() {
                        chars.next();
                        if ch.is_ascii_alphabetic() || ch == '~' {
                            break;
                        }
                    }
                    continue;
                } else if next == ']' {
                    // OSC sequence - skip until BEL or ST
                    chars.next();
                    while let Some(ch) = chars.next() {
                        if ch == '\x07' {
                            break;
                        }
                        if ch == '\x1b'
                            && chars.peek() == Some(&'\\') {
                                chars.next();
                                break;
                            }
                    }
                    continue;
                }
            }
        } else if c == '\r' {
            // Skip carriage returns
            continue;
        }
        result.push(c);
    }

    result
}

/// Check if output looks like it came from a TUI program
/// TUI programs typically clear the screen or exit alternate buffer when done
fn is_tui_output(content: &str) -> bool {
    // Look for terminal reset/clear sequences anywhere in output
    // These indicate a full-screen TUI program was running

    // ESC[?1049l - exit alternate screen buffer (most common)
    if content.contains("\x1b[?1049l") {
        return true;
    }
    // ESC[?1049h - enter alternate screen buffer
    if content.contains("\x1b[?1049h") {
        return true;
    }
    // ESC[?47l / ESC[?47h - older alternate screen
    if content.contains("\x1b[?47l") || content.contains("\x1b[?47h") {
        return true;
    }
    // ESC[2J - clear entire screen
    if content.contains("\x1b[2J") {
        return true;
    }
    // ESC c - full terminal reset (RIS)
    if content.contains("\x1bc") {
        return true;
    }
    // ESC[H followed by ESC[J - home cursor + clear
    if content.contains("\x1b[H\x1b[J") {
        return true;
    }

    false
}

/// Run a command directly in terminal, capture output for shell area
fn run_command_with_pty_detection(
    command: &str,
    cwd: &Path,
    _force_interactive: bool,
    app: &mut App,
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
) -> io::Result<()> {
    let shell = resolve_shell(&app.config.general.shell);
    let shell_arg = shell_command_flag(&shell);

    // Leave alternate screen so user sees the terminal
    restore_terminal()?;

    // Echo the command so user sees what's being run (like a normal shell)
    println!("{}> {}", cwd.display(), command);

    // Use script command to capture output while still allowing interaction
    // Note: script -c syntax varies by platform and may not exist on BusyBox/minimal systems
    #[cfg(target_os = "linux")]
    let (capture_cmd, capture_file) = {
        let tmp = format!("/tmp/rc_capture_{}", std::process::id());
        // Check if script supports -c (util-linux vs BusyBox)
        let script_supports_c = std::process::Command::new("script")
            .arg("--help")
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).contains("-c"))
            .unwrap_or(false);
        if script_supports_c {
            // Use the user's shell with -ic so aliases (like ls --color) are loaded
            let inner = format!("{} -ic {}", shell_quote(&shell), shell_quote(command));
            (format!("script -q -c {} {}", shell_quote(&inner), &tmp), tmp)
        } else {
            // Fallback: use Python's pty module (available on virtually all Linux systems)
            let py_script = format!(
                r#"import pty,os,sys;f=open('{}','wb')
def r(fd):
 d=os.read(fd,1024);f.write(d);sys.stdout.buffer.write(d);sys.stdout.buffer.flush();return d
pty.spawn([{},'-ic',{}],r);f.close()"#,
                &tmp,
                python_quote(&shell),
                python_quote(command)
            );
            (format!("python3 -c {}", shell_quote(&py_script)), tmp)
        }
    };

    #[cfg(target_os = "macos")]
    let (capture_cmd, capture_file) = {
        let tmp = format!("/tmp/rc_capture_{}", std::process::id());
        // macOS BSD script: script -q <file> command [args...]
        // No -c flag on macOS. Use the user's shell with -ic so aliases are loaded.
        (format!("script -q {} {} -ic {}", &tmp, &shell, shell_quote(command)), tmp)
    };

    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    let (capture_cmd, capture_file) = (command.to_string(), String::new());

    // Run the command
    let cmd_to_run = if capture_file.is_empty() { command } else { &capture_cmd };

    let status = std::process::Command::new(&shell)
        .arg(shell_arg)
        .arg(cmd_to_run)
        .current_dir(cwd)
        .status();

    // Read captured output if available - store ALL lines
    if !capture_file.is_empty() {
        if let Ok(content) = std::fs::read_to_string(&capture_file) {
            // Check if this looks like TUI output (has screen clear/reset at end)
            // If so, discard all output from this command
            if !is_tui_output(&content) {
                for line in content.lines() {
                    // First strip trailing \r (handles \r\n line endings)
                    let line = line.trim_end_matches('\r');

                    // Handle carriage returns: keep only content after last \r
                    // This simulates terminal behavior for progress indicators
                    let line = if let Some(pos) = line.rfind('\r') {
                        &line[pos + 1..]
                    } else {
                        line
                    };

                    // Skip script command header/footer lines
                    let clean = strip_ansi(line);
                    if clean.starts_with("Script started on ") || clean.starts_with("Script done on ") {
                        continue;
                    }
                    // Skip empty lines (check stripped version)
                    if clean.is_empty() {
                        continue;
                    }
                    // Keep the original line with ANSI codes for colored output
                    app.add_shell_output(line.to_string());
                }
            }
        }
        let _ = std::fs::remove_file(&capture_file);
    }

    // Return to TUI
    *terminal = setup_terminal()?;

    if let Err(e) = status {
        app.add_shell_output(format!("Error: {}", e));
    }

    Ok(())
}

/// Quote a command for shell
#[cfg(unix)]
fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace("'", "'\\''"))
}

/// Quote a string for embedding in a Python string literal
#[cfg(target_os = "linux")]
fn python_quote(s: &str) -> String {
    let escaped = s
        .replace('\\', "\\\\")
        .replace('\'', "\\'")
        .replace('\n', "\\n")
        .replace('\r', "\\r");
    format!("'{}'", escaped)
}

/// Resolve which shell to use. If `configured` is non-empty, use it directly.
/// Otherwise auto-detect: on Windows pwsh > powershell > cmd.exe, on Unix $SHELL > /bin/sh.
fn resolve_shell(configured: &str) -> String {
    if !configured.is_empty() {
        return configured.to_string();
    }
    if cfg!(windows) {
        // Prefer modern PowerShell (pwsh) first
        if std::process::Command::new("pwsh").arg("-Version").output().is_ok() {
            return "pwsh".to_string();
        }
        // Then Windows PowerShell
        if std::process::Command::new("powershell").arg("-Version").output().is_ok() {
            return "powershell".to_string();
        }
        // Fall back to COMSPEC / cmd.exe
        std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".to_string())
    } else {
        std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string())
    }
}

/// Determine the right shell argument flag for running a command
fn shell_command_flag(shell: &str) -> &'static str {
    let lower = shell.to_lowercase();
    if lower.contains("powershell") || lower.contains("pwsh") {
        "-Command"
    } else if cfg!(windows) {
        "/C"
    } else {
        "-c"
    }
}

/// Run an interactive shell with PTY support (for tab completion, etc.)
/// Returns captured output lines when user presses Ctrl+O
fn run_interactive_shell(cwd: &Path, shell_config: &str) -> io::Result<Vec<String>> {
    // Get terminal size
    let (cols, rows) = crossterm::terminal::size().unwrap_or((80, 24));

    // Create PTY system
    let pty_system = native_pty_system();

    // Open a PTY pair
    let pair = pty_system
        .openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| io::Error::other(e.to_string()))?;

    // Build the shell command
    let shell = resolve_shell(shell_config);

    let mut cmd = CommandBuilder::new(&shell);
    cmd.cwd(cwd);

    // Spawn the shell
    let mut child = pair
        .slave
        .spawn_command(cmd)
        .map_err(|e| io::Error::other(e.to_string()))?;

    // Get reader/writer for the PTY master
    let mut reader = pair.master.try_clone_reader()
        .map_err(|e| io::Error::other(e.to_string()))?;
    let mut writer = pair.master.take_writer()
        .map_err(|e| io::Error::other(e.to_string()))?;

    // Set raw mode via libc and configure for poll()-based reading.
    // We use raw byte forwarding (no crossterm event reader) so terminal
    // responses flow transparently from the real terminal to the shell.
    #[cfg(unix)]
    let orig_termios = unsafe {
        let mut orig: libc::termios = std::mem::zeroed();
        libc::tcgetattr(libc::STDIN_FILENO, &mut orig);
        let mut raw = orig;
        libc::cfmakeraw(&mut raw);
        // VMIN=0, VTIME=0: read returns immediately (non-blocking via termios)
        raw.c_cc[libc::VMIN] = 0;
        raw.c_cc[libc::VTIME] = 0;
        libc::tcsetattr(libc::STDIN_FILENO, libc::TCSAFLUSH, &raw);
        orig
    };
    #[cfg(not(unix))]
    enable_raw_mode()?;

    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Mutex;
    let running = Arc::new(AtomicBool::new(true));
    let running_clone = Arc::clone(&running);

    // Shared buffer to capture PTY output for shell history
    let captured = Arc::new(Mutex::new(Vec::<u8>::new()));
    let captured_clone = Arc::clone(&captured);

    // Spawn thread to read from PTY, write to stdout, and capture output.
    let stdout_handle = std::thread::spawn(move || {
        let mut buf = [0u8; 4096];
        let mut stdout = io::stdout();
        while running_clone.load(Ordering::Relaxed) {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    let _ = stdout.write_all(&buf[..n]);
                    let _ = stdout.flush();
                    if let Ok(mut cap) = captured_clone.lock() {
                        cap.extend_from_slice(&buf[..n]);
                    }
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(_) => break,
            }
        }
    });

    // Main loop: use poll() to wait for stdin data, then read raw bytes.
    // Fully transparent - all bytes (including terminal responses) flow
    // from the real terminal through to the shell via the PTY.
    // Ctrl+O (0x0F) is detected to return to Bark.
    'shell_loop: loop {
        // Check if child process has exited
        if let Ok(Some(_)) = child.try_wait() {
            break;
        }

        #[cfg(unix)]
        {
            // Wait for stdin to have data (50ms timeout)
            let mut pfd = libc::pollfd {
                fd: libc::STDIN_FILENO,
                events: libc::POLLIN,
                revents: 0,
            };
            let ret = unsafe { libc::poll(&mut pfd, 1, 50) };
            if ret > 0 && (pfd.revents & libc::POLLIN) != 0 {
                let mut buf = [0u8; 4096];
                let n = unsafe {
                    libc::read(
                        libc::STDIN_FILENO,
                        buf.as_mut_ptr() as *mut libc::c_void,
                        buf.len(),
                    )
                };
                if n > 0 {
                    let data = &buf[..n as usize];
                    // Check for Ctrl+O to return to Bark.
                    // Traditional encoding: raw byte 0x0F
                    // Kitty keyboard protocol: ESC[111;5u (used by iTerm2 + fish)
                    if data.contains(&0x0F) || data.windows(8).any(|w| w == b"\x1b[111;5u") {
                        break 'shell_loop;
                    }
                    let _ = writer.write_all(data);
                    let _ = writer.flush();
                } else if n == 0 {
                    break; // EOF
                }
                // n < 0: read error, just retry
            }
        }

        #[cfg(not(unix))]
        {
            if crossterm::event::poll(Duration::from_millis(50))? {
                if let Event::Key(key) = event::read()? {
                    let ctrl = key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL);
                    if ctrl && matches!(key.code, crossterm::event::KeyCode::Char('o' | 'O')) {
                        break 'shell_loop;
                    }
                    let bytes: Vec<u8> = match key.code {
                        crossterm::event::KeyCode::Char(c) if ctrl => {
                            vec![(c.to_ascii_lowercase() as u8).wrapping_sub(b'a').wrapping_add(1)]
                        }
                        crossterm::event::KeyCode::Char(c) => {
                            let mut b = [0u8; 4];
                            let s = c.encode_utf8(&mut b);
                            s.as_bytes().to_vec()
                        }
                        crossterm::event::KeyCode::Enter => vec![b'\r'],
                        crossterm::event::KeyCode::Backspace => vec![127],
                        crossterm::event::KeyCode::Tab => vec![b'\t'],
                        crossterm::event::KeyCode::Esc => vec![27],
                        _ => vec![],
                    };
                    if !bytes.is_empty() {
                        let _ = writer.write_all(&bytes);
                        let _ = writer.flush();
                    }
                }
            }
        }
    }

    // Restore terminal settings
    #[cfg(unix)]
    unsafe {
        libc::tcsetattr(libc::STDIN_FILENO, libc::TCSAFLUSH, &orig_termios);
    }
    #[cfg(not(unix))]
    disable_raw_mode()?;

    // Signal thread to stop
    running.store(false, Ordering::Relaxed);

    // Kill child process first to make everything close
    let _ = child.kill();

    // Drop the writer to close the write end of PTY
    drop(writer);

    // Drop the master PTY - this closes the PTY and should unblock the reader thread
    drop(pair.master);

    // Brief pause to let things settle
    std::thread::sleep(Duration::from_millis(100));

    // Non-blocking wait for child (don't block if it hasn't exited yet)
    let _ = child.try_wait();

    // Wait for reader thread to finish so all output is captured
    let _ = stdout_handle.join();

    print!("\r\n");
    let _ = io::stdout().flush();

    // Convert captured bytes into lines for shell history.
    // Unlike single-command capture, we don't filter TUI output here —
    // the interactive shell may use screen clears, alternate buffer, etc.
    // as part of normal operation (e.g., fish shell init).
    let raw = captured.lock().unwrap_or_else(|e| e.into_inner());
    let content = String::from_utf8_lossy(&raw);
    let mut lines = Vec::new();

    for line in content.lines() {
        let line = line.trim_end_matches('\r');

        // Handle carriage returns: keep only content after last \r
        let line = if let Some(pos) = line.rfind('\r') {
            &line[pos + 1..]
        } else {
            line
        };

        let clean = strip_ansi(line);
        if clean.is_empty() {
            continue;
        }

        lines.push(line.to_string());
    }

    Ok(lines)
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
            if let Event::Key(key) = event::read()? {
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
