use crate::config;

/// Manages command history and navigation
pub struct CommandHistory {
    /// History entries (oldest first)
    entries: Vec<String>,
    /// Current position in history (None = not browsing history)
    index: Option<usize>,
    /// Temporary storage for current input when browsing history
    temp_input: String,
}

impl CommandHistory {
    /// Create new history manager and load from disk
    pub fn new() -> Self {
        Self {
            entries: config::load_command_history(),
            index: None,
            temp_input: String::new(),
        }
    }

    /// Add a command to history
    pub fn push(&mut self, command: &str) {
        let trimmed = command.trim();
        if !trimmed.is_empty() {
            // Avoid duplicates of the last command
            if self.entries.last().map(|s| s.as_str()) != Some(trimmed) {
                self.entries.push(trimmed.to_string());
                // Limit history size
                if self.entries.len() > 1000 {
                    self.entries.remove(0);
                }
                // Save history immediately
                self.save();
            }
        }
        // Reset history navigation
        self.reset_navigation();
    }

    /// Reset navigation state
    pub fn reset_navigation(&mut self) {
        self.index = None;
        self.temp_input.clear();
    }

    /// Save history to disk
    pub fn save(&self) {
        config::save_command_history(&self.entries);
    }

    /// Navigate up (older commands)
    /// Returns the command string to display
    pub fn up(&mut self, current_input: &mut String) {
        if self.entries.is_empty() {
            return;
        }

        match self.index {
            None => {
                // Save current input and go to most recent history entry
                self.temp_input = std::mem::take(current_input);
                self.index = Some(self.entries.len() - 1);
                *current_input = self.entries.last().unwrap().clone();
            }
            Some(idx) if idx > 0 => {
                // Go to older entry
                self.index = Some(idx - 1);
                *current_input = self.entries[idx - 1].clone();
            }
            Some(_) => {
                // Already at oldest entry, do nothing
            }
        }
    }

    /// Navigate down (newer commands)
    /// Returns the command string to display
    pub fn down(&mut self, current_input: &mut String) {
        match self.index {
            Some(idx) if idx + 1 < self.entries.len() => {
                // Go to newer entry
                self.index = Some(idx + 1);
                *current_input = self.entries[idx + 1].clone();
            }
            Some(_) => {
                // At most recent history entry, restore temp input
                self.index = None;
                *current_input = std::mem::take(&mut self.temp_input);
            }
            None => {
                // Not browsing history, do nothing
            }
        }
    }

    /// Get all entries
    pub fn get_entries(&self) -> &Vec<String> {
        &self.entries
    }
}
