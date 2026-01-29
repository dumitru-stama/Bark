//! File operation dialog handlers (mkdir, find, select)

use std::path::PathBuf;
use crossterm::event::{KeyCode, KeyEvent};
use crate::input::TextField;
use crate::state::app::App;
use crate::state::mode::Mode;

pub fn handle_mkdir_mode(app: &mut App, key: KeyEvent) {
    let Mode::MakingDir { name_input, cursor_pos, focus } = &mut app.mode else {
        return;
    };

    match key.code {
        KeyCode::Esc => {
            app.ui.input_selected = false;
            app.mode = Mode::Normal;
        }

        KeyCode::Tab => {
            *focus = (*focus + 1) % 3;
            // Select text when entering text field (focus 0) with content
            app.ui.input_selected = *focus == 0 && !name_input.is_empty();
        }

        KeyCode::BackTab => {
            *focus = if *focus == 0 { 2 } else { *focus - 1 };
            app.ui.input_selected = *focus == 0 && !name_input.is_empty();
        }

        KeyCode::Enter => {
            app.ui.input_selected = false;
            match *focus {
                0 | 1 => {
                    let name = name_input.clone();
                    app.mode = Mode::Normal;
                    app.create_directory(&name);
                }
                2 => {
                    app.mode = Mode::Normal;
                }
                _ => {}
            }
        }

        KeyCode::Backspace if *focus == 0 => {
            if app.ui.input_selected && !name_input.is_empty() {
                name_input.clear();
                *cursor_pos = 0;
            } else {
                TextField::backspace(name_input, cursor_pos);
            }
            app.ui.input_selected = false;
        }
        KeyCode::Delete if *focus == 0 => {
            if app.ui.input_selected && !name_input.is_empty() {
                name_input.clear();
                *cursor_pos = 0;
            } else {
                TextField::delete(name_input, *cursor_pos);
            }
            app.ui.input_selected = false;
        }
        KeyCode::Left if *focus == 0 => {
            app.ui.input_selected = false;
            TextField::left(cursor_pos);
        }
        KeyCode::Right if *focus == 0 => {
            app.ui.input_selected = false;
            TextField::right(name_input, cursor_pos);
        }
        KeyCode::Home if *focus == 0 => {
            app.ui.input_selected = false;
            TextField::home(cursor_pos);
        }
        KeyCode::End if *focus == 0 => {
            app.ui.input_selected = false;
            TextField::end(name_input, cursor_pos);
        }
        KeyCode::Char(c) if *focus == 0 => {
            if app.ui.input_selected && !name_input.is_empty() {
                name_input.clear();
                *cursor_pos = 0;
            }
            app.ui.input_selected = false;
            TextField::insert_char(name_input, cursor_pos, c);
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

/// Check if focus is on a text field for find files dialog
fn is_find_text_field(focus: usize) -> bool {
    matches!(focus, 0 | 2 | 4)
}

/// Get field length for find files dialog
fn find_field_len(focus: usize, pattern: &str, content: &str, path: &str) -> usize {
    match focus {
        0 => pattern.len(),
        2 => content.len(),
        4 => path.len(),
        _ => 0,
    }
}

pub fn handle_find_files_mode(app: &mut App, key: KeyEvent) {
    let Mode::FindFiles {
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
    } = &mut app.mode
    else {
        return;
    };

    match key.code {
        KeyCode::Esc => {
            app.ui.input_selected = false;
            app.mode = Mode::Normal;
        }

        KeyCode::Tab => {
            *focus = (*focus + 1) % 8;
            let len = find_field_len(*focus, pattern_input, content_input, path_input);
            app.ui.input_selected = is_find_text_field(*focus) && len > 0;
        }

        KeyCode::BackTab => {
            *focus = if *focus == 0 { 7 } else { *focus - 1 };
            let len = find_field_len(*focus, pattern_input, content_input, path_input);
            app.ui.input_selected = is_find_text_field(*focus) && len > 0;
        }

        KeyCode::Char(' ') if *focus == 1 => {
            *pattern_case_sensitive = !*pattern_case_sensitive;
        }
        KeyCode::Char(' ') if *focus == 3 => {
            *content_case_sensitive = !*content_case_sensitive;
        }
        KeyCode::Char(' ') if *focus == 5 => {
            *recursive = !*recursive;
        }

        KeyCode::Enter => {
            app.ui.input_selected = false;
            match *focus {
                0..=6 => {
                    let pattern = pattern_input.clone();
                    let pat_case = *pattern_case_sensitive;
                    let content = content_input.clone();
                    let cont_case = *content_case_sensitive;
                    let path = PathBuf::from(path_input.clone());
                    let rec = *recursive;
                    app.mode = Mode::Normal;
                    app.execute_find_files(&pattern, pat_case, &content, cont_case, &path, rec);
                }
                7 => {
                    app.mode = Mode::Normal;
                }
                _ => {}
            }
        }

        // Pattern input (focus == 0)
        KeyCode::Backspace if *focus == 0 => {
            if app.ui.input_selected && !pattern_input.is_empty() {
                pattern_input.clear();
                *pattern_cursor = 0;
            } else {
                TextField::backspace(pattern_input, pattern_cursor);
            }
            app.ui.input_selected = false;
        }
        KeyCode::Delete if *focus == 0 => {
            if app.ui.input_selected && !pattern_input.is_empty() {
                pattern_input.clear();
                *pattern_cursor = 0;
            } else {
                TextField::delete(pattern_input, *pattern_cursor);
            }
            app.ui.input_selected = false;
        }
        KeyCode::Left if *focus == 0 => {
            app.ui.input_selected = false;
            TextField::left(pattern_cursor);
        }
        KeyCode::Right if *focus == 0 => {
            app.ui.input_selected = false;
            TextField::right(pattern_input, pattern_cursor);
        }
        KeyCode::Home if *focus == 0 => {
            app.ui.input_selected = false;
            TextField::home(pattern_cursor);
        }
        KeyCode::End if *focus == 0 => {
            app.ui.input_selected = false;
            TextField::end(pattern_input, pattern_cursor);
        }
        KeyCode::Char(c) if *focus == 0 => {
            if app.ui.input_selected && !pattern_input.is_empty() {
                pattern_input.clear();
                *pattern_cursor = 0;
            }
            app.ui.input_selected = false;
            TextField::insert_char(pattern_input, pattern_cursor, c);
        }

        // Content input (focus == 2)
        KeyCode::Backspace if *focus == 2 => {
            if app.ui.input_selected && !content_input.is_empty() {
                content_input.clear();
                *content_cursor = 0;
            } else {
                TextField::backspace(content_input, content_cursor);
            }
            app.ui.input_selected = false;
        }
        KeyCode::Delete if *focus == 2 => {
            if app.ui.input_selected && !content_input.is_empty() {
                content_input.clear();
                *content_cursor = 0;
            } else {
                TextField::delete(content_input, *content_cursor);
            }
            app.ui.input_selected = false;
        }
        KeyCode::Left if *focus == 2 => {
            app.ui.input_selected = false;
            TextField::left(content_cursor);
        }
        KeyCode::Right if *focus == 2 => {
            app.ui.input_selected = false;
            TextField::right(content_input, content_cursor);
        }
        KeyCode::Home if *focus == 2 => {
            app.ui.input_selected = false;
            TextField::home(content_cursor);
        }
        KeyCode::End if *focus == 2 => {
            app.ui.input_selected = false;
            TextField::end(content_input, content_cursor);
        }
        KeyCode::Char(c) if *focus == 2 => {
            if app.ui.input_selected && !content_input.is_empty() {
                content_input.clear();
                *content_cursor = 0;
            }
            app.ui.input_selected = false;
            TextField::insert_char(content_input, content_cursor, c);
        }

        // Path input (focus == 4)
        KeyCode::Backspace if *focus == 4 => {
            if app.ui.input_selected && !path_input.is_empty() {
                path_input.clear();
                *path_cursor = 0;
            } else {
                TextField::backspace(path_input, path_cursor);
            }
            app.ui.input_selected = false;
        }
        KeyCode::Delete if *focus == 4 => {
            if app.ui.input_selected && !path_input.is_empty() {
                path_input.clear();
                *path_cursor = 0;
            } else {
                TextField::delete(path_input, *path_cursor);
            }
            app.ui.input_selected = false;
        }
        KeyCode::Left if *focus == 4 => {
            app.ui.input_selected = false;
            TextField::left(path_cursor);
        }
        KeyCode::Right if *focus == 4 => {
            app.ui.input_selected = false;
            TextField::right(path_input, path_cursor);
        }
        KeyCode::Home if *focus == 4 => {
            app.ui.input_selected = false;
            TextField::home(path_cursor);
        }
        KeyCode::End if *focus == 4 => {
            app.ui.input_selected = false;
            TextField::end(path_input, path_cursor);
        }
        KeyCode::Char(c) if *focus == 4 => {
            if app.ui.input_selected && !path_input.is_empty() {
                path_input.clear();
                *path_cursor = 0;
            }
            app.ui.input_selected = false;
            TextField::insert_char(path_input, path_cursor, c);
        }

        KeyCode::Left if *focus >= 6 => {
            *focus -= 1;
        }
        KeyCode::Right if *focus == 6 => {
            *focus = 7;
        }
        KeyCode::Up if *focus >= 6 => {
            *focus = 5;
            let len = find_field_len(*focus, pattern_input, content_input, path_input);
            app.ui.input_selected = is_find_text_field(*focus) && len > 0;
        }
        KeyCode::Down if *focus == 5 => {
            *focus = 6;
            app.ui.input_selected = false;
        }

        _ => {}
    }
}

pub fn handle_select_files_mode(app: &mut App, key: KeyEvent) {
    let Mode::SelectFiles {
        pattern_input,
        pattern_cursor,
        include_dirs,
        focus,
    } = &mut app.mode
    else {
        return;
    };

    match key.code {
        KeyCode::Esc => {
            app.ui.input_selected = false;
            app.mode = Mode::Normal;
        }

        KeyCode::Tab => {
            *focus = (*focus + 1) % 4;
            // Select text when entering text field (focus 0) with content
            app.ui.input_selected = *focus == 0 && !pattern_input.is_empty();
        }

        KeyCode::BackTab => {
            *focus = if *focus == 0 { 3 } else { *focus - 1 };
            app.ui.input_selected = *focus == 0 && !pattern_input.is_empty();
        }

        KeyCode::Char(' ') if *focus == 1 => {
            *include_dirs = !*include_dirs;
        }

        KeyCode::Enter => {
            app.ui.input_selected = false;
            match *focus {
                0..=2 => {
                    let pattern = pattern_input.clone();
                    let dirs = *include_dirs;
                    app.mode = Mode::Normal;
                    app.execute_select_files(&pattern, dirs);
                }
                3 => {
                    app.mode = Mode::Normal;
                }
                _ => {}
            }
        }

        KeyCode::Backspace if *focus == 0 => {
            if app.ui.input_selected && !pattern_input.is_empty() {
                pattern_input.clear();
                *pattern_cursor = 0;
            } else {
                TextField::backspace(pattern_input, pattern_cursor);
            }
            app.ui.input_selected = false;
        }
        KeyCode::Delete if *focus == 0 => {
            if app.ui.input_selected && !pattern_input.is_empty() {
                pattern_input.clear();
                *pattern_cursor = 0;
            } else {
                TextField::delete(pattern_input, *pattern_cursor);
            }
            app.ui.input_selected = false;
        }
        KeyCode::Left if *focus == 0 => {
            app.ui.input_selected = false;
            TextField::left(pattern_cursor);
        }
        KeyCode::Right if *focus == 0 => {
            app.ui.input_selected = false;
            TextField::right(pattern_input, pattern_cursor);
        }
        KeyCode::Home if *focus == 0 => {
            app.ui.input_selected = false;
            TextField::home(pattern_cursor);
        }
        KeyCode::End if *focus == 0 => {
            app.ui.input_selected = false;
            TextField::end(pattern_input, pattern_cursor);
        }
        KeyCode::Char(c) if *focus == 0 => {
            if app.ui.input_selected && !pattern_input.is_empty() {
                pattern_input.clear();
                *pattern_cursor = 0;
            }
            app.ui.input_selected = false;
            TextField::insert_char(pattern_input, pattern_cursor, c);
        }

        KeyCode::Left if *focus == 3 => {
            *focus = 2;
        }
        KeyCode::Right if *focus == 2 => {
            *focus = 3;
        }
        KeyCode::Up if *focus >= 2 => {
            *focus = 1;
        }
        KeyCode::Down if *focus == 1 => {
            *focus = 2;
            app.ui.input_selected = false;
        }
        KeyCode::Up if *focus == 1 => {
            *focus = 0;
            app.ui.input_selected = !pattern_input.is_empty();
        }
        KeyCode::Down if *focus == 0 => {
            *focus = 1;
            app.ui.input_selected = false;
        }

        _ => {}
    }
}
