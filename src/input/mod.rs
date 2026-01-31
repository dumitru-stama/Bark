//! Input handling
//!
//! This module handles keyboard input dispatching based on the current application mode.

mod normal;
mod viewing;
mod dialogs;
mod text_field;

pub use text_field::TextField;
pub use normal::shell_escape;

pub use viewing::get_help_text;

use crossterm::event::{KeyCode, KeyEvent};

use crate::state::app::App;
use crate::state::mode::Mode;

/// Handle a key event based on current mode
pub fn handle_key(app: &mut App, key: KeyEvent) {
    match &app.mode {
        Mode::Normal => normal::handle_normal_mode(app, key),
        Mode::Viewing { .. } => {
            let height = app.ui.viewer_height;
            viewing::handle_viewing_mode(app, key, height);
        }
        Mode::ViewingPlugin { .. } => {
            let height = app.ui.viewer_height;
            viewing::handle_plugin_viewing_mode(app, key, height);
        }
        Mode::ViewerPluginMenu { .. } => {
            viewing::handle_viewer_plugin_menu(app, key);
        }
        Mode::ViewerSearch { .. } => {
            dialogs::handle_viewer_search_mode(app, key);
        }
        Mode::Help { .. } => {
            let height = app.ui.viewer_height;
            viewing::handle_help_mode(app, key, height);
        }
        Mode::Editing { .. } => {} // Handled in main loop
        Mode::RunningCommand { .. } => {} // Handled in main loop
        Mode::ShellVisible => dialogs::handle_shell_mode(app, key),
        Mode::Confirming { .. } => dialogs::handle_confirming_mode(app, key),
        Mode::SimpleConfirm { .. } => dialogs::handle_simple_confirm_mode(app, key),
        Mode::ScpPasswordPrompt { .. } => dialogs::handle_scp_password_prompt_mode(app, key),
        Mode::SourceSelector { .. } => dialogs::handle_source_selector_mode(app, key),
        Mode::MakingDir { .. } => dialogs::handle_mkdir_mode(app, key),
        Mode::CommandHistory { .. } => dialogs::handle_command_history_mode(app, key),
        Mode::FindFiles { .. } => dialogs::handle_find_files_mode(app, key),
        Mode::SelectFiles { .. } => dialogs::handle_select_files_mode(app, key),
        Mode::ScpConnect { .. } => dialogs::handle_scp_connect_mode(app, key),
        Mode::PluginConnect { .. } => dialogs::handle_plugin_connect_mode(app, key),
        Mode::UserMenu { .. } => dialogs::handle_user_menu_mode(app, key),
        Mode::UserMenuEdit { .. } => dialogs::handle_user_menu_edit_mode(app, key),
        Mode::ArchivePasswordPrompt { .. } => dialogs::handle_archive_password_prompt_mode(app, key),
        Mode::OverwriteConfirm { .. } => dialogs::handle_overwrite_confirm_mode(app, key),
        Mode::FileOpProgress { .. } => {
            // During file operations, Escape cancels
            if key.code == KeyCode::Esc {
                app.cancel_file_operation();
            }
        }
        Mode::BackgroundTask { .. } => {
            // During background tasks, Escape cancels
            if key.code == KeyCode::Esc {
                app.cancel_background_task();
            }
            // Other keys ignored while task is running
        }
    }
}
