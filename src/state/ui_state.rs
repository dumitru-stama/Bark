//! UI-related state that changes during rendering.

/// State related to terminal dimensions and UI layout.
///
/// These values are updated during rendering to reflect the current
/// terminal size and layout configuration.
#[derive(Debug, Clone)]
pub struct UiState {
    /// Visible height for viewer (updated during rendering)
    pub viewer_height: usize,
    /// Pending 'g' key for gg/ge commands in viewer
    pub viewer_pending_g: bool,
    /// Terminal height (updated during rendering)
    pub terminal_height: u16,
    /// Terminal width (updated during rendering)
    pub terminal_width: u16,
    /// Height of the shell area (1 = just command line, more = shows history)
    pub shell_height: u16,
    /// Left panel width as percentage (10-90)
    pub left_panel_percent: u16,
    /// Whether the current text input field has its content selected
    /// (typing will replace all content). Set on Tab into a field with content.
    pub input_selected: bool,
    /// Last PTY columns sent to resize (avoid redundant ConPTY resizes on Windows)
    pub last_pty_cols: u16,
    /// Last PTY rows sent to resize (avoid redundant ConPTY resizes on Windows)
    pub last_pty_rows: u16,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            viewer_height: 0,
            viewer_pending_g: false,
            terminal_height: 24,
            terminal_width: 80,
            shell_height: 1,
            left_panel_percent: 50,
            input_selected: false,
            last_pty_cols: 0,
            last_pty_rows: 0,
        }
    }
}

#[allow(dead_code)]
impl UiState {
    /// Create a new UiState with values from config
    pub fn from_config(shell_height: u16, left_panel_percent: u16) -> Self {
        Self {
            shell_height,
            left_panel_percent,
            ..Default::default()
        }
    }

    /// Grow the shell area by one line
    pub fn grow_shell(&mut self) {
        // Maximum shell height is terminal_height - 10 (leave room for panels)
        let max_height = self.terminal_height.saturating_sub(10);
        if self.shell_height < max_height {
            self.shell_height += 1;
        }
    }

    /// Shrink the shell area by one line
    pub fn shrink_shell(&mut self) {
        // Minimum shell height is 1 (just command line)
        if self.shell_height > 1 {
            self.shell_height -= 1;
        }
    }

    /// Grow left panel (shrink right)
    pub fn grow_left_panel(&mut self) {
        if self.left_panel_percent < 90 {
            self.left_panel_percent += 5;
        }
    }

    /// Shrink left panel (grow right)
    pub fn shrink_left_panel(&mut self) {
        if self.left_panel_percent > 10 {
            self.left_panel_percent -= 5;
        }
    }
}
