//! Generic plugin connection dialog handlers

use crossterm::event::{KeyCode, KeyEvent};
use crate::plugins::provider_api::DialogFieldType;
use crate::state::app::App;
use crate::state::mode::Mode;

/// Check if focus is on a text input field (based on field type)
fn is_text_field(focus: usize, fields: &[crate::plugins::provider_api::DialogField]) -> bool {
    if focus >= fields.len() {
        return false; // It's a button
    }
    matches!(
        fields[focus].field_type,
        DialogFieldType::Text
            | DialogFieldType::Password
            | DialogFieldType::Number
            | DialogFieldType::TextArea
            | DialogFieldType::FilePath
    )
}

/// Get the content length of the field at the given focus
fn field_len(focus: usize, values: &[String], fields: &[crate::plugins::provider_api::DialogField]) -> usize {
    if focus >= fields.len() {
        return 0;
    }
    values.get(focus).map(|s| s.len()).unwrap_or(0)
}

/// Check if focus is on a checkbox field
fn is_checkbox(focus: usize, fields: &[crate::plugins::provider_api::DialogField]) -> bool {
    if focus >= fields.len() {
        return false;
    }
    matches!(fields[focus].field_type, DialogFieldType::Checkbox)
}

/// Count the total number of focusable elements (fields + 3 buttons: Connect, Save, Cancel)
fn total_focus_elements(fields: &[crate::plugins::provider_api::DialogField]) -> usize {
    fields.len() + 3
}

pub fn handle_plugin_connect_mode(app: &mut App, key: KeyEvent) {
    let Mode::PluginConnect {
        fields,
        values,
        cursors,
        focus,
        error,
        ..
    } = &mut app.mode
    else {
        return;
    };

    *error = None;
    let num_fields = fields.len();
    let total_elements = total_focus_elements(fields);

    match key.code {
        KeyCode::Esc => {
            app.ui.input_selected = false;
            app.mode = Mode::Normal;
        }

        KeyCode::Tab => {
            *focus = (*focus + 1) % total_elements;
            let len = field_len(*focus, values, fields);
            app.ui.input_selected = is_text_field(*focus, fields) && len > 0;
        }

        KeyCode::BackTab => {
            *focus = (*focus + total_elements - 1) % total_elements;
            let len = field_len(*focus, values, fields);
            app.ui.input_selected = is_text_field(*focus, fields) && len > 0;
        }

        KeyCode::Enter => {
            app.ui.input_selected = false;
            if *focus < num_fields {
                // Focus on a field - move to Connect button or trigger connect
                *focus = num_fields; // Move to Connect button
            } else if *focus == num_fields {
                // Connect button
                app.connect_plugin();
            } else if *focus == num_fields + 1 {
                // Save button
                app.save_plugin_connection();
            } else {
                // Cancel button
                app.mode = Mode::Normal;
            }
        }

        // Space to toggle checkboxes
        KeyCode::Char(' ') if is_checkbox(*focus, fields) => {
            if let Some(value) = values.get_mut(*focus) {
                *value = if value == "true" { "false".to_string() } else { "true".to_string() };
            }
        }

        KeyCode::Backspace => {
            if *focus < num_fields && is_text_field(*focus, fields) {
                if app.ui.input_selected {
                    // Clear the field when text is selected
                    if let Some(value) = values.get_mut(*focus) {
                        value.clear();
                    }
                    if let Some(cursor) = cursors.get_mut(*focus) {
                        *cursor = 0;
                    }
                } else if let (Some(value), Some(cursor)) = (values.get_mut(*focus), cursors.get_mut(*focus))
                    && *cursor > 0
                {
                    value.remove(*cursor - 1);
                    *cursor -= 1;
                }
            }
            app.ui.input_selected = false;
        }

        KeyCode::Delete => {
            if *focus < num_fields && is_text_field(*focus, fields) {
                if app.ui.input_selected {
                    // Clear the field when text is selected
                    if let Some(value) = values.get_mut(*focus) {
                        value.clear();
                    }
                    if let Some(cursor) = cursors.get_mut(*focus) {
                        *cursor = 0;
                    }
                } else if let (Some(value), Some(cursor)) = (values.get_mut(*focus), cursors.get_mut(*focus))
                    && *cursor < value.len()
                {
                    value.remove(*cursor);
                }
            }
            app.ui.input_selected = false;
        }

        KeyCode::Left => {
            app.ui.input_selected = false;
            if *focus < num_fields && is_text_field(*focus, fields)
                && let Some(cursor) = cursors.get_mut(*focus)
                && *cursor > 0
            {
                *cursor -= 1;
            } else if *focus >= num_fields && *focus > num_fields {
                // Navigate between buttons
                *focus -= 1;
            }
        }

        KeyCode::Right => {
            app.ui.input_selected = false;
            if *focus < num_fields && is_text_field(*focus, fields)
                && let (Some(value), Some(cursor)) = (values.get(*focus), cursors.get_mut(*focus))
                && *cursor < value.len()
            {
                *cursor += 1;
            } else if *focus >= num_fields && *focus < total_elements - 1 {
                // Navigate between buttons
                *focus += 1;
            }
        }

        KeyCode::Home => {
            app.ui.input_selected = false;
            if *focus < num_fields && is_text_field(*focus, fields)
                && let Some(cursor) = cursors.get_mut(*focus)
            {
                *cursor = 0;
            }
        }

        KeyCode::End => {
            app.ui.input_selected = false;
            if *focus < num_fields && is_text_field(*focus, fields)
                && let (Some(value), Some(cursor)) = (values.get(*focus), cursors.get_mut(*focus))
            {
                *cursor = value.len();
            }
        }

        KeyCode::Up => {
            let old_focus = *focus;
            if *focus > 0 {
                *focus -= 1;
            }
            if old_focus != *focus {
                let len = field_len(*focus, values, fields);
                app.ui.input_selected = is_text_field(*focus, fields) && len > 0;
            }
        }

        KeyCode::Down => {
            let old_focus = *focus;
            if *focus < total_elements - 1 {
                *focus += 1;
            }
            if old_focus != *focus {
                let len = field_len(*focus, values, fields);
                app.ui.input_selected = is_text_field(*focus, fields) && len > 0;
            }
        }

        KeyCode::Char(c) => {
            if *focus < num_fields && is_text_field(*focus, fields) {
                // Check if this is a Number field - only allow digits
                let allow_char = if let Some(field) = fields.get(*focus) {
                    match field.field_type {
                        DialogFieldType::Number => c.is_ascii_digit(),
                        _ => true,
                    }
                } else {
                    true
                };

                if allow_char {
                    // Clear field if text is selected
                    if app.ui.input_selected {
                        if let Some(value) = values.get_mut(*focus) {
                            value.clear();
                        }
                        if let Some(cursor) = cursors.get_mut(*focus) {
                            *cursor = 0;
                        }
                        app.ui.input_selected = false;
                    }

                    // Insert character
                    if let (Some(value), Some(cursor)) = (values.get_mut(*focus), cursors.get_mut(*focus)) {
                        value.insert(*cursor, c);
                        *cursor += 1;
                    }
                }
            }
        }

        _ => {}
    }
}
