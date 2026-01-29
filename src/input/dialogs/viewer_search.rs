//! Viewer search dialog handler

use crossterm::event::{KeyCode, KeyEvent};
use crate::input::TextField;
use crate::state::app::App;
use crate::state::mode::Mode;

/// Check if focus is on a text field for viewer search dialog
fn is_search_text_field(focus: usize) -> bool {
    matches!(focus, 0 | 2)
}

/// Get field length for viewer search dialog
fn search_field_len(focus: usize, text: &str, hex: &str) -> usize {
    match focus {
        0 => text.len(),
        2 => hex.len(),
        _ => 0,
    }
}

pub fn handle_viewer_search_mode(app: &mut App, key: KeyEvent) {
    let Mode::ViewerSearch {
        text_input,
        text_cursor,
        case_sensitive,
        hex_input,
        hex_cursor,
        focus,
        ..
    } = &mut app.mode
    else {
        return;
    };

    match key.code {
        KeyCode::Esc => {
            app.ui.input_selected = false;
            app.cancel_viewer_search();
        }

        KeyCode::Tab => {
            *focus = (*focus + 1) % 5;
            let len = search_field_len(*focus, text_input, hex_input);
            app.ui.input_selected = is_search_text_field(*focus) && len > 0;
        }

        KeyCode::BackTab => {
            *focus = if *focus == 0 { 4 } else { *focus - 1 };
            let len = search_field_len(*focus, text_input, hex_input);
            app.ui.input_selected = is_search_text_field(*focus) && len > 0;
        }

        KeyCode::Char(' ') if *focus == 1 => {
            *case_sensitive = !*case_sensitive;
        }

        KeyCode::Enter => {
            app.ui.input_selected = false;
            match *focus {
                0..=3 => {
                    app.execute_viewer_search();
                }
                4 => {
                    app.cancel_viewer_search();
                }
                _ => {}
            }
        }

        // Text input (focus == 0)
        KeyCode::Backspace if *focus == 0 => {
            if app.ui.input_selected && !text_input.is_empty() {
                text_input.clear();
                *text_cursor = 0;
            } else {
                TextField::backspace(text_input, text_cursor);
            }
            app.ui.input_selected = false;
        }
        KeyCode::Delete if *focus == 0 => {
            if app.ui.input_selected && !text_input.is_empty() {
                text_input.clear();
                *text_cursor = 0;
            } else {
                TextField::delete(text_input, *text_cursor);
            }
            app.ui.input_selected = false;
        }
        KeyCode::Left if *focus == 0 => {
            app.ui.input_selected = false;
            TextField::left(text_cursor);
        }
        KeyCode::Right if *focus == 0 => {
            app.ui.input_selected = false;
            TextField::right(text_input, text_cursor);
        }
        KeyCode::Home if *focus == 0 => {
            app.ui.input_selected = false;
            TextField::home(text_cursor);
        }
        KeyCode::End if *focus == 0 => {
            app.ui.input_selected = false;
            TextField::end(text_input, text_cursor);
        }
        KeyCode::Char(c) if *focus == 0 => {
            if app.ui.input_selected && !text_input.is_empty() {
                text_input.clear();
                *text_cursor = 0;
            }
            app.ui.input_selected = false;
            TextField::insert_char(text_input, text_cursor, c);
        }

        // Hex input (focus == 2)
        KeyCode::Backspace if *focus == 2 => {
            if app.ui.input_selected && !hex_input.is_empty() {
                hex_input.clear();
                *hex_cursor = 0;
            } else {
                TextField::backspace(hex_input, hex_cursor);
            }
            app.ui.input_selected = false;
        }
        KeyCode::Delete if *focus == 2 => {
            if app.ui.input_selected && !hex_input.is_empty() {
                hex_input.clear();
                *hex_cursor = 0;
            } else {
                TextField::delete(hex_input, *hex_cursor);
            }
            app.ui.input_selected = false;
        }
        KeyCode::Left if *focus == 2 => {
            app.ui.input_selected = false;
            TextField::left(hex_cursor);
        }
        KeyCode::Right if *focus == 2 => {
            app.ui.input_selected = false;
            TextField::right(hex_input, hex_cursor);
        }
        KeyCode::Home if *focus == 2 => {
            app.ui.input_selected = false;
            TextField::home(hex_cursor);
        }
        KeyCode::End if *focus == 2 => {
            app.ui.input_selected = false;
            TextField::end(hex_input, hex_cursor);
        }
        KeyCode::Char(c) if *focus == 2 => {
            if app.ui.input_selected && !hex_input.is_empty() {
                hex_input.clear();
                *hex_cursor = 0;
            }
            app.ui.input_selected = false;
            TextField::insert_char_if(hex_input, hex_cursor, c, |c| c.is_ascii_hexdigit() || c == ' ')
        }

        KeyCode::Left if *focus >= 3 => {
            *focus -= 1;
        }
        KeyCode::Right if *focus == 3 => {
            *focus = 4;
        }
        KeyCode::Up if *focus >= 3 => {
            *focus = 2;
            let len = search_field_len(*focus, text_input, hex_input);
            app.ui.input_selected = is_search_text_field(*focus) && len > 0;
        }
        KeyCode::Down if *focus == 2 => {
            *focus = 3;
            app.ui.input_selected = false;
        }

        _ => {}
    }
}
