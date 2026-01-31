//! Archive password prompt dialog handler

use crossterm::event::{KeyCode, KeyEvent};
use crate::input::TextField;
use crate::state::app::App;
use crate::state::mode::Mode;

pub fn handle_archive_password_prompt_mode(app: &mut App, key: KeyEvent) {
    let Mode::ArchivePasswordPrompt {
        password_input,
        cursor_pos,
        focus,
        error,
        ..
    } = &mut app.mode
    else {
        return;
    };

    *error = None;

    match key.code {
        KeyCode::Esc => {
            app.ui.input_selected = false;
            app.mode = Mode::Normal;
        }

        KeyCode::Tab => {
            *focus = (*focus + 1) % 3;
            app.ui.input_selected = *focus == 0 && !password_input.is_empty();
        }

        KeyCode::BackTab => {
            *focus = (*focus + 2) % 3;
            app.ui.input_selected = *focus == 0 && !password_input.is_empty();
        }

        KeyCode::Enter => {
            app.ui.input_selected = false;
            match *focus {
                0 | 1 => {
                    app.connect_archive_with_password();
                }
                2 => {
                    app.mode = Mode::Normal;
                }
                _ => {}
            }
        }

        KeyCode::Backspace if *focus == 0 => {
            if app.ui.input_selected && !password_input.is_empty() {
                password_input.clear();
                *cursor_pos = 0;
            } else {
                TextField::backspace(password_input, cursor_pos);
            }
            app.ui.input_selected = false;
        }
        KeyCode::Delete if *focus == 0 => {
            if app.ui.input_selected && !password_input.is_empty() {
                password_input.clear();
                *cursor_pos = 0;
            } else {
                TextField::delete(password_input, *cursor_pos);
            }
            app.ui.input_selected = false;
        }
        KeyCode::Left if *focus == 0 => {
            app.ui.input_selected = false;
            TextField::left(cursor_pos);
        }
        KeyCode::Right if *focus == 0 => {
            app.ui.input_selected = false;
            TextField::right(password_input, cursor_pos);
        }
        KeyCode::Home if *focus == 0 => {
            app.ui.input_selected = false;
            TextField::home(cursor_pos);
        }
        KeyCode::End if *focus == 0 => {
            app.ui.input_selected = false;
            TextField::end(password_input, cursor_pos);
        }
        KeyCode::Char(c) if *focus == 0 => {
            if app.ui.input_selected && !password_input.is_empty() {
                password_input.clear();
                *cursor_pos = 0;
            }
            app.ui.input_selected = false;
            TextField::insert_char(password_input, cursor_pos, c);
        }

        KeyCode::Left if *focus > 0 => {
            *focus -= 1;
        }

        KeyCode::Right if *focus >= 1 && *focus < 2 => {
            *focus += 1;
        }

        _ => {}
    }
}
