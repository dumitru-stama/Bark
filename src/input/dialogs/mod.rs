//! Dialog mode handlers
//!
//! Split into focused submodules for maintainability.

mod archive_password;
mod confirm;
mod file_ops;
mod plugin;
mod scp;
mod shell;
mod source;
mod user_menu;
mod viewer_search;

pub use archive_password::handle_archive_password_prompt_mode;
pub use confirm::{handle_confirming_mode, handle_overwrite_confirm_mode, handle_simple_confirm_mode};
pub use file_ops::{handle_find_files_mode, handle_mkdir_mode, handle_select_files_mode};
pub use plugin::handle_plugin_connect_mode;
pub use scp::{handle_scp_connect_mode, handle_scp_password_prompt_mode};
pub use shell::{handle_command_history_mode, handle_shell_mode, handle_shell_history_view};
pub use source::handle_source_selector_mode;
pub use user_menu::{handle_user_menu_mode, handle_user_menu_edit_mode};
pub use viewer_search::handle_viewer_search_mode;
