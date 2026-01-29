//! User menu dialog handlers

use crossterm::event::{KeyCode, KeyEvent};
use crate::config::UserMenuRule;
use crate::input::TextField;
use crate::state::app::App;
use crate::state::mode::Mode;

/// Handle input in user menu list mode
pub fn handle_user_menu_mode(app: &mut App, key: KeyEvent) {
    let Mode::UserMenu { rules, selected, scroll } = &mut app.mode else {
        return;
    };

    match key.code {
        KeyCode::Esc => {
            app.mode = Mode::Normal;
        }

        KeyCode::Up => {
            if *selected > 0 {
                *selected -= 1;
                // Adjust scroll if needed
                if *selected < *scroll {
                    *scroll = *selected;
                }
            }
        }

        KeyCode::Down => {
            if !rules.is_empty() && *selected < rules.len() - 1 {
                *selected += 1;
                // Scroll down if needed (assuming ~10 visible items)
                let visible = 10;
                if *selected >= *scroll + visible {
                    *scroll = *selected - visible + 1;
                }
            }
        }

        KeyCode::Home => {
            *selected = 0;
            *scroll = 0;
        }

        KeyCode::End => {
            if !rules.is_empty() {
                *selected = rules.len() - 1;
                // Adjust scroll
                let visible = 10;
                if rules.len() > visible {
                    *scroll = rules.len() - visible;
                }
            }
        }

        KeyCode::PageUp => {
            let page_size = 10;
            *selected = selected.saturating_sub(page_size);
            *scroll = scroll.saturating_sub(page_size);
        }

        KeyCode::PageDown => {
            if !rules.is_empty() {
                let page_size = 10;
                *selected = (*selected + page_size).min(rules.len() - 1);
                let visible = 10;
                if *selected >= *scroll + visible {
                    *scroll = *selected - visible + 1;
                }
            }
        }

        KeyCode::Enter => {
            // Execute selected rule
            if !rules.is_empty() {
                let rule = rules[*selected].clone();
                app.mode = Mode::Normal;
                app.execute_user_menu_command(&rule.command);
            }
        }

        // Insert key - add new rule
        KeyCode::Insert => {
            app.mode = Mode::UserMenuEdit {
                editing_index: None,
                name_input: String::new(),
                name_cursor: 0,
                command_input: String::new(),
                command_cursor: 0,
                hotkey_input: String::new(),
                hotkey_cursor: 0,
                focus: 0,
                error: None,
            };
        }

        // F4 - edit selected rule
        KeyCode::F(4) => {
            if !rules.is_empty() {
                let rule = &rules[*selected];
                let sel = *selected;
                // Select text in first field if it has content
                app.ui.input_selected = !rule.name.is_empty();
                app.mode = Mode::UserMenuEdit {
                    editing_index: Some(sel),
                    name_input: rule.name.clone(),
                    name_cursor: rule.name.len(),
                    command_input: rule.command.clone(),
                    command_cursor: rule.command.len(),
                    hotkey_input: rule.hotkey.clone().unwrap_or_default(),
                    hotkey_cursor: rule.hotkey.as_ref().map(|s| s.len()).unwrap_or(0),
                    focus: 0,
                    error: None,
                };
            }
        }

        // F8 or Delete - delete selected rule
        KeyCode::F(8) | KeyCode::Delete => {
            if !rules.is_empty() {
                let sel = *selected;
                if let Err(e) = app.config.remove_user_menu_rule(sel) {
                    app.add_shell_output(format!("Error removing rule: {}", e));
                } else {
                    // Refresh the menu
                    let new_rules = app.config.user_menu.clone();
                    let new_selected = if sel >= new_rules.len() && !new_rules.is_empty() {
                        new_rules.len() - 1
                    } else {
                        sel.min(new_rules.len().saturating_sub(1))
                    };
                    app.mode = Mode::UserMenu {
                        rules: new_rules,
                        selected: new_selected,
                        scroll: 0,
                    };
                }
            }
        }

        // Hotkey - check if pressed character matches a rule's hotkey
        KeyCode::Char(c) => {
            let c_lower = c.to_ascii_lowercase();
            for rule in rules.iter() {
                if let Some(ref hotkey) = rule.hotkey
                    && hotkey.chars().next().map(|h| h.to_ascii_lowercase()) == Some(c_lower) {
                        let command = rule.command.clone();
                        app.mode = Mode::Normal;
                        app.execute_user_menu_command(&command);
                        return;
                    }
            }
        }

        _ => {}
    }
}

/// Check if focus is on a text field for user menu edit dialog
fn is_text_field(focus: usize) -> bool {
    matches!(focus, 0..=2)
}

/// Get field length for user menu edit dialog
fn field_len(focus: usize, name: &str, command: &str, hotkey: &str) -> usize {
    match focus {
        0 => name.len(),
        1 => command.len(),
        2 => hotkey.len(),
        _ => 0,
    }
}

/// Handle input in user menu edit mode
pub fn handle_user_menu_edit_mode(app: &mut App, key: KeyEvent) {
    let Mode::UserMenuEdit {
        editing_index,
        name_input,
        name_cursor,
        command_input,
        command_cursor,
        hotkey_input,
        hotkey_cursor,
        focus,
        error,
    } = &mut app.mode
    else {
        return;
    };

    // Clear error on any input
    *error = None;

    match key.code {
        KeyCode::Esc => {
            app.ui.input_selected = false;
            // Return to user menu
            app.show_user_menu();
        }

        KeyCode::Tab => {
            *focus = (*focus + 1) % 5;
            let len = field_len(*focus, name_input, command_input, hotkey_input);
            app.ui.input_selected = is_text_field(*focus) && len > 0;
        }

        KeyCode::BackTab => {
            *focus = if *focus == 0 { 4 } else { *focus - 1 };
            let len = field_len(*focus, name_input, command_input, hotkey_input);
            app.ui.input_selected = is_text_field(*focus) && len > 0;
        }

        KeyCode::Enter => {
            app.ui.input_selected = false;
            match *focus {
                // Save button or enter on input fields
                0..=3 => {
                    // Validate inputs
                    if name_input.trim().is_empty() {
                        *error = Some("Name cannot be empty".to_string());
                        return;
                    }
                    if command_input.trim().is_empty() {
                        *error = Some("Command cannot be empty".to_string());
                        return;
                    }
                    // Validate hotkey (must be single character or empty)
                    let hotkey = if hotkey_input.trim().is_empty() {
                        None
                    } else {
                        let h = hotkey_input.trim();
                        if h.chars().count() != 1 {
                            *error = Some("Hotkey must be a single character".to_string());
                            return;
                        }
                        Some(h.to_string())
                    };

                    let rule = UserMenuRule {
                        name: name_input.clone(),
                        command: command_input.clone(),
                        hotkey,
                    };
                    let idx = *editing_index;

                    if let Err(e) = app.config.save_user_menu_rule(rule, idx) {
                        *error = Some(format!("Error saving rule: {}", e));
                    } else {
                        // Return to user menu
                        app.show_user_menu();
                    }
                }
                // Cancel button
                4 => {
                    app.show_user_menu();
                }
                _ => {}
            }
        }

        // Name input (focus == 0)
        KeyCode::Backspace if *focus == 0 => {
            if app.ui.input_selected && !name_input.is_empty() {
                name_input.clear();
                *name_cursor = 0;
            } else {
                TextField::backspace(name_input, name_cursor);
            }
            app.ui.input_selected = false;
        }
        KeyCode::Delete if *focus == 0 => {
            if app.ui.input_selected && !name_input.is_empty() {
                name_input.clear();
                *name_cursor = 0;
            } else {
                TextField::delete(name_input, *name_cursor);
            }
            app.ui.input_selected = false;
        }
        KeyCode::Left if *focus == 0 => {
            app.ui.input_selected = false;
            TextField::left(name_cursor);
        }
        KeyCode::Right if *focus == 0 => {
            app.ui.input_selected = false;
            TextField::right(name_input, name_cursor);
        }
        KeyCode::Home if *focus == 0 => {
            app.ui.input_selected = false;
            TextField::home(name_cursor);
        }
        KeyCode::End if *focus == 0 => {
            app.ui.input_selected = false;
            TextField::end(name_input, name_cursor);
        }
        KeyCode::Char(c) if *focus == 0 => {
            if app.ui.input_selected && !name_input.is_empty() {
                name_input.clear();
                *name_cursor = 0;
            }
            app.ui.input_selected = false;
            TextField::insert_char(name_input, name_cursor, c);
        }

        // Command input (focus == 1)
        KeyCode::Backspace if *focus == 1 => {
            if app.ui.input_selected && !command_input.is_empty() {
                command_input.clear();
                *command_cursor = 0;
            } else {
                TextField::backspace(command_input, command_cursor);
            }
            app.ui.input_selected = false;
        }
        KeyCode::Delete if *focus == 1 => {
            if app.ui.input_selected && !command_input.is_empty() {
                command_input.clear();
                *command_cursor = 0;
            } else {
                TextField::delete(command_input, *command_cursor);
            }
            app.ui.input_selected = false;
        }
        KeyCode::Left if *focus == 1 => {
            app.ui.input_selected = false;
            TextField::left(command_cursor);
        }
        KeyCode::Right if *focus == 1 => {
            app.ui.input_selected = false;
            TextField::right(command_input, command_cursor);
        }
        KeyCode::Home if *focus == 1 => {
            app.ui.input_selected = false;
            TextField::home(command_cursor);
        }
        KeyCode::End if *focus == 1 => {
            app.ui.input_selected = false;
            TextField::end(command_input, command_cursor);
        }
        KeyCode::Char(c) if *focus == 1 => {
            if app.ui.input_selected && !command_input.is_empty() {
                command_input.clear();
                *command_cursor = 0;
            }
            app.ui.input_selected = false;
            TextField::insert_char(command_input, command_cursor, c);
        }

        // Hotkey input (focus == 2)
        KeyCode::Backspace if *focus == 2 => {
            if app.ui.input_selected && !hotkey_input.is_empty() {
                hotkey_input.clear();
                *hotkey_cursor = 0;
            } else {
                TextField::backspace(hotkey_input, hotkey_cursor);
            }
            app.ui.input_selected = false;
        }
        KeyCode::Delete if *focus == 2 => {
            if app.ui.input_selected && !hotkey_input.is_empty() {
                hotkey_input.clear();
                *hotkey_cursor = 0;
            } else {
                TextField::delete(hotkey_input, *hotkey_cursor);
            }
            app.ui.input_selected = false;
        }
        KeyCode::Left if *focus == 2 => {
            app.ui.input_selected = false;
            TextField::left(hotkey_cursor);
        }
        KeyCode::Right if *focus == 2 => {
            app.ui.input_selected = false;
            TextField::right(hotkey_input, hotkey_cursor);
        }
        KeyCode::Home if *focus == 2 => {
            app.ui.input_selected = false;
            TextField::home(hotkey_cursor);
        }
        KeyCode::End if *focus == 2 => {
            app.ui.input_selected = false;
            TextField::end(hotkey_input, hotkey_cursor);
        }
        KeyCode::Char(c) if *focus == 2 => {
            if app.ui.input_selected && !hotkey_input.is_empty() {
                hotkey_input.clear();
                *hotkey_cursor = 0;
            }
            app.ui.input_selected = false;
            // Only allow single character for hotkey
            if hotkey_input.is_empty() {
                TextField::insert_char(hotkey_input, hotkey_cursor, c);
            }
        }

        // Arrow navigation for buttons
        KeyCode::Left if *focus >= 3 => {
            if *focus > 3 {
                *focus -= 1;
            }
        }
        KeyCode::Right if *focus == 3 => {
            *focus = 4;
        }
        KeyCode::Up if *focus >= 3 => {
            *focus = 2;
            let len = field_len(*focus, name_input, command_input, hotkey_input);
            app.ui.input_selected = len > 0;
        }
        KeyCode::Down if *focus == 2 => {
            *focus = 3;
            app.ui.input_selected = false;
        }

        _ => {}
    }
}
