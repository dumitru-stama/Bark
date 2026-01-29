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
                let cwd = match app.active_panel {
                    crate::state::Side::Left => app.left_panel.path.clone(),
                    crate::state::Side::Right => app.right_panel.path.clone(),
                };
                app.mode = Mode::RunningCommand { command: cmd, cwd };
            }
        }

        _ => {}
    }
}
