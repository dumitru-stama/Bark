//! UI components

pub mod dialog;
mod dialog_helpers;
pub mod help;
pub mod panel;
pub mod plugin_viewer;
pub mod shell;
pub mod spinner;
pub mod status;
pub mod theme;
pub mod viewer;
pub mod viewer_menu;
pub mod cp437;
pub mod viewer_utils;

pub use dialog::ConfirmDialog;
pub use dialog::DeleteIterativeDialog;
pub use dialog::SimpleConfirmDialog;
pub use dialog::SourceSelector;
pub use dialog::MkdirDialog;
pub use dialog::CommandHistoryDialog;
pub use dialog::FindFilesDialog;
pub use dialog::ViewerSearchDialog;
pub use dialog::SelectFilesDialog;
pub use dialog::ScpConnectDialog;
pub use dialog::ScpPasswordPromptDialog;
pub use dialog::ArchivePasswordPromptDialog;
pub use dialog::UserMenuDialog;
pub use dialog::UserMenuEditDialog;
pub use help::HelpViewer;
pub use panel::PanelWidget;
pub use plugin_viewer::PluginViewer;
pub use shell::ShellArea;
pub use shell::ShellHistoryViewer;
pub use status::StatusBar;
pub use theme::Theme;
pub use theme::ThemeConfig;
pub use viewer::FileViewer;
pub use viewer_menu::ViewerPluginMenu;
pub use dialog::OverwriteConfirmDialog;
pub use spinner::SpinnerDialog;
pub use spinner::FileOpProgressDialog;
#[cfg(not(windows))]
pub use dialog::EditPermissionsDialog;
#[cfg(not(windows))]
pub use dialog::EditOwnerDialog;
