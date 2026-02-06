//! Command line state and history management.

/// State for the command line input and shell output.
///
/// Manages command input, history navigation, tab completion,
/// and shell output display.
#[derive(Debug, Clone, Default)]
pub struct CommandState {
    /// Command line input buffer
    pub input: String,
    /// Cursor position within input (byte offset, always on a char boundary)
    pub cursor: usize,
    /// Whether command line is focused
    pub focused: bool,
    /// Command history (most recent last)
    pub history: Vec<String>,
    /// Current position in command history (None = not browsing history)
    pub history_index: Option<usize>,
    /// Temporary storage for current input when browsing history
    pub history_temp: String,
    /// Tab completion state: (original prefix, matches, current index)
    pub completion_state: Option<(String, Vec<String>, usize)>,
    /// Shell output history (recent commands and their output)
    pub output: Vec<String>,
    /// Scroll offset for shell area (0 = bottom/newest)
    pub scroll_offset: usize,
}

#[allow(dead_code)]
impl CommandState {
    /// Create a new CommandState with the given history
    pub fn with_history(history: Vec<String>) -> Self {
        Self {
            history,
            ..Default::default()
        }
    }

    // --- Cursor movement ---

    /// Move cursor one character to the left
    pub fn cursor_left(&mut self) {
        if self.cursor > 0 {
            // Find previous char boundary
            let prev = self.input[..self.cursor]
                .char_indices()
                .next_back()
                .map(|(i, _)| i)
                .unwrap_or(0);
            self.cursor = prev;
        }
    }

    /// Move cursor one character to the right
    pub fn cursor_right(&mut self) {
        if self.cursor < self.input.len() {
            let ch = self.input[self.cursor..].chars().next().unwrap();
            self.cursor += ch.len_utf8();
        }
    }

    /// Move cursor to the start of the previous word (Ctrl+Left)
    pub fn cursor_word_left(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let bytes = self.input.as_bytes();
        let mut pos = self.cursor;
        // Skip whitespace/punctuation to the left
        while pos > 0 && !bytes[pos - 1].is_ascii_alphanumeric() {
            pos -= 1;
        }
        // Skip word characters to the left
        while pos > 0 && bytes[pos - 1].is_ascii_alphanumeric() {
            pos -= 1;
        }
        self.cursor = pos;
    }

    /// Move cursor to the end of the next word (Ctrl+Right)
    pub fn cursor_word_right(&mut self) {
        let len = self.input.len();
        if self.cursor >= len {
            return;
        }
        let bytes = self.input.as_bytes();
        let mut pos = self.cursor;
        // Skip word characters to the right
        while pos < len && bytes[pos].is_ascii_alphanumeric() {
            pos += 1;
        }
        // Skip whitespace/punctuation to the right
        while pos < len && !bytes[pos].is_ascii_alphanumeric() {
            pos += 1;
        }
        self.cursor = pos;
    }

    /// Move cursor to the beginning of the line
    pub fn cursor_home(&mut self) {
        self.cursor = 0;
    }

    /// Move cursor to the end of the line
    pub fn cursor_end(&mut self) {
        self.cursor = self.input.len();
    }

    // --- Editing at cursor ---

    /// Insert a character at the cursor position
    pub fn insert_char(&mut self, c: char) {
        self.input.insert(self.cursor, c);
        self.cursor += c.len_utf8();
    }

    /// Insert a string at the cursor position
    pub fn insert_str(&mut self, s: &str) {
        self.input.insert_str(self.cursor, s);
        self.cursor += s.len();
    }

    /// Delete the character before the cursor (Backspace)
    pub fn delete_char_before(&mut self) {
        if self.cursor > 0 {
            let prev = self.input[..self.cursor]
                .char_indices()
                .next_back()
                .map(|(i, _)| i)
                .unwrap_or(0);
            self.input.drain(prev..self.cursor);
            self.cursor = prev;
        }
    }

    /// Delete the character at the cursor (Delete key)
    pub fn delete_char_at(&mut self) {
        if self.cursor < self.input.len() {
            let ch = self.input[self.cursor..].chars().next().unwrap();
            self.input.drain(self.cursor..self.cursor + ch.len_utf8());
        }
    }

    /// Delete the word before the cursor (Ctrl+Backspace / Ctrl+W)
    pub fn delete_word_before(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let start = self.cursor;
        let bytes = self.input.as_bytes();
        let mut pos = self.cursor;
        // Skip whitespace/punctuation to the left
        while pos > 0 && !bytes[pos - 1].is_ascii_alphanumeric() {
            pos -= 1;
        }
        // Skip word characters to the left
        while pos > 0 && bytes[pos - 1].is_ascii_alphanumeric() {
            pos -= 1;
        }
        self.input.drain(pos..start);
        self.cursor = pos;
    }

    /// Delete from cursor to end of line (Ctrl+K)
    pub fn delete_to_end(&mut self) {
        self.input.truncate(self.cursor);
    }

    /// Delete from cursor to beginning of line (Ctrl+U)
    pub fn delete_to_start(&mut self) {
        self.input.drain(..self.cursor);
        self.cursor = 0;
    }

    /// Set input text and place cursor at the end
    pub fn set_input(&mut self, s: String) {
        self.cursor = s.len();
        self.input = s;
    }

    /// Clear input and reset cursor
    pub fn clear_input(&mut self) {
        self.input.clear();
        self.cursor = 0;
    }

    /// Character count up to cursor (for display positioning)
    pub fn cursor_display_offset(&self) -> usize {
        self.input[..self.cursor].chars().count()
    }

    // --- History navigation ---

    /// Navigate up in command history
    pub fn history_up(&mut self) {
        if self.history.is_empty() {
            return;
        }

        match self.history_index {
            None => {
                // Start browsing from the end
                self.history_temp = self.input.clone();
                self.history_index = Some(self.history.len() - 1);
                self.set_input(self.history.last().cloned().unwrap_or_default());
            }
            Some(0) => {
                // Already at the beginning, do nothing
            }
            Some(idx) => {
                self.history_index = Some(idx - 1);
                self.set_input(self.history.get(idx - 1).cloned().unwrap_or_default());
            }
        }
    }

    /// Navigate down in command history
    pub fn history_down(&mut self) {
        match self.history_index {
            None => {
                // Not browsing history, do nothing
            }
            Some(idx) if idx + 1 >= self.history.len() => {
                // At the end, restore temp input
                self.history_index = None;
                let temp = std::mem::take(&mut self.history_temp);
                self.set_input(temp);
            }
            Some(idx) => {
                self.history_index = Some(idx + 1);
                self.set_input(self.history.get(idx + 1).cloned().unwrap_or_default());
            }
        }
    }

    /// Add a command to history
    pub fn add_to_history(&mut self, cmd: String) {
        // Don't add empty commands or duplicates of the last command
        if cmd.is_empty() {
            return;
        }
        if self.history.last() == Some(&cmd) {
            return;
        }
        self.history.push(cmd);

        // Limit history size
        const MAX_HISTORY: usize = 1000;
        if self.history.len() > MAX_HISTORY {
            self.history.remove(0);
        }
    }

    /// Reset completion state
    pub fn reset_completion(&mut self) {
        self.completion_state = None;
    }

    /// Add output line to shell history
    pub fn add_output(&mut self, line: String) {
        // On Windows, ConPTY sends a screen-buffer redraw after returning
        // from Ctrl+O shell mode which duplicates the last prompt line.
        // Skip consecutive identical lines to suppress the noise.
        #[cfg(windows)]
        if let Some(last) = self.output.last() {
            if *last == line {
                return;
            }
        }
        self.output.push(line);
        // Keep last 1000 lines
        const MAX_OUTPUT: usize = 1000;
        if self.output.len() > MAX_OUTPUT {
            self.output.remove(0);
        }
        // Auto-scroll to bottom on new output
        self.scroll_offset = 0;
    }

    /// Scroll shell area up by n lines
    pub fn scroll_up(&mut self, n: usize) {
        let max_offset = self.output.len();
        self.scroll_offset = (self.scroll_offset + n).min(max_offset);
    }

    /// Scroll shell area down by n lines
    pub fn scroll_down(&mut self, n: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(n);
    }

    /// Scroll shell area to bottom (newest output)
    pub fn scroll_to_bottom(&mut self) {
        self.scroll_offset = 0;
    }

    /// Clear the command line
    pub fn clear(&mut self) {
        self.input.clear();
        self.cursor = 0;
        self.history_index = None;
        self.history_temp.clear();
        self.reset_completion();
    }
}
