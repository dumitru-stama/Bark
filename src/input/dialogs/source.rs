//! Source selector dialog handler

use crossterm::event::{KeyCode, KeyEvent};
use crate::state::app::App;
use crate::state::mode::Mode;

pub fn handle_source_selector_mode(app: &mut App, key: KeyEvent) {
    // Allow switching target panel while source selector is open
    let is_left = app.key_matches("drive_left", &key) || app.key_matches("source_left", &key)
        || app.key_matches("source_left_shift", &key) || app.key_matches("source_left_alt", &key);
    let is_right = app.key_matches("drive_right", &key) || app.key_matches("source_right", &key)
        || app.key_matches("source_right_shift", &key) || app.key_matches("source_right_alt", &key);
    if is_left || is_right {
        let target = if is_left { crate::state::Side::Left } else { crate::state::Side::Right };
        app.show_source_selector(target);
        return;
    }

    let Mode::SourceSelector { target_panel, sources, selected } = &mut app.mode else {
        return;
    };

    match key.code {
        KeyCode::Esc => {
            app.mode = Mode::Normal;
        }

        KeyCode::Up | KeyCode::Char('k') => {
            if *selected > 0 {
                *selected -= 1;
            }
        }

        KeyCode::Down | KeyCode::Char('j') => {
            if *selected + 1 < sources.len() {
                *selected += 1;
            }
        }

        KeyCode::Enter => {
            let target = *target_panel;
            let source = sources[*selected].clone();
            app.mode = Mode::Normal;
            app.select_source(target, &source);
        }

        KeyCode::F(4) => {
            use crate::providers::{PanelSource, ProviderType};
            let source = &sources[*selected];
            if let PanelSource::Provider { connection_name, info, .. } = source {
                let name = connection_name.clone();
                let target = *target_panel;
                let provider_type = info.provider_type;
                app.mode = Mode::Normal;
                match provider_type {
                    ProviderType::Scp => app.edit_scp_connection(target, &name),
                    ProviderType::Plugin => {
                        // Edit plugin connection - find the scheme from saved connections
                        if let Some(conn) = app.config.plugin_connections.iter().find(|c| c.name == name) {
                            let scheme = conn.scheme.clone();
                            app.edit_plugin_connection(target, &scheme, &name);
                        }
                    }
                    _ => {}
                }
            }
        }

        KeyCode::F(8) | KeyCode::Delete => {
            use crate::providers::{PanelSource, ProviderType};
            let source = &sources[*selected];
            match source {
                PanelSource::Provider { connection_name, info, .. } => {
                    let name = connection_name.clone();
                    let display_name = info.name.clone();
                    let action = match info.provider_type {
                        ProviderType::Scp => crate::state::mode::SimpleConfirmAction::DeleteConnection { name },
                        ProviderType::Plugin => {
                            // Find the scheme from saved connections
                            let scheme = app.config.plugin_connections.iter()
                                .find(|c| c.name == name)
                                .map(|c| c.scheme.clone())
                                .unwrap_or_default();
                            crate::state::mode::SimpleConfirmAction::DeletePluginConnection { scheme, name }
                        }
                        _ => return, // Don't handle other types
                    };
                    app.mode = Mode::SimpleConfirm {
                        message: format!("Delete connection '{}'?", display_name),
                        action,
                        focus: 1,
                    };
                }
                PanelSource::QuickAccess { name, path, is_favorite: true } => {
                    let path = path.clone();
                    let display_name = name.clone();
                    app.mode = Mode::SimpleConfirm {
                        message: format!("Remove '{}' from favorites?", display_name),
                        action: crate::state::mode::SimpleConfirmAction::DeleteFavorite { path },
                        focus: 1,
                    };
                }
                _ => {
                    // Can't delete built-in items
                }
            }
        }

        KeyCode::Char(c) if c.is_ascii_alphabetic() => {
            use crate::providers::PanelSource;
            let letter = c.to_ascii_uppercase();
            let drive_str = format!("{}:", letter);
            if let Some(idx) = sources.iter().position(|s| {
                matches!(s, PanelSource::Drive { letter: l, .. } if l == &drive_str)
            }) {
                let target = *target_panel;
                let source = sources[idx].clone();
                app.mode = Mode::Normal;
                app.select_source(target, &source);
            }
        }

        _ => {}
    }
}
