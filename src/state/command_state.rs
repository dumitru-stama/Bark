//! Command line state and history management.

/// State for the command line input and shell output.
///
/// Manages command input, history navigation, tab completion,
/// and shell output display.
#[derive(Debug, Clone, Default)]
pub struct CommandState {
    /// Command line input buffer
    pub input: String,
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
                self.input = self.history.last().cloned().unwrap_or_default();
            }
            Some(0) => {
                // Already at the beginning, do nothing
            }
            Some(idx) => {
                self.history_index = Some(idx - 1);
                self.input = self.history.get(idx - 1).cloned().unwrap_or_default();
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
                self.input = std::mem::take(&mut self.history_temp);
            }
            Some(idx) => {
                self.history_index = Some(idx + 1);
                self.input = self.history.get(idx + 1).cloned().unwrap_or_default();
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
        self.output.push(line);
        // Keep last 1000 lines
        const MAX_OUTPUT: usize = 1000;
        if self.output.len() > MAX_OUTPUT {
            self.output.remove(0);
        }
    }

    /// Clear the command line
    pub fn clear(&mut self) {
        self.input.clear();
        self.history_index = None;
        self.history_temp.clear();
        self.reset_completion();
    }
}
