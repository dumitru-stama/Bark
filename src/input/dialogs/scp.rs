//! SCP connection dialog handlers

use crossterm::event::{KeyCode, KeyEvent};
use crate::input::TextField;
use crate::state::app::App;
use crate::state::mode::Mode;

/// Check if focus is on a text input field for SCP connect dialog
fn is_scp_text_field(focus: usize) -> bool {
    focus <= 5
}

/// Get field length for SCP connect dialog
fn scp_field_len(focus: usize, name: &str, user: &str, host: &str, port: &str, path: &str, password: &str) -> usize {
    match focus {
        0 => name.len(),
        1 => user.len(),
        2 => host.len(),
        3 => port.len(),
        4 => path.len(),
        5 => password.len(),
        _ => 0,
    }
}

pub fn handle_scp_password_prompt_mode(app: &mut App, key: KeyEvent) {
    let Mode::ScpPasswordPrompt {
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
            // Select text when entering password field (focus 0) with content
            app.ui.input_selected = *focus == 0 && !password_input.is_empty();
        }

        KeyCode::BackTab => {
            *focus = (*focus + 2) % 3;
            // Select text when entering password field (focus 0) with content
            app.ui.input_selected = *focus == 0 && !password_input.is_empty();
        }

        KeyCode::Enter => {
            app.ui.input_selected = false;
            match *focus {
                0 | 1 => {
                    app.connect_scp_with_password();
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

pub fn handle_scp_connect_mode(app: &mut App, key: KeyEvent) {
    let Mode::ScpConnect {
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
            *focus = (*focus + 1) % 9;
            let len = scp_field_len(*focus, name_input, user_input, host_input, port_input, path_input, password_input);
            app.ui.input_selected = is_scp_text_field(*focus) && len > 0;
        }

        KeyCode::BackTab => {
            *focus = (*focus + 8) % 9;
            let len = scp_field_len(*focus, name_input, user_input, host_input, port_input, path_input, password_input);
            app.ui.input_selected = is_scp_text_field(*focus) && len > 0;
        }

        KeyCode::Enter => {
            app.ui.input_selected = false;
            match *focus {
                0..=6 => {
                    app.connect_scp();
                }
                7 => {
                    app.save_scp_connection();
                }
                8 => {
                    app.mode = Mode::Normal;
                }
                _ => {}
            }
        }

        KeyCode::Backspace => {
            if app.ui.input_selected {
                // Clear the focused field when text is selected
                match *focus {
                    0 if !name_input.is_empty() => { name_input.clear(); *name_cursor = 0; }
                    1 if !user_input.is_empty() => { user_input.clear(); *user_cursor = 0; }
                    2 if !host_input.is_empty() => { host_input.clear(); *host_cursor = 0; }
                    3 if !port_input.is_empty() => { port_input.clear(); *port_cursor = 0; }
                    4 if !path_input.is_empty() => { path_input.clear(); *path_cursor = 0; }
                    5 if !password_input.is_empty() => { password_input.clear(); *password_cursor = 0; }
                    _ => {}
                }
            } else {
                match *focus {
                    0 if *name_cursor > 0 => {
                        name_input.remove(*name_cursor - 1);
                        *name_cursor -= 1;
                    }
                    1 if *user_cursor > 0 => {
                        user_input.remove(*user_cursor - 1);
                        *user_cursor -= 1;
                    }
                    2 if *host_cursor > 0 => {
                        host_input.remove(*host_cursor - 1);
                        *host_cursor -= 1;
                    }
                    3 if *port_cursor > 0 => {
                        port_input.remove(*port_cursor - 1);
                        *port_cursor -= 1;
                    }
                    4 if *path_cursor > 0 => {
                        path_input.remove(*path_cursor - 1);
                        *path_cursor -= 1;
                    }
                    5 if *password_cursor > 0 => {
                        password_input.remove(*password_cursor - 1);
                        *password_cursor -= 1;
                    }
                    _ => {}
                }
            }
            app.ui.input_selected = false;
        }

        KeyCode::Delete => {
            if app.ui.input_selected {
                // Clear the focused field when text is selected
                match *focus {
                    0 if !name_input.is_empty() => { name_input.clear(); *name_cursor = 0; }
                    1 if !user_input.is_empty() => { user_input.clear(); *user_cursor = 0; }
                    2 if !host_input.is_empty() => { host_input.clear(); *host_cursor = 0; }
                    3 if !port_input.is_empty() => { port_input.clear(); *port_cursor = 0; }
                    4 if !path_input.is_empty() => { path_input.clear(); *path_cursor = 0; }
                    5 if !password_input.is_empty() => { password_input.clear(); *password_cursor = 0; }
                    _ => {}
                }
            } else {
                match *focus {
                    0 if *name_cursor < name_input.len() => {
                        name_input.remove(*name_cursor);
                    }
                    1 if *user_cursor < user_input.len() => {
                        user_input.remove(*user_cursor);
                    }
                    2 if *host_cursor < host_input.len() => {
                        host_input.remove(*host_cursor);
                    }
                    3 if *port_cursor < port_input.len() => {
                        port_input.remove(*port_cursor);
                    }
                    4 if *path_cursor < path_input.len() => {
                        path_input.remove(*path_cursor);
                    }
                    5 if *password_cursor < password_input.len() => {
                        password_input.remove(*password_cursor);
                    }
                    _ => {}
                }
            }
            app.ui.input_selected = false;
        }

        KeyCode::Left => {
            app.ui.input_selected = false;
            match *focus {
                0 if *name_cursor > 0 => *name_cursor -= 1,
                1 if *user_cursor > 0 => *user_cursor -= 1,
                2 if *host_cursor > 0 => *host_cursor -= 1,
                3 if *port_cursor > 0 => *port_cursor -= 1,
                4 if *path_cursor > 0 => *path_cursor -= 1,
                5 if *password_cursor > 0 => *password_cursor -= 1,
                6..=8 if *focus > 6 => *focus -= 1,
                _ => {}
            }
        }

        KeyCode::Right => {
            app.ui.input_selected = false;
            match *focus {
                0 if *name_cursor < name_input.len() => *name_cursor += 1,
                1 if *user_cursor < user_input.len() => *user_cursor += 1,
                2 if *host_cursor < host_input.len() => *host_cursor += 1,
                3 if *port_cursor < port_input.len() => *port_cursor += 1,
                4 if *path_cursor < path_input.len() => *path_cursor += 1,
                5 if *password_cursor < password_input.len() => *password_cursor += 1,
                6..=7 => *focus += 1,
                _ => {}
            }
        }

        KeyCode::Home => {
            app.ui.input_selected = false;
            match *focus {
                0 => *name_cursor = 0,
                1 => *user_cursor = 0,
                2 => *host_cursor = 0,
                3 => *port_cursor = 0,
                4 => *path_cursor = 0,
                5 => *password_cursor = 0,
                _ => {}
            }
        }

        KeyCode::End => {
            app.ui.input_selected = false;
            match *focus {
                0 => *name_cursor = name_input.len(),
                1 => *user_cursor = user_input.len(),
                2 => *host_cursor = host_input.len(),
                3 => *port_cursor = port_input.len(),
                4 => *path_cursor = path_input.len(),
                5 => *password_cursor = password_input.len(),
                _ => {}
            }
        }

        KeyCode::Up => {
            let old_focus = *focus;
            if *focus > 0 {
                *focus -= 1;
            }
            if old_focus != *focus {
                let len = scp_field_len(*focus, name_input, user_input, host_input, port_input, path_input, password_input);
                app.ui.input_selected = is_scp_text_field(*focus) && len > 0;
            }
        }

        KeyCode::Down => {
            let old_focus = *focus;
            if *focus < 8 {
                *focus += 1;
            }
            if old_focus != *focus {
                let len = scp_field_len(*focus, name_input, user_input, host_input, port_input, path_input, password_input);
                app.ui.input_selected = is_scp_text_field(*focus) && len > 0;
            }
        }

        KeyCode::Char(c) => {
            // If text is selected and field has content, clear it first
            if app.ui.input_selected {
                match *focus {
                    0 if !name_input.is_empty() => {
                        name_input.clear();
                        *name_cursor = 0;
                    }
                    1 if !user_input.is_empty() => {
                        user_input.clear();
                        *user_cursor = 0;
                    }
                    2 if !host_input.is_empty() => {
                        host_input.clear();
                        *host_cursor = 0;
                    }
                    3 if !port_input.is_empty() => {
                        port_input.clear();
                        *port_cursor = 0;
                    }
                    4 if !path_input.is_empty() => {
                        path_input.clear();
                        *path_cursor = 0;
                    }
                    5 if !password_input.is_empty() => {
                        password_input.clear();
                        *password_cursor = 0;
                    }
                    _ => {}
                }
                app.ui.input_selected = false;
            }

            match *focus {
                0 => {
                    name_input.insert(*name_cursor, c);
                    *name_cursor += 1;
                }
                1 => {
                    user_input.insert(*user_cursor, c);
                    *user_cursor += 1;
                }
                2 => {
                    host_input.insert(*host_cursor, c);
                    *host_cursor += 1;
                }
                3 => {
                    if c.is_ascii_digit() {
                        port_input.insert(*port_cursor, c);
                        *port_cursor += 1;
                    }
                }
                4 => {
                    path_input.insert(*path_cursor, c);
                    *path_cursor += 1;
                }
                5 => {
                    password_input.insert(*password_cursor, c);
                    *password_cursor += 1;
                }
                _ => {}
            }
        }

        _ => {}
    }
}
