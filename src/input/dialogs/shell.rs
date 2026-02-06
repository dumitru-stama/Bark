//! Shell and command history dialog handlers

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use crate::state::app::App;
use crate::state::mode::Mode;

pub fn handle_shell_mode(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Char('o') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.mode = Mode::Normal;
        }
        _ => {}
    }
}

pub fn handle_shell_history_view(app: &mut App, key: KeyEvent, content_height: usize) {
    let Mode::ShellHistoryView { scroll } = &mut app.mode else {
        return;
    };

    let max_scroll = app.cmd.output.len().saturating_sub(content_height);

    match key.code {
        KeyCode::Esc => {
            app.mode = Mode::Normal;
        }
        KeyCode::Char('o') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.mode = Mode::Normal;
        }
        KeyCode::Up | KeyCode::Char('k') => {
            if *scroll < max_scroll {
                *scroll += 1;
            }
        }
        KeyCode::Down | KeyCode::Char('j') => {
            *scroll = scroll.saturating_sub(1);
        }
        KeyCode::PageUp => {
            *scroll = (*scroll + content_height).min(max_scroll);
        }
        KeyCode::PageDown => {
            *scroll = scroll.saturating_sub(content_height);
        }
        KeyCode::Home => {
            *scroll = max_scroll;
        }
        KeyCode::End => {
            *scroll = 0;
        }
        _ => {}
    }
}

pub fn handle_command_history_mode(app: &mut App, key: KeyEvent) {
    let Mode::CommandHistory { selected, scroll } = &mut app.mode else {
        return;
    };

    let visible_height = app.ui.terminal_height.saturating_sub(10) as usize;

    match key.code {
        KeyCode::Esc => {
            app.mode = Mode::Normal;
        }

        KeyCode::Up | KeyCode::Char('k') => {
            if *selected > 0 {
                *selected -= 1;
                if *selected < *scroll {
                    *scroll = *selected;
                }
            }
        }

        KeyCode::Down | KeyCode::Char('j') => {
            if *selected + 1 < app.cmd.history.len() {
                *selected += 1;
                if *selected >= *scroll + visible_height {
                    *scroll = selected.saturating_sub(visible_height - 1);
                }
            }
        }

        KeyCode::PageUp => {
            *selected = selected.saturating_sub(visible_height);
            *scroll = scroll.saturating_sub(visible_height);
        }

        KeyCode::PageDown => {
            let max_idx = app.cmd.history.len().saturating_sub(1);
            *selected = (*selected + visible_height).min(max_idx);
            if *selected >= *scroll + visible_height {
                *scroll = selected.saturating_sub(visible_height - 1);
            }
        }

        KeyCode::Home => {
            *selected = 0;
            *scroll = 0;
        }

        KeyCode::End => {
            *selected = app.cmd.history.len().saturating_sub(1);
            *scroll = selected.saturating_sub(visible_height - 1);
        }

        KeyCode::Enter => {
            if let Some(cmd) = app.cmd.history.get(*selected).cloned() {
                app.mode = Mode::Normal;
                app.cmd.set_input(cmd);
                app.cmd.focused = true;
            }
        }

        _ => {}
    }
}
