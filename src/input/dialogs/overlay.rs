//! Input handlers for overlay plugin mode

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::state::app::App;
use crate::state::mode::Mode;

/// Handle input in overlay mode (forward keys to plugin)
pub fn handle_overlay_mode(app: &mut App, key: KeyEvent) {
    // Escape always closes the overlay (safety fallback)
    if key.code == KeyCode::Esc {
        app.close_overlay();
        return;
    }

    let (key_str, modifiers) = key_event_to_plugin_key(&key);
    if key_str.is_empty() {
        return;
    }

    let mod_strs: Vec<&str> = modifiers.iter().map(|s| s.as_str()).collect();
    app.overlay_send_key(&key_str, &mod_strs);
}

/// Handle input in overlay selector mode
pub fn handle_overlay_selector_mode(app: &mut App, key: KeyEvent) {
    if let Mode::OverlaySelector { ref plugins, ref mut selected } = app.mode {
        match key.code {
            KeyCode::Esc => {
                app.mode = Mode::Normal;
            }
            KeyCode::Up => {
                if *selected > 0 {
                    *selected -= 1;
                }
            }
            KeyCode::Down => {
                if *selected + 1 < plugins.len() {
                    *selected += 1;
                }
            }
            KeyCode::Enter => {
                let name = plugins[*selected].0.clone();
                app.launch_overlay_plugin(&name);
            }
            _ => {}
        }
    }
}

/// Convert a crossterm KeyEvent to (key_name, modifiers) for the plugin protocol
fn key_event_to_plugin_key(key: &KeyEvent) -> (String, Vec<String>) {
    let mut modifiers = Vec::new();
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        modifiers.push("ctrl".to_string());
    }
    if key.modifiers.contains(KeyModifiers::ALT) {
        modifiers.push("alt".to_string());
    }
    if key.modifiers.contains(KeyModifiers::SHIFT) {
        modifiers.push("shift".to_string());
    }

    let key_str = match key.code {
        KeyCode::Char(c) => c.to_string(),
        KeyCode::Enter => "Enter".to_string(),
        KeyCode::Backspace => "Backspace".to_string(),
        KeyCode::Tab => "Tab".to_string(),
        KeyCode::BackTab => {
            if !modifiers.contains(&"shift".to_string()) {
                modifiers.push("shift".to_string());
            }
            "Tab".to_string()
        }
        KeyCode::Up => "Up".to_string(),
        KeyCode::Down => "Down".to_string(),
        KeyCode::Left => "Left".to_string(),
        KeyCode::Right => "Right".to_string(),
        KeyCode::Home => "Home".to_string(),
        KeyCode::End => "End".to_string(),
        KeyCode::PageUp => "PageUp".to_string(),
        KeyCode::PageDown => "PageDown".to_string(),
        KeyCode::Delete => "Delete".to_string(),
        KeyCode::Insert => "Insert".to_string(),
        KeyCode::F(n) => format!("F{}", n),
        KeyCode::Esc => "Escape".to_string(),
        _ => return (String::new(), modifiers),
    };

    (key_str, modifiers)
}
