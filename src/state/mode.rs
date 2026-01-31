use std::collections::HashSet;
use std::path::PathBuf;
use crate::plugins::provider_api::DialogField;
use crate::providers::PanelSource;
use super::Side;
use crate::utils::calculate_hex_bytes_per_line;

/// Type of file operation for confirmation dialog
#[derive(Clone, Debug)]
pub enum FileOperation {
    Copy,
    Move,
    Delete,
}

/// Action to perform on simple confirmation
#[derive(Clone, Debug)]
#[allow(clippy::enum_variant_names)]
pub enum SimpleConfirmAction {
    /// Delete a saved SCP connection
    DeleteConnection { name: String },
    /// Delete a saved plugin connection
    DeletePluginConnection { scheme: String, name: String },
    /// Delete a favorite path
    DeleteFavorite { path: String },
}

/// Content type for the file viewer
#[derive(Clone, Debug)]
pub enum ViewContent {
    /// Text content (UTF-8) with precomputed line byte offsets
    /// (Vec contains byte offset of each line start; length = line count)
    Text(String, Vec<usize>),
    /// Binary content (for hex dump)
    Binary(Vec<u8>),
    /// Memory-mapped local file (efficient for large files)
    MappedFile {
        /// The memory map (Arc for Clone support)
        mmap: std::sync::Arc<memmap2::Mmap>,
        /// Whether content is valid UTF-8 text
        is_text: bool,
        /// Line byte offsets for text mode (index i = byte offset of line i)
        /// Only populated for text files
        line_offsets: Vec<usize>,
    },
}

impl ViewContent {
    pub fn byte_offset_to_line(&self, offset: usize, binary_mode: BinaryViewMode, term_width: usize) -> usize {
        match self {
            ViewContent::Text(_text, line_offsets) => {
                match binary_mode {
                    BinaryViewMode::Cp437 => {
                        // Binary search on precomputed offsets
                        match line_offsets.binary_search(&offset) {
                            Ok(i) => i,
                            Err(i) => i.saturating_sub(1),
                        }
                    }
                    BinaryViewMode::Hex => {
                        let bytes_per_line = calculate_hex_bytes_per_line(term_width);
                        offset / bytes_per_line
                    }
                }
            }
            ViewContent::Binary(_) => {
                match binary_mode {
                    BinaryViewMode::Hex => {
                        let bytes_per_line = calculate_hex_bytes_per_line(term_width);
                        offset / bytes_per_line
                    }
                    BinaryViewMode::Cp437 => {
                        let bytes_per_line = term_width.max(1);
                        offset / bytes_per_line
                    }
                }
            }
            ViewContent::MappedFile { is_text, line_offsets, .. } => {
                match (*is_text, binary_mode) {
                    (true, BinaryViewMode::Cp437) => {
                        // Text view - binary search through line offsets
                        match line_offsets.binary_search(&offset) {
                            Ok(line) => line,
                            Err(line) => line.saturating_sub(1),
                        }
                    }
                    (_, BinaryViewMode::Hex) => {
                        let bytes_per_line = calculate_hex_bytes_per_line(term_width);
                        offset / bytes_per_line
                    }
                    (false, BinaryViewMode::Cp437) => {
                        let bytes_per_line = term_width.max(1);
                        offset / bytes_per_line
                    }
                }
            }
        }
    }
}

/// Binary view mode
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinaryViewMode {
    /// Hex dump with offset, hex bytes, and CP437 ASCII
    Hex,
    /// CP437 text only
    Cp437,
}

/// Application mode
#[derive(Clone, Debug)]
pub enum Mode {
    /// Normal file browsing
    Normal,
    /// Viewing a file (F3)
    Viewing {
        content: ViewContent,
        scroll: usize,
        path: std::path::PathBuf,
        /// Binary view mode (hex or CP437 text)
        binary_mode: BinaryViewMode,
        /// Search matches: (byte_offset, length)
        search_matches: Vec<(usize, usize)>,
        /// Current match index (None if no matches or search not active)
        current_match: Option<usize>,
    },
    /// Viewing a file via plugin
    ViewingPlugin {
        /// Plugin name
        plugin_name: String,
        /// Path being viewed
        path: std::path::PathBuf,
        /// Current scroll offset
        scroll: usize,
        /// Cached rendered lines
        lines: Vec<String>,
        /// Total number of lines
        total_lines: usize,
        /// Transient status message (e.g. "Saved to …"), cleared on next key
        status_message: Option<String>,
    },
    /// Plugin selection menu in viewer (F2)
    ViewerPluginMenu {
        /// Path being viewed
        path: std::path::PathBuf,
        /// Original content (to return to built-in viewer)
        content: ViewContent,
        /// Original binary mode
        binary_mode: BinaryViewMode,
        /// Original scroll position
        original_scroll: usize,
        /// List of available plugins (name, can_handle)
        plugins: Vec<(String, bool)>,
        /// Currently selected index (0 = built-in viewer)
        selected: usize,
    },
    /// Showing help (F1)
    Help {
        scroll: usize,
    },
    /// Editing a file (F4) - signals main loop to launch editor
    Editing {
        /// Local path (temp file for remote, actual file for local)
        path: std::path::PathBuf,
        /// If editing a remote file: (panel_side, remote_path)
        remote_info: Option<(Side, String)>,
    },
    /// Running a command - signals main loop to execute
    RunningCommand {
        command: String,
        cwd: std::path::PathBuf,
    },
    /// Shell visible (Ctrl+O)
    ShellVisible,
    /// Confirmation dialog for file operations
    Confirming {
        operation: FileOperation,
        sources: Vec<PathBuf>,
        dest_input: String,
        cursor_pos: usize,
        /// Focused element: 0 = input field (copy/move only), 1 = OK/Delete, 2 = Cancel
        focus: usize,
    },
    /// Source selection dialog (drives on Windows, quick access + remote connections on all platforms)
    SourceSelector {
        /// Which panel to change (Left or Right)
        target_panel: Side,
        /// List of available sources
        sources: Vec<PanelSource>,
        /// Currently selected index
        selected: usize,
    },
    /// Simple yes/no confirmation dialog
    SimpleConfirm {
        /// Message to display
        message: String,
        /// Action to perform on confirm
        action: SimpleConfirmAction,
        /// Which button is focused: 0 = Yes, 1 = No
        focus: usize,
    },
    /// Password prompt for SCP connection
    ScpPasswordPrompt {
        /// Target panel to connect
        target_panel: Side,
        /// Connection URI
        connection_string: String,
        /// Display name for the connection
        display_name: String,
        /// Password input
        password_input: String,
        /// Cursor position
        cursor_pos: usize,
        /// Focused element: 0 = input, 1 = OK, 2 = Cancel
        focus: usize,
        /// Error message if connection failed
        error: Option<String>,
    },
    /// Creating a new directory (F7)
    MakingDir {
        /// Name for the new directory
        name_input: String,
        /// Cursor position in the input
        cursor_pos: usize,
        /// Focused element: 0 = input field, 1 = OK, 2 = Cancel
        focus: usize,
    },
    /// Command history panel (Alt+H)
    CommandHistory {
        /// Currently selected command index (0 = oldest, len-1 = newest)
        selected: usize,
        /// Scroll offset for display
        scroll: usize,
    },
    /// Find files dialog (Alt+/)
    FindFiles {
        /// File pattern to search for (supports * and ?)
        pattern_input: String,
        /// Cursor position in pattern input
        pattern_cursor: usize,
        /// Case sensitive filename matching
        pattern_case_sensitive: bool,
        /// Text to search for inside files (empty = don't search content)
        content_input: String,
        /// Cursor position in content input
        content_cursor: usize,
        /// Case sensitive content matching
        content_case_sensitive: bool,
        /// Starting path for search
        path_input: String,
        /// Cursor position in path input
        path_cursor: usize,
        /// Whether to search recursively
        recursive: bool,
        /// Focused element: 0 = pattern, 1 = pattern_case, 2 = content, 3 = content_case, 4 = path, 5 = recursive, 6 = Search, 7 = Cancel
        focus: usize,
    },
    /// Viewer search dialog ( / )
    ViewerSearch {
        /// Original viewing state to return to
        content: ViewContent,
        scroll: usize,
        path: std::path::PathBuf,
        binary_mode: BinaryViewMode,
        /// Previous search matches (to restore if cancelled)
        prev_matches: Vec<(usize, usize)>,
        prev_current: Option<usize>,
        /// Text search input (supports * wildcard)
        text_input: String,
        /// Cursor position in text input
        text_cursor: usize,
        /// Case sensitive text search
        case_sensitive: bool,
        /// Hex search input (e.g., "4D 5A" or "4D5A")
        hex_input: String,
        /// Cursor position in hex input
        hex_cursor: usize,
        /// Focused element: 0 = text, 1 = case_sensitive, 2 = hex, 3 = Search, 4 = Cancel
        focus: usize,
    },
    /// Select files by pattern dialog (Ctrl+A / Alt+A)
    SelectFiles {
        /// Pattern to match (supports * and ?)
        pattern_input: String,
        /// Cursor position in pattern input
        pattern_cursor: usize,
        /// Include directories in selection
        include_dirs: bool,
        /// Focused element: 0 = pattern, 1 = include_dirs, 2 = Select, 3 = Cancel
        focus: usize,
    },
    /// New SCP connection dialog
    ScpConnect {
        /// Which panel to connect (after successful connection)
        target_panel: Side,
        /// Connection name (for saving)
        name_input: String,
        name_cursor: usize,
        /// Username
        user_input: String,
        user_cursor: usize,
        /// Hostname
        host_input: String,
        host_cursor: usize,
        /// Port (as string for editing)
        port_input: String,
        port_cursor: usize,
        /// Initial path (optional)
        path_input: String,
        path_cursor: usize,
        /// Password (not saved, only for current connection)
        password_input: String,
        password_cursor: usize,
        /// Focused element: 0=name, 1=user, 2=host, 3=port, 4=path, 5=password, 6=Connect, 7=Save, 8=Cancel
        focus: usize,
        /// Error message (if any)
        error: Option<String>,
    },
    /// Background task in progress (shows spinner)
    BackgroundTask {
        /// Title for the spinner dialog
        title: String,
        /// Message to display
        message: String,
        /// Current spinner animation frame
        frame: usize,
    },
    /// User menu dialog (F2)
    UserMenu {
        /// List of user menu rules from config
        rules: Vec<crate::config::UserMenuRule>,
        /// Currently selected rule index
        selected: usize,
        /// Scroll offset for display
        scroll: usize,
    },
    /// User menu edit dialog (add/edit a rule)
    UserMenuEdit {
        /// Index of rule being edited (None = adding new rule)
        editing_index: Option<usize>,
        /// Rule name input
        name_input: String,
        name_cursor: usize,
        /// Command template input
        command_input: String,
        command_cursor: usize,
        /// Hotkey input (single character)
        hotkey_input: String,
        hotkey_cursor: usize,
        /// Focused element: 0=name, 1=command, 2=hotkey, 3=Save, 4=Cancel
        focus: usize,
        /// Error message (if any)
        error: Option<String>,
    },
    /// Generic plugin connection dialog (works with any provider plugin)
    PluginConnect {
        /// Which panel to connect (after successful connection)
        target_panel: Side,
        /// Plugin scheme (e.g., "ftp", "s3", "gdrive")
        plugin_scheme: String,
        /// Plugin display name
        plugin_name: String,
        /// Field definitions from the plugin
        fields: Vec<DialogField>,
        /// Field values (indexed same as fields)
        values: Vec<String>,
        /// Cursor positions for text fields (indexed same as fields)
        cursors: Vec<usize>,
        /// Currently focused element index
        /// 0 to fields.len()-1 = fields, fields.len() = Connect, fields.len()+1 = Save, fields.len()+2 = Cancel
        focus: usize,
        /// Error message (if any)
        error: Option<String>,
    },
    /// Overwrite confirmation dialog for file operations
    OverwriteConfirm {
        /// File operation type (Copy or Move)
        operation: FileOperation,
        /// All source paths for the operation
        all_sources: Vec<PathBuf>,
        /// Destination directory
        dest: PathBuf,
        /// Conflicting destination filenames
        conflicts: Vec<PathBuf>,
        /// Current conflict index being shown
        current_conflict: usize,
        /// Paths the user chose to skip
        skip_set: HashSet<PathBuf>,
        /// Whether user chose "Overwrite All"
        overwrite_all: bool,
        /// Focused button: 0=Yes, 1=All, 2=Skip, 3=SkipAll, 4=Cancel
        focus: usize,
    },
    /// Password prompt for encrypted archive
    ArchivePasswordPrompt {
        /// Target panel
        target_panel: Side,
        /// Path to the archive file
        archive_path: PathBuf,
        /// Display name of the archive
        archive_name: String,
        /// Password input
        password_input: String,
        /// Cursor position
        cursor_pos: usize,
        /// Focused element: 0 = input, 1 = OK, 2 = Cancel
        focus: usize,
        /// Error message (e.g., "Wrong password")
        error: Option<String>,
        /// File path to retry viewing after password is set (for encrypted-files archives)
        retry_path: Option<PathBuf>,
    },
    /// File operation progress dialog
    FileOpProgress {
        /// Title (e.g., "Copying" or "Moving")
        title: String,
        /// Bytes transferred so far
        bytes_done: u64,
        /// Total bytes to transfer
        bytes_total: u64,
        /// Current file being processed
        current_file: String,
        /// Number of files completed
        files_done: usize,
        /// Total number of files
        files_total: usize,
        /// Spinner animation frame
        frame: usize,
    },
}